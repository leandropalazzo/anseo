//! Story 41.4 — `anseo plugin {keygen, sign}`: the signing *producers*.
//!
//! These are the operator-/CI-side tools that produce the artifacts the
//! verify-only install path (`anseo_plugin_host::signing::verify_signed_plugin`)
//! checks. They call the producer primitives that live in the same module as
//! the verifier (`sign_plugin` / `sign_namespace_claim`), so a signature these
//! commands emit is guaranteed to verify under the exact same byte layout the
//! worker / `anseo plugin install` enforces.
//!
//! Output layout (matches `FsRegistry` / the GitHub flat-file registry):
//!
//! ```text
//! <version-dir>/manifest.yaml    # input
//! <version-dir>/entrypoint.wasm  # input
//! <version-dir>/signature.bin    # 64-byte detached author signature  (written)
//! <version-dir>/claim.toml       # namespace claim + root signature    (written)
//! ```
//!
//! Keys never live in the public repo: `keygen` writes a keypair to a path the
//! operator chooses (then uploads the secret to the `ANSEO_PLUGIN_SIGNING_KEY`
//! GitHub Actions secret in `opengeo-internal`); `sign` reads the secret seed
//! from `ANSEO_PLUGIN_SIGNING_KEY` (env / `--key-file`). See
//! `docs/plugin-signing.md`.

use std::path::PathBuf;

use anseo_core::OpenGeoError;
use anseo_plugin_host::signing::{sign_namespace_claim, sign_plugin, Keypair, NamespaceClaim};
use clap::Args;

/// `anseo plugin keygen` — generate a fresh Ed25519 keypair.
#[derive(Debug, Args)]
pub struct KeygenArgs {
    /// Write the 32-byte secret seed (hex) here. The PUBLIC key is printed to
    /// stdout (pin it as `ANSEO_ROOT_PUBKEY`). The secret must NEVER be
    /// committed — store it as the `ANSEO_PLUGIN_SIGNING_KEY` GitHub Actions
    /// secret. If omitted, both keys print to stdout (use only in a scratch
    /// shell).
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,
}

pub fn run_keygen(args: KeygenArgs) -> Result<(), OpenGeoError> {
    let kp = Keypair::generate();
    println!("public  (pin as ANSEO_ROOT_PUBKEY): {}", kp.public_hex());
    match args.out {
        Some(path) => {
            write_secret(&path, &kp.secret_hex())?;
            println!("secret  written to {} (DO NOT COMMIT)", path.display());
            println!(
                "next: upload it as the `ANSEO_PLUGIN_SIGNING_KEY` secret in opengeo-internal."
            );
        }
        None => {
            println!(
                "secret  (store as ANSEO_PLUGIN_SIGNING_KEY): {}",
                kp.secret_hex()
            );
            println!("warning: secret printed to stdout — only do this in a private shell.");
        }
    }
    Ok(())
}

/// `anseo plugin sign` — sign a plugin bundle + emit its namespace claim.
#[derive(Debug, Args)]
pub struct SignArgs {
    /// Directory holding `manifest.yaml` + `entrypoint.wasm`. `signature.bin`
    /// and `claim.toml` are written alongside.
    #[arg(value_name = "VERSION_DIR")]
    pub dir: PathBuf,

    /// The plugin namespace (e.g. `anseo`), recorded in the claim.
    #[arg(long)]
    pub namespace: String,

    /// The signing key id recorded in the claim (e.g. `root-2026`).
    #[arg(long, default_value = "root")]
    pub keyid: String,

    /// Path to a file containing the hex of the AUTHOR's 32-byte secret seed.
    /// The author key may also be supplied via the `ANSEO_PLUGIN_AUTHOR_KEY`
    /// env var (the file takes precedence). Secret material is NEVER accepted on
    /// the command line — argv is visible in process listings, shell history and
    /// CI logs. When neither is set, the author key is the same as the root key
    /// (first-party single-key model).
    #[arg(long, value_name = "PATH")]
    pub author_key_file: Option<PathBuf>,

    /// Path to a file containing the hex root secret seed. Overrides the
    /// `ANSEO_PLUGIN_SIGNING_KEY` env var.
    #[arg(long, value_name = "PATH")]
    pub key_file: Option<PathBuf>,
}

