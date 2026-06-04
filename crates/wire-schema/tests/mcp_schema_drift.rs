//! Token-level CI lint: re-emit JSON Schemas for every MCP tool DTO and
//! assert byte-identical match against the committed copies under
//! `crates/wire-schema/schemas/mcp/`.
//!
//! Failure mode: "wire shape changed without committing the schema regen."
//!
//! Regenerating: set `OPENGEO_WIRE_SCHEMA_UPDATE=1` to overwrite the
//! on-disk snapshots. The CI lint then re-runs and asserts they match.

use std::fs;
use std::path::PathBuf;

use opengeo_wire_schema::mcp::json_schema::all_schemas;

fn schemas_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas/mcp")
}

#[test]
fn mcp_schemas_match_committed_snapshots() {
    let dir = schemas_dir();
    let update = std::env::var("OPENGEO_WIRE_SCHEMA_UPDATE").is_ok();

    if update {
        fs::create_dir_all(&dir).expect("create schemas dir");
    }

    let mut failures: Vec<String> = Vec::new();

    for entry in all_schemas() {
        let path = dir.join(format!("{}.json", entry.name));

        if update {
            fs::write(&path, &entry.schema_json).expect("write schema");
            continue;
        }

        let on_disk = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "missing committed schema {}: {e}\n\
                 Run with OPENGEO_WIRE_SCHEMA_UPDATE=1 to seed.",
                path.display()
            )
        });

        if on_disk != entry.schema_json {
            failures.push(entry.name.to_string());
        }
    }

    assert!(
        failures.is_empty(),
        "MCP wire shape drift detected for: {failures:?}\n\
         Run `OPENGEO_WIRE_SCHEMA_UPDATE=1 cargo test -p opengeo-wire-schema mcp_schemas_match_committed_snapshots` \
         to regen, then commit the updated schemas."
    );
}

#[test]
fn mcp_schema_count_is_twenty_four() {
    // 6 Phase-2 tools + 5 Story-19.7 recommend.* tools + 1 Roadmap Epic-32
    // audit tool = 12 tools × (input + output) = 24 schemas.
    assert_eq!(all_schemas().len(), 24);
}
