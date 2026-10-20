//! Downloads landen im Download-Ordner (gleiche Namen bekommen „(1)“, „(2)“ …,
//! das erledigt WebKit bzw. WebView2). Nach Abschluss eine System-Mitteilung.

use tauri::{webview::DownloadEvent, AppHandle, Manager};

use crate::{texte::t, zustand::Zustand};

pub fn ereignis(app: &AppHandle, event: DownloadEvent<'_>) -> bool {
    match event {
        DownloadEvent::Requested { url, destination } => {
            // Nur Debug: Zielordner umleiten (Tests ohne Zugriff auf ~/Downloads).
            if cfg!(debug_assertions) {
                if let (Ok(dir), Some(name)) =
                    (std::env::var("ATLAS_DESKTOP_DOWNLOADS"), destination.file_name())
                {
                    *destination = std::path::Path::new(&dir).join(name);
                }
            }
            log::info!("Download gestartet: {} → {}", url.scheme(), destination.display());
            app.state::<Zustand>()
                .downloads
                .lock()
                .unwrap()
                .insert(url.to_string(), destination.clone());
            true
        }
        DownloadEvent::Finished { url, path, success } => {
            let gemerkt = app.state::<Zustand>().downloads.lock().unwrap().remove(url.as_str());
            match path.or(gemerkt) {
                Some(p) if success => gespeichert(app, &p),
                _ => {
                    let _ = crate::mitteilung::zeigen(
                        app,
                        t("Download fehlgeschlagen", "Download failed"),
                        t("Bitte erneut versuchen.", "Please try again."),
                    );
                }
            }
            true
        }
        _ => true,
    }
}

/// Download-Ordner im Finder bzw. Explorer öffnen (Menü).
pub fn ordner_oeffnen(app: &AppHandle) {
    use tauri_plugin_opener::OpenerExt;
    if let Ok(dir) = app.path().download_dir() {
        let _ = app.opener().open_path(dir.to_string_lossy(), None::<&str>);
    }
}

/// Höchstgröße für Downloads, die die Seite selbst übergibt (Blob/Data-URL).
const MAX_BYTES: usize = 200 * 1024 * 1024;

/// Dateiname ohne Pfadanteile und Steuerzeichen.
fn sauberer_name(name: &str) -> String {
    let n: String = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .take(180)
        .collect();
    let n = n.trim().trim_start_matches('.').to_string();
    if n.is_empty() { "Download".to_string() } else { n }
}

/// Freier Name im Ordner: „Datei.pdf“, „Datei (1).pdf“, …
fn freier_pfad(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stamm, endung) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for i in 1..1000 {
        let k = dir.join(format!("{stamm} ({i}){endung}"));
        if !k.exists() {
            return k;
        }
    }
    dir.join(format!("{stamm}-{}{endung}", std::process::id()))
}

/// Downloads aus Blob- oder Data-Adressen (WebKit meldet sie nicht als
/// Download): Die Seite liest die Datei und übergibt sie hier. Gespeichert
/// wird ausschließlich im Download-Ordner.
#[tauri::command]
pub fn datei_speichern(
    app: AppHandle,
    name: String,
    inhalt: String,
    mandant: Option<String>,
) -> Result<String, String> {
    use base64::Engine;
    let daten = base64::engine::general_purpose::STANDARD
        .decode(inhalt.as_bytes())
        .map_err(|e| format!("Ungültige Daten: {e}"))?;
    if daten.len() > MAX_BYTES {
        return Err("Datei zu groß.".into());
    }
    let dir = if cfg!(debug_assertions) {
        std::env::var("ATLAS_DESKTOP_DOWNLOADS").map(std::path::PathBuf::from).ok()
    } else {
        None
    }
    .or_else(|| export_ordner(&app, mandant.as_deref()))
    .or_else(|| app.path().download_dir().ok())
    .ok_or("Download-Ordner nicht gefunden.")?;
    let ziel = freier_pfad(&dir, &sauberer_name(&name));
    std::fs::write(&ziel, &daten).map_err(|e| e.to_string())?;
    let anzeige = ziel.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    log::info!("Download gespeichert: {} Bytes", daten.len());
    gespeichert(&app, &ziel);
    Ok(anzeige)
}

/// Zuletzt gespeicherte Dateien (für „Im Finder zeigen“ — die Seite bekommt
/// nur den Dateinamen, nie den vollen Pfad).
static ZULETZT: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Nach dem Speichern: Downloads-Stapel im Dock hüpfen lassen (wie Safari)
/// und Atlas Bescheid geben, damit dort „Gespeichert · Im Finder zeigen“ steht.
fn gespeichert(app: &AppHandle, pfad: &std::path::Path) {
    {
        let mut z = ZULETZT.lock().unwrap();
        z.push(pfad.to_path_buf());
        let n = z.len();
        if n > 30 {
            z.drain(0..n - 30);
        }
    }
    #[cfg(target_os = "macos")]
    dock_huepfen(pfad);
    let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ordner = pfad
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let js = format!(
        "window.__atlasDesktopGespeichert && window.__atlasDesktopGespeichert({})",
        serde_json::json!({ "name": name, "ordner": ordner })
    );
    if let Some(w) = crate::fenster::aktives_fenster(app) {
        let _ = w.eval(&js);
    }
}

/// Dock: Downloads-Stapel hüpfen lassen (gleiche Meldung, die Safari schickt).
#[cfg(target_os = "macos")]
fn dock_huepfen(pfad: &std::path::Path) {
    use objc2_foundation::{NSDistributedNotificationCenter, NSString};
    let center = NSDistributedNotificationCenter::defaultCenter();
    let name = NSString::from_str("com.apple.DownloadFileFinished");
    let objekt = NSString::from_str(&pfad.to_string_lossy());
    unsafe {
        center.postNotificationName_object(&name, Some(&objekt));
    }
}

/// „Im Finder zeigen“ für eine zuletzt gespeicherte Datei (nach Namen).
#[tauri::command]
pub fn im_finder_zeigen(name: String) -> Result<(), String> {
    let pfad = ZULETZT
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|p| p.file_name().map(|n| n.to_string_lossy() == name.as_str()).unwrap_or(false))
        .cloned()
        .ok_or("Datei nicht gefunden.")?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("/usr/bin/open").arg("-R").arg(&pfad).spawn().map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer").arg(format!("/select,{}", pfad.display())).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Export-Ordner aus den Einstellungen, auf Wunsch mit Unterordner je Mandant.
fn export_ordner(app: &AppHandle, mandant: Option<&str>) -> Option<std::path::PathBuf> {
    let e = crate::einstellungen::aktuell(app);
    let basis = std::path::PathBuf::from(e.export_ordner?);
    if !basis.is_dir() {
        return None;
    }
    let ziel = match mandant.map(sauberer_name).filter(|m| m != "Download") {
        Some(m) if e.export_je_mandant => basis.join(m),
        _ => basis,
    };
    std::fs::create_dir_all(&ziel).ok()?;
    Some(ziel)
}
