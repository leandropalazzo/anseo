//! Story 41.1 — HERMETIC registry-client tests.
//!
//! Every test builds a fake registry tree in a temp dir (or an in-memory map)
//! and drives the client through [`FileTransport`] / [`InMemoryTransport`].
//! Nothing here touches the network or a live registry, so the suite is
//! CI-green offline.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::*;
use crate::signing::{signing_digest, NamespaceClaim, SignatureStatus, SigningError};

const MANIFEST_YAML: &str = r#"
name: "priya.perplexity-pro"
version: "0.3.1"
description: "Higher-recall extraction."
capabilities:
  - kind: "network"
    allowlist: ["api.perplexity.ai"]
plugin_type: "extractor"
entry_point: "entrypoint.wasm"
"#;

const ENTRYPOINT: &[u8] = b"\0asm\x01\0\0\0fake-wasm-bytes";
const ID: &str = "priya.perplexity-pro";
const VERSION: &str = "0.3.1";

fn gen_key() -> SigningKey {
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    SigningKey::from_bytes(&seed)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// A self-consistent fixture: real root + author keys, valid claim + signature.
struct Fixture {
    dir: TempDir,
    root_pub: [u8; 32],
    author_pub: [u8; 32],
}

/// Knobs to deliberately break a fixture for the negative cases.
#[derive(Default)]
struct Tamper {
    /// Put a wrong sha256 in the index for the artifact.
    bad_checksum: bool,
    /// Corrupt the entrypoint bytes after signing (signature won't verify, but
    /// keep checksum honest so we isolate the signature path).
    corrupt_entrypoint_after_sign: bool,
    /// Drop signature.bin / claim.toml entirely.
    omit_signature: bool,
}

fn write(dir: &std::path::Path, rel: &str, bytes: &[u8]) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

struct MockHttpRegistry {
    base_url: String,
}

fn spawn_http_registry(routes: Vec<(String, Vec<u8>)>, max_requests: usize) -> MockHttpRegistry {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let routes: HashMap<String, Vec<u8>> = routes.into_iter().collect();
    thread::spawn(move || {
        for _ in 0..max_requests {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0_u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let path = path.trim_start_matches('/');

            if let Some(body) = routes.get(path) {
                let headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(headers.as_bytes());
                let _ = stream.write_all(body);
            } else {
                let body = b"not found";
                let headers = format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(headers.as_bytes());
                let _ = stream.write_all(body);
            }
        }
    });

    MockHttpRegistry {
        base_url: format!("http://{addr}"),
    }
}

fn build_fixture(t: Tamper) -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = gen_key();
    let author = gen_key();

    let manifest_bytes = MANIFEST_YAML.as_bytes();

    // Author signs SHA-256(manifest || entrypoint).
    let plugin_sig = author
        .sign(&signing_digest(manifest_bytes, ENTRYPOINT))
        .to_bytes();

    // Root signs the canonical namespace claim.
    let claim = NamespaceClaim {
        namespace: "priya".into(),
        keyid: "k1".into(),
        author_pubkey: author.verifying_key().to_bytes(),
        rotation_of: None,
    };
    let claim_sig = root.sign(&claim.canonical_bytes()).to_bytes();

    // index.toml — checksum optionally tampered.
    let sha = if t.bad_checksum {
        "00".repeat(32)
    } else {
        sha256_hex(ENTRYPOINT)
    };
    let index = format!(
        r#"schema_version = "1"

[[plugin]]
id = "{ID}"
version = "{VERSION}"
description = "Higher-recall extraction."
sha256 = "{sha}"
"#
    );
    write(dir.path(), "index.toml", index.as_bytes());

    let base = format!("plugins/{ID}/{VERSION}");
    write(dir.path(), &format!("{base}/manifest.yaml"), manifest_bytes);

    let entry_on_disk = if t.corrupt_entrypoint_after_sign {
        b"\0asm\x01\0\0\0EVIL-wasm-bytes".to_vec()
    } else {
        ENTRYPOINT.to_vec()
    };
    write(
        dir.path(),
        &format!("{base}/entrypoint.wasm"),
        &entry_on_disk,
    );

    if !t.omit_signature {
        write(dir.path(), &format!("{base}/signature.bin"), &plugin_sig);
        let claim_toml = format!(
            r#"namespace = "priya"
keyid = "k1"
author_pubkey = "{}"
signature = "{}"
"#,
            hex::encode(claim.author_pubkey),
            hex::encode(claim_sig)
        );
        write(
            dir.path(),
            &format!("{base}/claim.toml"),
            claim_toml.as_bytes(),
        );
    }

    Fixture {
        dir,
        root_pub: root.verifying_key().to_bytes(),
        author_pub: author.verifying_key().to_bytes(),
    }
}

