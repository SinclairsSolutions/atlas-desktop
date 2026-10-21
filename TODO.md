# TODO — Atlas Desktop

Stand 20.10.2026 (Version 0.2.0). Gebaut und lokal auf dem Mac (Apple Silicon) getestet;
Releases werden von GitHub Actions signiert und notarisiert.
Hier steht, was **du** erledigen musst, was noch **getestet** werden muss, was für
**Windows** offen ist und welche **Annahmen** ich getroffen habe.

Seit 0.2.0 arbeitet Atlas mit der App zusammen (Posteingang, Schnellerfassung,
Mitteilungen, Dock-Zahl, Touch ID für den Tresor, Kopfleiste) — siehe Abschnitt 0.

---

## 0. Was die App kann (0.2.0)

| Bereich | Wie | Wo es landet |
| --- | --- | --- |
| Belege aufs Dock-Symbol ziehen | PDF/Bild/XML/EML auf das Symbol | Posteingang des gerade offenen Workspace (ohne Workspace: Auswahl-Dialog) |
| Überwachter Ordner | Einstellungen → Ordner wählen; neue Dateien alle 20 s | wie oben; Original wandert danach nach `Hochgeladen/` |
| Menüleiste | Belege …, Bildschirmfoto, Schnellerfassung, Einstellungen | Belege/Fotos → Posteingang; Schnellerfassung → Aufgabe |
| Schnellerfassung | ⌥⌘A von überall; erste Zeile = Titel | Aufgabe (Tag „Schnellerfassung“) bzw. Notiz (Tag „Notiz“) |
| Offline | Warteschlange im App-Datenordner, neuer Versuch alle 20 s | geht raus, sobald Atlas geladen und angemeldet ist |
| Exporte | Blob-Downloads (PDF, Excel, DATEV) | Download-Ordner oder Export-Ordner, auf Wunsch je Mandant |
| Fenster | ⌘N neues Fenster, Kopfleiste ziehbar, Titelleiste als Overlay | — |
| Mitteilungen | Atlas sendet im Hintergrund System-Mitteilungen | Mitteilungszentrale |
| Dock-Zahl | ungelesene Mitteilungen der Glocke | abschaltbar |
| App-Symbol | Schwarz / Weiß (Darstellung → App-Symbol oder Einstellungen) | — |
| Touch ID | App-Sperre (Start + nach Abwesenheit) und Tresor entsperren | Gerätegeheimnis im macOS-Schlüsselbund |
| Start | Beim Anmelden starten | — |

Justieren (mit dir abzustimmen): Zielordner bzw. Zielmodul je Quelle (z. B. Ordner →
fester Workspace statt „gerade offen“), Unternehmen-Zuordnung beim Upload.

---

## 1. Selbst erledigen

### 1.1 GitHub-Repo anlegen und pushen

Das Repo liegt bisher nur lokal unter `~/Projekte/atlas-desktop` (Branch `main`).

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

- [x] Schlüssel erzeugt und als Secrets hinterlegt (19.10.2026, `scripts/updater-schluessel.sh`).
- [x] Den Inhalt von `~/.tauri/atlas-desktop.key.pub` in `src-tauri/tauri.conf.json` bei
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

- [x] Probelauf: Signatur, Notarisierung, Universal-Binary — ok (20.10.2026).
- [ ] Entwurf **v0.1.0 löschen** (hat den Absturz beim Öffnen neuer Fenster).
- [ ] Entwurf **v0.2.0** testen (Abschnitt 2) und *Publish*. v0.1.1 kann gelöscht werden,
      wenn 0.2.0 passt — ab der ersten veröffentlichten Version laufen Updates automatisch.

### 1.6 Passkeys / Domain-Verknüpfung (für „Tresor per Touch ID entsperren“)

Seit 0.2.0 entsperrt der Tresor in der App per Touch ID über den Schlüsselbund —
dafür ist die Domain-Verknüpfung **nicht** mehr nötig. Sie braucht es nur noch, wenn
Passkeys (auch zum Anmelden) in der App selbst funktionieren sollen.

