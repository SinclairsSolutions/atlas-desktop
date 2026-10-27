//! Belege in den Atlas-Eingang: aufs Dock-Symbol ziehen, über die Menüleiste
//! wählen, Bildschirmfoto oder überwachter Ordner.
//!
//! Alles läuft über eine Warteschlange im App-Datenordner. Sie wird an die
//! Atlas-Seite im Hauptfenster übergeben (`window.__atlasDesktopDateien`);
//! Atlas lädt mit der Anmeldung der Person hoch und bestätigt je Datei
//! (`dateien_erledigt`). Erst dann wird die Datei aus der Warteschlange
//! gelöscht — ohne Netz oder ohne Anmeldung bleibt sie liegen und geht beim
//! nächsten Versuch raus (alle 20 Sekunden).
//!
//! Gegen Schleifen (0.3.7): Übergeben wird nur, wenn die Seite gemeldet hat,
//! dass sie Dateien annimmt (`eingang_bereit`). Bleibt die Bestätigung nach
//! `MAX_UEBERGABEN` Übergaben aus, gilt die Datei als „unklar“: einmalige
//! Mitteilung, Kopie nach `warteschlange/unklar`, Original nach „Hochgeladen“.
//! Gleicher Inhalt (SHA-256) wird nicht doppelt eingereiht.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{einstellungen, fenster, importordner, texte::t, ziel, zustand::Zustand};

/// Größte Datei, die übergeben wird (Atlas-Upload-Grenze für Belege).
const MAX_BYTES: u64 = 25 * 1024 * 1024;
/// Dateiendungen, die als Beleg in Frage kommen.
const ENDUNGEN: &[&str] = &["pdf", "png", "jpg", "jpeg", "heic", "tif", "tiff", "webp", "xml", "eml"];
/// So oft wird eine Datei ohne Bestätigung übergeben, danach gilt sie als „unklar“.
const MAX_UEBERGABEN: u32 = 2;
/// Abstand zwischen zwei Übergaben derselben Datei.
const WARTEZEIT: Duration = Duration::from_secs(120);
/// Gleicher Inhalt innerhalb dieser Zeit nach dem Hochladen: nicht erneut senden.
const SPERRZEIT: Duration = Duration::from_secs(15 * 60);

#[derive(Serialize, Deserialize, Clone)]
struct Meta {
    name: String,
    quelle: String,
    /// Herkunft im überwachten Ordner — wird nach dem Upload nach „Hochgeladen“ verschoben.
    original: Option<PathBuf>,
    zeit: u64,
    /// Ziel in Atlas (siehe `ziel_aus_einstellung`); fehlt bei älteren Einträgen.
    #[serde(default)]
    ziel: Option<serde_json::Value>,
    /// Wie oft schon an Atlas übergeben (ohne Bestätigung).
    #[serde(default)]
    uebergaben: u32,
    /// SHA-256 des Inhalts (hex) — gegen doppeltes Einreihen.
    #[serde(default)]
    hash: Option<String>,
}

#[derive(Default)]
pub struct Eingang {
    /// Token → Zeitpunkt der Übergabe (gegen doppeltes Senden).
    unterwegs: Mutex<HashMap<String, Instant>>,
    /// Überwachter Ordner: zuletzt gesehene Größe je Datei (erst stabile Dateien laden).
    gesehen: Mutex<HashMap<PathBuf, u64>>,
    /// Bereits eingereihte Originale (Pfad), damit nichts doppelt kommt.
    eingereiht: Mutex<HashSet<PathBuf>>,
    /// Die Atlas-Seite im Hauptfenster nimmt gerade Dateien an (meldet `bruecke.js`).
    bereit: AtomicBool,
    /// Inhalt (Hash) → Zeitpunkt, zu dem er hochgeladen bzw. als unklar abgelegt wurde.
    kuerzlich: Mutex<HashMap<String, Instant>>,
}

fn ordner(app: &AppHandle) -> Option<PathBuf> {
    let d = app.path().app_data_dir().ok()?.join("warteschlange");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

/// Dateien, deren Übergabe nie bestätigt wurde (Kopie zum Nachsehen).
fn unklar_ordner(dir: &Path) -> PathBuf {
    dir.join("unklar")
}

fn hash_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn meta_lesen(pfad: &Path) -> Option<Meta> {
    std::fs::read(pfad).ok().and_then(|b| serde_json::from_slice::<Meta>(&b).ok())
}

/// Wartet derselbe Inhalt schon in der Warteschlange?
fn schon_eingereiht(dir: &Path, hash: &str) -> bool {
    let Ok(eintraege) = std::fs::read_dir(dir) else { return false };
    eintraege
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| meta_lesen(&p))
        .any(|m| m.hash.as_deref() == Some(hash))
}