fn client(fx: &Fixture) -> RegistryClient<FileTransport> {
    RegistryClient::new(FileTransport::new(fx.dir.path()))
}

#[test]
fn happy_path_verifies_checksum_and_signature() {
    let fx = build_fixture(Tamper::default());
    let v = client(&fx)
        .fetch_verified(ID, VERSION, &[fx.root_pub], None)
        .expect("valid artifact must verify");
    assert_eq!(v.id, ID);
    assert_eq!(v.version, VERSION);
    assert_eq!(v.status, SignatureStatus::Signed);
    assert_eq!(v.author_key_to_pin, fx.author_pub);
    assert_eq!(v.entrypoint_bytes, ENTRYPOINT);
    assert_eq!(v.manifest.name, ID);
}

#[test]
fn latest_resolves_to_artifact() {
    let fx = build_fixture(Tamper::default());
    let v = client(&fx)
        .fetch_verified(ID, "latest", &[fx.root_pub], None)
        .expect("latest must resolve");
    assert_eq!(v.version, VERSION);
}

#[test]
fn tampered_checksum_is_rejected() {
    let fx = build_fixture(Tamper {
        bad_checksum: true,
        ..Default::default()
    });
    let err = client(&fx)
        .fetch_verified(ID, VERSION, &[fx.root_pub], None)
        .unwrap_err();
    match err {
        RegistryError::ChecksumMismatch {
            expected, actual, ..
        } => {
            assert_eq!(expected, "00".repeat(32));
            assert_eq!(actual, sha256_hex(ENTRYPOINT));
        }
        other => panic!("expected ChecksumMismatch, got {other:?}"),
    }
}

#[test]
fn bad_signature_is_rejected() {
    // Entrypoint corrupted after signing; checksum is recomputed honestly over
    // the corrupt bytes so we reach (and fail) the signature step.
    let fx = build_fixture(Tamper {
        corrupt_entrypoint_after_sign: true,
        ..Default::default()
    });
    // Rewrite the index checksum to match the corrupt artifact on disk.
    let corrupt = b"\0asm\x01\0\0\0EVIL-wasm-bytes";
    let index = format!(
        r#"schema_version = "1"

[[plugin]]
id = "{ID}"
version = "{VERSION}"
sha256 = "{}"
"#,
        sha256_hex(corrupt)
    );
    std::fs::write(fx.dir.path().join("index.toml"), index).unwrap();

    let err = client(&fx)
        .fetch_verified(ID, VERSION, &[fx.root_pub], None)
        .unwrap_err();
    match err {
        RegistryError::Verification { source, .. } => {
            assert_eq!(source, SigningError::BadSignature);
        }
        other => panic!("expected Verification(BadSignature), got {other:?}"),
    }
}

#[test]
fn untrusted_root_is_rejected() {
    let fx = build_fixture(Tamper::default());
    let impostor = gen_key().verifying_key().to_bytes();
    let err = client(&fx)
        .fetch_verified(ID, VERSION, &[impostor], None)
        .unwrap_err();
    match err {
        RegistryError::Verification { source, .. } => {
            assert!(matches!(source, SigningError::UntrustedNamespaceClaim(_)));
        }
        other => panic!("expected Verification(Untrusted...), got {other:?}"),
    }
}

#[test]
fn unsigned_artifact_is_rejected() {
    let fx = build_fixture(Tamper {
        omit_signature: true,
        ..Default::default()
    });
    let err = client(&fx)
        .fetch_verified(ID, VERSION, &[fx.root_pub], None)
        .unwrap_err();
    assert!(matches!(err, RegistryError::Unsigned { .. }), "got {err:?}");
}

