//! `opengeo.yaml` schema, parser, and stable-ID derivation (FR-1, FR-9, FR-23, FR-24).
//!
//! The YAML file is the canonical declaration of what the system should observe; the
//! database stores what the system observed. A run of OpenGEO loads `opengeo.yaml`,
//! converts it into a [`Config`], and works from the [`Config`] thereafter.
//!
//! # Stable IDs
//!
//! [`Config::project_id`] and [`Config::prompt_id`] derive ULID newtypes via a SHA-256
//! hash of the namespaced canonical input. The same `opengeo.yaml` content produces
//! the same IDs run after run — important for FR-1 ("Project/Prompt IDs are stable
//! across runs when config unchanged") and for FR-23 ("removing a Prompt from YAML
//! and re-running does not delete its historical Prompt Runs", since the historical
//! rows can still be located by the same ID if the Prompt is re-added).
//!
//! # Schema versioning
//!
//! The top-level `schema_version` field is the version of the YAML schema (FR-24).
//! Phase 1 freezes `0.1`. Unknown values produce a [`ConfigError::UnsupportedSchemaVersion`]
//! mapped to [`crate::ExitCode::ConfigError`] (64).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::ids::{ProjectId, PromptId};

/// Phase 1 schema version (FR-24).
pub const SCHEMA_VERSION_V0_1: &str = "0.1";

/// Phase 2 schema version. v0.2 is a non-breaking superset of v0.1.
pub const SCHEMA_VERSION_V0_2: &str = "0.2";

const SUPPORTED_SCHEMA_VERSIONS: &str = "0.1 or 0.2";

/// Default per-provider timeout in seconds (PRD FR-9 / Story 2.4 AC). Used when a
/// provider entry omits `timeout_seconds`.
pub const DEFAULT_PROVIDER_TIMEOUT_SECONDS: u64 = 60;

/// Default concurrency for `ogeo prompt run`. Conservative; users tune up.
pub const DEFAULT_CONCURRENCY: u32 = 4;

/// Default OpenAI model when a provider entry omits `model` (PRD FR-7).
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-2024-08-06";

/// Default Anthropic model when a provider entry omits `model` (PRD FR-8).
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-3-5-sonnet-20241022";

pub const DEFAULT_GEMINI_MODEL: &str = "gemini-1.5-pro-002";
pub const DEFAULT_PERPLEXITY_MODEL: &str = "sonar-large-online-128k";
pub const DEFAULT_GROK_MODEL: &str = "grok-2-1212";
pub const DEFAULT_MISTRAL_MODEL: &str = "mistral-large-2411";
pub const DEFAULT_OPENROUTER_MODEL: &str = "openai/gpt-4o-2024-08-06";

/// Default debounce window for schedule declarations.
pub const DEFAULT_SCHEDULE_DEBOUNCE_MINUTES: u32 = 5;

/// Top-level `opengeo.yaml` document (v0.1).
///
/// `#[serde(deny_unknown_fields)]` enforces FR-24's "unknown fields produce
/// exit-code-compatible config errors" — surfaced via [`ConfigError::Parse`]
/// with file + line + column when the input came from a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// YAML schema version. Phase 1: must be `"0.1"`.
    pub schema_version: String,
    pub brand: BrandConfig,
    #[serde(default)]
    pub competitors: Vec<CompetitorConfig>,
    pub prompts: Vec<PromptConfig>,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    /// Phase 2 schedule declarations. Non-empty only with schema_version 0.2.
    #[serde(default)]
    pub schedules: Vec<ScheduleConfig>,
    /// Concurrency for `ogeo prompt run`. Optional; defaults to
    /// [`DEFAULT_CONCURRENCY`].
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    /// Phase 2 anomaly-detector tuning. Optional; defaults to
    /// [`AnomalySensitivity::default`].
    #[serde(default)]
    pub anomaly_sensitivity: AnomalySensitivity,
}

