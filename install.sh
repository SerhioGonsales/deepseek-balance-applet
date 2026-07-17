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
ICON_BASE="${HOME}/.local/share/icons/hicolor"
DESKTOP_DIR="${HOME}/.local/share/applications"
APP_ID="com.github.serhio.DeepSeekBalance"
BIN_PATH="${BIN_DIR}/deepseek-balance-applet"

echo "Downloading deepseek-balance-applet..."

if [ "$VERSION" = "latest" ]; then
    URL="https://github.com/${REPO}/releases/latest/download/deepseek-balance-applet"
else
    URL="https://github.com/${REPO}/releases/download/${VERSION}/deepseek-balance-applet"
fi

mkdir -p "$BIN_DIR"
curl -sSLf "$URL" -o "$BIN_PATH"
chmod +x "$BIN_PATH"

echo "Installing desktop entry and icons..."

# Desktop file — patch Exec with absolute path
curl -sSLf "https://raw.githubusercontent.com/${REPO}/main/resources/app.desktop" \
    -o /tmp/${APP_ID}.desktop
mkdir -p "$DESKTOP_DIR"
sed "s|^Exec=.*|Exec=${BIN_PATH} %F|" /tmp/${APP_ID}.desktop \
    > "$DESKTOP_DIR/${APP_ID}.desktop"

# SVG icon (scalable)
curl -sSLf "https://raw.githubusercontent.com/${REPO}/main/resources/icon.svg" \
    -o /tmp/${APP_ID}.svg
mkdir -p "${ICON_BASE}/scalable/apps"
cp /tmp/${APP_ID}.svg "${ICON_BASE}/scalable/apps/${APP_ID}.svg"

# PNG icon (for applet list)
ICON_PNG_DIR="${ICON_BASE}/128x128/apps"
mkdir -p "$ICON_PNG_DIR"
if curl -sSLf "https://raw.githubusercontent.com/${REPO}/main/resources/deepseek-48.png" \
    -o /tmp/${APP_ID}.png 2>/dev/null; then
    cp /tmp/${APP_ID}.png "${ICON_PNG_DIR}/${APP_ID}.png"
else
    echo "Note: no PNG icon found (non-critical)"
fi

# Update icon cache
if command -v update-icon-caches &>/dev/null; then
    update-icon-caches "$ICON_BASE" 2>/dev/null || true
elif command -v gtk-update-icon-cache &>/dev/null; then
    gtk-update-icon-cache "$ICON_BASE" 2>/dev/null || true
fi

# Update desktop database
if command -v update-desktop-database &>/dev/null; then
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi

echo
echo "Done! Add via Settings > Desktop > Panel > Applets → DeepSeek Balance."
echo "If it isn't listed, re-login or run: systemctl restart --user cosmic-panel"