pub fn run_sign(args: SignArgs) -> Result<(), OpenGeoError> {
    let root = load_root_key(&args)?;
    let author = match load_author_seed(&args)? {
        Some(seed) => Keypair::from_secret_bytes(seed),
        None => root.clone(),
    };

    let manifest_path = args.dir.join("manifest.yaml");
    let entry_path = args.dir.join("entrypoint.wasm");
    let manifest_bytes = std::fs::read(&manifest_path)
        .map_err(|e| OpenGeoError::Config(format!("read {}: {e}", manifest_path.display())))?;
    let entrypoint_bytes = std::fs::read(&entry_path)
        .map_err(|e| OpenGeoError::Config(format!("read {}: {e}", entry_path.display())))?;

    // Author signs the bundle digest; root signs the namespace claim.
    let plugin_sig = sign_plugin(&author, &manifest_bytes, &entrypoint_bytes);

    let claim = NamespaceClaim {
        namespace: args.namespace.clone(),
        keyid: args.keyid.clone(),
        author_pubkey: author.public,
        rotation_of: None, // TODO(key-rotation): wire when the rotation story lands.
    };
    let claim_sig = sign_namespace_claim(&root, &claim);

    // signature.bin = raw 64-byte detached author signature.
    let sig_path = args.dir.join("signature.bin");
    std::fs::write(&sig_path, plugin_sig)
        .map_err(|e| OpenGeoError::Config(format!("write {}: {e}", sig_path.display())))?;

    // claim.toml = the ClaimFile shape FsRegistry parses (all hex).
    let claim_toml = format!(
        "namespace = \"{}\"\nkeyid = \"{}\"\nauthor_pubkey = \"{}\"\nsignature = \"{}\"\n",
        args.namespace,
        args.keyid,
        hex::encode(author.public),
        hex::encode(claim_sig),
    );
    let claim_path = args.dir.join("claim.toml");
    std::fs::write(&claim_path, claim_toml)
        .map_err(|e| OpenGeoError::Config(format!("write {}: {e}", claim_path.display())))?;

    println!(
        "signed {} ({} bytes wasm)",
        args.dir.display(),
        entrypoint_bytes.len()
    );
    println!("  wrote {}", sig_path.display());
    println!("  wrote {}", claim_path.display());
    println!(
        "  root pubkey (must be pinned as ANSEO_ROOT_PUBKEY): {}",
        root.public_hex()
    );
    Ok(())
}

const SIGNING_KEY_ENV: &str = "ANSEO_PLUGIN_SIGNING_KEY";
const AUTHOR_KEY_ENV: &str = "ANSEO_PLUGIN_AUTHOR_KEY";

/// Resolve the AUTHOR seed without ever reading secret material from argv.
/// Precedence: `--author-key-file` (a 0600 file) > `ANSEO_PLUGIN_AUTHOR_KEY`
/// env var. Returns `None` when neither is set (single-key model: author == root).
fn load_author_seed(args: &SignArgs) -> Result<Option<[u8; 32]>, OpenGeoError> {
    if let Some(path) = &args.author_key_file {
        let hex = std::fs::read_to_string(path).map_err(|e| {
            OpenGeoError::Config(format!("read author key file {}: {e}", path.display()))
        })?;
        return Ok(Some(decode_seed(hex.trim())?));
    }
    match std::env::var(AUTHOR_KEY_ENV) {
        Ok(hex) => Ok(Some(decode_seed(hex.trim())?)),
        Err(_) => Ok(None),
    }
}

fn load_root_key(args: &SignArgs) -> Result<Keypair, OpenGeoError> {
    let hex = match &args.key_file {
        Some(path) => std::fs::read_to_string(path)
            .map_err(|e| OpenGeoError::Config(format!("read key file {}: {e}", path.display())))?,
        None => std::env::var(SIGNING_KEY_ENV).map_err(|_| {
            OpenGeoError::Config(format!(
                "no signing key: set ${SIGNING_KEY_ENV} or pass --key-file <path>"
            ))
        })?,
    };
    Ok(Keypair::from_secret_bytes(decode_seed(hex.trim())?))
}

