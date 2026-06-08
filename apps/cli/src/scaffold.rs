//! Scaffolded files emitted by `anseo init` (FR-10).
//!
//! Three files land in the target directory: `anseo.yaml`, `.gitignore`,
//! `README.md`. The exact bytes are part of the FR-10 contract — schema-v0.1
//! conformance of the scaffolded yaml is asserted by an integration test.

/// The scaffolded `anseo.yaml` template (formerly `anseo.yaml`).
pub const ANSEO_YAML: &str = r#"# Anseo project — Phase 1 schema v0.1.
#
# This file is the canonical declaration of what to monitor (FR-23).
# Commit it to version control. Provider API keys live in your system
# keychain via `anseo login`, NEVER in this file.
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
  # Each prompt is sent to every configured Provider on every `anseo prompt run`.
  # `name` is a slug (^[a-z][a-z0-9-]*$); `text` is the prompt body.
  - name: example-prompt
    text: |
      Replace this with a real prompt your customers would ask an AI
      assistant — e.g. "What are the best vector databases for RAG?"
    description: Example placeholder; safe to delete once you have real prompts.

providers:
  # Uncomment and authenticate via `anseo login openai|anthropic` to enable.
  #
  #   - name: openai
  #     # model: gpt-4o-2024-08-06       # optional; provider default
  #     # timeout_seconds: 60            # optional; default 60
  #
  #   - name: anthropic
  #     # model: claude-3-5-sonnet-20241022
  []

# Optional. Parallelism for `anseo prompt run`. Defaults to 4.
# concurrency: 4
"#;

/// Deprecated alias for [`ANSEO_YAML`]. Use [`ANSEO_YAML`] in new code.
#[deprecated(since = "0.7.0", note = "use ANSEO_YAML instead")]
pub const OPENGEO_YAML: &str = ANSEO_YAML;

pub const GITIGNORE: &str = r#"# Anseo — entries scaffolded by `anseo init` (FR-10).
/data
.env
"#;

pub const README: &str = r#"# Anseo project

Scaffolded by `anseo init`. Five-minute path:

1. Edit `anseo.yaml`:
   - Set `brand.name` and `brand.variants` to your brand.
   - Add one or more `prompts` — questions your customers ask AI assistants.
   - Uncomment a provider block (`openai` and/or `anthropic`).
2. Authenticate at least one provider:
   ```sh
   anseo login openai
   anseo login anthropic
   ```
3. Run prompts and view results:
   ```sh
   anseo prompt run
   anseo dashboard open
   ```

See `docs/config/anseo-yaml-schema.md` for the full schema.
"#;
