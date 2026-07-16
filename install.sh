#!/usr/bin/env bash
# Installs the DeepSeek Balance COSMIC applet for the current user
# (~/.local — no sudo needed).
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/serhio/deepseek-balance-applet/main/install.sh | bash
set -euo pipefail

REPO_URL="https://github.com/SerhioGonsales/deepseek-balance-applet.git"
BRANCH="main"

command -v cargo >/dev/null 2>&1 || {
    echo "error: cargo/rust not found. Install it from https://rustup.rs first." >&2
    exit 1
}
command -v just >/dev/null 2>&1 || {
    echo "error: 'just' not found. Install with: cargo install just" >&2
    exit 1
}
command -v git >/dev/null 2>&1 || {
    echo "error: git not found." >&2
    exit 1
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

echo "Cloning ${REPO_URL} (${BRANCH})..."
git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$tmp_dir/src"

echo "Building release binary..."
cd "$tmp_dir/src"
just install-user

echo
echo "Done! Open Settings > Desktop > Panel > Applets and add 'DeepSeek Balance'."
echo "If it isn't listed, add ~/.local/bin to your PATH and re-login."