- [ ] developer.apple.com → Identifiers: App-ID `com.sinclairs-solutions.atlas` anlegen,
      Fähigkeit **Associated Domains** einschalten.
- [ ] Profiles: **Developer ID**-Provisioning-Profil für diese App-ID erzeugen und herunterladen.
- [ ] Profil als Secret `APPLE_PROVISIONING_PROFILE` (base64) hinterlegen und mir Bescheid
      geben. Ich stelle dann auf `src-tauri/Entitlements.passkeys.plist` um, trage die
      Team-ID ein und bette das Profil im Workflow ein.
- [ ] Atlas muss die Datei `/.well-known/apple-app-site-association` ausliefern (siehe 3.4).

### 1.7 Windows (Einschalten kostenlos, Signatur kostet ca. 10 USD/Monat)

Ausliefern: Repo-Variable `WINDOWS_AUSLIEFERN = true` (4.3). Signatur: Azure Trusted
Signing einrichten, Secrets `AZURE_CLIENT_ID`/`AZURE_CLIENT_SECRET`/`AZURE_TENANT_ID`,
Variable `WINDOWS_SIGNIEREN = true` (4.2). Vorher auf einem Windows-PC testen (4.4).

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

**Safari-Erweiterung in der App** (seit 21.10.2026; die eigene Container-App aus dem
Atlas-Workflow „Safari-Erweiterung“ ist damit überflüssig)
- [ ] Erster CI-Lauf: Schritt „Safari-Erweiterung bauen und signieren“ grün, Prüfschritt
      findet `Contents/PlugIns/*.appex` in App und Updater-Paket, Notarisierung angenommen.
- [ ] App nach „Programme“, einmal starten → Einstellungen → Passwörter zeigt „Noch aus“;
      „In Safari einschalten“ öffnet Safari bei „Erweiterungen“; nach dem Anhaken „Eingeschaltet“.
- [ ] Safari: Atlas-Symbol → „Mit Atlas verbinden“ → Kopplung und Ausfüllen wie bisher.
- [ ] Wer die alte Container-App „Atlas Passwort-Safe“ hat: andere Bundle-ID, also zwei
      Einträge in Safari — alte App löschen, neue Erweiterung einmal neu verbinden.
- [ ] Nach einem Update über den Updater bleibt die Erweiterung eingeschaltet.
- Erweiterung aktualisieren: `bash scripts/erweiterung-uebernehmen.sh` (baut im Atlas-Repo,
  kopiert nach `safari-erweiterung/`), committen, neue App-Version taggen.

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

## 4. Windows ausliefern

Stand 21.10.2026 (Branch `windows-ausliefern`). Der Installer (NSIS, x64,
Benutzerinstallation ohne Administratorrechte, DE/EN) wird bei jedem Lauf gebaut.
Ins Release kommt er erst, wenn du es einschaltest — bis dahin ändert sich für
Mac-Nutzer nichts.

### 4.1 Was schon erledigt ist

- [x] Windows-Code wird in CI mit `-D warnings` übersetzt (inkl. Ablage im Explorer,
      `src-tauri/src/ablage/windows.rs`) — Warnungen brechen den Lauf ab.
- [x] **Updates für Windows:** Mit der Repo-Variable `WINDOWS_AUSLIEFERN = true`
      baut der Windows-Job bei einem Tag zusätzlich die Updater-Signatur
      (`…-setup.exe.sig`, mit demselben `TAURI_SIGNING_PRIVATE_KEY` wie macOS). Der
      Job **„windows-release“** läuft erst nach macOS **und** Windows, lädt
      `Sinclair.Atlas_<version>_x64-setup.exe`, `.sig` und `windows.json` in den
      **Entwurf**, den macOS angelegt hat, und ergänzt dort `latest.json` um
      `windows-x86_64` (+ `windows-x86_64-nsis`) — `scripts/windows-in-latest-json.sh`,
      prüft Version und lässt die macOS-Einträge unverändert. Ein schon
      veröffentlichter Release wird nie angefasst.
      Ohne die Variable: wie bisher nur Artefakt `Sinclair-Atlas-Windows-ungeliefert`
      (`tauri.windows.ungeliefert.conf.json` gilt nur noch dann).
