//! Downloads landen im Download-Ordner (gleiche Namen bekommen „(1)“, „(2)“ …,
//! das erledigt WebKit bzw. WebView2). Nach Abschluss eine System-Mitteilung.

use tauri::{webview::DownloadEvent, AppHandle};
use tauri_plugin_notification::NotificationExt;

use crate::texte::t;

pub fn ereignis(app: &AppHandle, event: DownloadEvent<'_>) -> bool {
    match event {
        DownloadEvent::Requested { url, destination } => {
            log::info!("Download gestartet: {} → {}", url.scheme(), destination.display());
            true
        }
        DownloadEvent::Finished { path, success, .. } => {
            let name = path
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
    use tauri::Manager;
    use tauri_plugin_opener::OpenerExt;
    if let Ok(dir) = app.path().download_dir() {
        let _ = app.opener().open_path(dir.to_string_lossy(), None::<&str>);
    }
}