/// Original aus dem überwachten Ordner bzw. Import-Ordner nach „Hochgeladen“.
fn original_ablegen(app: &AppHandle, orig: &Path) {
    if let Some(eltern) = orig.parent() {
        let erl = eltern.join(importordner::hochgeladen());
        let _ = std::fs::create_dir_all(&erl);
        if let Some(n) = orig.file_name() {
            // Gleichnamige, früher hochgeladene Datei nicht überschreiben.
            let ziel = crate::downloads::freier_pfad(&erl, &n.to_string_lossy());
            let _ = std::fs::rename(orig, ziel);
        }
    }
    app.state::<Eingang>().eingereiht.lock().unwrap().remove(orig);
}

/// Hauptfenster lädt eine neue Seite: bis Atlas sich wieder meldet, nichts übergeben.
pub fn seite_wechselt(app: &AppHandle) {
    app.state::<Eingang>().bereit.store(false, Ordering::SeqCst);
}

/// Von `bruecke.js`: Atlas hört (nicht mehr) auf übergebene Dateien.
#[tauri::command]
pub fn eingang_bereit(app: AppHandle, webview: tauri::Webview, bereit: bool) {
    if webview.label() != fenster::HAUPT {
        return;
    }
    let vorher = app.state::<Eingang>().bereit.swap(bereit, Ordering::SeqCst);
    if bereit && !vorher {
        abarbeiten(&app);
    }
}

fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "heic" => "image/heic",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "xml" => "application/xml",
        "eml" => "message/rfc822",
        _ => "application/octet-stream",
    }
}