/// Story 41.3 install-audit fix: an unsigned artifact requested as `latest`
/// surfaces the *resolved concrete version* in `RegistryError::Unsigned`, not
/// the literal `"latest"`. The API install handler records this version in the
/// audit table, so the audit row must carry the real version that was installed.
#[test]
fn unsigned_latest_reports_resolved_version() {
    let fx = build_fixture(Tamper {
        omit_signature: true,
        ..Default::default()
    });
    let err = client(&fx)
        .fetch_verified(ID, "latest", &[fx.root_pub], None)
        .unwrap_err();
    match err {
        RegistryError::Unsigned { id, version } => {
            assert_eq!(id, ID);
            assert_eq!(
                version, VERSION,
                "unsigned error must carry the resolved concrete version, not `latest`"
            );
        }
        other => panic!("expected Unsigned with resolved version, got {other:?}"),
    }
}

#[test]
fn unknown_plugin_is_rejected() {
    let fx = build_fixture(Tamper::default());
    let err = client(&fx)
        .fetch_verified("nobody.nothing", VERSION, &[fx.root_pub], None)
        .unwrap_err();
    assert!(
        matches!(err, RegistryError::UnknownPlugin { .. }),
        "got {err:?}"
    );
}

#[test]
fn search_matches_id_and_description() {
    let fx = build_fixture(Tamper::default());
    let c = client(&fx);
    assert_eq!(c.search("perplexity").unwrap().len(), 1);
    assert_eq!(c.search("recall").unwrap().len(), 1);
    assert_eq!(c.search("nonexistent").unwrap().len(), 0);
}

#[test]
fn malformed_index_search_is_hard_error_but_lenient_is_empty() {
    // A valid fixture, then clobber index.toml with non-TOML garbage.
    let fx = build_fixture(Tamper::default());
    std::fs::write(
        fx.dir.path().join("index.toml"),
        b"this is not = valid toml [[[",
    )
    .unwrap();
    let c = client(&fx);

    // Strict search surfaces the parse error...
    let err = c.search("anything").unwrap_err();
    assert!(matches!(err, RegistryError::Parse { .. }), "got {err:?}");

    // ...but the lenient path used by `anseo plugin search` degrades to empty
    // (Story 41.1 AC6 / Notes) rather than crashing.
    let hits = c.search_lenient("anything").unwrap();
    assert!(hits.is_empty());
}

#[test]
fn empty_index_search_returns_zero_results() {
    // Zero-state: a registry whose index lists no plugins (the seed state).
    let mem = InMemoryTransport::new();
    mem.insert("index.toml", b"schema_version = \"1\"\n".to_vec());
    let c = RegistryClient::new(mem);
    assert!(c.search("anything").unwrap().is_empty());
    assert!(c.search_lenient("anything").unwrap().is_empty());
}

#[test]
fn missing_index_search_is_transport_not_found() {
    // No index.toml at all → NotFound (the CLI maps this to the AC4
    // "registry unreachable" message; search_lenient still propagates it).
    let mem = InMemoryTransport::new();
    let c = RegistryClient::new(mem);
    let err = c.search_lenient("anything").unwrap_err();
    assert!(matches!(err, RegistryError::NotFound { .. }), "got {err:?}");
}

#[test]
fn revocation_list_absent_is_empty() {
    let fx = build_fixture(Tamper::default());
    let revs = client(&fx).revocations().unwrap();
    assert!(revs.revoked_keys.is_empty());
    assert!(revs.revoked_plugins.is_empty());
}

#[test]
fn revoked_plugin_blocks_install() {
    let fx = build_fixture(Tamper::default());
    write(
        fx.dir.path(),
        "keys/revoked.toml",
        format!("revoked_plugins = [[\"{ID}\", \"{VERSION}\"]]\n").as_bytes(),
    );
    let err = client(&fx)
        .fetch_verified(ID, VERSION, &[fx.root_pub], None)
        .unwrap_err();
    match err {
        RegistryError::Verification { source, .. } => {
            assert!(matches!(source, SigningError::RevokedPlugin { .. }));
        }
        other => panic!("expected Verification(RevokedPlugin), got {other:?}"),
    }
}

