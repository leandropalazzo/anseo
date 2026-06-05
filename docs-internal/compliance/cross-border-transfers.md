# Cross-Border Data Transfer Controls

**Story:** 44.4 — Geo-Gating: High-Friction Jurisdiction Controls  
**Status:** Implementation complete; SCC/TIA text pending legal finalisation (39.8).  
**Last updated:** 2026-06-05

---

## Jurisdictions Served

Anseo (OpenGEO) is operated from the European Economic Area (EEA) and accepts
anonymous, aggregate benchmark contributions from all jurisdictions.

**Identified-tier (class-c) endpoints** — those that carry or link to personal
or brand-attributed data — are currently restricted in the jurisdictions below.

---

## Per-Jurisdiction Restrictions

| Jurisdiction | ISO-3166 Code | Restriction | Applicable Law | Notes |
|---|---|---|---|---|
| China | CN | Identified-tier blocked | PIPL (Personal Information Protection Law, 2021) | Localisation and cross-border transfer rules are disproportionate to current scale. |
| India | IN | Identified-tier blocked | DPDP (Digital Personal Data Protection Act, 2023) | Framework still maturing; consent + localisation obligations under review. |
| Brazil | BR | Identified-tier blocked | LGPD (Lei Geral de Proteção de Dados, 2020) | Transfer restrictions and DPA registration not yet completed. |

Default blocked list: `CN,IN,BR`. Configurable via `ANSEO_HIGH_FRICTION_JURISDICTIONS`.

---

## Transfer Mechanism

For jurisdictions that are served (outside the blocked list above):

- **EEA → EEA / UK / adequacy-decision countries**: no additional safeguards required.
- **EEA → US**: Data Privacy Framework (DPF) — *PLACEHOLDER: confirm DPF self-certification status with legal before launch.*
- **EEA → other third countries**: Standard Contractual Clauses (SCCs, EU 2021/914) — *PLACEHOLDER: SCC drafting in progress under story 39.8; Transfer Impact Assessments (TIA) to be completed per country.*

---

## Geo-Gate Rationale

The geo-gate is a risk-reduction posture, not a legal enforcement guarantee. It is:

- **Targeted**: only identified-tier (class-c) endpoints are blocked; anonymous and
  aggregate-only endpoints (`/v1/benchmark/density-check`, `/v1/visibility/*`, etc.)
  are fully accessible from all jurisdictions.
- **Configurable**: the blocked jurisdiction list is driven by the
  `ANSEO_HIGH_FRICTION_JURISDICTIONS` environment variable and takes effect within
  one request cycle (no restart required).
- **Logged**: rejections are logged with `{jurisdiction_code, endpoint, timestamp}`.
  No personal data (IP address, User-Agent, etc.) is logged in the rejection record.

---

## Acknowledged Limitations (VPN / Proxy Detection Gaps)

The geo-gate relies on request headers:

1. `CF-IPCountry` (Cloudflare CDN — highest reliability in CF deployments).
2. `X-Country-Code` (operator-set custom header).

These signals can be bypassed by:
- VPN exit nodes in non-blocked countries.
- Open/anonymous proxies.
- Tor exit nodes.

This is acknowledged and accepted: the geo-gate is a good-faith compliance
posture at current scale. Operators running Anseo behind their own reverse proxy
can inject an authoritative `X-Country-Code` header to improve accuracy.

When neither header is present, the request is not blocked (fail-open). This
is a deliberate product decision: blocking all un-geolocated traffic would
impair self-hosted operators with no CDN. Operators requiring stricter enforcement
should add their own upstream IP-geolocation layer.

---

## Coordination

- Story 39.8 (SCCs + TIA drafting): legal process, not dev work. This document
  will be updated when 39.8 closes.
- Story 44.4 (implementation): complete.
- Configuration: `ANSEO_HIGH_FRICTION_JURISDICTIONS` (default: `CN,IN,BR`).

---

*This document is a dev/compliance coordination artifact. It is not legal advice.
Final legal text lives in the legal folder (not committed to this repository).*