- [x] Probe ohne Release: `gh workflow run bauen.yml --ref <branch> -f windows_probe=true`
      baut das Updater-Paket und führt die latest.json-Zusammenführung als
      Trockenlauf gegen den neuesten veröffentlichten Release aus.
- [x] **Symbol:** eigenes, randloses Windows-Symbol (`icon.ico` mit 16/24/32/48/64/256 px,
      kleine Größen mit kräftigeren Linien), erzeugt aus `quelle-1024.png` mit
      `scripts/windows-symbole-erzeugen.py` (läuft auch in `bun run symbole` mit).
      Ursache des kleinen, gerahmten Taskleisten-Symbols war, dass die App beim Start
      das Mac-Dock-Bild (mit Apple-Rand und Schatten) als Fenstersymbol gesetzt hat —
      jetzt nimmt sie unter Windows das Symbol aus der Programmdatei bzw. eine
      randlose weiße Variante.
- [x] **Infobereich (Tray):** eigene Symbole statt der Mac-Template-Bilder (die auf der
      dunklen Taskleiste unsichtbar waren). **Schließen** blendet das Hauptfenster in
      den Infobereich aus, solange das Symbol an ist (Einstellung „Menüleiste“,
      Standard an) — Mitteilungen und Taskleisten-Zahl laufen weiter. „Sinclair Atlas
      beenden“ im Infobereich-Menü bzw. Datei → Beenden beendet. Ist das Symbol aus,
      beendet Schließen die App wie bisher.
- [x] Atlas-Seite vorbereitet (Atlas-Branch `windows-download`, nicht deployt):
      `/api/public/desktop-download?os=windows` reicht den Installer durch, Profil →
      Apps zeigt „Für Windows herunterladen“ nur, wenn der veröffentlichte Release einen
      Installer hat, und den SmartScreen-Hinweis, solange `windows.json` `signiert: false` meldet.

### 4.2 Signatur (kostet Geld — optional, aber empfohlen)

Ohne Signatur funktioniert alles, aber beim **ersten Start des Installers** zeigt
Windows SmartScreen: „Der Computer wurde durch Windows geschützt“ (engl. „Windows
protected your PC“), Herausgeber „Unbekannt“. Weiter geht es über **„Weitere
Informationen“ → „Trotzdem ausführen“**. Manche Firmen-PCs (Intune/Gruppenrichtlinie)
lassen das gar nicht zu; gelegentlich meldet ein Virenscanner unsignierte Installer.

**Updates gehen auch unsigniert:** Der Tauri-Updater prüft nur seine eigene Signatur
(`.sig`, minisign, Schlüssel aus 1.3) — Authenticode spielt dafür keine Rolle. Die App lädt
das Update selbst (ohne „Mark of the Web“), deshalb fragt SmartScreen dabei in der Regel
nicht; der Installer läuft im Modus „passive“ (nur Fortschrittsbalken). Auf einem echten
PC bestätigen (4.4).

Weg über **Azure Trusted Signing** (heißt inzwischen auch „Azure Artifact Signing“):

| | |
| --- | --- |
| Kosten | Basic **9,99 USD/Monat** (5.000 Signaturen, 1 Zertifikatprofil — reicht), Abrechnung über ein Azure-Abo (Kreditkarte). Kein Einmalbetrag, keine Hardware. |
| Voraussetzung | Organisation mit Sitz u. a. in der EU und **nachweisbar mindestens 3 Jahren Geschäftstätigkeit** (Microsoft-Vorgabe für „Public Trust“, Stand 2025 — vor dem Abschluss auf learn.microsoft.com unter „Trusted Signing / Artifact Signing“ nachsehen). Ist die GmbH jünger: Signatur über ein klassisches OV-Zertifikat (ca. 200–400 €/Jahr, mit Hardware-Token bzw. Cloud-HSM; dann passe ich `signCommand` an). |
| Identitätsprüfung | Sinclair's Solutions GmbH: Name und Anschrift genau wie im Handelsregister, Handelsregisternummer, ggf. Website/Domain mit Firmen-E-Mail. Dauer laut Microsoft einige Werktage bis ca. 3 Wochen; es können Unterlagen nachgefordert werden. |
| SmartScreen danach | Herausgeber „Sinclair's Solutions GmbH“ statt „Unbekannt“. Die Reputation baut sich über Downloads auf — in den ersten Tagen kann SmartScreen trotzdem noch warnen. |