fn decode_seed(hex_str: &str) -> Result<[u8; 32], OpenGeoError> {
    let bytes = hex::decode(hex_str.trim())
        .map_err(|e| OpenGeoError::Config(format!("invalid hex key: {e}")))?;
    <[u8; 32]>::try_from(bytes.as_slice())
        .map_err(|_| OpenGeoError::Config("signing key must be a 32-byte hex seed".into()))
}

fn write_secret(path: &std::path::Path, hex: &str) -> Result<(), OpenGeoError> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| OpenGeoError::Config(format!("mkdir {}: {e}", parent.display())))?;
        }
    }

    // On Unix, create the file with mode 0600 *up front* so the secret seed is
    // never briefly world-readable between create and chmod. Any failure to
    // apply restrictive permissions is surfaced, not ignored.
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| {
                OpenGeoError::Config(format!("create {} (mode 0600): {e}", path.display()))
            })?
    };

    #[cfg(not(unix))]
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| OpenGeoError::Config(format!("create {}: {e}", path.display())))?;

    file.write_all(format!("{hex}\n").as_bytes())
        .map_err(|e| OpenGeoError::Config(format!("write {}: {e}", path.display())))?;

    // Verify the on-disk permissions are actually restrictive (e.g. a
    // pre-existing umask or fs that ignored the create mode) and fail loudly
    // if the secret ended up readable by group/other.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = file
            .metadata()
            .map_err(|e| OpenGeoError::Config(format!("stat {}: {e}", path.display())))?
            .permissions()
            .mode()
            & 0o777;
        if mode & 0o077 != 0 {
            // Try once to tighten it, then re-check.
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| OpenGeoError::Config(format!("chmod 0600 {}: {e}", path.display())))?;
            let mode = std::fs::metadata(path)
                .map_err(|e| OpenGeoError::Config(format!("stat {}: {e}", path.display())))?
                .permissions()
                .mode()
                & 0o777;
            if mode & 0o077 != 0 {
                return Err(OpenGeoError::Config(format!(
                    "refusing to leave secret {} with permissions {:o} (group/other readable)",
                    path.display(),
                    mode
                )));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anseo_plugin_host::signing::{verify_signed_plugin, RevocationList, SignedPlugin};

    /// End-to-end: `keygen` → `sign` → the produced `signature.bin` + `claim.toml`
    /// verify under the real install-path verifier. This is the AC2 guarantee.
    #[test]
    fn sign_command_output_verifies() {
        let dir = tempfile::tempdir().unwrap();
        let vdir = dir.path();
        let manifest = b"name: anseo/demo\nversion: 1.0.0\npublisher: anseo.ai\n";
        let entrypoint = b"\0asm\x01\0\0\0";
        std::fs::write(vdir.join("manifest.yaml"), manifest).unwrap();
        std::fs::write(vdir.join("entrypoint.wasm"), entrypoint).unwrap();

        let root = Keypair::generate();
        let key_file = vdir.join("root.key");
        std::fs::write(&key_file, root.secret_hex()).unwrap();

        run_sign(SignArgs {
            dir: vdir.to_path_buf(),
            namespace: "anseo".into(),
            keyid: "root-2026".into(),
            author_key_file: None,
            key_file: Some(key_file),
        })
        .expect("sign must succeed");

        // Re-read what we wrote and verify it the way install does.
        let plugin_sig = std::fs::read(vdir.join("signature.bin")).unwrap();
        let claim_raw = std::fs::read_to_string(vdir.join("claim.toml")).unwrap();
        let author_hex = claim_raw
            .lines()
            .find(|l| l.starts_with("author_pubkey"))
            .and_then(|l| l.split('"').nth(1))
            .unwrap();
        let sig_hex = claim_raw
            .lines()
            .find(|l| l.starts_with("signature"))
            .and_then(|l| l.split('"').nth(1))
            .unwrap();
        let author_pubkey: [u8; 32] = hex::decode(author_hex)
            .unwrap()
            .as_slice()
            .try_into()
            .unwrap();
        let claim_sig = hex::decode(sig_hex).unwrap();

        let claim = NamespaceClaim {
            namespace: "anseo".into(),
            keyid: "root-2026".into(),
            author_pubkey,
            rotation_of: None,
        };
        let signed = SignedPlugin {
            plugin_id: "anseo/demo",
            version: "1.0.0",
            manifest_bytes: manifest,
            entrypoint_bytes: entrypoint,
            signature: &plugin_sig,
            claim: &claim,
            claim_signature: &claim_sig,
        };
        verify_signed_plugin(&signed, &[root.public], &RevocationList::default(), None)
            .expect("CLI-signed bundle must verify under the install-path verifier");
    }

    /// `keygen --out` must write the secret seed file with mode 0600 on Unix so
    /// it is never group/other readable (Finding 3).
    #[cfg(unix)]
    #[test]
    fn keygen_secret_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("root.key");

        run_keygen(KeygenArgs {
            out: Some(out.clone()),
        })
        .expect("keygen must succeed");

        let mode = std::fs::metadata(&out).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "secret seed file must be 0600, got {mode:o}");

        // Sanity: it actually contains a 32-byte hex seed (+ trailing newline).
        let contents = std::fs::read_to_string(&out).unwrap();
        assert_eq!(hex::decode(contents.trim()).unwrap().len(), 32);
    }

    /// A distinct AUTHOR key supplied via `--author-key-file` (never argv) must
    /// be used for the bundle signature and still verify under the install-path
    /// verifier. Guards the Finding-2 input mechanism.
    #[test]
    fn author_key_from_file_is_used_and_verifies() {
        let dir = tempfile::tempdir().unwrap();
        let vdir = dir.path();
        let manifest = b"name: anseo/demo\nversion: 1.0.0\npublisher: anseo.ai\n";
        let entrypoint = b"\0asm\x01\0\0\0";
        std::fs::write(vdir.join("manifest.yaml"), manifest).unwrap();
        std::fs::write(vdir.join("entrypoint.wasm"), entrypoint).unwrap();

        let root = Keypair::generate();
        let author = Keypair::generate();
        let root_file = vdir.join("root.key");
        let author_file = vdir.join("author.key");
        std::fs::write(&root_file, root.secret_hex()).unwrap();
        std::fs::write(&author_file, author.secret_hex()).unwrap();

        run_sign(SignArgs {
            dir: vdir.to_path_buf(),
            namespace: "anseo".into(),
            keyid: "root-2026".into(),
            author_key_file: Some(author_file),
            key_file: Some(root_file),
        })
        .expect("sign must succeed");

        // claim.toml must record the AUTHOR pubkey (distinct from root).
        let claim_raw = std::fs::read_to_string(vdir.join("claim.toml")).unwrap();
        let author_hex = claim_raw
            .lines()
            .find(|l| l.starts_with("author_pubkey"))
            .and_then(|l| l.split('"').nth(1))
            .unwrap();
        let recorded: [u8; 32] = hex::decode(author_hex)
            .unwrap()
            .as_slice()
            .try_into()
            .unwrap();
        assert_eq!(
            recorded, author.public,
            "author key file must drive the signature"
        );
        assert_ne!(
            recorded, root.public,
            "author must be distinct from root here"
        );

        // And the produced bundle verifies under the install-path verifier.
        let plugin_sig = std::fs::read(vdir.join("signature.bin")).unwrap();
        let sig_hex = claim_raw
            .lines()
            .find(|l| l.starts_with("signature"))
            .and_then(|l| l.split('"').nth(1))
            .unwrap();
        let claim_sig = hex::decode(sig_hex).unwrap();
        let claim = NamespaceClaim {
            namespace: "anseo".into(),
            keyid: "root-2026".into(),
            author_pubkey: recorded,
            rotation_of: None,
        };
        let signed = SignedPlugin {
            plugin_id: "anseo/demo",
            version: "1.0.0",
            manifest_bytes: manifest,
            entrypoint_bytes: entrypoint,
            signature: &plugin_sig,
            claim: &claim,
            claim_signature: &claim_sig,
        };
        verify_signed_plugin(&signed, &[root.public], &RevocationList::default(), None)
            .expect("author-key-file bundle must verify");
    }
}