pub fn passt(pfad: &Path) -> bool {
    pfad.extension()
        .and_then(|e| e.to_str())
        .map(|e| ENDUNGEN.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn jetzt() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Ziel nach der Einstellung „Wohin gehen neue Belege?“ (Dock, Menüleiste,
/// Bildschirmfoto, Schnellerfassung, überwachter Ordner).
fn ziel_aus_einstellung(app: &AppHandle) -> serde_json::Value {
    match einstellungen::aktuell(app).eingang_ziel.as_str() {
        "persoenlich" => importordner::ziel_persoenlich(),
        "aktueller_workspace" => serde_json::json!({ "art": "aktuell" }),
        _ => serde_json::json!({ "art": "fragen" }),
    }
}

/// Datei mit dem Ziel aus den Einstellungen einreihen.
pub fn einreihen(app: &AppHandle, pfad: &Path, quelle: &str, original: Option<PathBuf>) -> Result<(), String> {
    let z = ziel_aus_einstellung(app);
    einreihen_mit_ziel(app, pfad, quelle, original, z)
}

/// Datei in die Warteschlange kopieren und gleich versuchen zu senden.
fn einreihen_mit_ziel(
    app: &AppHandle,
    pfad: &Path,
    quelle: &str,
    original: Option<PathBuf>,
    ziel: serde_json::Value,
) -> Result<(), String> {
    let gr = std::fs::metadata(pfad).map_err(|e| e.to_string())?.len();
    if gr == 0 || gr > MAX_BYTES {
        return Err(t("Datei ist leer oder größer als 25 MB.", "File is empty or larger than 25 MB.").into());
    }
    let dir = ordner(app).ok_or("Warteschlange nicht verfügbar")?;
    let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Beleg".into());
    let bytes = std::fs::read(pfad).map_err(|e| e.to_string())?;
    let hash = hash_hex(&bytes);
    // Derselbe Inhalt wartet schon bzw. ging gerade erst hoch: nicht noch einmal.
    let kuerzlich = app
        .state::<Eingang>()
        .kuerzlich
        .lock()
        .unwrap()
        .get(&hash)
        .is_some_and(|t| t.elapsed() < SPERRZEIT);
    if kuerzlich || schon_eingereiht(&dir, &hash) {
        log::info!("Eingang ({quelle}): gleicher Inhalt schon übergeben bzw. wartend — übersprungen");
        if let Some(orig) = original.as_deref().filter(|_| kuerzlich) {
            // Gerade hochgeladen: Original wie nach einer Bestätigung ablegen.
            original_ablegen(app, orig);
        }
        if original.is_none() {
            let _ = crate::mitteilung::zeigen(
                app,
                t("Schon im Atlas-Eingang", "Already in your Atlas inbox"),
                &format!("{name} {}", t("wurde gerade schon übergeben.", "was just handed over.")),
            );
        }
        return Ok(());
    }
    let token = format!("{:x}{:x}", jetzt(), rand_teil());
    std::fs::write(dir.join(format!("{token}.bin")), &bytes).map_err(|e| e.to_string())?;
    let meta = Meta {
        name,
        quelle: quelle.into(),
        original,
        zeit: jetzt(),
        ziel: Some(ziel),
        uebergaben: 0,
        hash: Some(hash),
    };
    std::fs::write(dir.join(format!("{token}.json")), serde_json::to_vec(&meta).unwrap_or_default())
        .map_err(|e| e.to_string())?;
    abarbeiten(app);
    Ok(())
}

fn rand_teil() -> u64 {
    // Ausreichend für eindeutige Dateinamen in der Warteschlange.
    let n = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    (n as u64) ^ (std::process::id() as u64) << 20
}

/// Anzahl wartender Dateien (für Einstellungen und Menüleiste).
pub fn anzahl(app: &AppHandle) -> usize {
    ordner(app)
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|r| r.filter_map(Result::ok).filter(|e| e.path().extension().is_some_and(|x| x == "json")).count())
        .unwrap_or(0)
}

/// Wartende Dateien an Atlas übergeben, sobald das Hauptfenster Atlas zeigt
/// und die Seite gemeldet hat, dass sie Dateien annimmt.
pub fn abarbeiten(app: &AppHandle) {
    let Some(w) = fenster::hauptfenster(app) else { return };
    let auf_atlas = w.url().map(|u| ziel::ist_atlas(&u)).unwrap_or(false);
    let zustand = app.state::<Eingang>();
    if !auf_atlas || !zustand.bereit.load(Ordering::SeqCst) {
        return;
    }
    let Some(dir) = ordner(app) else { return };
    let Ok(eintraege) = std::fs::read_dir(&dir) else { return };
    let mut paket = Vec::new();
    let mut unklar = Vec::new();
    let mut groesse = 0u64;
    for e in eintraege.filter_map(Result::ok) {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let token = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if zustand.unterwegs.lock().unwrap().get(&token).is_some_and(|t| t.elapsed() < WARTEZEIT) {
            continue;
        }
        let Some(mut meta) = meta_lesen(&p) else { continue };
        if meta.uebergaben >= MAX_UEBERGABEN {
            // Mehrfach übergeben, nie bestätigt: nicht endlos weiterreichen.
            unklar_ablegen(app, &dir, &token, &meta);
            unklar.push(meta.name);
            continue;
        }
        let Ok(bytes) = std::fs::read(dir.join(format!("{token}.bin"))) else { continue };
        meta.uebergaben += 1;
        if std::fs::write(&p, serde_json::to_vec(&meta).unwrap_or_default()).is_err() {
            // Ohne gezählte Übergabe nicht senden — sonst droht wieder eine Schleife.
            continue;
        }
        zustand.unterwegs.lock().unwrap().insert(token.clone(), Instant::now());
        groesse += bytes.len() as u64;
        paket.push(serde_json::json!({
            "token": token,
            "name": meta.name,
            "mime": mime(&meta.name),
            "quelle": meta.quelle,
            "ziel": meta.ziel.unwrap_or_else(|| ziel_aus_einstellung(app)),
            "inhalt": base64::engine::general_purpose::STANDARD.encode(&bytes),
        }));
        // Je Übergabe höchstens ~40 MB bzw. 10 Dateien.
        if paket.len() >= 10 || groesse > 40 * 1024 * 1024 {
            break;
        }
    }
    if !unklar.is_empty() {
        let text = if unklar.len() == 1 {
            format!(
                "„{}“ {}",
                unklar[0],
                t(
                    "konnte nicht bestätigt werden — bitte im Atlas-Eingang prüfen.",
                    "could not be confirmed — please check your Atlas inbox.",
                )
            )
        } else {
            format!(
                "{} {}",
                unklar.len(),
                t(
                    "Dateien konnten nicht bestätigt werden — bitte im Atlas-Eingang prüfen.",
                    "files could not be confirmed — please check your Atlas inbox.",
                )
            )
        };
        let _ = crate::mitteilung::zeigen(app, t("Atlas-Eingang", "Atlas inbox"), &text);
    }
    if paket.is_empty() {
        return;
    }
    let js = format!(
        "window.__atlasDesktopDateien && window.__atlasDesktopDateien({})",
        serde_json::Value::Array(paket)
    );
    let _ = w.eval(&js);
}

/// Datei ohne Bestätigung: Kopie nach `warteschlange/unklar`, Original nach
/// „Hochgeladen“ (Atlas hat sie mehrfach bekommen und erkennt Doppelte selbst).
fn unklar_ablegen(app: &AppHandle, dir: &Path, token: &str, meta: &Meta) {
    let ziel = unklar_ordner(dir);
    let _ = std::fs::create_dir_all(&ziel);
    for endung in ["bin", "json"] {
        let von = dir.join(format!("{token}.{endung}"));
        if std::fs::rename(&von, ziel.join(format!("{token}.{endung}"))).is_err() {
            let _ = std::fs::remove_file(&von);
        }
    }
    if let Some(orig) = &meta.original {
        original_ablegen(app, orig);
    }
    let zustand = app.state::<Eingang>();
    if let Some(h) = &meta.hash {
        zustand.kuerzlich.lock().unwrap().insert(h.clone(), Instant::now());
    }
    zustand.unterwegs.lock().unwrap().remove(token);
    log::warn!("Eingang: {token} nach {MAX_UEBERGABEN} Übergaben ohne Bestätigung — unklar");
}

/// Bestätigung von Atlas: erledigte Dateien löschen, Originale aus dem
/// überwachten Ordner nach „Hochgeladen“ verschieben. Kommt die Bestätigung
/// erst, nachdem die Datei als „unklar“ abgelegt wurde, wird auch diese Kopie
/// entfernt.
#[tauri::command]
pub fn dateien_erledigt(app: AppHandle, erledigt: Vec<String>, fehler: Vec<String>, ziel: Option<String>) {
    let Some(dir) = ordner(&app) else { return };
    let zustand = app.state::<Eingang>();
    let mut ok = 0;
    for token in erledigt.iter().filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_hexdigit())) {
        let mut gefunden = false;
        for basis in [dir.clone(), unklar_ordner(&dir)] {
            let meta_pfad = basis.join(format!("{token}.json"));
            let Some(meta) = meta_lesen(&meta_pfad) else { continue };
            gefunden = true;
            if let Some(orig) = meta.original.as_deref().filter(|o| o.exists()) {
                original_ablegen(&app, orig);
            }
            if let Some(h) = meta.hash {
                zustand.kuerzlich.lock().unwrap().insert(h, Instant::now());
            }
            let _ = std::fs::remove_file(&meta_pfad);
            let _ = std::fs::remove_file(basis.join(format!("{token}.bin")));
        }
        zustand.unterwegs.lock().unwrap().remove(token);
        if gefunden {
            ok += 1;
        }
    }
    // Fehlgeschlagen (z. B. kein Netz): Atlas hat geantwortet, die Datei ist
    // sicher nicht hochgeladen. Übergabe nicht mitzählen und nach der
    // Wartezeit erneut versuchen.
    for token in fehler.iter().filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_hexdigit())) {
        let meta_pfad = dir.join(format!("{token}.json"));
        if let Some(mut meta) = meta_lesen(&meta_pfad) {
            meta.uebergaben = meta.uebergaben.saturating_sub(1);
            let _ = std::fs::write(&meta_pfad, serde_json::to_vec(&meta).unwrap_or_default());
        }
        zustand.unterwegs.lock().unwrap().insert(token.clone(), Instant::now());
    }
    if ok > 0 {
        let text = match ziel.map(|z| z.chars().take(200).collect::<String>()) {
            Some(z) if !z.is_empty() => format!("{ok} → {z}"),
            _ => format!("{ok}"),
        };
        let _ = crate::mitteilung::zeigen(&app, t("Im Atlas-Eingang", "In your Atlas inbox"), &text);
    }
}

