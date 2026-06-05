use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anseo_core::ProjectId;
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

use crate::bot_identity::BotRangeVerifier;
use crate::model::{CrawlerIngestError, PrivacyMode, RawCrawlerHit};
use crate::sink::IngestSink;

#[async_trait]
pub trait IngestAdapter {
    fn source_adapter(&self) -> &'static str;

    async fn read_hits(&mut self) -> Result<Vec<RawCrawlerHit>, CrawlerIngestError>;

    async fn ingest<S: IngestSink + Sync>(
        &mut self,
        sink: &S,
        project_id: ProjectId,
        verifier: &BotRangeVerifier,
        privacy_mode: PrivacyMode,
        privacy_salt: &str,
    ) -> Result<u64, CrawlerIngestError> {
        let hits = self.read_hits().await?;
        let mut events = Vec::new();
        for hit in hits {
            let ip_verified = hit
                .client_ip
                .as_deref()
                .map(|ip| verifier.verify_user_agent_ip(&hit.user_agent, ip))
                .unwrap_or(false);
            if let Some(event) = hit.normalize(project_id, privacy_mode, privacy_salt, ip_verified)
            {
                events.push(event);
            }
        }
        sink.insert_events(&events).await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessLogFormat {
    Common,
    Combined,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdapterCursor {
    pub next_line: u64,
}

impl AdapterCursor {
    pub async fn load(path: &Path) -> Result<Self, CrawlerIngestError> {
        match tokio::fs::read_to_string(path).await {
            Ok(s) => Ok(Self {
                next_line: s.trim().parse().unwrap_or(0),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn save(&self, path: &Path) -> Result<(), CrawlerIngestError> {
        tokio::fs::write(path, self.next_line.to_string()).await?;
        Ok(())
    }
}

pub struct AccessLogAdapter {
    path: PathBuf,
    format: AccessLogFormat,
    custom_pattern: Option<String>,
    cursor: AdapterCursor,
    cursor_path: Option<PathBuf>,
}

impl AccessLogAdapter {
    pub fn new(path: impl Into<PathBuf>, format: AccessLogFormat) -> Self {
        Self {
            path: path.into(),
            format,
            custom_pattern: None,
            cursor: AdapterCursor::default(),
            cursor_path: None,
        }
    }

    pub fn with_custom_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.custom_pattern = Some(pattern.into());
        self.format = AccessLogFormat::Custom;
        self
    }

    pub async fn with_cursor_file(
        mut self,
        cursor_path: impl Into<PathBuf>,
    ) -> Result<Self, CrawlerIngestError> {
        let cursor_path = cursor_path.into();
        self.cursor = AdapterCursor::load(&cursor_path).await?;
        self.cursor_path = Some(cursor_path);
        Ok(self)
    }

    pub fn parse_line(
        line_no: u64,
        line: &str,
        format: AccessLogFormat,
        custom_pattern: Option<&str>,
        source_adapter: &str,
    ) -> Option<RawCrawlerHit> {
        match format {
            AccessLogFormat::Common | AccessLogFormat::Combined => {
                parse_common_or_combined(line_no, line, source_adapter)
            }
            AccessLogFormat::Custom => parse_custom(line_no, line, custom_pattern?, source_adapter),
        }
    }
}

#[async_trait]
impl IngestAdapter for AccessLogAdapter {
    fn source_adapter(&self) -> &'static str {
        "web_log"
    }

    async fn read_hits(&mut self) -> Result<Vec<RawCrawlerHit>, CrawlerIngestError> {
        let content = tokio::fs::read_to_string(&self.path).await?;
        let mut hits = Vec::new();
        let mut next = self.cursor.next_line;
        for (idx, line) in content.lines().enumerate() {
            let line_no = idx as u64;
            if line_no < self.cursor.next_line {
                continue;
            }
            next = line_no + 1;
            if let Some(hit) = Self::parse_line(
                line_no,
                line,
                self.format,
                self.custom_pattern.as_deref(),
                self.source_adapter(),
            ) {
                hits.push(hit);
            }
        }
        self.cursor.next_line = next;
        if let Some(path) = &self.cursor_path {
            self.cursor.save(path).await?;
        }
        Ok(hits)
    }
}

fn parse_common_or_combined(
    line_no: u64,
    line: &str,
    source_adapter: &str,
) -> Option<RawCrawlerHit> {
    let ip = line.split_whitespace().next()?.to_string();
    let ts = parse_between(line, '[', ']')
        .and_then(parse_access_log_ts)
        .unwrap_or_else(Utc::now);
    let request = parse_nth_quoted(line, 0)?;
    let user_agent = parse_nth_quoted(line, 2).unwrap_or_default();
    let mut request_parts = request.split_whitespace();
    let _method = request_parts.next();
    let path = request_parts.next().unwrap_or("/").to_string();
    let after_request = line.split_once(&format!("\"{request}\""))?.1;
    let status = after_request
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    Some(RawCrawlerHit {
        raw_event_id: format!("{source_adapter}:{line_no}"),
        ts,
        user_agent,
        path,
        status,
        source_adapter: source_adapter.to_string(),
        client_ip: Some(ip),
        region: None,
    })
}

fn parse_custom(
    line_no: u64,
    line: &str,
    pattern: &str,
    source_adapter: &str,
) -> Option<RawCrawlerHit> {
    let labels: Vec<&str> = pattern
        .split_whitespace()
        .map(|part| part.trim_matches('{').trim_matches('}'))
        .collect();
    let fields: Vec<&str> = line.split_whitespace().collect();
    if labels.len() != fields.len() {
        return None;
    }
    let mut map = HashMap::new();
    for (label, field) in labels.into_iter().zip(fields) {
        map.insert(label, field);
    }
    let ts = map
        .get("ts")
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    Some(RawCrawlerHit {
        raw_event_id: map
            .get("id")
            .map(|s| (*s).to_string())
            .unwrap_or_else(|| format!("{source_adapter}:{line_no}")),
        ts,
        user_agent: map.get("ua").or_else(|| map.get("user_agent"))?.to_string(),
        path: map.get("path")?.to_string(),
        status: map.get("status").and_then(|s| s.parse().ok()).unwrap_or(0),
        source_adapter: source_adapter.to_string(),
        client_ip: map.get("ip").map(|s| (*s).to_string()),
        region: map.get("region").map(|s| (*s).to_string()),
    })
}

fn parse_between(s: &str, left: char, right: char) -> Option<&str> {
    let start = s.find(left)? + left.len_utf8();
    let end = s[start..].find(right)? + start;
    Some(&s[start..end])
}

fn parse_nth_quoted(s: &str, n: usize) -> Option<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else {
            break;
        };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out.get(n).cloned()
}

fn parse_access_log_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(s, "%d/%b/%Y:%H:%M:%S %z")
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

macro_rules! json_adapter {
    ($name:ident, $source:literal, $parser:ident) => {
        pub struct $name {
            payloads: Vec<Value>,
        }

        impl $name {
            pub fn from_payloads(payloads: Vec<Value>) -> Self {
                Self { payloads }
            }
        }

        #[async_trait]
        impl IngestAdapter for $name {
            fn source_adapter(&self) -> &'static str {
                $source
            }

            async fn read_hits(&mut self) -> Result<Vec<RawCrawlerHit>, CrawlerIngestError> {
                Ok(self
                    .payloads
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, v)| $parser(idx, v, $source))
                    .collect())
            }
        }
    };
}

json_adapter!(
    CloudflareLogpushAdapter,
    "cloudflare_logpush",
    parse_cdn_json
);
json_adapter!(
    CloudflareWorkersAdapter,
    "cloudflare_workers",
    parse_cdn_json
);
json_adapter!(FastlyAdapter, "fastly", parse_cdn_json);
json_adapter!(CloudFrontAdapter, "cloudfront", parse_cdn_json);
json_adapter!(Ga4Adapter, "ga4", parse_ga4_json);

fn parse_cdn_json(idx: usize, v: &Value, source: &str) -> Option<RawCrawlerHit> {
    let ts = read_ts(v, &["ts", "timestamp", "datetime", "time"])?;
    Some(RawCrawlerHit {
        raw_event_id: read_string(v, &["id", "ray_id", "request_id"])
            .unwrap_or_else(|| format!("{source}:{idx}")),
        ts,
        user_agent: read_string(v, &["user_agent", "UserAgent", "client_user_agent"])?,
        path: read_string(v, &["path", "Path", "uri", "url"])?,
        status: read_u16(v, &["status", "Status", "status_code"]).unwrap_or(0),
        source_adapter: source.to_string(),
        client_ip: read_string(v, &["ip", "client_ip", "ClientIP", "c-ip"]),
        region: read_string(v, &["region", "country", "colo"]),
    })
}

fn parse_ga4_json(idx: usize, v: &Value, source: &str) -> Option<RawCrawlerHit> {
    let ts = read_ts(v, &["ts", "timestamp", "event_timestamp"])?;
    Some(RawCrawlerHit {
        raw_event_id: read_string(v, &["event_id", "id"])
            .unwrap_or_else(|| format!("{source}:{idx}")),
        ts,
        user_agent: read_string(v, &["user_agent", "ua"])?,
        path: read_string(v, &["page_path", "path", "page_location"])?,
        status: read_u16(v, &["status"]).unwrap_or(200),
        source_adapter: source.to_string(),
        client_ip: read_string(v, &["ip", "client_ip"]),
        region: read_string(v, &["region", "country"]),
    })
}

fn read_string(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(*k))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn read_u16(v: &Value, keys: &[&str]) -> Option<u16> {
    keys.iter()
        .find_map(|k| v.get(*k))
        .and_then(Value::as_u64)
        .and_then(|n| u16::try_from(n).ok())
}

fn read_ts(v: &Value, keys: &[&str]) -> Option<DateTime<Utc>> {
    if let Some(s) = read_string(v, keys) {
        return DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc));
    }
    keys.iter()
        .find_map(|k| v.get(*k))
        .and_then(Value::as_i64)
        .and_then(|micros| Utc.timestamp_micros(micros).single())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::identify_bot;

    #[test]
    fn parses_combined_log_ai_bot_hit() {
        let line = r#"203.0.113.9 - - [31/May/2026:09:00:00 +0000] "GET /docs HTTP/1.1" 200 42 "-" "GPTBot/1.0""#;
        let hit = AccessLogAdapter::parse_line(7, line, AccessLogFormat::Combined, None, "nginx")
            .expect("hit");
        assert_eq!(hit.raw_event_id, "nginx:7");
        assert_eq!(hit.path, "/docs");
        assert_eq!(hit.status, 200);
        assert_eq!(
            identify_bot(&hit.user_agent).as_deref(),
            Some("openai-gptbot")
        );
    }

    #[test]
    fn parses_custom_pattern() {
        let line = "evt-1 2026-05-31T09:00:00Z 203.0.113.9 /pricing 500 NL ClaudeBot";
        let hit = AccessLogAdapter::parse_line(
            0,
            line,
            AccessLogFormat::Custom,
            Some("{id} {ts} {ip} {path} {status} {region} {ua}"),
            "custom",
        )
        .expect("hit");
        assert_eq!(hit.raw_event_id, "evt-1");
        assert_eq!(hit.region.as_deref(), Some("NL"));
        assert_eq!(hit.user_agent, "ClaudeBot");
    }

    #[tokio::test]
    async fn cdn_adapters_parse_recorded_payload_shape() {
        let mut adapter = CloudflareLogpushAdapter::from_payloads(vec![serde_json::json!({
            "ray_id": "ray-1",
            "timestamp": "2026-05-31T09:00:00Z",
            "user_agent": "PerplexityBot",
            "path": "/docs",
            "status": 200,
            "client_ip": "203.0.113.9",
            "country": "NL"
        })]);
        let hits = adapter.read_hits().await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source_adapter, "cloudflare_logpush");
        assert_eq!(hits[0].region.as_deref(), Some("NL"));
    }
}