/// Phase 2 FR-26a — tuning for the z-score visibility detector and the
/// citation-novelty detector. Defaults are calibrated so that a 1-year
/// stable stream emits ≤ 12 visibility anomalies (P1-103 budget).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnomalySensitivity {
    /// |z| threshold for emitting a `visibility_anomaly`. Higher = stricter.
    #[serde(default = "default_zscore_threshold")]
    pub zscore_threshold: f64,
    /// Trailing window size (in samples) used to compute the running mean
    /// and stddev for the z-score detector.
    #[serde(default = "default_window_samples")]
    pub window_samples: u32,
    /// Minimum frequency a previously-unseen citation domain must reach in
    /// the current sample before it counts as an anomaly.
    #[serde(default = "default_citation_min_frequency")]
    pub citation_min_frequency: u32,
}

impl Default for AnomalySensitivity {
    fn default() -> Self {
        Self {
            zscore_threshold: default_zscore_threshold(),
            window_samples: default_window_samples(),
            citation_min_frequency: default_citation_min_frequency(),
        }
    }
}

impl Eq for AnomalySensitivity {}

fn default_zscore_threshold() -> f64 {
    2.5
}

fn default_window_samples() -> u32 {
    14
}

fn default_citation_min_frequency() -> u32 {
    2
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrandConfig {
    /// Canonical brand name. Mentions of this string (case-insensitive) count as
    /// Brand Mentions for ranking.
    pub name: String,
    /// Alternate spellings, aliases, and casings (PRD FR-3 "configurable name
    /// variants per entity"). Matched in addition to `name`.
    #[serde(default)]
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompetitorConfig {
    pub name: String,
    #[serde(default)]
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromptConfig {
    /// Slug-safe identifier (lowercase ASCII, digits, hyphens). Used as the
    /// stable `name` column in the `prompts` table and as the CLI selector
    /// for `--prompt NAME` (PRD FR-12, FR-13).
    pub name: String,
    /// Prompt body sent to providers.
    pub text: String,
    /// Optional free-form description shown in `ogeo prompt list`.
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub name: ProviderName,
    /// Model identifier passed verbatim to the provider API. Optional; when
    /// missing the provider's documented default is used (PRD FR-9).
    #[serde(default)]
    pub model: Option<String>,
    /// Per-provider request timeout in seconds. Defaults to
    /// [`DEFAULT_PROVIDER_TIMEOUT_SECONDS`].
    #[serde(default = "default_provider_timeout")]
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScheduleConfig {
    /// Slug-safe schedule name.
    pub name: String,
    /// Cron-style cadence or supported shorthand such as `hourly` or `daily`.
    pub cron: String,
    /// Prompt names declared in `prompts`.
    pub prompts: Vec<String>,
    /// Providers declared in `providers`.
    pub providers: Vec<ProviderName>,
    /// Debounce window for recent manual runs. Defaults to 5 minutes.
    #[serde(default = "default_schedule_debounce_minutes")]
    pub debounce_minutes: u32,
    /// RFC3339 timestamp recorded when a user acknowledges a high projected cost.
    #[serde(default)]
    pub projection_acknowledged_at: Option<String>,
}

/// Closed set of providers known to the YAML schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProviderName {
    Openai,
    Anthropic,
    Gemini,
    Perplexity,
    Grok,
    Mistral,
    Openrouter,
}

impl ProviderName {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "openai" => Some(Self::Openai),
            "anthropic" => Some(Self::Anthropic),
            "gemini" => Some(Self::Gemini),
            "perplexity" => Some(Self::Perplexity),
            "grok" => Some(Self::Grok),
            "mistral" => Some(Self::Mistral),
            "openrouter" => Some(Self::Openrouter),
            _ => None,
        }
    }

    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::Perplexity => "perplexity",
            Self::Grok => "grok",
            Self::Mistral => "mistral",
            Self::Openrouter => "openrouter",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::Openai => DEFAULT_OPENAI_MODEL,
            Self::Anthropic => DEFAULT_ANTHROPIC_MODEL,
            Self::Gemini => DEFAULT_GEMINI_MODEL,
            Self::Perplexity => DEFAULT_PERPLEXITY_MODEL,
            Self::Grok => DEFAULT_GROK_MODEL,
            Self::Mistral => DEFAULT_MISTRAL_MODEL,
            Self::Openrouter => DEFAULT_OPENROUTER_MODEL,
        }
    }

    pub fn all_wire_names() -> &'static [&'static str] {
        &[
            "openai",
            "anthropic",
            "gemini",
            "perplexity",
            "grok",
            "mistral",
            "openrouter",
        ]
    }
}

