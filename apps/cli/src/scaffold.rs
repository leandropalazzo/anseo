//! Scaffolded files emitted by `anseo init` (FR-10).
//!
//! Three files land in the target directory: `anseo.yaml`, `.gitignore`,
//! `README.md`. The exact bytes are part of the FR-10 contract — schema-v0.1
//! conformance of the scaffolded yaml is asserted by an integration test.

/// Returns the scaffolded `anseo.yaml` content for the given deployment tier.
///
/// `tier` is the value chosen at `anseo init`:
/// - `0` = solo CLI (no long-running server)
/// - `1` = single binary (`anseo serve`)
/// - `2` = Docker Compose
///
/// The `tier:` key is omitted from the YAML when `tier == 0` (default, backward-compat).
pub fn anseo_yaml(tier: u8) -> String {
    let tier_line = match tier {
        0 => String::new(),
        t => format!(
            "tier: {t}             # 0=solo CLI  1=single binary (anseo serve)  2=Docker Compose\n"
        ),
    };
    format!(
        r#"# Anseo project — Phase 1 schema v0.1.
#
# This file is the canonical declaration of what to monitor (FR-23).
# Commit it to version control. Provider API keys live in your system
# keychain via `anseo login`, NEVER in this file.
schema_version: '0.1'
{tier_line}
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
"#,
        tier_line = tier_line
    )
}

/// The scaffolded `anseo.yaml` for tier 0 (backward-compat constant).
///
/// Prefer [`anseo_yaml(tier)`] in new code.
pub const ANSEO_YAML: &str = concat!(
    "# Anseo project — Phase 1 schema v0.1.\n",
    "#\n",
    "# This file is the canonical declaration of what to monitor (FR-23).\n",
    "# Commit it to version control. Provider API keys live in your system\n",
    "# keychain via `anseo login`, NEVER in this file.\n",
    "schema_version: '0.1'\n",
    "\n",
    "brand:\n",
    "  # Your brand's canonical name. Mentions of this string (and the variants\n",
    "  # below) count toward your visibility score.\n",
    "  name: \"Your Brand\"\n",
    "  # Aliases, abbreviations, product names, common misspellings. Optional\n",
    "  # but recommended — Mentions match these case-insensitively.\n",
    "  variants: []\n",
    "\n",
    "competitors:\n",
    "  # Add one entry per competitor you want to compare against. Example:\n",
    "  #   - name: Competitor A\n",
    "  #     variants: [comp-a]\n",
    "  []\n",
    "\n",
    "prompts:\n",
    "  # Each prompt is sent to every configured Provider on every `anseo prompt run`.\n",
    "  # `name` is a slug (^[a-z][a-z0-9-]*$); `text` is the prompt body.\n",
    "  - name: example-prompt\n",
    "    text: |\n",
    "      Replace this with a real prompt your customers would ask an AI\n",
    "      assistant — e.g. \"What are the best vector databases for RAG?\"\n",
    "    description: Example placeholder; safe to delete once you have real prompts.\n",
    "\n",
    "providers:\n",
    "  # Uncomment and authenticate via `anseo login openai|anthropic` to enable.\n",
    "  #\n",
    "  #   - name: openai\n",
    "  #     # model: gpt-4o-2024-08-06       # optional; provider default\n",
    "  #     # timeout_seconds: 60            # optional; default 60\n",
    "  #\n",
    "  #   - name: anthropic\n",
    "  #     # model: claude-3-5-sonnet-20241022\n",
    "  []\n",
    "\n",
    "# Optional. Parallelism for `anseo prompt run`. Defaults to 4.\n",
    "# concurrency: 4\n",
);

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
