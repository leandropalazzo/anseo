//! `anseo init` — scaffold a new Anseo project (FR-10) with tier selection (37.6).
//!
//! Behavior:
//! - Detects recommended deployment tier (T2 if Docker Compose available, T1 otherwise).
//! - In interactive mode (TTY, no --yes): asks the user to confirm or change the tier.
//! - In non-interactive mode (--yes or non-TTY stdin): defaults to Tier 0 (solo CLI).
//! - Creates `anseo.yaml`, `.gitignore`, `README.md` in the target directory.
//! - Pre-existing files at any of those paths prompt per-file before overwrite
//!   (interactive). `--force` overwrites without prompting; `--no-overwrite`
//!   refuses to touch any existing file and exits non-zero if at least one
//!   would have been overwritten.
//! - Declining the interactive prompt exits non-zero **without** writing any
//!   partial scaffold (no half-applied state).
//!
//! The scaffolded `anseo.yaml` is verified valid against the v0.1 schema by the
//! `anseo_init_writes_a_valid_schema_v0_1_config` test in `tests/cli_smoke.rs`.

use std::borrow::Cow;
use std::io::{BufRead as _, IsTerminal, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;

use anseo_core::OpenGeoError;
use clap::Args;

use crate::scaffold::{anseo_yaml, GITIGNORE, README};

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

    /// Non-interactive mode: skip the tier prompt and default to Tier 0
    /// (solo CLI). Implies --force for existing files unless --no-overwrite
    /// is also set.
    #[arg(long)]
    pub yes: bool,
}

/// Each scaffolded file as a `(path, contents)` pair.
fn scaffold_files(tier: u8) -> Vec<(&'static str, Cow<'static, str>)> {
    vec![
        ("anseo.yaml", Cow::Owned(anseo_yaml(tier))),
        (".gitignore", Cow::Borrowed(GITIGNORE)),
        ("README.md", Cow::Borrowed(README)),
    ]
}

/// Probe for Docker Compose availability.
///
/// Returns `true` when `docker compose version` exits 0 (daemon reachable +
/// compose plugin present). Abstracted as a function for unit-test injection.
fn docker_compose_available() -> bool {
    Command::new("docker")
        .args(["compose", "version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Detect the recommended deployment tier based on the current environment.
///
/// - Tier 2 (Docker Compose): `docker compose version` exits 0.
/// - Tier 1 (single binary `anseo serve`): Docker not available.
pub fn detect_recommended_tier() -> u8 {
    if docker_compose_available() { 2 } else { 1 }
}

fn tier_description(tier: u8) -> &'static str {
    match tier {
        0 => "solo CLI (no server)",
        1 => "single binary (anseo serve)",
        2 => "Docker Compose",
        _ => "unknown",
    }
}

/// Interactive tier-confirmation prompt (recommendation-with-Enter, not a menu).
///
/// Prints the detected recommendation and waits for the user to press Enter
/// (accept) or type 0/1/2 (change). Re-prompts on invalid input.
fn prompt_tier(recommended: u8) -> Result<u8, OpenGeoError> {
    let desc = tier_description(recommended);
    loop {
        print!(
            "Detected: Tier {recommended} — {desc}.\nPress Enter to confirm, or type 0 / 1 / 2 to change: "
        );
        std::io::stdout()
            .flush()
            .map_err(|e| OpenGeoError::Config(format!("stdout flush failed: {e}")))?;

        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|e| OpenGeoError::Config(format!("failed to read tier input: {e}")))?;

        match line.trim() {
            "" => return Ok(recommended),
            "0" => return Ok(0),
            "1" => return Ok(1),
            "2" => return Ok(2),
            _ => {
                eprintln!("Please type 0, 1, or 2 (or press Enter to accept Tier {recommended}).");
            }
        }
    }
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

    // Determine the deployment tier.
    // Non-interactive (--yes or non-TTY): always Tier 0 (solo CLI, CI-safe).
    // Interactive TTY: detect + confirm with user.
    let tier = if args.yes || !std::io::stdin().is_terminal() {
        0u8
    } else {
        let recommended = detect_recommended_tier();
        prompt_tier(recommended)?
    };

    let files = scaffold_files(tier);
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
        if !path.exists() || args.force || args.yes {
            planned_writes.push((name, contents.as_ref()));
            continue;
        }
        // Interactive prompt path.
        let confirmed = confirm_overwrite(name)?;
        if confirmed {
            planned_writes.push((name, contents.as_ref()));
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

    eprintln!(
        "Scaffolded Anseo project (Tier {tier} — {}) at {}.",
        tier_description(tier),
        dir.display()
    );
    eprintln!("Next: edit anseo.yaml, then run `anseo login openai` (or anthropic).");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_description_covers_all_valid_values() {
        assert_eq!(tier_description(0), "solo CLI (no server)");
        assert_eq!(tier_description(1), "single binary (anseo serve)");
        assert_eq!(tier_description(2), "Docker Compose");
    }

    #[test]
    fn detect_recommended_tier_returns_1_or_2() {
        // We can't control whether Docker is installed in the test env, but the
        // result must always be either 1 (no Docker) or 2 (Docker present).
        let tier = detect_recommended_tier();
        assert!(tier == 1 || tier == 2, "tier must be 1 or 2, got {tier}");
    }

    #[test]
    fn scaffold_files_tier_0_has_no_tier_line() {
        let files = scaffold_files(0);
        let (_, yaml) = files.iter().find(|(n, _)| *n == "anseo.yaml").unwrap();
        assert!(!yaml.contains("tier:"), "tier=0 must not appear in YAML");
    }

    #[test]
    fn scaffold_files_tier_1_has_tier_line() {
        let files = scaffold_files(1);
        let (_, yaml) = files.iter().find(|(n, _)| *n == "anseo.yaml").unwrap();
        assert!(yaml.contains("tier: 1"), "tier: 1 must appear in YAML");
    }

    #[test]
    fn scaffold_files_tier_2_has_tier_line() {
        let files = scaffold_files(2);
        let (_, yaml) = files.iter().find(|(n, _)| *n == "anseo.yaml").unwrap();
        assert!(yaml.contains("tier: 2"), "tier: 2 must appear in YAML");
    }
}
