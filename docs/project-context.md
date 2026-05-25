# OpenGEO — BMad Product Lifecycle Document

**Version:** 0.2
**Category:** OSS AI Search Observability Platform

## 1. Product Overview

**Product Name:** OpenGEO (working title)

**Category:**
- AI Search Observability
- Generative Engine Optimization (GEO)
- LLM Visibility Analytics

**Tagline:** The open-source observability stack for AI search visibility.

## 2. Vision

Build the leading open-source infrastructure platform for monitoring, analyzing, and improving visibility inside AI-powered search and recommendation systems.

OpenGEO enables organizations to understand:

- how they appear in LLM outputs
- which sources influence recommendations
- how rankings change over time
- how competitors perform across AI systems

## 3. Mission

Provide developers, startups, and enterprises with:

- transparent GEO infrastructure
- reproducible AI visibility monitoring
- programmable APIs
- self-hosted deployment
- AI-native integrations

## 4. Core Product Thesis

AI search is becoming a foundational discovery layer.

Organizations will require:

- observability
- monitoring
- reproducibility
- analytics
- governance

for AI-generated recommendations.

OpenGEO aims to become **the open infrastructure standard for AI visibility observability**.

## 5. Strategic Positioning

**Primary Positioning:** "Prometheus + Grafana for AI Search Visibility."

**Secondary Positioning:** "Programmable observability infrastructure for AI visibility."

## 6. Product Shape

```
Core Engine
├── API Server
├── Worker Runtime
├── CLI
├── Frontend Dashboard
└── MCP Server
```

## 7. Product Philosophy

**Infrastructure First.** OpenGEO should feel like:

- Terraform
- Grafana
- Prometheus
- Sentry
- Supabase

NOT:

- an SEO agency tool
- AI content generation software
- marketing automation software

## 8. User Personas

### Developer Marketing Teams
Need:
- AI visibility monitoring
- automated reporting
- reproducible rankings

### Startups
Need:
- affordable GEO tooling
- competitive intelligence
- self-hosted observability

### Enterprises
Need:
- governance
- auditing
- historical analytics
- reproducibility

### Agencies
Need:
- multi-client monitoring
- benchmark reports
- dashboards

### Open Source Projects
Need:
- visibility tracking
- citation monitoring
- AI recommendation insights

## 9. Product Interfaces

### 9.1 CLI (Primary Interface)

**Purpose:** Primary developer workflow.
**Positioning:** Infrastructure-grade command-line tooling.

Example Commands:

```
ogeo init
ogeo login
ogeo monitor add
ogeo prompt run
ogeo benchmark pull
ogeo report generate
ogeo dashboard open
```

CI/CD Example:

```
ogeo check visibility \
  --prompt "best vector database" \
  --brand "Pinecone" \
  --expect-rank-lte 3
```

GitOps Example:

```yaml
prompts:
  - name: vector-db
    text: best vector database

providers:
  - openai
  - anthropic

competitors:
  - pinecone
  - qdrant
  - weaviate
```

### 9.2 Frontend Dashboard

**Purpose:** Visualization and reporting.

MVP Dashboard Features:
- prompt runs
- rankings
- visibility trends
- citation analysis
- competitor comparison
- analytics graphs

Frontend Stack:
- Next.js
- TypeScript
- Tailwind
- shadcn/ui

### 9.3 MCP Server

**Purpose:** AI-native integrations.

**Strategic Goal:** Allow AI agents to query OpenGEO programmatically.

Example MCP Tools:
- `run_prompt`
- `get_visibility`
- `compare_brands`
- `get_citations`
- `list_trends`
- `search_benchmarks`

Example Workflow — user asks Claude Desktop: "Why did our AI visibility drop this week?" Claude uses OpenGEO MCP to:
- pull rankings
- analyze citation changes
- compare competitors
- generate recommendations

## 10. Core Features

### 10.1 Prompt Monitoring

Run prompts across multiple LLM providers and track:
- brand mentions
- ranking positions
- response changes
- competitor visibility

Example Prompt:

```
prompt: "best observability platforms for startups"
```

### 10.2 Multi-Provider Support

Initial Providers:
- OpenAI
- Anthropic
- Gemini
- Perplexity

Future Providers:
- Grok
- Mistral
- OpenRouter
- Local OSS models

**Provider Philosophy:** Use direct provider APIs first for reproducibility, stability, traceability. OpenRouter added later as optional provider aggregation layer.

### 10.3 Citation Extraction

Track:
- URLs
- domains
- Reddit citations
- Wikipedia references
- YouTube references
- documentation links

Output: structured citation graph.

### 10.4 Analytics Engine