/// Überwachten Ordner und Import-Ordner prüfen: neue, fertig geschriebene
/// Dateien einreihen.
fn ordner_pruefen(app: &AppHandle) {
    if let Some(pfad) = einstellungen::aktuell(app).ueberwachter_ordner {
        let z = ziel_aus_einstellung(app);
        ordner_scannen(app, Path::new(&pfad), "ordner", &z);
    }
    for (dir, z) in importordner::ordner_mit_ziel(app) {
        ordner_scannen(app, &dir, "import", &z);
    }
}

/// Teilweise geladene bzw. noch geschriebene Dateien (Browser, Sync-Dienste).
fn unfertig(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with('.') || n.ends_with(".download") || n.ends_with(".crdownload") || n.ends_with(".part")
}

fn ordner_scannen(app: &AppHandle, basis: &Path, quelle: &str, ziel: &serde_json::Value) {
    let Ok(eintraege) = std::fs::read_dir(basis) else { return };
    let zustand = app.state::<Eingang>();
    for e in eintraege.filter_map(Result::ok) {
        let p = e.path();
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        // Nur echte Dateien: `file_type` folgt keinem Symlink. Ein Link im Ordner
        // (z. B. „x.pdf“ → ~/.ssh/…) würde sonst die Zieldatei hochladen.
        let echte_datei = e.file_type().is_ok_and(|t| t.is_file());
        if !echte_datei || unfertig(&name) || !passt(&p) {
            continue;
        }
        if zustand.eingereiht.lock().unwrap().contains(&p) {
            continue;
        }
        let gr = e.metadata().map(|m| m.len()).unwrap_or(0);
        // Erst laden, wenn die Größe zwischen zwei Durchläufen gleich blieb.
        let stabil = {
            let mut g = zustand.gesehen.lock().unwrap();
            let vorher = g.insert(p.clone(), gr);
            vorher == Some(gr) && gr > 0
        };
        if !stabil {
            continue;
        }
        match einreihen_mit_ziel(app, &p, quelle, Some(p.clone()), ziel.clone()) {
            Ok(()) => {
                zustand.eingereiht.lock().unwrap().insert(p.clone());
            }
            Err(err) => log::warn!("Ordner ({quelle}): {name} nicht eingereiht: {err}"),
        }
    }
}

