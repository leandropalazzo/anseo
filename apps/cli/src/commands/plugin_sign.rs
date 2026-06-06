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
            println!("secret  (store as ANSEO_PLUGIN_SIGNING_KEY): {}", kp.secret_hex());
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

    /// Hex of the AUTHOR's 32-byte secret seed. When omitted, the author key is
    /// the same as the root key (first-party single-key model).
    #[arg(long, value_name = "HEX")]
    pub author_secret: Option<String>,

    /// Path to a file containing the hex root secret seed. Overrides the
    /// `ANSEO_PLUGIN_SIGNING_KEY` env var.
    #[arg(long, value_name = "PATH")]
    pub key_file: Option<PathBuf>,
}

pub fn run_sign(args: SignArgs) -> Result<(), OpenGeoError> {
    let root = load_root_key(&args)?;
    let author = match &args.author_secret {
        Some(hex) => Keypair::from_secret_bytes(decode_seed(hex)?),
        None => root.clone(),
    };

    let manifest_path = args.dir.join("manifest.yaml");
    let entry_path = args.dir.join("entrypoint.wasm");
    let manifest_bytes = std::fs::read(&manifest_path).map_err(|e| {
        OpenGeoError::Config(format!("read {}: {e}", manifest_path.display()))
    })?;
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

    println!("signed {} ({} bytes wasm)", args.dir.display(), entrypoint_bytes.len());
    println!("  wrote {}", sig_path.display());
    println!("  wrote {}", claim_path.display());
    println!("  root pubkey (must be pinned as ANSEO_ROOT_PUBKEY): {}", root.public_hex());
    Ok(())
}

const SIGNING_KEY_ENV: &str = "ANSEO_PLUGIN_SIGNING_KEY";

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
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| OpenGeoError::Config(format!("mkdir {}: {e}", parent.display())))?;
        }
    }
    std::fs::write(path, format!("{hex}\n"))
        .map_err(|e| OpenGeoError::Config(format!("write {}: {e}", path.display())))?;
    // Best-effort 0600 on Unix so the secret seed isn't world-readable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
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
            author_secret: None,
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
        let author_pubkey: [u8; 32] =
            hex::decode(author_hex).unwrap().as_slice().try_into().unwrap();
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
}
