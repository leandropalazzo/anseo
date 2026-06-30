#!/bin/sh
# anseo installer — https://anseo.ai/install
# Source: https://github.com/leandropalazzo/anseo/blob/main/scripts/install.sh
#
# Usage:
#   curl -fsSL https://anseo.ai/install | sh
#
# What it does:
#   1. Detects OS + arch → maps to a GitHub release target triple
#   2. Fetches the latest release tag from the GitHub API
#   3. Downloads anseo-{version}-{target}.tar.gz from GitHub Releases
#   4. Extracts the anseo binary to /usr/local/bin/anseo
#   5. Creates an ogeo symlink for backward compatibility
#
# To inspect before running:
#   curl -fsSL https://raw.githubusercontent.com/leandropalazzo/anseo/main/scripts/install.sh | less
#
# Environment overrides:
#   ANSEO_VERSION     — install a specific version (e.g. v0.5.0); default: latest
#   ANSEO_INSTALL_DIR — install location; default: /usr/local/bin
set -eu

REPO="leandropalazzo/anseo"

# Default install dir: /usr/local/bin if actually writable, else ~/.local/bin (no sudo needed).
# Use a real write probe instead of [ -w ] which misreports on macOS with ACLs.
if [ -z "${ANSEO_INSTALL_DIR:-}" ]; then
  if _probe="$(mktemp /usr/local/bin/.anseo-probe.XXXXXX 2>/dev/null)" && rm -f "$_probe"; then
    INSTALL_DIR="/usr/local/bin"
  else
    INSTALL_DIR="${HOME}/.local/bin"
  fi
else
  INSTALL_DIR="$ANSEO_INSTALL_DIR"
fi

# ---------------------------------------------------------------------------
# 1. Detect platform
# ---------------------------------------------------------------------------
detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux)
      case "$arch" in
        x86_64)         echo "x86_64-unknown-linux-musl" ;;
        aarch64|arm64)  echo "aarch64-unknown-linux-musl" ;;
        *)
          printf 'Unsupported Linux architecture: %s\n' "$arch" >&2
          exit 1
          ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        x86_64)  echo "x86_64-apple-darwin" ;;
        arm64)   echo "aarch64-apple-darwin" ;;
        *)
          printf 'Unsupported macOS architecture: %s\n' "$arch" >&2
          exit 1
          ;;
      esac
      ;;
    *)
      printf 'Unsupported OS: %s\n' "$os" >&2
      exit 1
      ;;
  esac
}

# ---------------------------------------------------------------------------
# 2. Resolve version
# ---------------------------------------------------------------------------
resolve_version() {
  if [ -n "${ANSEO_VERSION:-}" ]; then
    echo "$ANSEO_VERSION"
    return
  fi
  api_url="https://api.github.com/repos/${REPO}/releases/latest"
  if command -v curl >/dev/null 2>&1; then
    tag="$(curl -fsSL "$api_url" | grep '"tag_name"' | head -1 \
      | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')"
  elif command -v wget >/dev/null 2>&1; then
    tag="$(wget -qO- "$api_url" | grep '"tag_name"' | head -1 \
      | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')"
  else
    printf 'curl or wget is required to install anseo.\n' >&2
    exit 1
  fi
  if [ -z "$tag" ]; then
    printf 'Could not resolve the latest anseo release from GitHub.\n' >&2
    printf 'Set ANSEO_VERSION to install a specific version (e.g. ANSEO_VERSION=v0.5.0).\n' >&2
    exit 1
  fi
  echo "$tag"
}

# ---------------------------------------------------------------------------
# 3. Download + extract + install
# ---------------------------------------------------------------------------
download_and_install() {
  target="$1"
  version="$2"
  bare="${version#v}"
  tarball="anseo-${bare}-${target}.tar.gz"
  url="https://github.com/${REPO}/releases/download/${version}/${tarball}"

  tmpdir="$(mktemp -d)"
  # shellcheck disable=SC2064
  trap "rm -rf '$tmpdir'" EXIT

  printf 'Downloading %s ...\n' "$tarball"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --progress-bar -o "${tmpdir}/${tarball}" "$url"
  else
    wget -q --show-progress -O "${tmpdir}/${tarball}" "$url"
  fi

  tar -xzf "${tmpdir}/${tarball}" -C "$tmpdir"

  if [ ! -f "${tmpdir}/anseo" ]; then
    printf 'Unexpected tarball layout: anseo binary not found after extraction.\n' >&2
    exit 1
  fi

  install -d "$INSTALL_DIR"
  install -m 755 "${tmpdir}/anseo" "${INSTALL_DIR}/anseo"

  ln -sf "${INSTALL_DIR}/anseo" "${INSTALL_DIR}/ogeo"

  printf '\nInstalled anseo %s → %s/anseo\n' "$version" "$INSTALL_DIR"
  printf 'Symlink: %s/ogeo → anseo\n' "$INSTALL_DIR"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  target="$(detect_target)"
  version="$(resolve_version)"

  printf 'anseo installer\n'
  printf '  version : %s\n' "$version"
  printf '  target  : %s\n' "$target"
  printf '  install : %s\n' "$INSTALL_DIR"
  printf '\n'

  download_and_install "$target" "$version"

  printf '\nNext step: anseo init\n'
  printf 'Docs     : https://anseo.ai/docs\n'

  # Remind the user to add ~/.local/bin to PATH if it is not already there.
  case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
      printf '\nNote: add %s to your PATH:\n' "$INSTALL_DIR"
      printf '  echo '\''export PATH="%s:$PATH"'\'' >> ~/.zshrc && source ~/.zshrc\n' "$INSTALL_DIR"
      ;;
  esac
}

main "$@"
