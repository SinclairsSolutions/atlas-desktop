# TODO — Atlas Desktop

Stand 19.10.2026. Die App ist gebaut und lokal auf dem Mac (Apple Silicon) getestet.
Hier steht, was **du** erledigen musst, was noch **getestet** werden muss, was für
**Windows** offen ist und welche **Annahmen** ich getroffen habe.

Am Atlas-Code wurde nichts geändert. Was Atlas für eine bessere Einbindung
bräuchte, steht unten unter „Atlas — nur notiert“.

---

## 1. Selbst erledigen

### 1.1 GitHub-Repo anlegen und pushen

Das Repo liegt bisher nur lokal unter `~/Projekte/atlas-desktop` (7 Commits auf `main`).

- [ ] Repo `SinclairsSolutions/atlas-desktop` anlegen und pushen:
      `gh repo create SinclairsSolutions/atlas-desktop --private --source ~/Projekte/atlas-desktop --push`
      (oder mir Bescheid geben, dann mache ich das).
- [ ] **Entscheiden: öffentlich oder privat?** Der Updater lädt `latest.json` und das
      Update-Paket ohne Anmeldung von
      `https://github.com/SinclairsSolutions/atlas-desktop/releases/latest/download/latest.json`.
      Das geht nur, wenn die Releases öffentlich sind.
      - **A (einfachste):** Repo öffentlich. Es enthält keine Geheimnisse, nur die Hülle.
      - **B:** Repo privat, Releases in ein zweites, öffentliches Repo
        (z. B. `atlas-desktop-releases`). Braucht einen Token und eine kleine Workflow-Änderung.
      - **C:** `latest.json` und Pakete auf STACKIT/R2 ablegen, Endpunkt in
        `src-tauri/tauri.conf.json` umstellen.

      Sag mir, welche Variante. Für B und C passe ich Workflow und Endpunkt an.

### 1.2 Apple-Konto

- [ ] Im Apple-Developer-Konto alle offenen **Vereinbarungen akzeptieren**
      (developer.apple.com → Account → Agreements). Solange eine offen ist, schlägt die
      Notarisierung fehl. Daran hing schon die Safari-Erweiterung.
- [ ] Prüfen, ob das Zertifikat **„Developer ID Application“** noch gültig ist. Es ist
      dasselbe wie für die Safari-Erweiterung, ein neues ist nicht nötig.
- [ ] Für die Notarisierung ein **app-spezifisches Passwort** (appleid.apple.com →
      Anmeldung und Sicherheit). Das vorhandene aus dem Atlas-Repo kann wiederverwendet werden.

### 1.3 Updater-Schlüssel (Tauri, nicht Apple)

Damit installierte Apps nur echte Updates annehmen, signiert der Build jedes
Update-Paket mit einem eigenen Schlüssel.

- [ ] Im Terminal: `cd ~/Projekte/atlas-desktop && bunx tauri signer generate -w ~/.tauri/atlas-desktop.key`
      (fragt nach einem Passwort).
- [ ] Den Inhalt von `~/.tauri/atlas-desktop.key.pub` in `src-tauri/tauri.conf.json` bei
      `plugins.updater.pubkey` statt `ERSETZEN_DURCH_UPDATER_PUBKEY` eintragen.
      Der öffentliche Schlüssel ist kein Geheimnis. Du kannst ihn mir auch geben, dann
      trage ich ihn ein und committe.
- [ ] **Privaten Schlüssel sichern** (Passwort-Safe). Geht er verloren, können bereits
      installierte Apps keine Updates mehr annehmen und müssen von Hand neu installiert werden.

### 1.4 GitHub-Secrets im neuen Repo

Settings → Secrets and variables → Actions → *New repository secret*. Die Apple-Namen
sind dieselben wie im Atlas-Repo. Alternativ legst du sie einmal als Organisations-Secrets an.

