use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::model::{identify_bot, CrawlerIngestError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CidrRange {
    network: IpAddr,
    prefix: u8,
}

impl CidrRange {
    pub fn parse(s: &str) -> Result<Self, CrawlerIngestError> {
        let (addr, prefix) = s
            .split_once('/')
            .ok_or_else(|| CrawlerIngestError::Invalid(format!("CIDR `{s}` is missing `/`")))?;
        let network = IpAddr::from_str(addr)
            .map_err(|e| CrawlerIngestError::Invalid(format!("invalid CIDR IP `{addr}`: {e}")))?;
        let prefix: u8 = prefix.parse().map_err(|e| {
            CrawlerIngestError::Invalid(format!("invalid CIDR prefix `{prefix}`: {e}"))
        })?;
        let max = match network {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix > max {
            return Err(CrawlerIngestError::Invalid(format!(
                "CIDR prefix `{prefix}` exceeds {max}"
            )));
        }
        Ok(Self { network, prefix })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => prefix_match_v4(net, ip, self.prefix),
            (IpAddr::V6(net), IpAddr::V6(ip)) => prefix_match_v6(net, ip, self.prefix),
            _ => false,
        }
    }
}

fn prefix_match_v4(net: Ipv4Addr, ip: Ipv4Addr, prefix: u8) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    (u32::from(net) & mask) == (u32::from(ip) & mask)
}

fn prefix_match_v6(net: Ipv6Addr, ip: Ipv6Addr, prefix: u8) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    };
    (u128::from(net) & mask) == (u128::from(ip) & mask)
}

#[derive(Debug, Clone)]
pub struct BotRangeVerifier {
    ranges: Vec<(String, CidrRange)>,
}

impl Default for BotRangeVerifier {
    fn default() -> Self {
        let seed = [
            ("openai-gptbot", "20.42.0.0/16"),
            ("openai-chatgpt-user", "20.42.0.0/16"),
            ("openai-oai-searchbot", "20.42.0.0/16"),
            ("anthropic-claudebot", "160.79.104.0/23"),
            ("perplexitybot", "34.117.0.0/16"),
            ("google-extended", "66.249.64.0/19"),
            ("googlebot", "66.249.64.0/19"),
            ("common-crawl-ccbot", "18.97.0.0/16"),
        ];
        Self {
            ranges: seed
                .into_iter()
                .filter_map(|(bot, cidr)| CidrRange::parse(cidr).ok().map(|r| (bot.to_string(), r)))
                .collect(),
        }
    }
}

impl BotRangeVerifier {
    pub fn from_ranges(ranges: Vec<(String, CidrRange)>) -> Self {
        Self { ranges }
    }

    pub fn refresh_from_file(path: &Path) -> Result<Self, CrawlerIngestError> {
        let body = fs::read_to_string(path)?;
        #[derive(Deserialize)]
        struct Row {
            bot_id: String,
            cidr: String,
        }
        let rows: Vec<Row> = serde_json::from_str(&body)?;
        let mut ranges = Vec::with_capacity(rows.len());
        for row in rows {
            ranges.push((row.bot_id, CidrRange::parse(&row.cidr)?));
        }
        Ok(Self { ranges })
    }

    pub fn verify_bot_ip(&self, bot_id: &str, ip: &str) -> bool {
        let Ok(ip) = IpAddr::from_str(ip) else {
            return false;
        };
        self.ranges
            .iter()
            .any(|(bot, range)| bot == bot_id && range.contains(ip))
    }

    pub fn verify_user_agent_ip(&self, user_agent: &str, ip: &str) -> bool {
        identify_bot(user_agent)
            .map(|bot_id| self.verify_bot_ip(&bot_id, ip))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spoofed_user_agent_from_wrong_ip_is_unverified() {
        let verifier = BotRangeVerifier::from_ranges(vec![(
            "openai-gptbot".into(),
            CidrRange::parse("203.0.113.0/24").unwrap(),
        )]);
        assert!(verifier.verify_user_agent_ip("GPTBot", "203.0.113.9"));
        assert!(!verifier.verify_user_agent_ip("GPTBot", "198.51.100.9"));
    }

    #[test]
    fn refresh_replaces_ranges_without_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ranges.json");
        fs::write(
            &path,
            r#"[{"bot_id":"anthropic-claudebot","cidr":"198.51.100.0/24"}]"#,
        )
        .unwrap();
        let verifier = BotRangeVerifier::refresh_from_file(&path).unwrap();
        assert!(verifier.verify_user_agent_ip("ClaudeBot", "198.51.100.7"));
        assert!(!verifier.verify_user_agent_ip("ClaudeBot", "203.0.113.7"));
    }
}
