#!/usr/bin/env bash
# Trägt das Windows-Update-Paket in eine vorhandene latest.json (aus dem
# macOS-Lauf von tauri-action) ein — so, wie tauri-action es selbst täte:
#   platforms["windows-x86_64"] und platforms["windows-x86_64-nsis"]
#   = { signature: <Inhalt der .sig>, url: <Download-Adresse des -setup.exe> }
#
# Aufruf: scripts/windows-in-latest-json.sh <latest.json> <ausgabe.json> <url> <datei.sig> <version>
# Bricht ab, wenn die Version in latest.json nicht <version> ist (sonst
# bekäme eine alte macOS-Version ein neues Windows-Paket oder umgekehrt).
set -euo pipefail
EIN="$1"; AUS="$2"; URL="$3"; SIG="$4"; VERSION="$5"

test -s "$SIG" || { echo "::error::Signatur $SIG fehlt oder ist leer"; exit 1; }
V_JSON=$(jq -r '.version' "$EIN")
[ "$V_JSON" = "$VERSION" ] || { echo "::error::latest.json hat Version $V_JSON, erwartet $VERSION"; exit 1; }
case "$URL" in https://github.com/*-setup.exe) ;; *) echo "::error::Unerwartete Adresse $URL"; exit 1;; esac

jq --arg url "$URL" --rawfile sig "$SIG" '
  ($sig | sub("\\s+$"; "")) as $s
  | .platforms["windows-x86_64"] = { signature: $s, url: $url }
  | .platforms["windows-x86_64-nsis"] = { signature: $s, url: $url }
' "$EIN" > "$AUS"

# Gegenprobe: macOS-Einträge unverändert, Windows-Einträge vollständig.
for k in darwin-aarch64 darwin-x86_64; do
  [ "$(jq -c ".platforms[\"$k\"]" "$EIN")" = "$(jq -c ".platforms[\"$k\"]" "$AUS")" ] \
    || { echo "::error::$k wurde verändert"; exit 1; }
done
jq -e '.platforms["windows-x86_64"] | (.url | length > 0) and (.signature | length > 0)' "$AUS" > /dev/null
jq '.platforms | keys' "$AUS"
