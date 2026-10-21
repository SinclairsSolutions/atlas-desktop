# Atlas Desktop

Desktop-Hülle für **Sinclair Atlas** auf Basis von [Tauri 2](https://tauri.app).
Die App lädt die Live-Seite `https://sinclair-atlas.com` in einem eigenen Fenster —
sie baut Atlas nicht nach und enthält keinen Atlas-Code. Jede Änderung an Atlas
ist nach dem Deploy sofort auch in der App.

Ziel ist zuerst macOS (Universal: Apple Silicon + Intel). Windows wird im selben
Projekt mitgebaut, aber noch nicht ausgeliefert. Offene Schritte: [`TODO.md`](TODO.md).

## Was die Hülle macht

| Bereich | Umsetzung |
| --- | --- |
| Fenster | Größe und Lage gemerkt (`tauri-plugin-window-state`), nur eine Instanz (`single-instance`). macOS: Schließen blendet aus, Dock-Klick holt zurück, ⌘Q beendet. |
| Menü | `src-tauri/src/menue.rs` — Kürzel über `CmdOrCtrl` (⌘ bzw. Strg); Zurück/Vor ⌘[ ⌘] bzw. Alt+←/→, Vollbild ⌃⌘F bzw. F11, Neu laden ⌘R bzw. Strg+R/F5. Sprache folgt dem System (DE/EN). |
| Nur Atlas in der App | `src-tauri/src/fenster.rs` + `bruecke.js`. Fremde Links, `location.href` auf fremde Seiten und `window.open` fremder Adressen gehen in den Standardbrowser. Eingebettete Vorschauen (iframes) bleiben in Atlas. |
| Anmeldungen | Google, Microsoft, Apple, Stripe, Slack, PayPal, SumUp, DATEV, finAPI (Bank-Login) laufen immer im System-Browser. Kehrt man danach in die App zurück, lädt sie neu und zeigt die neue Verbindung. Rücksprung per Deep-Link `sinclair-atlas://open/<pfad>` ist vorbereitet. |
| Mitteilungen | `Notification` der Seite → System-Mitteilungen; `navigator.setAppBadge(n)` → Zahl am Dock (Windows: Overlay-Symbol). |
| Downloads/Uploads | Downloads (auch Blob-Links) landen im Download-Ordner, danach eine Mitteilung. Datei-Auswahl und Drag & Drop funktionieren wie im Browser. |
| Drucken | `window.print()` öffnet unter macOS den nativen Druckdialog. |
| Offline | Mitgelieferte Seite `ui/index.html` mit „Erneut versuchen“, automatische Wiederverbindung alle 15 s. Hinweisleiste, wenn im laufenden Betrieb das Netz wegfällt. |
| Updates | `tauri-plugin-updater` gegen GitHub Releases — 20 s nach Start, dann alle 6 h, und über das Menü. Fragt vor der Installation. |

Atlas erkennt die App an `window.__ATLAS_DESKTOP__` (`{ version, plattform, schema }`).

### Sicherheit

Die Atlas-Seite darf über die Capability `src-tauri/capabilities/atlas-remote.json`
genau fünf eigene Befehle aufrufen (Mitteilung, Mitteilungs-Erlaubnis, Dock-Zahl,
Drucken, fremde Adresse öffnen). Kein Dateizugriff, keine Shell, keine
Tauri-Plugins. `extern_oeffnen` lässt nur `http(s)`, `mailto`, `tel`, `sms`,
`facetime` zu. Deep-Links werden nur angenommen, wenn sie auf die Atlas-Domain führen.

## Entwickeln

Voraussetzungen: [Rust](https://rustup.rs) (stable), [bun](https://bun.sh), Xcode Command Line Tools.

```sh
bun install
bun run dev                 # App im Entwicklungsmodus gegen sinclair-atlas.com
cd src-tauri && cargo test  # Regeln für Adressen und Deep-Links
```

Nur in Debug-Builds wirksam:

| Variable | Zweck |
| --- | --- |
| `ATLAS_DESKTOP_URL=http://localhost:8080` | gegen einen lokalen Atlas-Dev-Server laden (bekommt dieselben Brückenrechte) |
| `ATLAS_DESKTOP_DOWNLOADS=/pfad` | Downloads umleiten (Tests ohne Zugriff auf `~/Downloads`) |
| `ATLAS_DESKTOP_UPDATE_OHNE_FRAGE=1` | Update ohne Dialog installieren (Ende-zu-Ende-Test des Updaters) |

Lokales App-Bundle ohne Signatur und ohne Updater-Paket:

```sh
bun run build:lokal         # → src-tauri/target/release/bundle/macos/Sinclair Atlas.app
```

App-Symbol neu erzeugen (aus dem Atlas-Kompass-Motiv):

```sh
python3 scripts/symbol-erzeugen.py ../sinclair-atlas/public/brand/atlas-compass.png
bun run symbole
```

Safari-Erweiterung „Atlas Passwort-Safe“ aus dem Atlas-Repo übernehmen (baut dort und
kopiert nach `safari-erweiterung/`; der Release-Workflow macht daraus
`Contents/PlugIns/*.appex`, lokale Builds enthalten sie nicht):

```sh
bash scripts/erweiterung-uebernehmen.sh   # Standard: ../sinclair-atlas
```

## Ausliefern

`.github/workflows/bauen.yml`:

1. Version in `package.json` erhöhen, committen.
2. Tag setzen und pushen: `git tag v0.2.0 && git push origin v0.2.0`.
3. GitHub baut macOS (signiert, notarisiert, DMG + Updater-Paket + `latest.json`)
   als **Release-Entwurf** und Windows als Artefakt.
4. Entwurf prüfen und veröffentlichen — erst dann sehen installierte Apps das Update.

Ohne Tag (`gh workflow run bauen.yml`) entstehen nur Artefakte, kein Release.

## Aufbau

```
src-tauri/
  src/
    lib.rs             Start, Plugins, Fenster-Ereignisse
    ziel.rs            Atlas-Adresse, Hosts, Anmelde-Hosts, Deep-Links (mit Tests)
    fenster.rs         Haupt-/Nebenfenster, Navigation, Pop-ups, Neu laden
    bruecke.rs/.js     Brücke Seite ↔ System (Mitteilungen, Dock-Zahl, Links, Drucken)
    downloads.rs       Download-Ordner, Mitteilung nach Abschluss
    erreichbarkeit.rs  Ist Atlas erreichbar? (vor jedem Laden)
    aktualisierung.rs  Updater
    menue.rs           Menüleiste
    taskleiste.rs      Windows: Zahl als Overlay-Symbol
    safari.rs          Safari-Erweiterung: Zustand, Safari-Einstellungen öffnen
  capabilities/        Rechte der lokalen Seite und der Atlas-Seite
  tauri.conf.json      Konfiguration (Updater-Pubkey: Platzhalter, siehe TODO.md)
  Entitlements*.plist  macOS-Rechte (Passkeys vorbereitet, nicht aktiv)
ui/index.html          Lade- und Offline-Seite
safari-erweiterung/    gebaute Browser-Erweiterung (aus sinclair-atlas, für Safari)
```
