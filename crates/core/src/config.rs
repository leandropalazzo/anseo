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
pub const DEFAULT_OPENROUTER_MODEL: &str = "openrouter/auto";

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
    /// Phase 3 analytics store wiring. Absent until the operator connects a
    /// ClickHouse instance via `/setup` (Story 15.4 `POST
    /// /v1/setup/clickhouse/connect` persists it here). Optional + additive,
    /// so pre-Phase-3 configs parse unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analytics: Option<AnalyticsConfig>,
}

/// Phase 3 — analytics store configuration. Today the only member is the
/// optional ClickHouse endpoint; future analytics backends slot in here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clickhouse: Option<ClickHouseEndpointConfig>,
}

/// Connection coordinates for a remote/managed ClickHouse, persisted by the
/// `/setup` remote-connect flow (Story 15.4). The password is intentionally
/// NOT stored here — it is supplied at runtime via the `CLICKHOUSE_PASSWORD`
/// env var (or the managed provider's secret store), matching the privacy
/// rule that secrets never land in the YAML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClickHouseEndpointConfig {
    /// Canonical origin URL, e.g. `https://abc.clickhouse.cloud:8443`.
    pub endpoint: String,
    /// Database name (defaults to `default` when omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// Username for HTTP basic auth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Which managed provider preset this endpoint came from (informational).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
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
    /// Optional URL of the brand's owned website. Used to scope `ogeo audit`
    /// and crawler observability to the brand's own pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_url: Option<String>,
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
    /// OpenRouter-only: fan a single OpenRouter key out to multiple upstream
    /// `<vendor>/<model>` models — each model becomes its own Prompt Run
    /// (the upstream is threaded into `raw_response.metadata.upstream_model`).
    /// Mutually exclusive with `model`. Unset/absent keeps single-model
    /// behaviour.
    #[serde(default)]
    pub models: Option<Vec<String>>,
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

/// Set of providers OpenGEO can dispatch to.
///
/// The seven first-party variants are a closed set known to the YAML schema.
/// [`ProviderName::Plugin`] (Phase 3, FR-52) carries the namespaced id of a
/// plugin-provided Provider — wire form `plugin:<id>` (e.g.
/// `plugin:test.mock-provider`). The `Plugin` variant is what makes this enum
/// non-`Copy`; treat it as `Clone`.
///
/// Serialization is a plain wire string in every format (YAML, JSON, OpenAPI)
/// via the manual `Serialize`/`Deserialize`/`JsonSchema` impls below, so a
/// plugin provider round-trips as `"plugin:test.mock-provider"` rather than a
/// tagged enum object.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProviderName {
    Openai,
    Anthropic,
    Gemini,
    Perplexity,
    Grok,
    Mistral,
    Openrouter,
    /// Plugin-provided Provider. Inner string is the plugin provider id
    /// (`test.mock-provider`); wire form is `plugin:<id>`.
    Plugin(String),
}

impl ProviderName {
    pub fn parse(s: &str) -> Option<Self> {
        if let Some(id) = s.strip_prefix("plugin:") {
            if id.is_empty() {
                return None;
            }
            return Some(Self::Plugin(id.to_string()));
        }
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

    /// Canonical wire string. First-party variants borrow a `'static` literal;
    /// `Plugin` owns the `plugin:<id>` rendering.
    pub fn as_wire_str(&self) -> std::borrow::Cow<'static, str> {
        use std::borrow::Cow;
        match self {
            Self::Openai => Cow::Borrowed("openai"),
            Self::Anthropic => Cow::Borrowed("anthropic"),
            Self::Gemini => Cow::Borrowed("gemini"),
            Self::Perplexity => Cow::Borrowed("perplexity"),
            Self::Grok => Cow::Borrowed("grok"),
            Self::Mistral => Cow::Borrowed("mistral"),
            Self::Openrouter => Cow::Borrowed("openrouter"),
            Self::Plugin(id) => Cow::Owned(format!("plugin:{id}")),
        }
    }