impl std::fmt::Display for ProviderName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

fn default_provider_timeout() -> u64 {
    DEFAULT_PROVIDER_TIMEOUT_SECONDS
}

fn default_concurrency() -> u32 {
    DEFAULT_CONCURRENCY
}

fn default_schedule_debounce_minutes() -> u32 {
    DEFAULT_SCHEDULE_DEBOUNCE_MINUTES
}

/// Structured config error. Maps to [`crate::ExitCode::ConfigError`] (64) by
/// way of `impl From<ConfigError> for OpenGeoError`.
///
/// `Display` is intentionally one-line and includes `file:line:col` where
/// available so editor diagnostics can jump straight to the offending line.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// I/O error reading the file.
    #[error("failed to read config from {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// YAML syntax or shape error. `location` is `None` for in-memory parses
    /// when serde_yaml could not localize the failure.
    #[error("{path_display}: {message}")]
    Parse {
        /// `<file>:<line>:<col>` when known, otherwise `<input>` or
        /// `<unknown location>`.
        path_display: String,
        message: String,
    },

    /// `schema_version` is present but not supported.
    #[error("unsupported schema_version `{found}` (supported: {supported})")]
    UnsupportedSchemaVersion {
        found: String,
        supported: &'static str,
    },

    /// One or more semantic-validation failures (duplicate prompt names,
    /// empty prompt text, etc). All collected so the user sees them at once
    /// instead of fixing one-and-rerun.
    #[error("invalid config: {0}")]
    Validation(String),
}

impl From<ConfigError> for crate::OpenGeoError {
    fn from(err: ConfigError) -> Self {
        crate::OpenGeoError::Config(err.to_string())
    }
}

impl Config {
    /// Parse a `Config` from a YAML string. Used by tests and for
    /// CLI-piped configs. Errors are not annotated with a file path —
    /// prefer [`Config::from_path`] for that.
    pub fn from_yaml_str(yaml: &str) -> Result<Self, ConfigError> {
        Self::parse_and_validate(yaml, None)
    }

