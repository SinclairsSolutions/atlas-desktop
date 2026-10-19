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
