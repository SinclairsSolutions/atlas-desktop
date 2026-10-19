#!/usr/bin/env bash
# Erzeugt den Updater-Schlüssel und hinterlegt ihn als GitHub-Secrets.
# In einem normalen Terminal-Fenster ausführen (fragt das Passwort verdeckt ab):
#   bash scripts/updater-schluessel.sh
# Gibt nur den öffentlichen Schlüssel aus — der private bleibt in ~/.tauri und GitHub.
set -euo pipefail
cd "$(dirname "$0")/.."

GH="${GH:-$(command -v gh || echo ~/.local/bin/gh)}"
REPO="SinclairsSolutions/atlas-desktop"
KEY="$HOME/.tauri/atlas-desktop.key"

if [ -e "$KEY" ]; then
  echo "Es gibt schon $KEY — nicht überschrieben. Zum Neuanlegen erst sichern/löschen." >&2
  exit 1
fi
mkdir -p "$HOME/.tauri"

read -r -s -p "Passwort für den Updater-Schlüssel: " PW; echo
read -r -s -p "Noch einmal: " PW2; echo
[ "$PW" = "$PW2" ] || { echo "Passwörter stimmen nicht überein." >&2; exit 1; }
[ -n "$PW" ] || { echo "Leeres Passwort nicht erlaubt." >&2; exit 1; }

# `generate` liest das Passwort nur als Argument (nicht aus der Umgebung) — es
# steht damit kurz in der Prozessliste dieses Macs, aber nicht in der History.
bunx tauri signer generate --ci -p "$PW" -w "$KEY" >/dev/null
chmod 600 "$KEY"

# Probe: lässt sich mit dem Passwort signieren?
PROBE="$(mktemp)"; echo probe > "$PROBE"
if ! TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$PW" bunx tauri signer sign -f "$KEY" "$PROBE" >/dev/null 2>&1; then
  rm -f "$PROBE" "$PROBE.sig"; echo "Probe-Signatur fehlgeschlagen — nichts hochgeladen." >&2; exit 1
fi
rm -f "$PROBE" "$PROBE.sig"

"$GH" secret set TAURI_SIGNING_PRIVATE_KEY --repo "$REPO" < "$KEY"
printf '%s' "$PW" | "$GH" secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo "$REPO"
unset PW PW2

echo
echo "Fertig. Privater Schlüssel: $KEY (bitte im Passwort-Safe sichern)."
echo "Öffentlicher Schlüssel (für tauri.conf.json):"
cat "$KEY.pub"; echo
