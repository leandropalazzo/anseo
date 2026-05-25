//! `ogeo init` — scaffold a new OpenGEO project (FR-10).
//!
//! Behavior:
//! - In an empty directory, creates `opengeo.yaml`, `.gitignore`, `README.md`.
//! - Pre-existing files at any of those paths prompt per-file before overwrite
//!   (interactive). `--force` overwrites without prompting; `--no-overwrite`
//!   refuses to touch any existing file and exits non-zero if at least one
//!   would have been overwritten.
//! - Declining the interactive prompt exits non-zero **without** writing any
//!   partial scaffold (no half-applied state).
//!
//! Scaffolded `opengeo.yaml` is verified valid against the v0.1 schema by the
//! `opengeo_init_writes_a_valid_schema_v0_1_config` test in
//! `tests/cli_smoke.rs`.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use clap::Args;
use opengeo_core::OpenGeoError;

use crate::scaffold::{GITIGNORE, OPENGEO_YAML, README};

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Target directory. Defaults to the current working directory.
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,

    /// Overwrite existing files without prompting.
    #[arg(long, conflicts_with = "no_overwrite")]
    pub force: bool,

    /// Refuse to overwrite any existing file; exit non-zero if any would have
    /// been overwritten. Useful for CI scripts that need a clean scaffold.
    #[arg(long, conflicts_with = "force")]
    pub no_overwrite: bool,
}

/// Each scaffolded file as a `(path, contents)` pair.
fn scaffold_files() -> [(&'static str, &'static str); 3] {
    [
        ("opengeo.yaml", OPENGEO_YAML),
        (".gitignore", GITIGNORE),
        ("README.md", README),
    ]
}

pub fn run(args: InitArgs) -> Result<(), OpenGeoError> {
    let dir = args
        .dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("CWD must be readable"));

    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| {
            OpenGeoError::Config(format!(
                "failed to create target directory `{}`: {e}",
                dir.display()
            ))
        })?;
    }

    let files = scaffold_files();
    let preexisting: Vec<&str> = files
        .iter()
        .filter(|(name, _)| dir.join(name).exists())
        .map(|(name, _)| *name)
        .collect();

    if !preexisting.is_empty() && args.no_overwrite {
        return Err(OpenGeoError::Config(format!(
            "refusing to overwrite existing file(s): {}",
            preexisting.join(", ")
        )));
    }

    // Decide per file whether to write.
    let mut planned_writes: Vec<(&str, &str)> = Vec::new();
    for (name, contents) in &files {
        let path = dir.join(name);
        if !path.exists() || args.force {
            planned_writes.push((name, contents));
            continue;
        }
        // Interactive prompt path.
        let confirmed = confirm_overwrite(name)?;
        if confirmed {
            planned_writes.push((name, contents));
        } else {
            // FR-10: "on decline, exits non-zero without partial overwrite".
            return Err(OpenGeoError::Config(format!(
                "init declined: existing `{name}` would have been overwritten"
            )));
        }
    }

    // Write everything only after every prompt resolved — keeps the dir
    // consistent if the user aborts mid-flow.
    for (name, contents) in planned_writes {
        let path = dir.join(name);
        write_file(&path, contents)?;
    }

    eprintln!("Scaffolded OpenGEO project at {}.", dir.display());
    eprintln!("Next: edit opengeo.yaml, then run `ogeo login openai` (or anthropic).");
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> Result<(), OpenGeoError> {
    std::fs::write(path, contents)
        .map_err(|e| OpenGeoError::Config(format!("failed to write `{}`: {e}", path.display())))?;
    Ok(())
}

/// Prompt the user. Returns `Ok(true)` on confirm, `Ok(false)` on decline.
/// In a non-TTY environment we never prompt — overwriting an existing file
/// in a script context requires `--force` to be explicit.
fn confirm_overwrite(name: &str) -> Result<bool, OpenGeoError> {
    if !std::io::stdin().is_terminal() {
        return Err(OpenGeoError::Config(format!(
            "`{name}` already exists; refusing to overwrite without --force in non-interactive mode"
        )));
    }
    let prompt = format!("`{name}` already exists. Overwrite?");
    dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|e| OpenGeoError::Config(format!("interactive prompt failed: {e}")))
}
