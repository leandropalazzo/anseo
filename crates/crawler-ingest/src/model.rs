use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use anseo_core::ProjectId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum CrawlerIngestError {
    #[error("invalid crawler event: {0}")]
    Invalid(String),
    #[error("storage error: {0}")]
    Storage(#[from] sqlx::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[cfg(feature = "clickhouse")]
    #[error("clickhouse transport error: {0}")]
    ClickHouseTransport(#[from] reqwest::Error),
    #[cfg(feature = "clickhouse")]
    #[error("clickhouse returned non-2xx ({status}): {body}")]
    ClickHouseStatus { status: u16, body: String },
}

impl From<CrawlerIngestError> for anseo_core::OpenGeoError {
    fn from(err: CrawlerIngestError) -> Self {
        anseo_core::OpenGeoError::Internal(anyhow::anyhow!(err))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PrivacyMode {
    Raw,
    Truncated,
    #[default]
    Hashed,
}

impl FromStr for PrivacyMode {
    type Err = CrawlerIngestError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "raw" => Ok(Self::Raw),
            "truncated" => Ok(Self::Truncated),
            "hashed" => Ok(Self::Hashed),
            other => Err(CrawlerIngestError::Invalid(format!(
                "unknown privacy mode `{other}`; expected raw, truncated, or hashed"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoredClientIp {
    Raw(String),
    Truncated(String),
    Hashed(String),
    Missing,
}

impl StoredClientIp {
    pub fn raw_column(&self) -> Option<&str> {
        match self {
            Self::Raw(v) => Some(v),
            _ => None,
        }
    }

    pub fn truncated_column(&self) -> Option<&str> {
        match self {
            Self::Truncated(v) => Some(v),
            _ => None,
        }
    }

    pub fn hash_column(&self) -> Option<&str> {
        match self {
            Self::Hashed(v) => Some(v),
            _ => None,
        }
    }

    pub fn apply(ip: Option<&str>, mode: PrivacyMode, salt: &str) -> Self {
        let Some(ip) = ip.map(str::trim).filter(|ip| !ip.is_empty()) else {
            return Self::Missing;
        };
        match mode {
            PrivacyMode::Raw => Self::Raw(ip.to_string()),
            PrivacyMode::Truncated => Self::Truncated(truncate_ip(ip)),
            PrivacyMode::Hashed => {
                let mut h = Sha256::new();
                h.update(salt.as_bytes());
                h.update(b":");
                h.update(ip.as_bytes());
                Self::Hashed(hex::encode(h.finalize()))
            }
        }
    }
}

fn truncate_ip(ip: &str) -> String {
    match IpAddr::from_str(ip) {
        Ok(IpAddr::V4(v4)) => {
            let [a, b, c, _] = v4.octets();
            Ipv4Addr::new(a, b, c, 0).to_string()
        }
        Ok(IpAddr::V6(v6)) => {
            let mut segments = v6.segments();
            segments[4] = 0;
            segments[5] = 0;
            segments[6] = 0;
            segments[7] = 0;
            Ipv6Addr::from(segments).to_string()
        }
        Err(_) => "invalid".to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawCrawlerHit {
    pub raw_event_id: String,
    pub ts: DateTime<Utc>,
    pub user_agent: String,
    pub path: String,
    pub status: u16,
    pub source_adapter: String,
    pub client_ip: Option<String>,
    pub region: Option<String>,
}

impl RawCrawlerHit {
    pub fn normalize(
        self,
        project_id: ProjectId,
        privacy_mode: PrivacyMode,
        privacy_salt: &str,
        ip_verified: bool,
    ) -> Option<NormalizedCrawlerEvent> {
        let bot_id = identify_bot(&self.user_agent)?;
        Some(NormalizedCrawlerEvent {
            project_id,
            ts: self.ts,
            bot_id,
            path: normalize_path(&self.path),
            status: self.status,
            source_adapter: self.source_adapter,
            raw_event_id: self.raw_event_id,
            ip_verified,
            region: self.region.filter(|r| !r.trim().is_empty()),
            client_ip: StoredClientIp::apply(self.client_ip.as_deref(), privacy_mode, privacy_salt),
            privacy_mode,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedCrawlerEvent {
    pub project_id: ProjectId,
    pub ts: DateTime<Utc>,
    pub bot_id: String,
    pub path: String,
    pub status: u16,
    pub source_adapter: String,
    pub raw_event_id: String,
    pub ip_verified: bool,
    pub region: Option<String>,
    pub client_ip: StoredClientIp,
    pub privacy_mode: PrivacyMode,
}

pub fn identify_bot(user_agent: &str) -> Option<String> {
    let ua = user_agent.to_ascii_lowercase();
    let known = [
        ("oai-searchbot", "openai-oai-searchbot"),
        ("chatgpt-user", "openai-chatgpt-user"),
        ("gptbot", "openai-gptbot"),
        ("claudebot", "anthropic-claudebot"),
        ("perplexitybot", "perplexitybot"),
        ("google-extended", "google-extended"),
        ("googlebot", "googlebot"),
        ("ccbot", "common-crawl-ccbot"),
        ("bytespider", "bytedance-bytespider"),
        ("amazonbot", "amazonbot"),
    ];
    known
        .iter()
        .find(|(needle, _)| ua.contains(*needle))
        .map(|(_, bot)| (*bot).to_string())
}

fn normalize_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return "/".to_string();
    }
    if let Some(without_scheme) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
    {
        if let Some(idx) = without_scheme.find('/') {
            return without_scheme[idx..].to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashed_and_truncated_privacy_do_not_keep_raw_ip() {
        let hashed = StoredClientIp::apply(Some("203.0.113.99"), PrivacyMode::Hashed, "salt");
        assert!(hashed.raw_column().is_none());
        assert!(matches!(hashed, StoredClientIp::Hashed(_)));

        let truncated = StoredClientIp::apply(Some("203.0.113.99"), PrivacyMode::Truncated, "");
        assert_eq!(truncated, StoredClientIp::Truncated("203.0.113.0".into()));
        assert!(truncated.raw_column().is_none());
    }

    #[test]
    fn normalizer_keeps_story_31_shape() {
        let raw = RawCrawlerHit {
            raw_event_id: "nginx:1".into(),
            ts: Utc::now(),
            user_agent: "Mozilla/5.0 GPTBot".into(),
            path: "https://example.com/docs?q=1".into(),
            status: 200,
            source_adapter: "nginx".into(),
            client_ip: Some("203.0.113.9".into()),
            region: Some("NL".into()),
        };

        let event = raw
            .normalize(ProjectId::new(), PrivacyMode::Hashed, "salt", true)
            .expect("ai bot");
        assert_eq!(event.bot_id, "openai-gptbot");
        assert_eq!(event.path, "/docs?q=1");
        assert!(event.ip_verified);
        assert!(event.client_ip.raw_column().is_none());
    }
}