| Secret | Inhalt |
| --- | --- |
| `APPLE_ZERTIFIKAT_P12` | Developer-ID-Zertifikat als .p12, **base64** (`base64 -i zert.p12 \| pbcopy`) |
| `APPLE_ZERTIFIKAT_PASSWORT` | Passwort der .p12 |
| `APPLE_TEAM_ID` | Team-ID (10 Zeichen) |
| `APPLE_ID` | Apple-ID-E-Mail |
| `APPLE_APP_PASSWORT` | app-spezifisches Passwort |
| `TAURI_SIGNING_PRIVATE_KEY` | Inhalt von `~/.tauri/atlas-desktop.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Passwort dazu |

Der Workflow prüft am Anfang, ob alle Secrets vorhanden sind und der Pubkey eingetragen
ist. Fehlt etwas, bricht er mit einer klaren Meldung ab.

### 1.5 Erstes Release

- [ ] `gh workflow run bauen.yml` als Probelauf: Nur Artefakte, dabei prüft der Workflow
      Signatur, Notarisierung und Universal-Binary.
- [ ] Danach `git tag v0.1.0 && git push origin v0.1.0` → Release-Entwurf prüfen → *Publish*.

### 1.6 Passkeys / Domain-Verknüpfung (für „Tresor per Touch ID entsperren“)

WebAuthn funktioniert in einer eingebetteten Ansicht nur, wenn die App mit der Domain
verknüpft ist. Solange das fehlt, schlägt in der App „Gerät entsperren“ fehl; das
Master-Passwort geht weiterhin.

- [ ] developer.apple.com → Identifiers: App-ID `com.sinclairs-solutions.atlas` anlegen,
      Fähigkeit **Associated Domains** einschalten.
- [ ] Profiles: **Developer ID**-Provisioning-Profil für diese App-ID erzeugen und herunterladen.
- [ ] Profil als Secret `APPLE_PROVISIONING_PROFILE` (base64) hinterlegen und mir Bescheid
      geben. Ich stelle dann auf `src-tauri/Entitlements.passkeys.plist` um, trage die
      Team-ID ein und bette das Profil im Workflow ein.
- [ ] Atlas muss die Datei `/.well-known/apple-app-site-association` ausliefern (siehe 3.4).

### 1.7 Windows-Signatur (erst wenn Windows ausgeliefert wird, kostet Geld)

Siehe Abschnitt 4.

---

## 2. Testen

Lokal schon geprüft (Mac, Apple Silicon, Debug-Build gegen eine Testseite und gegen
sinclair-atlas.com):

- Atlas lädt.
- Brücke und `window.__ATLAS_DESKTOP__` sind vorhanden.
- Mitteilungs-Erlaubnis und System-Mitteilung funktionieren.
- Dock-Zahl setzen und entfernen funktioniert.
- `location.href` auf eine fremde Seite geht in den Browser, ein fremdes iframe nicht.
- Leere Pop-ups lassen sich öffnen, beschreiben und schließen.
- Blob-Fenster öffnen sich.
- HTTP- und Blob-Downloads kommen an.
- Offline-Seite erscheint, Wiederverbindung geht automatisch.
- Deep-Link öffnet das Ziel, fremder Host wird verworfen.
- Es läuft nur eine Instanz.
- Updater: 0.1.0 → 0.1.1 installiert und neu gestartet (mit Test-Schlüssel und lokalem Server).

Noch zu testen, am besten mit der signierten DMG aus dem ersten Workflow-Lauf:

**Installation**
- [ ] DMG auf einem Apple-Silicon- **und** einem Intel-Mac öffnen, App in „Programme“
      ziehen, starten: keine Gatekeeper-Warnung.

**Logins**
- [ ] Atlas-Login mit E-Mail/Passwort und MFA; nach Neustart noch angemeldet.
- [ ] Mail-Konto Google und Microsoft verbinden: Browser öffnet sich, danach in die App
      zurück → App lädt neu, Verbindung ist da.
- [ ] Kalender Google/Microsoft, Stripe, Slack, Bank (finAPI) genauso. Beim Bank-Login
      soll sich das leere App-Fenster von selbst schließen.
- [ ] Passkey/„Gerät entsperren“ erst nach 1.6.

**Mitteilungen und Dock-Zahl**
- [ ] Erst sinnvoll, wenn Atlas sie auslöst (3.2/3.3). Bis dahin: Mitteilungen in den
      Atlas-Einstellungen aktivieren; in macOS-Einstellungen → Mitteilungen steht danach
      „Sinclair Atlas“.

**Uploads und Downloads**
- [ ] Datei-Auswahl (einzeln, mehrere), Drag & Drop aus dem Finder (Belege, Dokumente).
- [ ] Mikrofon: Meeting-Aufnahme und Sprachnachricht im Chat. macOS fragt einmal nach.
      Tab-Audio (`getDisplayMedia`) geht in der App vermutlich nicht. Prüfen, ob die
      Aufnahme dann sauber nur mit dem Mikrofon läuft.
- [ ] Downloads: DATEV-Export, PDF, Dokumente, ZIP. Sie landen in „Downloads“ mit
      Mitteilung; macOS fragt beim ersten Mal nach Zugriff auf den Download-Ordner.
- [ ] Drucken: Aktivitäts-Report (`window.print`).

**Fenster und Links**
- [ ] Zweitfenster, Meeting-Markierungen („Eigenes Fenster“), TIFF-Ansicht, PDF in neuem
      Fenster öffnen sich in der App.
- [ ] Vorschauen in Atlas (Beleg-, Drive-, Layer-Vorschau) erscheinen eingebettet und
      öffnen keinen Browser-Tab.
- [ ] Externe Links (Website eines Kontakts, LinkedIn, Maps, Mail-Adressen) gehen in den
      Standardbrowser bzw. das Mail-Programm.
- [ ] Größe und Lage nach Neustart gleich. Roter Knopf blendet aus, Klick aufs Dock holt
      das Fenster zurück, ⌘Q beendet.
- [ ] Kürzel: ⌘R, ⌘[ / ⌘], ⌘= / ⌘- / ⌘0, ⌘⇧H (Startseite), ⌘⇧O (im Browser), ⌥⌘L
      (Downloads), ⌘W, ⌘Q, ⌘C/V/X/A/Z, ⌃⌘F.
- [ ] Deep-Link: im Terminal `open "sinclair-atlas://open/mitteilungen"`.

**Offline**
- [ ] WLAN aus → App starten → Offline-Seite (auch im Dunkelmodus ansehen) → WLAN an →
      lädt von selbst. ⌘R ohne Netz → Offline-Seite statt weißer Seite.

**Updates**
- [ ] Nach 0.1.0 eine 0.1.1 veröffentlichen. Spätestens 20 s nach dem Start fragt die
      App; „Nach Updates suchen …“ im App-Menü funktioniert.

---

## 3. Atlas — nur notiert, nicht geändert

Atlas funktioniert im Browser unverändert. Die folgenden Punkte würden die App runder
machen. Jeder lässt sich so bauen, dass er nur greift, wenn `window.__ATLAS_DESKTOP__`
gesetzt ist.

### 3.1 Rücksprung nach OAuth (Google, Microsoft, Stripe, Slack, Bank)
Heute endet der Rückruf im **Browser** auf einer Atlas-Seite. Die App lädt neu, sobald
man zurückwechselt; das funktioniert, ist aber nicht elegant. Besser:
- Beim Start der Verbindung aus der App ein Kennzeichen in den `state` mitgeben
  (z. B. `client: "desktop"`).
- Am Ende des Callbacks dann statt der Atlas-Seite eine kleine Seite „Zurück zur App“
  zeigen, die `sinclair-atlas://open/<zielpfad>` aufruft (mit Link als Rückfall).
