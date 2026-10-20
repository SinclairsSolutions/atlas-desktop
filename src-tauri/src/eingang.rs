//! Belege in den Atlas-Eingang: aufs Dock-Symbol ziehen, über die Menüleiste
//! wählen, Bildschirmfoto oder überwachter Ordner.
//!
//! Alles läuft über eine Warteschlange im App-Datenordner. Sie wird an die
//! Atlas-Seite im Hauptfenster übergeben (`window.__atlasDesktopDateien`);
//! Atlas lädt mit der Anmeldung der Person hoch und bestätigt je Datei
//! (`dateien_erledigt`). Erst dann wird die Datei aus der Warteschlange
//! gelöscht — ohne Netz oder ohne Anmeldung bleibt sie liegen und geht beim
//! nächsten Versuch raus (alle 20 Sekunden).

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Mutex,
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
}

#[derive(Default)]
pub struct Eingang {
    /// Token → Zeitpunkt der Übergabe (gegen doppeltes Senden).
    unterwegs: Mutex<HashMap<String, Instant>>,
    /// Überwachter Ordner: zuletzt gesehene Größe je Datei (erst stabile Dateien laden).
    gesehen: Mutex<HashMap<PathBuf, u64>>,
    /// Bereits eingereihte Originale (Pfad), damit nichts doppelt kommt.
    eingereiht: Mutex<HashSet<PathBuf>>,
}

fn ordner(app: &AppHandle) -> Option<PathBuf> {
    let d = app.path().app_data_dir().ok()?.join("warteschlange");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
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
    let token = format!("{:x}{:x}", jetzt(), rand_teil());
    std::fs::copy(pfad, dir.join(format!("{token}.bin"))).map_err(|e| e.to_string())?;
    let meta = Meta { name, quelle: quelle.into(), original, zeit: jetzt(), ziel: Some(ziel) };
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

/// Wartende Dateien an Atlas übergeben, sobald das Hauptfenster Atlas zeigt.
pub fn abarbeiten(app: &AppHandle) {
    let Some(w) = fenster::hauptfenster(app) else { return };
    let auf_atlas = w.url().map(|u| ziel::ist_atlas(&u)).unwrap_or(false);
    if !auf_atlas {
        return;
    }
    let Some(dir) = ordner(app) else { return };
    let Ok(eintraege) = std::fs::read_dir(&dir) else { return };
    let zustand = app.state::<Eingang>();
    let mut paket = Vec::new();
    let mut groesse = 0u64;
    for e in eintraege.filter_map(Result::ok) {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let token = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        {
            let mut u = zustand.unterwegs.lock().unwrap();
            if u.get(&token).is_some_and(|t| t.elapsed() < Duration::from_secs(120)) {
                continue;
            }
            u.insert(token.clone(), Instant::now());
        }
        let Ok(meta) = std::fs::read(&p).map_err(|_| ()).and_then(|b| serde_json::from_slice::<Meta>(&b).map_err(|_| ())) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(dir.join(format!("{token}.bin"))) else { continue };
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
    if paket.is_empty() {
        return;
    }
    let js = format!(
        "window.__atlasDesktopDateien && window.__atlasDesktopDateien({})",
        serde_json::Value::Array(paket)
    );
    let _ = w.eval(&js);
}

/// Bestätigung von Atlas: erledigte Dateien löschen, Originale aus dem
/// überwachten Ordner nach „Hochgeladen“ verschieben.
#[tauri::command]
pub fn dateien_erledigt(app: AppHandle, erledigt: Vec<String>, fehler: Vec<String>, ziel: Option<String>) {
    let Some(dir) = ordner(&app) else { return };
    let zustand = app.state::<Eingang>();
    let mut ok = 0;
    for token in erledigt.iter().filter(|t| t.chars().all(|c| c.is_ascii_hexdigit())) {
        let meta_pfad = dir.join(format!("{token}.json"));
        if let Some(meta) = std::fs::read(&meta_pfad).ok().and_then(|b| serde_json::from_slice::<Meta>(&b).ok()) {
            if let Some(orig) = meta.original {
                if let Some(eltern) = orig.parent() {
                    let erl = eltern.join(importordner::hochgeladen());
                    let _ = std::fs::create_dir_all(&erl);
                    if let Some(n) = orig.file_name() {
                        let _ = std::fs::rename(&orig, erl.join(n));
                    }
                }
                zustand.eingereiht.lock().unwrap().remove(&orig);
            }
        }
        let _ = std::fs::remove_file(&meta_pfad);
        let _ = std::fs::remove_file(dir.join(format!("{token}.bin")));
        zustand.unterwegs.lock().unwrap().remove(token);
        ok += 1;
    }
    // Fehlgeschlagene dürfen beim nächsten Durchlauf erneut versucht werden.
    for token in &fehler {
        zustand.unterwegs.lock().unwrap().remove(token);
    }
    if ok > 0 {
        let text = match ziel {
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
        if !p.is_file() || unfertig(&name) || !passt(&p) {
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