Metrics:
- visibility share
- ranking trends
- prompt volatility
- citation frequency
- competitor comparisons

Visualizations:
- time-series charts
- heatmaps
- ranking tables
- citation graphs

### 10.5 Scheduling & Monitoring

- scheduled prompt runs
- automated monitoring
- alerting
- anomaly detection

## 11. Advanced Features

### GEO Recommendations

Examples:
- improve docs
- optimize citations
- identify weak visibility areas

### Public Benchmark Dataset

Example Reports:
- most cited domains in ChatGPT
- fastest-growing AI-visible startups
- GPT vs Claude recommendation differences

### GitHub Action

```
- uses: opengEO/check-visibility@v1
```

### Browser Extension

- citation overlays
- ranking insights
- prompt analysis

## 12. Technical Architecture

**Backend Stack:**
- Rust
- Axum
- Tokio
- SQLx
- PostgreSQL
- Redis

**Analytics:** ClickHouse (Phase 2 scaling)

**Deployment:**
- Docker Compose
- Kubernetes
- Helm
- Railway
- Render
- Fly.io

## 13. Suggested Repository Structure

```
/opengEO
  /apps
    /web
    /api
    /worker
    /cli
    /mcp

  /crates
    /core
    /providers
    /analytics
    /extractors
    /storage
    /scheduler
    /mcp-shared

  /infra
    /docker
    /k8s
    /terraform

  /docs
  /examples
```

## 14. Data Model

**Core Entities:**

### Organization
- users
- projects
- billing

### Project
- prompts
- providers
- competitors

### Prompt Run
- timestamp
- provider
- region
- raw response
- extracted mentions

### Citation
- URL
- domain
- type
- frequency

## 15. API Strategy

**Principles:**
- API-first
- REST-based
- OpenAPI docs
- webhook support
- SDK generation

**Public APIs** expose:
- prompt execution
- rankings
- citation analytics
- trend analysis
- benchmark access

## 16. Open Source Strategy

**License:** MIT License.

**Open Core Model:**

Open Source:
- core engine
- CLI
- dashboard
- connectors
- MCP server
- APIs
- basic analytics

Commercial Cloud:
- managed hosting
- enterprise datasets
- advanced benchmarking
- historical archives
- team workflows
- advanced analytics

## 17. Growth Strategy

**Core Growth Engine:**

```
Product creates data
      ↓
Data creates content
      ↓
Content creates visibility
      ↓
Visibility creates GitHub stars
      ↓
Stars create contributors
      ↓
Contributors improve product
```

**Primary Growth Channels:**
- GitHub
- Hacker News
- Reddit
- Twitter/X
- Product Hunt

**Content Strategy — Automated Benchmark Reports:**
- "Top 100 AI-visible startups"
- "Most cited domains in ChatGPT"
- "Claude vs GPT recommendation rankings"

**Public Dashboard Strategy:**
- AI visibility leaderboards
- citation tracking dashboards
- ranking observatories

## 18. Monetization Strategy

**Free Tier:**
- self-hosted OSS
- limited cloud usage

**Pro Tier:**
- hosted dashboards
- advanced analytics
- increased prompt volume

**Enterprise Tier:**
- SSO
- compliance
- audit logs
- SLA support
- private datasets

## 19. Risks

**Technical Risks:**
- provider API instability
- LLM inconsistency
- rate limiting
- inference costs

**Market Risks:**
- immature GEO category
- rapidly evolving ecosystem
- provider lock-in

**Operational Risks:**
- scaling analytics
- infrastructure costs
- maintaining benchmark quality

## 20. Product Roadmap

### Phase 1 — MVP
- prompt monitoring
- OpenAI + Anthropic support
- CLI
- basic dashboard
- Docker deployment

### Phase 2 — OSS Expansion
- GitHub Action
- scheduling
- analytics improvements
- public benchmark dashboards

### Phase 3 — Ecosystem
- MCP server
- plugin ecosystem
- benchmark APIs
- browser extension

### Phase 4 — Enterprise
- hosted cloud
- enterprise analytics
- governance tooling
- large-scale datasets

## 21. Initial Success Metrics

**Product Metrics:**
- GitHub stars
- self-hosted deployments
- active projects
- prompt runs/day

**Community Metrics:**
- contributors
- Discord members
- docs traffic
- benchmark backlinks

**Business Metrics:**
- MRR
- enterprise accounts
- cloud conversion rate

## 22. Final Strategic Goal

OpenGEO becomes:

- the observability layer for AI search
- the largest open GEO dataset
- the programmable infrastructure platform for AI visibility monitoring
- the default developer platform for AI search analytics
