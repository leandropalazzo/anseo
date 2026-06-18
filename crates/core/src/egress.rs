//! Outbound destination guardrails for provider and webhook traffic.
//!
//! Phase 4 egress controls have two layers: cloud network policy, and an
//! application fail-closed check that is testable in OSS CI. This module owns
//! the shared host/IP classification so provider clients, webhook declaration,
//! and webhook delivery reject the same dangerous destinations.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EgressPolicyError {
    #[error("host is empty")]
    EmptyHost,
    #[error("host `{host}` resolves to forbidden address `{ip}`")]
    ForbiddenAddress { host: String, ip: IpAddr },
}

/// Return `Ok(())` only when every known literal address for `host` is safe for
/// outbound traffic. DNS names that are not numeric/IP literals are resolved by
/// callers at delivery time and checked with [`validate_resolved_ip`].
pub fn validate_host_literal(host: &str) -> Result<(), EgressPolicyError> {
    let normalized = host.trim().trim_matches('[').trim_matches(']');
    if normalized.is_empty() {
        return Err(EgressPolicyError::EmptyHost);
    }
    if let Some(ip) = parse_ip_literal(normalized) {
        validate_resolved_ip(normalized, ip)?;
    }
    Ok(())
}

/// Validate an address returned by DNS before connecting. Rejects private,
/// loopback, link-local, metadata-service, multicast, unspecified, and unique
/// local IPv6 ranges.
pub fn validate_resolved_ip(host: &str, ip: IpAddr) -> Result<(), EgressPolicyError> {
    if is_forbidden_ip(ip) {
        return Err(EgressPolicyError::ForbiddenAddress {
            host: host.to_string(),
            ip,
        });
    }
    Ok(())
}

pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_forbidden_ipv4(ip),
        IpAddr::V6(ip) => is_forbidden_ipv6(ip),
    }
}

fn is_forbidden_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.octets() == [169, 254, 169, 254]
        || ip.octets()[0] == 0
}

fn is_forbidden_ipv6(ip: Ipv6Addr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || is_ipv6_unique_local(ip)
        || is_ipv6_unicast_link_local(ip)
}

fn is_ipv6_unique_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn is_ipv6_unicast_link_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

/// Parse ordinary IP literals plus the legacy decimal/octal/hex IPv4 forms
/// browsers and libc accept for addresses such as `169.254.169.254`.
pub fn parse_ip_literal(host: &str) -> Option<IpAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip);
    }
    parse_ipv4_legacy_literal(host).map(IpAddr::V4)
}

fn parse_ipv4_legacy_literal(host: &str) -> Option<Ipv4Addr> {
    if host.is_empty() || host.starts_with('-') {
        return None;
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.iter().any(|p| p.is_empty()) || parts.len() > 4 {
        return None;
    }
    let mut nums = Vec::with_capacity(parts.len());
    for part in parts {
        nums.push(parse_legacy_int(part)?);
    }
    let value = match nums.as_slice() {
        [a] if *a <= 0xffff_ffff => *a,
        [a, b] if *a <= 0xff && *b <= 0x00ff_ffff => (a << 24) | b,
        [a, b, c] if *a <= 0xff && *b <= 0xff && *c <= 0xffff => (a << 24) | (b << 16) | c,
        [a, b, c, d] if *a <= 0xff && *b <= 0xff && *c <= 0xff && *d <= 0xff => {
            (a << 24) | (b << 16) | (c << 8) | d
        }
        _ => return None,
    };
    Some(Ipv4Addr::from(value as u32))
}

fn parse_legacy_int(s: &str) -> Option<u64> {
    let (radix, digits) = if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
    {
        (16, rest)
    } else if s.len() > 1 && s.starts_with('0') {
        (8, &s[1..])
    } else {
        (10, s)
    };
    if digits.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(digits, radix).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_private_loopback_link_local_and_metadata() {
        for ip in [
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1)),
            IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            "fe80::1".parse().unwrap(),
            "fc00::1".parse().unwrap(),
        ] {
            assert!(validate_resolved_ip("target", ip).is_err(), "{ip}");
        }
    }

    #[test]
    fn accepts_public_addresses() {
        for ip in [
            IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
            "2606:4700:4700::1111".parse().unwrap(),
        ] {
            assert!(validate_resolved_ip("target", ip).is_ok(), "{ip}");
        }
    }

    #[test]
    fn parses_legacy_ipv4_metadata_spellings() {
        let metadata = IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254));
        for host in [
            "169.254.169.254",
            "0251.0376.0251.0376",
            "0xA9.0xFE.0xA9.0xFE",
            "2852039166",
            "0xA9FEA9FE",
        ] {
            assert_eq!(parse_ip_literal(host), Some(metadata), "{host}");
            assert!(validate_host_literal(host).is_err(), "{host}");
        }
    }

    #[test]
    fn leaves_dns_names_for_runtime_resolution() {
        assert_eq!(parse_ip_literal("api.openai.com"), None);
        assert!(validate_host_literal("api.openai.com").is_ok());
    }
}
