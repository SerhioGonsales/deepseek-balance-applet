#!/usr/bin/env bash
# Installs the DeepSeek Balance COSMIC applet for the current user
# (~/.local — no sudo needed).
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/SerhioGonsales/deepseek-balance-applet/main/install.sh | bash
set -euo pipefail

REPO="SerhioGonsales/deepseek-balance-applet"
VERSION="${1:-latest}"
BIN_DIR="${HOME}/.local/bin"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"
DESKTOP_DIR="${HOME}/.local/share/applications"
APP_ID="com.github.serhio.DeepSeekBalance"

echo "Downloading deepseek-balance-applet..."

if [ "$VERSION" = "latest" ]; then
    URL="https://github.com/${REPO}/releases/latest/download/deepseek-balance-applet"
else
    URL="https://github.com/${REPO}/releases/download/${VERSION}/deepseek-balance-applet"
fi

mkdir -p "$BIN_DIR"
curl -sSLf "$URL" -o "$BIN_DIR/deepseek-balance-applet"
chmod +x "$BIN_DIR/deepseek-balance-applet"

echo "Installing desktop entry and icon..."

# Download icon and desktop file from the repo
curl -sSLf "https://raw.githubusercontent.com/${REPO}/main/resources/icon.svg" \
    -o /tmp/cosmic-applet-icon.svg
curl -sSLf "https://raw.githubusercontent.com/${REPO}/main/resources/app.desktop" \
    -o /tmp/cosmic-applet.desktop

mkdir -p "$ICON_DIR" "$DESKTOP_DIR"
install -Dm0644 /tmp/cosmic-applet-icon.svg "$ICON_DIR/${APP_ID}.svg"
install -Dm0644 /tmp/cosmic-applet.desktop "$DESKTOP_DIR/${APP_ID}.desktop"

echo
echo "Done! Open Settings > Desktop > Panel > Applets and add 'DeepSeek Balance'."
echo "If it isn't listed, make sure ~/.local/bin is on your PATH and re-login."
