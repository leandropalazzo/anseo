//! `ogeo prompt add` and `ogeo prompt list` (FR-12).
//!
//! Both commands operate on `anseo.yaml` at the working directory (or
//! `--config PATH`).
//!
//! - `add` interactive mode walks the user through name/text/description.
//! - `add` non-interactive mode (`--name`/`--text`/`--description`) accepts
//!   flags directly; missing required flags exit non-zero.
//! - `list` prints a two-column table by default and a stable JSON array
//!   when `--format json` is passed.
//!
//! YAML write strategy: re-serialize the parsed `Config`. This loses
//! arbitrary inline comments — a known v0.1 limitation we document in the
//! help text. Preserving comments would require a YAML AST library; deferring
//! that to a Phase 2 polish story.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anseo_core::{Config, OpenGeoError, PromptConfig};
use clap::{Args, ValueEnum};

const DEFAULT_CONFIG_PATH: &str = "anseo.yaml";

#[derive(Debug, Args)]
pub struct AddArgs {
    /// Slug-validated name (lowercase ASCII, digits, hyphens; starts with a letter).
    #[arg(long)]
    pub name: Option<String>,

    /// Prompt text. May span multiple lines when quoted.
    #[arg(long)]
    pub text: Option<String>,

    /// Optional free-form description.
    #[arg(long)]
    pub description: Option<String>,

    /// Path to `anseo.yaml`. Defaults to the file at the working directory.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Output format. `table` (default) prints two columns; `json` prints a
    /// stable JSON array suitable for piping into jq.
    #[arg(long, value_enum, default_value_t = ListFormat::Table)]
    pub format: ListFormat,

    /// Path to `anseo.yaml`. Defaults to the file at the working directory.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ListFormat {
    Table,
    Json,
}

fn resolve_config_path(arg: Option<PathBuf>) -> PathBuf {
    let path = arg.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));
    Config::auto_migrate_config_filename(&path, "opengeo.yaml")
}

pub fn run_add(args: AddArgs) -> Result<(), OpenGeoError> {
    let path = resolve_config_path(args.config.clone());
    let mut cfg = Config::from_path(&path)?;

    let (name, text, description) = match (args.name, args.text) {
        // Both flags present: non-interactive.
        (Some(name), Some(text)) => (name, text, args.description),
        // Either flag missing in a non-interactive context → structured error.
        (name, text) if !std::io::stdin().is_terminal() => {
            return Err(OpenGeoError::Config(format!(
                "non-interactive `ogeo prompt add` requires --name and --text \
                 (got --name={}, --text={})",
                pretty_present(name.as_deref()),
                pretty_present(text.as_deref())
            )));
        }
        // Interactive: prompt for missing pieces.
        (name, text) => {
            let name = match name {
                Some(n) => n,
                None => dialoguer::Input::<String>::new()
                    .with_prompt("Prompt name (slug)")
                    .interact_text()
                    .map_err(prompt_err)?,
            };
            let text = match text {
                Some(t) => t,
                None => dialoguer::Input::<String>::new()
                    .with_prompt("Prompt text")
                    .interact_text()
                    .map_err(prompt_err)?,
            };
            let description = match args.description {
                Some(d) => Some(d),
                None => {
                    let d = dialoguer::Input::<String>::new()
                        .with_prompt("Description (optional, blank to skip)")
                        .allow_empty(true)
                        .interact_text()
                        .map_err(prompt_err)?;
                    if d.trim().is_empty() {
                        None
                    } else {
                        Some(d)
                    }
                }
            };
            (name, text, description)
        }
    };

    if cfg.prompts.iter().any(|p| p.name == name) {
        return Err(OpenGeoError::Config(format!(
            "duplicate prompt name `{name}`; pick a different slug"
        )));
    }

    cfg.prompts.push(PromptConfig {
        name,
        text,
        description,
    });

    // Re-validate before writing — surfaces slug-shape errors etc.
    let yaml = serde_yaml::to_string(&cfg)
        .map_err(|e| OpenGeoError::Config(format!("failed to serialize anseo.yaml: {e}")))?;
    let _round_trip = Config::from_yaml_str(&yaml)?; // catches invalid slugs etc.

    write_atomic(&path, &yaml)?;
    eprintln!(
        "Added prompt to {} (total prompts now {})",
        path.display(),
        cfg.prompts.len()
    );
    Ok(())
}

pub fn run_list(args: ListArgs) -> Result<(), OpenGeoError> {
    let path = resolve_config_path(args.config.clone());
    let cfg = Config::from_path(&path)?;
    match args.format {
        ListFormat::Json => {
            let rows: Vec<_> = cfg
                .prompts
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "name": p.name,
                        "text": p.text,
                        "description": p.description,
                    })
                })
                .collect();
            let json = serde_json::to_string_pretty(&rows).map_err(|e| {
                OpenGeoError::Internal(anyhow::anyhow!("failed to serialize prompts as JSON: {e}"))
            })?;
            println!("{json}");
        }
        ListFormat::Table => {
            print_table(&cfg);
        }
    }
    Ok(())
}

fn print_table(cfg: &Config) {
    if cfg.prompts.is_empty() {
        println!("(no prompts declared)");
        return;
    }
    // Headers + two-column data per FR-12 ("two-column table (name, text
    // truncated to 60 chars)").
    println!("{:<32} TEXT", "NAME");
    println!("{:<32} ----", "----");
    for p in &cfg.prompts {
        let truncated = truncate(&p.text, 60);
        println!("{:<32} {}", p.name, truncated);
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max_chars {
        s
    } else {
        let cut: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

fn write_atomic(path: &Path, contents: &str) -> Result<(), OpenGeoError> {
    // Plain write is fine for Phase 1 — `anseo.yaml` is small and humans
    // edit it. We just guard against missing parent dirs.
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                OpenGeoError::Config(format!(
                    "failed to create parent dir `{}`: {e}",
                    parent.display()
                ))
            })?;
        }
    }
    std::fs::write(path, contents)
        .map_err(|e| OpenGeoError::Config(format!("failed to write `{}`: {e}", path.display())))?;
    Ok(())
}

fn prompt_err(e: dialoguer::Error) -> OpenGeoError {
    OpenGeoError::Config(format!("interactive prompt failed: {e}"))
}

fn pretty_present(o: Option<&str>) -> &str {
    if o.is_some() {
        "set"
    } else {
        "unset"
    }
}