    /// True for the [`ProviderName::Plugin`] variant.
    pub fn is_plugin(&self) -> bool {
        matches!(self, Self::Plugin(_))
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            Self::Openai => DEFAULT_OPENAI_MODEL,
            Self::Anthropic => DEFAULT_ANTHROPIC_MODEL,
            Self::Gemini => DEFAULT_GEMINI_MODEL,
            Self::Perplexity => DEFAULT_PERPLEXITY_MODEL,
            Self::Grok => DEFAULT_GROK_MODEL,
            Self::Mistral => DEFAULT_MISTRAL_MODEL,
            Self::Openrouter => DEFAULT_OPENROUTER_MODEL,
            // Plugin providers declare their own model; this is only a
            // fallback label used when none is supplied.
            Self::Plugin(_) => "plugin",
        }
    }

    /// First-party provider wire names. Plugin providers are not enumerable
    /// here — they are discovered from installed plugins, not this static set.
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
        f.write_str(&self.as_wire_str())
    }
}

impl Serialize for ProviderName {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_wire_str())
    }
}

impl<'de> Deserialize<'de> for ProviderName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        ProviderName::parse(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown provider `{s}`")))
    }
}

impl JsonSchema for ProviderName {
    fn schema_name() -> String {
        "ProviderName".to_string()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        // Wire form is a string: one of the first-party names or `plugin:<id>`.
        String::json_schema(gen)
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

    /// Serialize this `Config` back to a YAML document. Used by the `/setup`
    /// remote-connect flow (Story 15.4) to persist the analytics endpoint.
    /// Comments in the operator's original file are not preserved.
    pub fn to_yaml_string(&self) -> Result<String, ConfigError> {
        serde_yaml::to_string(self).map_err(|e| ConfigError::Parse {
            path_display: "<output>".to_string(),
            message: e.to_string(),
        })
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
            if !seen_providers.insert(p.name.clone()) {
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
            // `models` (multi-upstream fan-out) is an OpenRouter-only mechanism.
            if let Some(models) = &p.models {
                if p.name != ProviderName::Openrouter {
                    errors.push(format!(
                        "providers[{idx}].models is only supported for the `openrouter` \
                         provider (it fans one OpenRouter key out to multiple upstreams); \
                         `{}` accepts a single `model`",
                        p.name
                    ));
                }
                if p.model.is_some() {
                    errors.push(format!(
                        "providers[{idx}]: set either `model` or `models`, not both"
                    ));
                }
                if models.is_empty() {
                    errors.push(format!(
                        "providers[{idx}].models must list at least one `<vendor>/<model>`"
                    ));
                }
                for (mdx, m) in models.iter().enumerate() {
                    if !m.contains('/') {
                        errors.push(format!(
                            "providers[{idx}].models[{mdx}] `{m}` must be in `<vendor>/<model>` \
                             form (e.g. `{DEFAULT_OPENROUTER_MODEL}`)"
                        ));
                    }
                }
            }
        }

        let declared_prompts: std::collections::HashSet<&str> =
            self.prompts.iter().map(|p| p.name.as_str()).collect();
        let declared_providers: std::collections::HashSet<ProviderName> =
            self.providers.iter().map(|p| p.name.clone()).collect();
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
        project_id_for_name(&self.brand.name)
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
        prompt_id_for(&self.brand.name, name)
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

/// Stable [`ProjectId`] for a brand name, independent of a full [`Config`].
/// Used when re-deriving identity after a brand rename (the brand name is
/// authoritative in the DB, not the YAML).
pub fn project_id_for_name(brand_name: &str) -> ProjectId {
    let canonical = canonical_project_input(brand_name);
    let bytes = sha256_first_16(canonical.as_bytes());
    ProjectId::from_ulid(ulid::Ulid::from_bytes(bytes))
}

/// Stable [`PromptId`] for a `(brand_name, prompt_name)` pair, independent of
/// a full [`Config`]. Prompt ids fold in the brand name, so a rename re-derives
/// every prompt id alongside the project id.
pub fn prompt_id_for(brand_name: &str, prompt_name: &str) -> PromptId {
    let canonical = canonical_prompt_input(brand_name, prompt_name);
    let bytes = sha256_first_16(canonical.as_bytes());
    PromptId::from_ulid(ulid::Ulid::from_bytes(bytes))
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
    fn analytics_endpoint_round_trips_through_yaml() {
        // Story 15.4 — pre-Phase-3 configs parse with `analytics: None`, and a
        // persisted endpoint survives a to_yaml_string → from_yaml_str round
        // trip (the password is never part of the persisted shape).
        let mut cfg = Config::from_yaml_str(minimal_yaml()).unwrap();
        assert!(cfg.analytics.is_none());
        cfg.analytics = Some(AnalyticsConfig {
            clickhouse: Some(ClickHouseEndpointConfig {
                endpoint: "https://abc.clickhouse.cloud:8443".to_string(),
                database: Some("default".to_string()),
                username: Some("svc".to_string()),
                preset: Some("clickhouse_cloud".to_string()),
            }),
        });
        let yaml = cfg.to_yaml_string().unwrap();
        assert!(
            !yaml.contains("password"),
            "password must never be persisted"
        );
        let reparsed = Config::from_yaml_str(&yaml).unwrap();
        let ch = reparsed.analytics.unwrap().clickhouse.unwrap();
        assert_eq!(ch.endpoint, "https://abc.clickhouse.cloud:8443");
        assert_eq!(ch.username.as_deref(), Some("svc"));
        assert_eq!(ch.preset.as_deref(), Some("clickhouse_cloud"));
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

    fn openrouter_models_yaml(provider_block: &str) -> String {
        format!(
            r#"
schema_version: '0.2'
brand:
  name: Acme
prompts:
  - name: foo
    text: x
providers:
{provider_block}
"#
        )
    }

    #[test]
    fn parses_openrouter_models_list() {
        let cfg = Config::from_yaml_str(&openrouter_models_yaml(
            "  - name: openrouter\n    models: [openai/gpt-4o, anthropic/claude-3.5-sonnet]",
        ))
        .unwrap();
        let p = cfg.provider(ProviderName::Openrouter).unwrap();
        assert_eq!(
            p.models.as_deref(),
            Some(
                &[
                    "openai/gpt-4o".to_string(),
                    "anthropic/claude-3.5-sonnet".to_string()
                ][..]
            )
        );
        assert!(p.model.is_none());
    }

    #[test]
    fn rejects_models_on_non_openrouter_provider() {
        let err = Config::from_yaml_str(&openrouter_models_yaml(
            "  - name: openai\n    models: [openai/gpt-4o]",
        ))
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("only supported for the `openrouter`"));
    }

    #[test]
    fn rejects_both_model_and_models() {
        let err = Config::from_yaml_str(&openrouter_models_yaml(
            "  - name: openrouter\n    model: openai/gpt-4o\n    models: [anthropic/claude-3.5-sonnet]",
        ))
        .unwrap_err();
        assert!(err.to_string().contains("either `model` or `models`"));
    }

    #[test]
    fn rejects_empty_models_list() {
        let err = Config::from_yaml_str(&openrouter_models_yaml(
            "  - name: openrouter\n    models: []",
        ))
        .unwrap_err();
        assert!(err.to_string().contains("at least one"));
    }

    #[test]
    fn rejects_openrouter_model_without_vendor_slash() {
        let err = Config::from_yaml_str(&openrouter_models_yaml(
            "  - name: openrouter\n    models: [gpt-4o]",
        ))
        .unwrap_err();
        assert!(err.to_string().contains("<vendor>/<model>"));
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