- Betrifft `api.oauth.google.callback`, `api.oauth.microsoft.callback`,
  `api.oauth.calendar.*.callback`, Stripe, Slack, Integrationen, finAPI (`/bank-verbunden`).

### 3.2 System-Mitteilungen
Atlas fragt zwar die Erlaubnis ab (`use-push-registration.ts`), erzeugt aber nirgends
eine `new Notification(…)`. Für Mitteilungen bei neuen Erwähnungen, Chat-Nachrichten
oder Aufgaben müsste Atlas sie beim Realtime-Ereignis auslösen (z. B. in
`notification-toaster.tsx`). Ein Klick auf die Mitteilung sollte die App nach vorn holen (testen); ein
Sprung zum Inhalt ist in der Hülle noch nicht verdrahtet.

### 3.3 Zahl am Dock
Atlas müsste `navigator.setAppBadge(anzahlUngelesen)` bzw. `navigator.clearAppBadge()`
aufrufen, etwa bei ungelesenen Mitteilungen und Chats. Im Browser ist das harmlos
(installierte PWAs in Chrome/Edge zeigen die Zahl ebenfalls).

### 3.4 Passkeys
Datei `public/.well-known/apple-app-site-association` (ohne Endung, `application/json`):
```json
{ "webcredentials": { "apps": ["<TEAMID>.com.sinclairs-solutions.atlas"] } }
```
Bis 1.6 erledigt ist, sollte Atlas „Tresor per Gerät entsperren“ in der App ausblenden
(`geraet-entsperren.ts`), weil WebAuthn dort sonst mit einem Fehler endet.

