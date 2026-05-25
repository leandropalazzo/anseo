//! Scaffolded files emitted by `ogeo init` (FR-10).
//!
//! Three files land in the target directory: `opengeo.yaml`, `.gitignore`,
//! `README.md`. The exact bytes are part of the FR-10 contract — schema-v0.1
//! conformance of the scaffolded yaml is asserted by an integration test.

pub const OPENGEO_YAML: &str = r#"# OpenGEO project — Phase 1 schema v0.1.
#
# This file is the canonical declaration of what to monitor (FR-23).
# Commit it to version control. Provider API keys live in your system
# keychain via `ogeo login`, NEVER in this file.
schema_version: '0.1'

brand:
  # Your brand's canonical name. Mentions of this string (and the variants
  # below) count toward your visibility score.
  name: "Your Brand"
  # Aliases, abbreviations, product names, common misspellings. Optional
  # but recommended — Mentions match these case-insensitively.
  variants: []

competitors:
  # Add one entry per competitor you want to compare against. Example:
  #   - name: Competitor A
  #     variants: [comp-a]
  []

prompts:
  # Each prompt is sent to every configured Provider on every `ogeo prompt run`.
  # `name` is a slug (^[a-z][a-z0-9-]*$); `text` is the prompt body.
  - name: example-prompt
    text: |
      Replace this with a real prompt your customers would ask an AI
      assistant — e.g. "What are the best vector databases for RAG?"
    description: Example placeholder; safe to delete once you have real prompts.

providers:
  # Uncomment and authenticate via `ogeo login openai|anthropic` to enable.
  #
  #   - name: openai
  #     # model: gpt-4o-2024-08-06       # optional; provider default
  #     # timeout_seconds: 60            # optional; default 60
  #
  #   - name: anthropic
  #     # model: claude-3-5-sonnet-20241022
  []

# Optional. Parallelism for `ogeo prompt run`. Defaults to 4.
# concurrency: 4
"#;

pub const GITIGNORE: &str = r#"# OpenGEO — entries scaffolded by `ogeo init` (FR-10).
/data
.env
"#;

pub const README: &str = r#"# OpenGEO project

Scaffolded by `ogeo init`. Five-minute path:

1. Edit `opengeo.yaml`:
   - Set `brand.name` and `brand.variants` to your brand.
   - Add one or more `prompts` — questions your customers ask AI assistants.
   - Uncomment a provider block (`openai` and/or `anthropic`).
2. Authenticate at least one provider:
   ```sh
   ogeo login openai
   ogeo login anthropic
   ```
3. Run prompts and view results:
   ```sh
   ogeo prompt run
   ogeo dashboard open
   ```

See `docs/config/opengeo-yaml-schema.md` for the full schema.
"#;