Schritte (alles im Azure-Portal, portal.azure.com):

- [ ] 1. Azure-Abo anlegen (falls keins da ist) → Abonnement → Ressourcenanbieter →
      `Microsoft.CodeSigning` registrieren.
- [ ] 2. Ressource **„Trusted Signing Account“** anlegen: Region **West Europe**,
      Tarif **Basic**, Name z. B. `sinclair-atlas` (= Konto).
- [ ] 3. Dir selbst auf dem Konto die Rolle **„Trusted Signing Identity Verifier“** geben
      (Zugriffssteuerung (IAM)).
- [ ] 4. Konto → **Identity validation** → *New* → **Public** → *Organization* →
      Firmendaten der GmbH eintragen, absenden, Prüfung abwarten (Status „Completed“).
- [ ] 5. Konto → **Certificate profiles** → *Create* → **Public Trust**, mit der geprüften
      Identität verknüpfen, Name z. B. `atlas-desktop` (= Zertifikatprofil).
- [ ] 6. Microsoft Entra ID → **App-Registrierungen** → *Neu* (z. B. `atlas-desktop-ci`) →
      „Zertifikate & Geheimnisse“ → **Geheimer Clientschlüssel** erstellen (Ablaufdatum
      notieren, z. B. 24 Monate). Auf dem Trusted-Signing-Konto dieser App die Rolle
      **„Trusted Signing Certificate Profile Signer“** geben.
- [ ] 7. GitHub `SinclairsSolutions/atlas-desktop` → Settings → Secrets and variables →
      Actions → **Secrets**:

      | Secret | Wert |
      | --- | --- |
      | `AZURE_CLIENT_ID` | Anwendungs-ID (Client-ID) der App-Registrierung |
      | `AZURE_CLIENT_SECRET` | Wert (nicht die ID) des geheimen Clientschlüssels |
      | `AZURE_TENANT_ID` | Verzeichnis-ID (Mandant) |

- [ ] 8. Mir Konto- und Profilnamen nennen (keine Geheimnisse) — ich trage sie in
      `src-tauri/tauri.windows-signatur.conf.json` ein (`ERSETZEN_ENDPUNKT` =
      `https://weu.codesigning.azure.net`, `ERSETZEN_KONTO`, `ERSETZEN_ZERTIFIKATPROFIL`).
      Der Workflow bricht ab, solange dort noch `ERSETZEN_` steht und die Signatur an ist.
- [ ] 9. Repo-**Variable** `WINDOWS_SIGNIEREN = true`. Der Schritt „Installer prüfen“
      schlägt fehl, wenn der Installer dann nicht gültig signiert ist.

### 4.3 Einschalten (kostenlos)

- [ ] Branch `windows-ausliefern` (baut auf `finder-ablage` auf) nach `main` übernehmen.
- [ ] Vorher einmal auf einem Windows-PC testen (4.4) — mit dem Artefakt eines Laufs
      (`gh workflow run bauen.yml --ref main -f windows_probe=true` → Artefakt
      `Sinclair-Atlas-Windows`).
- [ ] Repo-**Variable** `WINDOWS_AUSLIEFERN = true` (Settings → Secrets and variables →
      Actions → *Variables*). Ab dem nächsten Tag landet Windows im Release-Entwurf.
      Neue Secrets braucht es dafür nicht (Updater-Schlüssel ist schon da).
- [ ] Nach dem Lauf im Entwurf prüfen: Assets `Sinclair.Atlas_<v>_x64-setup.exe`, `.sig`,
      `windows.json`; `latest.json` enthält `windows-x86_64`. Dann *Publish*.
