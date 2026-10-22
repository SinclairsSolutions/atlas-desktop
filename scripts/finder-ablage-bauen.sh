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

# Seitenleisten-Symbol: eigenes SF Symbol (Symbole.xcassets) → Contents/Resources/Assets.car.
# actool gibt es nur mit Xcode; ohne Xcode (nur Command Line Tools) wird lokal ohne
# Symbol gebaut — der Finder zeigt dann kein Symbol. Im Workflow (CI) ist es Pflicht.
if xcrun --find actool >/dev/null 2>&1; then
  mkdir -p "$APPEX/Contents/Resources"
  xcrun actool "$QUELLE/Symbole.xcassets" --compile "$APPEX/Contents/Resources" \
    --output-format human-readable-text --notices --warnings --errors \
    --output-partial-info-plist "$ARBEIT/symbol-partial.plist" \
    --enable-on-demand-resources NO --development-region de \
    --target-device mac --platform macosx --minimum-deployment-target 12.0
  test -s "$APPEX/Contents/Resources/Assets.car" || { echo "actool hat keine Assets.car erzeugt" >&2; exit 1; }
  SYM=$(/usr/libexec/PlistBuddy -c "Print :CFBundleIcons:CFBundlePrimaryIcon:CFBundleSymbolName" "$APPEX/Contents/Info.plist")
  xcrun --sdk macosx assetutil --info "$APPEX/Contents/Resources/Assets.car" > "$ARBEIT/assets.json"
  grep -F "\"Name\" : \"$SYM\"" "$ARBEIT/assets.json" >/dev/null \
    || { echo "Symbol $SYM fehlt in Assets.car der Erweiterung" >&2; cat "$ARBEIT/assets.json" >&2; exit 1; }
elif [ -n "${CI:-}" ]; then
  echo "actool fehlt (Xcode nötig) — ohne Assets.car kein Seitenleisten-Symbol" >&2; exit 1
else
  echo "Hinweis: actool fehlt (kein Xcode) — Erweiterung ohne Seitenleisten-Symbol gebaut" >&2
fi

if [ -n "$IDENT" ]; then
  codesign --force --options runtime --timestamp --entitlements "$QUELLE/Ablage.entitlements" --sign "$IDENT" "$APPEX"
else
  codesign --force --options runtime --entitlements "$QUELLE/Ablage.entitlements" --sign - "$APPEX"
fi
codesign --verify --strict --verbose=2 "$APPEX"
echo "Gebaut: $APPEX ($(lipo -archs "$APPEX/Contents/MacOS/AtlasAblage"), Version $V)"