    /// Parse a `Config` from a file. The path is threaded through into
    /// [`ConfigError::Parse::path_display`] so error messages start
    /// with `path/to/opengeo.yaml:LINE:COL`.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let yaml = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse_and_validate(&yaml, Some(path))
    }

    fn parse_and_validate(yaml: &str, path: Option<&Path>) -> Result<Self, ConfigError> {
        let path_label = path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<input>".to_string());

        let cfg: Config = serde_yaml::from_str(yaml).map_err(|e| {
            let loc = e.location();
            let path_display = match loc {
                Some(l) => format!("{}:{}:{}", path_label, l.line(), l.column()),
                None => path_label.clone(),
            };
            ConfigError::Parse {
                path_display,
                message: e.to_string(),
            }
        })?;

        cfg.validate()?;
        Ok(cfg)
    }

    /// Semantic checks beyond what serde+deny_unknown_fields catches.
    ///
    /// Collects every failure into one Validation error so a user gets the
    /// full list on first run rather than discovering them one at a time.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != SCHEMA_VERSION_V0_1 && self.schema_version != SCHEMA_VERSION_V0_2
        {
            return Err(ConfigError::UnsupportedSchemaVersion {
                found: self.schema_version.clone(),
                supported: SUPPORTED_SCHEMA_VERSIONS,
            });
        }

        let mut errors: Vec<String> = Vec::new();

        if self.schema_version == SCHEMA_VERSION_V0_1 && !self.schedules.is_empty() {
            errors.push("schedules require schema_version `0.2`".into());
        }

        if self.brand.name.trim().is_empty() {
            errors.push("brand.name must not be empty".into());
        }

        // Prompt-name uniqueness and slug shape (PRD FR-12 "slug-validated").
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (idx, p) in self.prompts.iter().enumerate() {
            if p.name.trim().is_empty() {
                errors.push(format!("prompts[{idx}].name must not be empty"));
            } else if !is_valid_prompt_slug(&p.name) {
                errors.push(format!(
                    "prompts[{idx}].name `{}` is not a valid slug \
                     (lowercase ASCII letters, digits, hyphens; must start with a letter)",
                    p.name
                ));
            } else if !seen.insert(p.name.as_str()) {
                errors.push(format!("duplicate prompt name `{}`", p.name));
            }
            if p.text.trim().is_empty() {
                errors.push(format!("prompts[{idx}].text must not be empty"));
            }
        }

        // Provider-name uniqueness (one entry per provider).
        let mut seen_providers: std::collections::HashSet<ProviderName> =
            std::collections::HashSet::new();
        for (idx, p) in self.providers.iter().enumerate() {
            if !seen_providers.insert(p.name) {
                errors.push(format!(
                    "duplicate provider entry `{}` at providers[{idx}]",
                    p.name
                ));
            }
            if p.timeout_seconds == 0 {
                errors.push(format!(
                    "providers[{idx}].timeout_seconds must be > 0 (got 0)"
                ));
            }
        }

        let declared_prompts: std::collections::HashSet<&str> =
            self.prompts.iter().map(|p| p.name.as_str()).collect();
        let declared_providers: std::collections::HashSet<ProviderName> =
            self.providers.iter().map(|p| p.name).collect();
        let mut seen_schedules: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (idx, s) in self.schedules.iter().enumerate() {
            if s.name.trim().is_empty() {
                errors.push(format!("schedules[{idx}].name must not be empty"));
            } else if !is_valid_prompt_slug(&s.name) {
                errors.push(format!(
                    "schedules[{idx}].name `{}` is not a valid slug \
                     (lowercase ASCII letters, digits, hyphens; must start with a letter)",
                    s.name
                ));
            } else if !seen_schedules.insert(s.name.as_str()) {
                errors.push(format!("duplicate schedule name `{}`", s.name));
            }

            if s.cron.trim().is_empty() {
                errors.push(format!("schedules[{idx}].cron must not be empty"));
            }
            if s.prompts.is_empty() {
                errors.push(format!("schedules[{idx}].prompts must not be empty"));
            }
            if s.providers.is_empty() {
                errors.push(format!("schedules[{idx}].providers must not be empty"));
            }
            if s.debounce_minutes == 0 {
                errors.push(format!(
                    "schedules[{idx}].debounce_minutes must be > 0 (got 0)"
                ));
            }

            for prompt_name in &s.prompts {
                if !declared_prompts.contains(prompt_name.as_str()) {
                    errors.push(format!(
                        "schedules[{idx}] references unknown prompt `{prompt_name}`"
                    ));
                }
            }
            for provider_name in &s.providers {
                if !declared_providers.contains(provider_name) {
                    errors.push(format!(
                        "schedules[{idx}] references provider `{provider_name}` not declared in providers"
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Validation(errors.join("; ")))
        }
    }

    /// Stable [`ProjectId`] derived from the canonical project namespace
    /// (currently the trimmed lowercase brand name). Two runs over the same
    /// `opengeo.yaml` produce the same `ProjectId`.
    pub fn project_id(&self) -> ProjectId {
        let canonical = canonical_project_input(&self.brand.name);
        let bytes = sha256_first_16(canonical.as_bytes());
        ProjectId::from_ulid(ulid::Ulid::from_bytes(bytes))
    }

    /// Stable [`PromptId`] for a named prompt within this Project. Returns
    /// `None` when `name` is not declared in this config.
    pub fn prompt_id(&self, name: &str) -> Option<PromptId> {
        if !self.prompts.iter().any(|p| p.name == name) {
            return None;
        }
        Some(self.derive_prompt_id(name))
    }

    /// Stable [`PromptId`] derivation without the membership check — used by
    /// iterators that already know the prompt is declared.
    fn derive_prompt_id(&self, name: &str) -> PromptId {
        let canonical = canonical_prompt_input(&self.brand.name, name);
        let bytes = sha256_first_16(canonical.as_bytes());
        PromptId::from_ulid(ulid::Ulid::from_bytes(bytes))
    }

    /// `(name, PromptId)` for every declared prompt, preserving YAML order.
    pub fn prompt_ids(&self) -> Vec<(String, PromptId)> {
        self.prompts
            .iter()
            .map(|p| (p.name.clone(), self.derive_prompt_id(&p.name)))
            .collect()
    }

    /// Look up a provider config by name.
    pub fn provider(&self, name: ProviderName) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.name == name)
    }

    /// Look up a schedule config by name.
    pub fn schedule(&self, name: &str) -> Option<&ScheduleConfig> {
        self.schedules.iter().find(|s| s.name == name)
    }
}