- [ ] Atlas-Branch `windows-download` mergen und deployen — erst dann erscheint der
      Windows-Knopf in Profil → Apps (er zeigt sich nur, wenn der veröffentlichte Release
      einen Installer hat; vorher ändert sich nichts).
- [ ] Zurück: Variable löschen oder auf `false` → nächste Version wieder nur Mac. Schon
      installierte Windows-Apps bleiben dann auf ihrer Version (Updater findet keinen
      Windows-Eintrag), bis die Variable wieder an ist.

### 4.4 Auf einem Windows-PC testen (Windows 10 22H2 oder Windows 11, x64)

Der Code ist nur übersetzt, **gestartet hat ihn noch niemand.**

- [ ] Installer starten: SmartScreen-Warnung (unsigniert) → „Weitere Informationen“ →
      „Trotzdem ausführen“. Keine Admin-Abfrage. Ohne WebView2 lädt der Installer es
      nach (braucht Internet; Windows 11 hat es schon).
- [ ] Startmenü-Eintrag und Symbol in Taskleiste/Titelleiste/Alt+Tab: randlos, groß
      genug, scharf (100 % und 150 % Anzeigeskalierung). Einstellungen → App-Symbol
      Weiß ausprobieren und zurück.
- [ ] Anmelden mit MFA, nach Neustart noch angemeldet.
- [ ] Infobereich: Atlas- und Schlüssel-Symbol sichtbar (helle und dunkle Taskleiste);
      Linksklick Atlas → Menü; Schlüssel → Passwort-Schnellzugriff erscheint über der
      Taskleiste.
- [ ] Fenster schließen (X) → App läuft im Infobereich weiter, Mitteilungen kommen;
      „Atlas öffnen“ holt sie zurück; „Sinclair Atlas beenden“ beendet. Mit
      Infobereich-Symbol aus: X beendet.
- [ ] Mitteilungen (erst nach Installation, vorher unter falschem Namen),
      Zahl an der Taskleiste (Overlay).
- [ ] Strg+R/F5/F11/Alt+←, Strg+Alt+A (Schnellerfassung), Downloads (WebView2-Leiste),
      Datei-Auswahl, Drag & Drop, Mikrofon.
- [ ] Deep-Link: Win+R → `sinclair-atlas://open/mitteilungen`; zweiter Start mit Link
      bei laufender App.
- [ ] Ablage im Explorer (siehe 6, Windows): einschalten, `%USERPROFILE%\Atlas`, Datei
      öffnen, neue Datei, umbenennen, löschen → Atlas-Papierkorb; App beenden → nicht
      geladene Dateien öffnen sich nicht. **Hier ist das größte Risiko** — notfalls den
      Schalter vor dem Ausliefern unter Windows ausblenden (sag Bescheid).
- [ ] **Update:** Version N installieren, N+1 mit `WINDOWS_AUSLIEFERN` veröffentlichen →
      die App fragt nach spätestens 20 s, installiert (Fortschrittsbalken), startet neu.
- [ ] Deinstallieren über Einstellungen → Apps.

### 4.5 Später / optional

- [ ] ARM64-Build (`aarch64-pc-windows-msvc`). Bis dahin läuft die x64-Version auf
      Windows-on-ARM per Emulation.
- [ ] „Öffnen mit“ bzw. Dateien auf das Taskleisten-Symbol: unter Windows kommen sie als
      Startargumente an, das wertet die App noch nicht aus (macOS: `RunEvent::Opened`).
- [ ] Ordner „Atlas“ in der Explorer-Navigationsleiste (braucht Paket-Identität/Signatur, siehe 6).

---

## 5. Getroffene Annahmen

