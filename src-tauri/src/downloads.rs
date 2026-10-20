//! Downloads landen im Download-Ordner (gleiche Namen bekommen „(1)“, „(2)“ …,
//! das erledigt WebKit bzw. WebView2). Nach Abschluss eine System-Mitteilung.

use tauri::{webview::DownloadEvent, AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

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
            let name = path
                .or(gemerkt)
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (titel, text) = if success {
                (t("Download abgeschlossen", "Download complete"), name)
            } else {
                (
                    t("Download fehlgeschlagen", "Download failed"),
                    t("Bitte erneut versuchen.", "Please try again.").to_string(),
                )
            };
            let _ = app.notification().builder().title(titel).body(text).show();
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
pub fn datei_speichern(app: AppHandle, name: String, inhalt: String) -> Result<String, String> {
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
    .or_else(|| app.path().download_dir().ok())
    .ok_or("Download-Ordner nicht gefunden.")?;
    let ziel = freier_pfad(&dir, &sauberer_name(&name));
    std::fs::write(&ziel, &daten).map_err(|e| e.to_string())?;
    let anzeige = ziel.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    log::info!("Download gespeichert: {} Bytes", daten.len());
    let _ = app
        .notification()
        .builder()
        .title(t("Download abgeschlossen", "Download complete"))
        .body(anzeige.clone())
        .show();
    Ok(anzeige)
}
