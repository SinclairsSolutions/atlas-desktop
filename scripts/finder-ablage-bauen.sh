#!/usr/bin/env bash
# Baut die File-Provider-Erweiterung „Ablage im Finder“ (finder-ablage/) als
# universelles .appex — ohne Xcode-Projekt, nur mit swiftc (Command Line Tools
# genügen).
#
#   bash scripts/finder-ablage-bauen.sh <Zielordner> [Signier-Identität]
#
# Ohne Identität wird ad hoc signiert (nur zum Prüfen des Baus; laden lässt
# sich die Erweiterung nur mit der Developer-ID-Signatur aus dem Workflow).
set -euo pipefail

HIER="$(cd "$(dirname "$0")/.." && pwd)"
QUELLE="$HIER/finder-ablage"
ZIEL="${1:?Zielordner fehlt}"
IDENT="${2:-}"
V=$(cd "$HIER" && (bun -e 'console.log(require("./package.json").version)' 2>/dev/null || node -p 'require("./package.json").version'))

ARBEIT="$(mktemp -d)"
trap 'rm -rf "$ARBEIT"' EXIT
SDK="$(xcrun --sdk macosx --show-sdk-path)"

for ARCH in arm64 x86_64; do
  xcrun --sdk macosx swiftc -O -swift-version 5 -parse-as-library -application-extension \
    -module-name AtlasAblage -target "$ARCH-apple-macos12.0" -sdk "$SDK" \
    -framework FileProvider -framework UniformTypeIdentifiers -framework Security \
    -Xlinker -application_extension -Xlinker -e -Xlinker _NSExtensionMain \
    "$QUELLE"/Quellen/*.swift -o "$ARBEIT/AtlasAblage-$ARCH"
done

APPEX="$ZIEL/AtlasAblage.appex"
rm -rf "$APPEX"
mkdir -p "$APPEX/Contents/MacOS"
lipo -create "$ARBEIT/AtlasAblage-arm64" "$ARBEIT/AtlasAblage-x86_64" -output "$APPEX/Contents/MacOS/AtlasAblage"
cp "$QUELLE/Info.plist" "$APPEX/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $V" -c "Set :CFBundleVersion $V" "$APPEX/Contents/Info.plist"
plutil -lint "$APPEX/Contents/Info.plist" >/dev/null

if [ -n "$IDENT" ]; then
  codesign --force --options runtime --timestamp --entitlements "$QUELLE/Ablage.entitlements" --sign "$IDENT" "$APPEX"
else
  codesign --force --options runtime --entitlements "$QUELLE/Ablage.entitlements" --sign - "$APPEX"
fi
codesign --verify --strict --verbose=2 "$APPEX"
echo "Gebaut: $APPEX ($(lipo -archs "$APPEX/Contents/MacOS/AtlasAblage"), Version $V)"