#[test]
fn in_memory_transport_round_trips() {
    // Reuse a file fixture to get valid bytes, then mirror it into memory.
    let fx = build_fixture(Tamper::default());
    let mem = InMemoryTransport::new();
    for rel in [
        "index.toml",
        &format!("plugins/{ID}/{VERSION}/manifest.yaml"),
        &format!("plugins/{ID}/{VERSION}/entrypoint.wasm"),
        &format!("plugins/{ID}/{VERSION}/signature.bin"),
        &format!("plugins/{ID}/{VERSION}/claim.toml"),
    ] {
        mem.insert(
            rel.to_string(),
            std::fs::read(fx.dir.path().join(rel)).unwrap(),
        );
    }
    let v = RegistryClient::new(mem)
        .fetch_verified(ID, VERSION, &[fx.root_pub], None)
        .expect("in-memory fixture must verify");
    assert_eq!(v.status, SignatureStatus::Signed);
}

#[test]
fn http_transport_searches_valid_index() {
    let fx = build_fixture(Tamper::default());
    let server = spawn_http_registry(
        vec![(
            "index.toml".to_string(),
            std::fs::read(fx.dir.path().join("index.toml")).unwrap(),
        )],
        1,
    );
    let c = RegistryClient::with_url(server.base_url);

    let hits = c.search("perplexity").unwrap();

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, ID);
}

#[test]
fn http_transport_malformed_index_is_strict_error_but_lenient_empty() {
    let server = spawn_http_registry(
        vec![("index.toml".to_string(), b"not valid toml [[[[".to_vec())],
        2,
    );
    let c = RegistryClient::with_url(server.base_url);

    let err = c.search("anything").unwrap_err();
    assert!(matches!(err, RegistryError::Parse { .. }), "got {err:?}");
    assert!(c.search_lenient("anything").unwrap().is_empty());
}

#[test]
fn http_transport_tampered_artifact_reports_integrity_check_failed() {
    let fx = build_fixture(Tamper {
        bad_checksum: true,
        ..Default::default()
    });
    let base = format!("plugins/{ID}/{VERSION}");
    let routes = vec![
        (
            "index.toml".to_string(),
            std::fs::read(fx.dir.path().join("index.toml")).unwrap(),
        ),
        (
            format!("{base}/manifest.yaml"),
            std::fs::read(fx.dir.path().join(format!("{base}/manifest.yaml"))).unwrap(),
        ),
        (
            format!("{base}/entrypoint.wasm"),
            std::fs::read(fx.dir.path().join(format!("{base}/entrypoint.wasm"))).unwrap(),
        ),
    ];
    let server = spawn_http_registry(routes, 3);
    let err = RegistryClient::with_url(server.base_url)
        .fetch_verified(ID, VERSION, &[fx.root_pub], None)
        .unwrap_err();

    assert!(
        err.to_string().contains("integrity check failed"),
        "unexpected error: {err}"
    );
    assert!(matches!(err, RegistryError::ChecksumMismatch { .. }));
}

#[test]
fn http_transport_url_and_env_defaults() {
    // No network: only assert URL composition + env defaulting.
    std::env::remove_var(REGISTRY_URL_ENV);
    let t = HttpTransport::from_env();
    assert_eq!(t.base_url, DEFAULT_REGISTRY_URL);

    std::env::set_var(REGISTRY_URL_ENV, "https://example.test/reg/");
    let t = HttpTransport::from_env();
    assert_eq!(t.base_url, "https://example.test/reg"); // trailing slash trimmed
    std::env::remove_var(REGISTRY_URL_ENV);
}

#[test]
fn file_transport_rejects_path_traversal() {
    let fx = build_fixture(Tamper::default());
    let t = FileTransport::new(fx.dir.path());
    let err = t.fetch("../../etc/passwd").unwrap_err();
    assert!(
        matches!(err, RegistryError::Transport { .. }),
        "got {err:?}"
    );
}
