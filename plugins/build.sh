#!/usr/bin/env bash
#
# Story 41.5 — materialize the first-party plugins into the on-disk registry /
# install layout the host loader and CLI read.
#
# Why this exists
# ---------------
# The `plugins/<dir>/` directories hold plugin SOURCE only (Cargo.toml + src/ +
# manifest.yaml). We deliberately do NOT commit built `entrypoint.wasm`
# artifacts to git (no binaries in version control). This script compiles each
# plugin and lays the produced artifact + manifest into the exact directory shape
# the runtime resolves:
#
#     <out>/plugins/<id>/<version>/manifest.yaml
#     <out>/plugins/<id>/<version>/entrypoint.wasm
#
# where <id> is the namespaced registry id (`anseo/<name>`) and <version> is read
# from each manifest. This matches:
#   * crates/plugin-host/src/registry.rs  (version_path → plugins/<id>/<version>/entrypoint.wasm)
#   * apps/cli/src/commands/plugin_install.rs (install_dir = home/plugins/<id>/<version>/, writes entrypoint.wasm)
#   * apps/cli/src/commands/plugin_registry.rs (reads <root>/plugins/<id>/<version>/entrypoint.wasm)
#
# The `entrypoint.wasm` filename is the install-layout convention regardless of
# the artifact's true format: provider plugins build to a real `.wasm`
# (wasm32-wasip1 cdylib); the analytics plugin is the supported native
# subprocess kind and its release binary is staged under the same filename (the
# host's subprocess adapter spawns whatever lives at that path — see
# crates/plugin-host/src/subprocess.rs).
#
# Signing (signature.bin + claim.toml) is NOT done here. That is the 41.4
# signing pipeline's job: it runs on top of this materialized layout, computing
# SHA-256(manifest.yaml || entrypoint.wasm) and producing the Ed25519 detached
# signature + root-countersigned namespace claim (see
# crates/plugin-host/src/signing.rs). See plugins/README.md.
#
# Usage:
#   plugins/build.sh [OUT_DIR]
#
# OUT_DIR defaults to plugins/dist (gitignored). The script is idempotent: re-
# running overwrites the staged bundle in place.

set -euo pipefail

# --- locate ourselves -------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_DIR="${1:-$SCRIPT_DIR/dist}"

# Each plugin: "<source-dir>:<plugin_type>". id + version are read from manifest.
PLUGINS=(
  "anseo-trend-analytics:analytics"
  "anseo-example-provider:provider"
)

# WASM target for provider/cdylib plugins. wasm32-wasip1 is the current Rust
# WASI target triple (formerly wasm32-wasi).
WASM_TARGET="wasm32-wasip1"

heartbeat() { printf '[build.sh] %s\n' "$*" >&2; }

# Minimal YAML scalar reader: `read_field <file> <key>`. Plugin manifests are
# flat top-level `key: value` lines, so a grep is sufficient (no YAML dep).
read_field() {
  local file="$1" key="$2" val
  val="$(grep -E "^${key}:[[:space:]]*" "$file" | head -n1 | sed -E "s/^${key}:[[:space:]]*//")"
  # strip surrounding quotes and trailing whitespace/comment
  val="${val%%#*}"
  val="$(printf '%s' "$val" | sed -E 's/[[:space:]]+$//; s/^"(.*)"$/\1/; s/^'\''(.*)'\''$/\1/')"
  printf '%s' "$val"
}