| Thema | Annahme |
| --- | --- |
| Adresse | `<ATLAS-URL>` = `https://sinclair-atlas.com`, dazu `www.sinclair-atlas.com`. Steht nur in `src-tauri/src/ziel.rs` und `capabilities/atlas-remote.json`. |
| Namen | App „Sinclair Atlas“, Bundle-ID `com.sinclairs-solutions.atlas` (wie die Safari-Erweiterung `…atlas-passwort-safe`), Deep-Link-Schema `sinclair-atlas://`. |
| Repo | `SinclairsSolutions/atlas-desktop` (wie `sinclair-atlas`), noch nicht angelegt. Das ist dein Schritt bzw. dein Okay (1.1). |
| Secrets | Gleiche Namen wie im Atlas-Repo, damit dieselben Werte passen. Notarisierung über Apple-ID + app-spezifisches Passwort (wie dort), nicht über einen App-Store-Connect-API-Schlüssel. |
| macOS | ab 12 (Monterey). Keine App-Sandbox (Vertrieb mit Developer ID, nicht über den App Store). Rechte: Mikrofon und Kamera (Meetings, Sprachnachrichten). |
| Schließen | macOS: roter Knopf blendet aus (App läuft weiter, Dock-Zahl bleibt). Windows: blendet in den Infobereich aus, solange das Symbol dort an ist; sonst beendet. |
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

---

## 6. Ablage im Finder (Branch `finder-ablage`, noch nicht veröffentlicht)

Die klassische Atlas-Ablage als Ordner „Atlas“ im Finder (File Provider,
`NSFileProviderReplicatedExtension`). Quellen: `finder-ablage/` (Swift,
Erweiterung), `src-tauri/src/ablage/` (App: Schalter, Kopplung, Anstoßen),
`ui/einstellungen.*` (Reiter „Ablage im Finder“). Server: Atlas-Branch
`finder-ablage` (`src/lib/desktop-ablage.*`, `/api/public/desktop/ablage/*`,
Migration `20261021200000_desktop_ablage_geraet.sql`).

### Aufbau

| Teil | Wo | Was |
| --- | --- | --- |
| Erweiterung | `Contents/PlugIns/AtlasAblage.appex` (`com.sinclairs-solutions.atlas.ablage`) | Finder-Ordner, Platzhalter, Laden beim Öffnen, Hochladen neuer Dateien/Ordner |
| App | `src-tauri/src/ablage/` | Schalter, Gerätezugang holen (über die angemeldete Atlas-Seite), Domäne an-/abmelden, alle 30 s Abgleich anstoßen |
| Atlas | `/api/public/desktop/ablage/{workspaces,liste,datei,hochladen,ordner,abmelden}` | Gerätetoken `atlas_abl_…`, Rechte über eine echte Sitzung der Person (RLS) |

Gerätezugang: Atlas stellt ihn nur mit bestandener zweiter Stufe aus; die App
legt ihn in ihren Schlüsselbund und übergibt ihn einmal über den
App-Gruppen-Ordner (`~/Library/Group Containers/KJ63M9QZUG.com.sinclairs-solutions.atlas/Ablage/zugang.json`)
an die Erweiterung, die ihn in ihren eigenen Schlüsselbund übernimmt und die
Datei löscht. Widerruf: Profil → Apps → „Trennen“, Ausschalten in der App,
Abmelden in Atlas.

### Selbst erledigen

- [ ] Migration `20261021200000_desktop_ablage_geraet.sql` im Atlas-Repo erst
      im Testprojekt, dann nach Freigabe in der Produktion ausführen.
- [ ] `APP_ENCRYPTION_KEY` muss auf dem Server gesetzt sein (wird schon für
      Signaturen genutzt) — der Server verschlüsselt damit seine Sitzung je Gerät.