/// Hintergrund: Ordner prüfen und Warteschlange senden (alle 20 Sekunden).
pub fn starten(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(20)).await;
            let a = app.clone();
            let _ = app.run_on_main_thread(move || {
                ordner_pruefen(&a);
                abarbeiten(&a);
            });
        }
    });
}

/// Dateien, die aufs Dock-Symbol gezogen bzw. mit „Öffnen mit“ an die App
/// gegeben wurden (macOS: RunEvent::Opened).
pub fn geoeffnet(app: &AppHandle, urls: &[url::Url]) {
    let mut n = 0;
    for u in urls {
        if let Ok(p) = u.to_file_path() {
            if p.is_file() && einreihen(app, &p, "dock", None).is_ok() {
                n += 1;
            }
        }
    }
    if n > 0 {
        fenster::hauptfenster_zeigen(app);
        let _ = app.state::<Zustand>();
    }
}

/// Datei-Auswahl (Menüleiste „Beleg hochladen…“).
pub fn auswaehlen(app: &AppHandle) {
    use tauri_plugin_dialog::DialogExt;
    let a = app.clone();
    app.dialog()
        .file()
        .set_title(t("Belege in den Atlas-Eingang", "Receipts to your Atlas inbox"))
        .add_filter(t("Belege", "Receipts"), ENDUNGEN)
        .pick_files(move |dateien| {
            for f in dateien.unwrap_or_default() {
                if let Some(p) = f.as_path() {
                    if let Err(e) = einreihen(&a, p, "auswahl", None) {
                        log::warn!("Auswahl nicht eingereiht: {e}");
                    }
                }
            }
        });
}

/// Bildschirmfoto (Bereich wählen) direkt in den Eingang — macOS `screencapture`.
pub fn bildschirmfoto(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let a = app.clone();
        std::thread::spawn(move || {
            let ziel = std::env::temp_dir().join(format!("Bildschirmfoto-{}.png", jetzt()));
            let ok = std::process::Command::new("/usr/sbin/screencapture")
                .args(["-i", "-x"])
                .arg(&ziel)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok && ziel.exists() {
                let a2 = a.clone();
                let _ = a.run_on_main_thread(move || {
                    let _ = einreihen(&a2, &ziel, "bildschirmfoto", None);
                    let _ = std::fs::remove_file(&ziel);
                });
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
    }
}
