#!/usr/bin/env bash
# Packt das Release-Binary in ein macOS-App-Bundle.
set -euo pipefail
cd "$(dirname "$0")"

APP_NAME="FileRune"
# Eigene Bundle-ID: die SwiftUI-Fassung benutzt com.lom.flyfiles, und beide
# sollen getrennte Einstellungen und TCC-Berechtigungen behalten.
BUNDLE_ID="com.lom.filerune-rs"
VERSION="0.1.2"
BUNDLE="build/${APP_NAME}.app"

# In der CI kommt das (dann universelle) Binary fertig herein; lokal bauen wir.
BIN="${FILERUNE_BIN:-target/release/filerune}"
if [[ -z "${FILERUNE_BIN:-}" ]]; then
  echo "→ cargo build --release"
  cargo build --release
fi

rm -rf "${BUNDLE}"
mkdir -p "${BUNDLE}/Contents/MacOS" "${BUNDLE}/Contents/Resources"
cp "${BIN}" "${BUNDLE}/Contents/MacOS/${APP_NAME}"

# Das Icon liegt im Projekt (aus der SwiftUI-Fassung übernommen), damit das
# Bundle ohne Nachbarordner baut. Fehlt es, brechen wir ab statt still eine App
# ohne Icon auszuliefern.
ICON_SRC="Icon.icns"
if [[ ! -f "${ICON_SRC}" ]]; then
  echo "✗ ${ICON_SRC} fehlt — ohne Icon wird nicht gebaut." >&2
  exit 1
fi
cp "${ICON_SRC}" "${BUNDLE}/Contents/Resources/Icon.icns"

cat > "${BUNDLE}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key><string>${APP_NAME}</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleExecutable</key><string>${APP_NAME}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>Icon</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Ad-hoc signieren, sonst verweigert Gatekeeper den Start des kopierten Binaries.
codesign --force --deep --sign - "${BUNDLE}" 2>/dev/null || true

# Finder und Dock halten Icons im Cache; das Anfassen des Bundles stößt eine
# Neubewertung an, sonst klebt bei einem Rebuild das alte Icon.
touch "${BUNDLE}"

echo "✓ ${BUNDLE}  (v${VERSION})"
echo "  Starten mit:  open ${BUNDLE}"