# Minimal TOML scalar reader scoped to a section:
# `toml_section_field <file> <section> <key>` returns the value of `key = "..."`
# within the first `[section]` table. Plugin Cargo.toml files are flat enough that
# this awk pass (no TOML dep) suffices for reading [package].name / [lib].name.
toml_section_field() {
  local file="$1" section="$2" key="$3"
  awk -v section="$section" -v key="$key" '
    /^[[:space:]]*\[/ { in_sec = ($0 ~ "^[[:space:]]*\\[" section "\\][[:space:]]*$"); next }
    in_sec {
      line = $0
      sub(/#.*$/, "", line)                       # strip comments
      if (line ~ "^[[:space:]]*" key "[[:space:]]*=") {
        sub("^[[:space:]]*" key "[[:space:]]*=[[:space:]]*", "", line)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
        gsub(/^"|"$/, "", line)                   # strip surrounding quotes
        gsub(/^'\''|'\''$/, "", line)
        print line
        exit
      }
    }
  ' "$file"
}

# Confirm a rustup target is installed; if not, emit a clear instruction and skip
# (rather than fail the whole run) so the native plugin still materializes.
have_wasm_target() {
  # Only confirm via rustup when it's on PATH. If rustup is unavailable we can't
  # verify the target is present; the provider build below tolerates failure and
  # skips with a documented fix, so report "not available" here to avoid a hard
  # toolchain error aborting the run.
  command -v rustup >/dev/null 2>&1 || return 1
  rustup target list --installed 2>/dev/null | grep -qx "$WASM_TARGET"
}

mkdir -p "$OUT_DIR"
heartbeat "output dir: $OUT_DIR"

PRODUCED=()
SKIPPED=()

for entry in "${PLUGINS[@]}"; do
  dir="${entry%%:*}"
  ptype="${entry##*:}"
  src_dir="$SCRIPT_DIR/$dir"
  manifest="$src_dir/manifest.yaml"

  [ -f "$manifest" ] || { echo "ERROR: missing manifest: $manifest" >&2; exit 1; }

  name="$(read_field "$manifest" name)"
  version="$(read_field "$manifest" version)"
  [ -n "$name" ]    || { echo "ERROR: $dir: could not read 'name' from manifest" >&2; exit 1; }
  [ -n "$version" ] || { echo "ERROR: $dir: could not read 'version' from manifest" >&2; exit 1; }

  id="anseo/$name"                       # namespaced registry id
  dest="$OUT_DIR/plugins/$id/$version"   # plugins/anseo/<name>/<version>/
  heartbeat "==> $id @ $version  (kind=$ptype)"

  case "$ptype" in
    analytics)
      # Supported native subprocess kind. Build the release [[bin]] in the
      # plugin's own standalone workspace (its Cargo.toml carries an empty
      # [workspace] table, so this never triggers a host-workspace build).
      heartbeat "$dir: cargo build --release (native subprocess)"
      ( cd "$src_dir" && cargo build --release --bin "$name" )
      artifact="$src_dir/target/release/$name"
      ;;
    provider)
      # WASM cdylib kind. Build to the WASI target → a real .wasm artifact.
      #
      # IMPORTANT: Rust's wasm targets do NOT apply the native `lib` prefix to
      # cdylib output. `rustc --print file-names --crate-type cdylib --target
      # wasm32-wasip1 --crate-name <crate>` emits `<crate>.wasm` (crate name with
      # '-' → '_', no `lib`, `.wasm` ext) — unlike a native cdylib which gets
      # `lib<crate>.so/.dylib/.dll`. So the provider artifact is `<crate>.wasm`.
      #
      # Derive the crate name from the plugin's own Cargo.toml ([lib] name if set,
      # else [package] name) rather than the manifest, since the on-disk artifact
      # filename is governed by Cargo/rustc, not the registry manifest.
      cargo_toml="$src_dir/Cargo.toml"
      [ -f "$cargo_toml" ] || { echo "ERROR: $dir: missing Cargo.toml: $cargo_toml" >&2; exit 1; }
      crate_name="$(toml_section_field "$cargo_toml" lib name)"
      [ -n "$crate_name" ] || crate_name="$(toml_section_field "$cargo_toml" package name)"
      [ -n "$crate_name" ] || { echo "ERROR: $dir: could not read crate name from $cargo_toml" >&2; exit 1; }
      crate_name_underscored="${crate_name//-/_}"
      # Prefer rustc's authoritative filename when rustc is on PATH; fall back to
      # the documented construction otherwise.
      wasm_filename=""
      if command -v rustc >/dev/null 2>&1; then
        wasm_filename="$(printf '' | rustc --print file-names \
          --crate-type cdylib --target "$WASM_TARGET" \
          --crate-name "$crate_name_underscored" - 2>/dev/null | head -n1 || true)"
      fi
      [ -n "$wasm_filename" ] || wasm_filename="$crate_name_underscored.wasm"
      artifact="$src_dir/target/$WASM_TARGET/release/$wasm_filename"
      if ! have_wasm_target; then
        heartbeat "$dir: SKIP — wasm target '$WASM_TARGET' is not installed."
        heartbeat "$dir: install it and re-run to materialize this bundle:"
        heartbeat "    rustup target add $WASM_TARGET && plugins/build.sh"
        SKIPPED+=("$id @ $version (needs rustup target $WASM_TARGET)")
        continue
      fi
      heartbeat "$dir: cargo build --release --target $WASM_TARGET (wasm cdylib)"
      # Don't let a toolchain miss abort the whole run: the native plugin must
      # still materialize. On failure, emit the documented fix and skip.
      if ! ( cd "$src_dir" && cargo build --release --target "$WASM_TARGET" ); then
        heartbeat "$dir: SKIP — wasm build failed (target '$WASM_TARGET' missing?)."
        heartbeat "$dir: install the target and re-run:"
        heartbeat "    rustup target add $WASM_TARGET && plugins/build.sh"
        SKIPPED+=("$id @ $version (wasm build failed; needs $WASM_TARGET)")
        continue
      fi
      ;;
    *)
      echo "ERROR: $dir: unknown plugin type '$ptype'" >&2
      exit 1
      ;;
  esac

  [ -f "$artifact" ] || { echo "ERROR: $dir: expected artifact not found: $artifact" >&2; exit 1; }

  # Materialize the install layout: manifest + entrypoint.wasm under <id>/<ver>/.
  mkdir -p "$dest"
  cp "$manifest" "$dest/manifest.yaml"
  cp "$artifact" "$dest/entrypoint.wasm"

  PRODUCED+=("$dest/manifest.yaml" "$dest/entrypoint.wasm")
  heartbeat "$dir: staged → $dest/"
done

echo
echo "Materialized first-party plugin bundles:"
for p in "${PRODUCED[@]}"; do
  printf '  %s\n' "$p"
done
if [ "${#SKIPPED[@]}" -gt 0 ]; then
  echo
  echo "Skipped (toolchain not available — install and re-run):"
  for s in "${SKIPPED[@]}"; do
    printf '  %s\n' "$s"
  done
fi
echo
echo "Next: sign each bundle (signature.bin + claim.toml) with the 41.4 signing"
echo "pipeline (SHA-256(manifest.yaml || entrypoint.wasm) → Ed25519; see"
echo "crates/plugin-host/src/signing.rs), then publish to the registry."
