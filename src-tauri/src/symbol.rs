//! App-Symbol: schwarz mit weißem Logo (Standard) oder weiß mit schwarzem Logo.
//! Die Wahl wird im App-Konfigurationsordner gemerkt und beim Start gesetzt.
//! macOS: Dock-Symbol zur Laufzeit (NSApplication), Windows: Fenstersymbol.
//! Das Symbol im Finder bzw. Startmenü bleibt das mitgelieferte (schwarz).

use tauri::{AppHandle, Manager};

const SCHWARZ: &[u8] = include_bytes!("../icons/dock-schwarz.png");
const WEISS: &[u8] = include_bytes!("../icons/dock-weiss.png");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Variante {
    Schwarz,
    Weiss,
}

fn datei(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("symbol.txt"))
}

pub fn gemerkt(app: &AppHandle) -> Variante {
    match datei(app).and_then(|p| std::fs::read_to_string(p).ok()).as_deref().map(str::trim) {
        Some("weiss") => Variante::Weiss,
        _ => Variante::Schwarz,
    }
}

pub fn setzen(app: &AppHandle, v: Variante, merken: bool) {
    if merken {
        if let Some(p) = datei(app) {
            let _ = p.parent().map(std::fs::create_dir_all);
            let _ = std::fs::write(p, if v == Variante::Weiss { "weiss" } else { "schwarz" });
        }
    }
    let bytes = if v == Variante::Weiss { WEISS } else { SCHWARZ };
    #[cfg(target_os = "macos")]
    {
        use objc2::{AllocAnyThread, MainThreadMarker};
        use objc2_app_kit::{NSApplication, NSImage};
        use objc2_foundation::NSData;
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(mtm) = MainThreadMarker::new() else { return };
            let daten = NSData::with_bytes(bytes);
            if let Some(bild) = NSImage::initWithData(NSImage::alloc(), &daten) {
                let ns = NSApplication::sharedApplication(mtm);
                unsafe { ns.setApplicationIconImage(Some(&bild)) };
            }
            let _ = app2;
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(bild) = tauri::image::Image::from_bytes(bytes) {
            for w in app.webview_windows().values() {
                let _ = w.set_icon(bild.clone());
            }
        }
    }
}