### 3.5 Drucken aus einem iframe
`projekt-report-editor.tsx` druckt über `frame.contentWindow.print()`. Das tut in der
Mac-App nichts. Nur der Hauptframe wird umgeleitet, und der würde die ganze Seite
drucken. Abhilfe: den Report in einem eigenen Fenster öffnen und dort `window.print()`.

### 3.6 Push-Gerät
`registerPushGeraet` meldet die App als `plattform: "web"` mit einer Installations-ID.
Optional eine eigene Plattform `desktop` einführen, damit die Geräteliste die App
erkennt.

---

## 4. Offen für Windows

Der Windows-Installer (NSIS, Benutzerinstallation, DE/EN) wird bei jedem Lauf gebaut
und als Artefakt `Sinclair-Atlas-Windows-ungeliefert` abgelegt, aber **nicht**
veröffentlicht und **nicht** signiert.

- [ ] **Noch nie auf Windows gelaufen.** Der Windows-Code wird erst in GitHub Actions
      übersetzt; lokal ließ er sich auf dem Mac nicht vollständig prüfen (`ring` braucht
      einen Windows-C-Compiler). Ersten Lauf abwarten und Fehler beheben.
- [ ] **Signatur:** vorbereitet für Azure Trusted Signing (ca. 10 $/Monat, Identitätsprüfung
      der Firma). Einschalten:
      1. Trusted-Signing-Konto und Zertifikatprofil in Azure anlegen.
      2. In `src-tauri/tauri.windows-signatur.conf.json` die drei `ERSETZEN_…` füllen
         (Endpunkt, Konto, Profil).
      3. Secrets `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID` anlegen.
      4. Repo-Variable `WINDOWS_SIGNIEREN = true` setzen.

      Ohne Signatur warnt SmartScreen beim Start.
- [ ] **Updates für Windows:** Derzeit erzeugt der Windows-Lauf bewusst kein Update-Paket
      (`tauri.windows.ungeliefert.conf.json`). Zum Ausliefern die Datei entfernen und den
      Windows-Job auf `tauri-action` mit derselben Release-ID wie macOS umstellen, damit
      `latest.json` den Eintrag `windows-x86_64` bekommt.
- [ ] Testen: WebView2 (installiert der Installer bei Bedarf), Menüleiste,
      Strg+R/F5/F11/Alt+←, Mitteilungen (erst nach Installation, vorher erscheinen sie
      unter falschem Namen), Overlay-Zahl an der Taskleiste, Deep-Link nach Installation,
      zweiter Start mit Deep-Link, Downloads (WebView2 zeigt eigene Download-Leiste).
