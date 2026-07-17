#!/usr/bin/env bash
# Uninstalls the DeepSeek Balance COSMIC applet
# Usage:
#   curl -sSL https://raw.githubusercontent.com/SerhioGonsales/deepseek-balance-applet/main/uninstall.sh | bash
set -euo pipefail

APP_ID="com.github.serhio.DeepSeekBalance"

echo "Removing DeepSeek Balance..."

rm -f "${HOME}/.local/bin/deepseek-balance-applet"
rm -f "${HOME}/.local/share/applications/${APP_ID}.desktop"
rm -f "${HOME}/.local/share/icons/hicolor/scalable/apps/${APP_ID}.svg"
rm -f "${HOME}/.local/share/icons/hicolor/128x128/apps/${APP_ID}.png"
rm -rf "${HOME}/.config/cosmic/${APP_ID}"

echo "Done. Re-login to remove from panel."
