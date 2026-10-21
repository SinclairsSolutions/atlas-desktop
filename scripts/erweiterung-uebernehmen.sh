#!/usr/bin/env bash
# Übernimmt die Browser-Erweiterung „Atlas Passwort-Safe“ aus dem Atlas-Repo
# nach safari-erweiterung/. Der Release-Workflow (bauen.yml) macht daraus die
# Safari-Erweiterung, die in der Mac-App steckt (Contents/PlugIns).
#
#   bash scripts/erweiterung-uebernehmen.sh [Pfad zum Atlas-Repo]
#
# Standard: ../sinclair-atlas neben diesem Repo. Baut dort mit
# `bun browser-extension/build.ts` (gegen sinclair-atlas.com) und kopiert genau
# die Dateien, die auch ins Store-Paket gehören. Danach committen — der Code
# der Erweiterung ist ohnehin öffentlich (er läuft im Browser).
set -euo pipefail

HIER="$(cd "$(dirname "$0")/.." && pwd)"
ATLAS="${1:-$HIER/../sinclair-atlas}"
ATLAS="$(cd "$ATLAS" && pwd)"
QUELLE="$ATLAS/browser-extension"
ZIEL="$HIER/safari-erweiterung"

[ -f "$QUELLE/manifest.json" ] || { echo "Keine Erweiterung unter $QUELLE" >&2; exit 1; }
if [ -n "${ATLAS_URL:-}" ]; then
  echo "ATLAS_URL ist gesetzt ($ATLAS_URL) — die ausgelieferte Erweiterung muss gegen sinclair-atlas.com gebaut sein." >&2
  exit 1
fi

(cd "$ATLAS" && bun browser-extension/build.ts)

rm -rf "$ZIEL"
mkdir -p "$ZIEL"
cp "$QUELLE/manifest.json" "$QUELLE/popup.html" "$QUELLE/popup.css" "$QUELLE/icon-mask.png" "$ZIEL/"
cp -R "$QUELLE/dist" "$ZIEL/dist"
cp -R "$QUELLE/icons" "$ZIEL/icons"
find "$ZIEL" -name .DS_Store -delete

# Herkunft festhalten (welcher Atlas-Stand steckt drin?).
STAND="$(git -C "$ATLAS" rev-parse --short HEAD 2>/dev/null || echo unbekannt)"
if ! git -C "$ATLAS" diff --quiet -- browser-extension 2>/dev/null; then STAND="$STAND (mit lokalen Änderungen)"; fi
VERSION="$(sed -nE 's/.*"version": *"([^"]+)".*/\1/p' "$ZIEL/manifest.json" | head -1)"
printf 'Atlas Passwort-Safe %s\nAus sinclair-atlas %s, übernommen %s\n' "$VERSION" "$STAND" "$(date +%Y-%m-%d)" > "$ZIEL/HERKUNFT.txt"

echo "Übernommen → $ZIEL (Version $VERSION, Atlas $STAND)"
