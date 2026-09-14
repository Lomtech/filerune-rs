#!/usr/bin/env bash
# Packt das Release-Binary in ein macOS-App-Bundle.
set -euo pipefail
cd "$(dirname "$0")"

# --install kopiert zusätzlich nach /Applications. Ohne das kennt Spotlight die
# App nicht: ein Build-Verzeichnis ist kein Ort, an dem macOS nach Programmen
# sucht — und ein Projektordner ist oft gar nicht indexiert.
INSTALL=0
for a in "$@"; do
  [[ "$a" == "--install" ]] && INSTALL=1
done

APP_NAME="FileRune"
# Eigene Bundle-ID: die SwiftUI-Fassung benutzt com.lom.flyfiles, und beide
# sollen getrennte Einstellungen und TCC-Berechtigungen behalten.
BUNDLE_ID="com.lom.filerune-rs"
VERSION="0.1.3"
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

if [[ "${INSTALL}" -eq 1 ]]; then
  # /Applications gehört der Gruppe admin und ist dort ohne sudo beschreibbar;
  # sonst weicht die Installation auf ~/Applications aus, das macOS ebenso kennt.
  if [[ -w /Applications ]]; then
    DEST="/Applications"
  else
    DEST="${HOME}/Applications"
    mkdir -p "${DEST}"
  fi
  TARGET="${DEST}/${APP_NAME}.app"

  if pgrep -f "${TARGET}/Contents/MacOS/" > /dev/null 2>&1; then
    echo "✗ ${APP_NAME} läuft gerade aus ${DEST} — erst beenden, dann erneut." >&2
    exit 1
  fi

  rm -rf "${TARGET}"
  cp -R "${BUNDLE}" "${TARGET}"
  # Bei LaunchServices anmelden und Spotlight anstoßen, sonst taucht die App
  # erst nach dem nächsten Indexlauf in der Suche auf.
  LSREG=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
  [[ -x "${LSREG}" ]] && "${LSREG}" -f "${TARGET}" 2>/dev/null || true
  mdimport "${TARGET}" 2>/dev/null || true

  echo "✓ installiert nach ${TARGET}"
  echo "  Findet sich jetzt über Spotlight (⌘Leertaste) und im Launchpad."
  echo "  Deinstallieren: rm -rf \"${TARGET}\""
else
  echo "  Starten mit:   open ${BUNDLE}"
  echo "  Installieren:  ./bundle.sh --install"
fi