fn canonical_project_input(brand_name: &str) -> String {
    format!("opengeo:v0.1:project:{}", brand_name.trim().to_lowercase())
}

fn canonical_prompt_input(brand_name: &str, prompt_name: &str) -> String {
    format!(
        "opengeo:v0.1:prompt:{}::{}",
        brand_name.trim().to_lowercase(),
        prompt_name.trim().to_lowercase()
    )
}

fn sha256_first_16(input: &[u8]) -> [u8; 16] {
    let digest = Sha256::digest(input);
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

fn is_valid_prompt_slug(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Render the JSON Schema for [`Config`]. Used by the docs build to write
/// `docs/config/opengeo-yaml-schema.json` (FR-24: "Schema docs generated from
/// a single JSON Schema source").
pub fn json_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(Config);
    serde_json::to_value(schema).expect("Config JSON Schema is always serializable")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_yaml() -> &'static str {
        r#"
schema_version: '0.1'
brand:
  name: Acme
  variants: [acme, Acme Inc.]
competitors:
  - name: Beta Corp
    variants: [beta, BetaCorp]
prompts:
  - name: ai-monitoring-tools
    text: "What are the best AI visibility monitoring tools?"
providers:
  - name: openai
    model: gpt-4o-2024-08-06
  - name: anthropic
"#
    }

    #[test]
    fn parses_minimal_config() {
        let cfg = Config::from_yaml_str(minimal_yaml()).unwrap();
        assert_eq!(cfg.schema_version, "0.1");
        assert_eq!(cfg.brand.name, "Acme");
        assert_eq!(cfg.brand.variants.len(), 2);
        assert_eq!(cfg.competitors.len(), 1);
        assert_eq!(cfg.prompts.len(), 1);
        assert_eq!(cfg.providers.len(), 2);
        assert_eq!(cfg.concurrency, DEFAULT_CONCURRENCY);
        assert_eq!(
            cfg.providers[1].timeout_seconds,
            DEFAULT_PROVIDER_TIMEOUT_SECONDS
        );
        assert_eq!(cfg.providers[1].name, ProviderName::Anthropic);
        assert!(cfg.providers[1].model.is_none());
        assert_eq!(
            cfg.providers[1].name.default_model(),
            DEFAULT_ANTHROPIC_MODEL
        );
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let yaml = "schema_version: '99.9'\nbrand:\n  name: A\nprompts: []\n";
        let err = Config::from_yaml_str(yaml).unwrap_err();
        match err {
            ConfigError::UnsupportedSchemaVersion { found, .. } => assert_eq!(found, "99.9"),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let yaml = "schema_version: '0.1'\nbrand:\n  name: A\nprompts: []\nbogus: 1\n";
        let err = Config::from_yaml_str(yaml).unwrap_err();
        assert!(matches!(err, ConfigError::Parse { .. }));
        assert!(err.to_string().contains("bogus"));
    }

    #[test]
    fn rejects_duplicate_prompt_names() {
        let yaml = r#"
schema_version: '0.1'
brand:
  name: Acme
prompts:
  - name: a
    text: "x"
  - name: a
    text: "y"
"#;
        let err = Config::from_yaml_str(yaml).unwrap_err();
        match err {
            ConfigError::Validation(msg) => assert!(msg.contains("duplicate prompt name")),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_prompt_slug() {
        let yaml = r#"
schema_version: '0.1'
brand:
  name: Acme
prompts:
  - name: "Bad Name"
    text: "x"
"#;
        let err = Config::from_yaml_str(yaml).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));
        assert!(err.to_string().contains("not a valid slug"));
    }

    #[test]
    fn rejects_empty_prompt_text() {
        let yaml = r#"
schema_version: '0.1'
brand:
  name: Acme
prompts:
  - name: foo
    text: ""
"#;
        let err = Config::from_yaml_str(yaml).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));
        assert!(err.to_string().contains("text must not be empty"));
    }

    #[test]
    fn rejects_empty_brand_name() {
        let yaml = r#"
schema_version: '0.1'
brand:
  name: ""
prompts: []
"#;
        let err = Config::from_yaml_str(yaml).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));
        assert!(err.to_string().contains("brand.name"));
    }

    #[test]
    fn rejects_duplicate_provider() {
        let yaml = r#"
schema_version: '0.1'
brand:
  name: Acme
prompts:
  - name: foo
    text: x
providers:
  - name: openai
  - name: openai
"#;
        let err = Config::from_yaml_str(yaml).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));
        assert!(err.to_string().contains("duplicate provider"));
    }

    #[test]
    fn parse_error_includes_line_and_column() {
        // Invalid YAML: tab where a space is expected.
        let yaml = "schema_version: '0.1'\nbrand:\n\tname: A\nprompts: []\n";
        let err = Config::from_yaml_str(yaml).unwrap_err();
        match err {
            ConfigError::Parse { path_display, .. } => {
                // serde_yaml localizes; assertion is permissive about exact line.
                assert!(
                    path_display.contains(':'),
                    "expected location, got `{path_display}`"
                );
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn project_id_is_stable_across_runs() {
        let cfg = Config::from_yaml_str(minimal_yaml()).unwrap();
        let id1 = cfg.project_id();
        let id2 = cfg.project_id();
        assert_eq!(id1, id2);

        // A second parse of the same input produces the same ID.
        let cfg2 = Config::from_yaml_str(minimal_yaml()).unwrap();
        assert_eq!(id1, cfg2.project_id());
    }

    #[test]
    fn project_id_changes_when_brand_changes() {
        let cfg_a = Config::from_yaml_str(minimal_yaml()).unwrap();
        let yaml_b = minimal_yaml().replace("name: Acme", "name: Different");
        let cfg_b = Config::from_yaml_str(&yaml_b).unwrap();
        assert_ne!(cfg_a.project_id(), cfg_b.project_id());
    }

    #[test]
    fn prompt_id_is_stable_and_namespaced() {
        let cfg = Config::from_yaml_str(minimal_yaml()).unwrap();
        let id1 = cfg.prompt_id("ai-monitoring-tools").unwrap();
        let id2 = cfg.prompt_id("ai-monitoring-tools").unwrap();
        assert_eq!(id1, id2);
        assert!(cfg.prompt_id("does-not-exist").is_none());
    }

    #[test]
    fn prompt_id_differs_across_projects_with_same_prompt_name() {
        let cfg_a = Config::from_yaml_str(minimal_yaml()).unwrap();
        let yaml_b = minimal_yaml().replace("name: Acme", "name: OtherCo");
        let cfg_b = Config::from_yaml_str(&yaml_b).unwrap();
        assert_ne!(
            cfg_a.prompt_id("ai-monitoring-tools").unwrap(),
            cfg_b.prompt_id("ai-monitoring-tools").unwrap()
        );
    }

    #[test]
    fn project_id_case_insensitive_in_brand_name() {
        let lower = Config::from_yaml_str(minimal_yaml()).unwrap();
        let upper_yaml = minimal_yaml().replace("name: Acme", "name: ACME");
        let upper = Config::from_yaml_str(&upper_yaml).unwrap();
        // FR-3 says brand matching is case-insensitive; we extend that to the
        // identity derivation so trivial casing edits do not destroy history.
        assert_eq!(lower.project_id(), upper.project_id());
    }

    #[test]
    fn json_schema_is_generated_and_includes_top_level_required() {
        let v = json_schema();
        let s = v.to_string();
        assert!(s.contains("schema_version"));
        assert!(s.contains("brand"));
        assert!(s.contains("prompts"));
        // The schema is the single source for generated docs (FR-24).
        let obj = v.as_object().expect("schema is a JSON object");
        assert!(obj.contains_key("$schema") || obj.contains_key("title"));
    }

    #[test]
    fn provider_default_models_match_constants() {
        assert_eq!(ProviderName::Openai.default_model(), DEFAULT_OPENAI_MODEL);
        assert_eq!(
            ProviderName::Anthropic.default_model(),
            DEFAULT_ANTHROPIC_MODEL
        );
        assert_eq!(ProviderName::Gemini.default_model(), DEFAULT_GEMINI_MODEL);
        assert_eq!(
            ProviderName::Perplexity.default_model(),
            DEFAULT_PERPLEXITY_MODEL
        );
        assert_eq!(ProviderName::Grok.default_model(), DEFAULT_GROK_MODEL);
        assert_eq!(ProviderName::Mistral.default_model(), DEFAULT_MISTRAL_MODEL);
        assert_eq!(
            ProviderName::Openrouter.default_model(),
            DEFAULT_OPENROUTER_MODEL
        );
    }

    #[test]
    fn provider_lookup_finds_or_returns_none() {
        let cfg = Config::from_yaml_str(minimal_yaml()).unwrap();
        assert!(cfg.provider(ProviderName::Openai).is_some());
        assert!(cfg.provider(ProviderName::Anthropic).is_some());

        let yaml_minus_anthropic = r#"
schema_version: '0.1'
brand:
  name: Acme
prompts:
  - name: foo
    text: x
providers:
  - name: openai
"#;
        let cfg = Config::from_yaml_str(yaml_minus_anthropic).unwrap();
        assert!(cfg.provider(ProviderName::Openai).is_some());
        assert!(cfg.provider(ProviderName::Anthropic).is_none());
    }

    #[test]
    fn parses_v0_2_schedule_config() {
        let yaml = r#"
schema_version: '0.2'
brand:
  name: Acme
prompts:
  - name: ai-monitoring-tools
    text: "What are the best AI visibility monitoring tools?"
providers:
  - name: openai
  - name: gemini
schedules:
  - name: daily-watch
    cron: daily
    prompts: [ai-monitoring-tools]
    providers: [openai, gemini]
"#;
        let cfg = Config::from_yaml_str(yaml).unwrap();
        assert_eq!(cfg.schema_version, SCHEMA_VERSION_V0_2);
        assert_eq!(cfg.schedules.len(), 1);
        assert_eq!(
            cfg.schedules[0].debounce_minutes,
            DEFAULT_SCHEDULE_DEBOUNCE_MINUTES
        );
        assert_eq!(
            cfg.schedules[0].providers,
            vec![ProviderName::Openai, ProviderName::Gemini]
        );
    }

    #[test]
    fn rejects_schedules_on_v0_1() {
        let yaml = r#"
schema_version: '0.1'
brand:
  name: Acme
prompts:
  - name: ai-monitoring-tools
    text: "What are the best AI visibility monitoring tools?"
providers:
  - name: openai
schedules:
  - name: daily-watch
    cron: daily
    prompts: [ai-monitoring-tools]
    providers: [openai]
"#;
        let err = Config::from_yaml_str(yaml).unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));
        assert!(err.to_string().contains("schema_version `0.2`"));
    }

    #[test]
    fn rejects_schedule_references_to_unknown_prompt_or_provider() {
        let yaml = r#"
schema_version: '0.2'
brand:
  name: Acme
prompts:
  - name: ai-monitoring-tools
    text: "What are the best AI visibility monitoring tools?"
providers:
  - name: openai
schedules:
  - name: daily-watch
    cron: daily
    prompts: [missing-prompt]
    providers: [gemini]
"#;
        let err = Config::from_yaml_str(yaml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown prompt"));
        assert!(msg.contains("not declared in providers"));
    }
}