- [ ] Apple: **zunächst nichts.** App und Erweiterung nutzen die App-Gruppe im
      macOS-Stil `KJ63M9QZUG.com.sinclairs-solutions.atlas`; die braucht laut
      Apple (DTS, „App Groups: macOS vs iOS“) kein Bereitstellungsprofil, und
      der Container-Schutz ab macOS 15 lässt sie ohne Rückfrage zu, weil sie
      mit der Team-ID beginnt.
      **Falls** die Erweiterung auf einem fremden Mac nicht lädt
      (`addDomain` → Fehler -2001 „provider not found“ oder Zugriff auf den
      Gruppen-Ordner verweigert): Im Developer-Konto für
      `com.sinclairs-solutions.atlas` und `com.sinclairs-solutions.atlas.ablage`
      je eine explizite App-ID mit „App Groups“ (Gruppe
      `KJ63M9QZUG.com.sinclairs-solutions.atlas`) anlegen, je ein
      **Developer ID**-Bereitstellungsprofil erzeugen und als Secrets
      hinterlegen — dann baue ich das Einbetten
      (`Contents/embedded.provisionprofile`) und `com.apple.application-identifier`
      in den Workflow ein.

### Testen (erst nach Migration + Deploy des Atlas-Branches)

- [ ] Workflow auf dem Branch starten: `gh workflow run bauen.yml --ref finder-ablage`
      (nur Artefakte, kein Release), DMG laden, App nach „Programme“.
- [ ] Einstellungen → „Ablage im Finder“ einschalten → Finder zeigt links „Sinclair Atlas“ /
      „Ablage“ mit den Workspaces.
- [ ] Datei öffnen (lädt), Rechtsklick „Download entfernen“, „Immer behalten“.
- [ ] Neue Datei und neuen Ordner im Finder anlegen → erscheint in Atlas
      (Timeline: `document.uploaded` / `folder.created`, Quelle `desktop_ablage`).
- [ ] In Atlas Ordner anlegen, Dokument verschieben/umbenennen → binnen ~30 s im Finder.
- [ ] Vertrauliches Dokument → erscheint nie. Recht entziehen → Workspace verschwindet.
- [ ] Profil → Apps → Gerät trennen → beim nächsten Abgleich verschwindet alles.
- [ ] In Atlas abmelden → Ordner weg; wieder anmelden → verbindet sich von selbst.
- [ ] FileVault aus → Hinweis im Reiter.
- [ ] Phase 2: Datei im Finder bearbeiten und speichern → in Atlas Version 2, unter
      „Frühere Versionen“ die alte; gleichzeitig in Atlas ersetzen → Konfliktkopie
      „… (Konflikt von <Name>, <Datum>)“. Umbenennen/Verschieben im Finder → in Atlas;
      Löschen → Atlas-Papierkorb (ohne Verwalten-Recht: Finder stellt wieder her);
      Verschieben in einen anderen Workspace → wird zurückgesetzt.

### Windows (Phase 3, Branch `finder-ablage`)

`src-tauri/src/ablage/windows.rs`: Cloud Files API (cfapi) direkt in der App —
Sync-Root `%USERPROFILE%\Atlas`, Platzhalter mit Atlas-Kennung, Laden beim
Öffnen (FETCH_DATA), Umbenennen/Verschieben/Löschen über Callbacks an Atlas,
Abgleich alle 30 s, Gerätezugang in der Anmeldeinformationsverwaltung,
BitLocker-Hinweis. Gleiche Server-Schnittstelle und gleicher Gerätezugang
wie auf dem Mac (`src-tauri/src/ablage/api.rs`).

- Nur gebaut (Windows-Job in `bauen.yml`), **nie ausgeführt** — es gibt hier
  keinen Windows-Rechner. Vor dem Ausliefern auf einem echten Windows 10/11
  testen (Liste oben sinngemäß, dazu: App beenden → nicht geladene Dateien
  lassen sich nicht öffnen, geladene schon).
- Bekannte Grenzen: Laden nur bei laufender App (kein eigener Dienst);
  Änderungen aus Atlas erscheinen spätestens nach 30 s; Ordner „Atlas“ steht
  noch nicht in der Explorer-Navigationsleiste (dafür bräuchte es die
  WinRT-Registrierung `StorageProviderSyncRootManager`, die für nicht
  paketierte Apps Identität/Signatur verlangt) — Zugang über
  „Im Explorer zeigen“ bzw. den Benutzerordner.
- Der Windows-Installer wird erst mit `WINDOWS_AUSLIEFERN = true` ausgeliefert (siehe Abschnitt 4).