- [ ] **Schließen beendet die App** (unter macOS nur ausblenden). Falls Mitteilungen auch
      bei geschlossenem Fenster kommen sollen: Infobereich-Symbol (Tray) ergänzen.
- [ ] **Symbol:** Das Mac-Symbol hat Rand und Schatten nach Apple-Vorgabe und wirkt in der
      Taskleiste klein. Eigenes, randloses Windows-Symbol erzeugen.
- [ ] Optional: ARM64-Build (`aarch64-pc-windows-msvc`).

---

## 5. Getroffene Annahmen

| Thema | Annahme |
| --- | --- |
| Adresse | `<ATLAS-URL>` = `https://sinclair-atlas.com`, dazu `www.sinclair-atlas.com`. Steht nur in `src-tauri/src/ziel.rs` und `capabilities/atlas-remote.json`. |
| Namen | App „Sinclair Atlas“, Bundle-ID `com.sinclairs-solutions.atlas` (wie die Safari-Erweiterung `…atlas-passwort-safe`), Deep-Link-Schema `sinclair-atlas://`. |
| Repo | `SinclairsSolutions/atlas-desktop` (wie `sinclair-atlas`), noch nicht angelegt. Das ist dein Schritt bzw. dein Okay (1.1). |
| Secrets | Gleiche Namen wie im Atlas-Repo, damit dieselben Werte passen. Notarisierung über Apple-ID + app-spezifisches Passwort (wie dort), nicht über einen App-Store-Connect-API-Schlüssel. |
| macOS | ab 12 (Monterey). Keine App-Sandbox (Vertrieb mit Developer ID, nicht über den App Store). Rechte: Mikrofon und Kamera (Meetings, Sprachnachrichten). |
| Schließen | macOS: roter Knopf blendet aus (App läuft weiter, Dock-Zahl bleibt). Windows: beendet. |
| Was in der App bleibt | Atlas-Seiten, `about:blank`/`blob:`/`data:` (Zweitfenster, Vorschauen). **Alles andere** im Hauptframe geht in den Browser, auch signierte Speicher-Links, die Atlas per `window.open` öffnet. |
| iframes | Fremde Inhalte **innerhalb** einer Atlas-Seite (Vorschauen) bleiben erlaubt. WebKit unterscheidet Navigationen von iframes nicht vom Hauptframe, daher ist der Hauptframe dreifach abgesichert (siehe Kopf von `fenster.rs`). |
| Anmeldungen | Liste der Anmelde-Hosts in `ziel.rs` (Google, Microsoft, Apple, Stripe, Slack, PayPal, SumUp, DATEV, finAPI). Bank-Login im Browser statt im App-Fenster. |
| Nach Anmeldung | Kehrt man in die App zurück, lädt sie die aktuelle Seite neu (nur wenn zuvor eine Anmeldung im Browser geöffnet wurde und mindestens 3 s vergangen sind). |
| Mitteilungen | Die Erlaubnis verhält sich wie im Browser: „default“, bis Atlas fragt, dann gemerkt. |
| Updates | Prüfung 20 s nach Start und alle 6 h; **immer mit Rückfrage**, weil Atlas ungespeicherte Eingaben haben kann. Release erst als Entwurf. |
| Sprache | Menü, Dialoge und Offline-Seite folgen der Systemsprache (Deutsch, sonst Englisch). |
| Daten | Eigener WebKit-Speicher der App: Anmeldung, Cookies und `localStorage` sind von Safari getrennt, beim ersten Start also einmal anmelden. |
| Zoom | Pro Fenster, wird nicht über einen Neustart gemerkt. |
| Symbol | Aus `public/brand/atlas-compass.png` (Kompass ohne Schriftzug) im Mac-Format. |
| Protokoll | `~/Library/Logs/com.sinclairs-solutions.atlas/` (nur Ursprung der geladenen Seite, keine Inhalte). |
| Werkzeuge | Rust per rustup unter `~/.cargo` installiert (ohne Änderung an deiner Shell-Konfiguration). Für Rust-Befehle im Terminal: `source ~/.cargo/env`. |
