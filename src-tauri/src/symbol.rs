//! App-Symbol: schwarz mit weißem Logo (Standard) oder weiß mit schwarzem Logo.
//! Die Wahl steht in den Einstellungen (einstellungen.rs) und gilt ab dem Start.
//! macOS: Dock-Symbol zur Laufzeit (NSApplication), Windows: Fenstersymbol.
//! Das Symbol im Finder bzw. Startmenü bleibt das mitgelieferte (schwarz).

use tauri::AppHandle;

const SCHWARZ: &[u8] = include_bytes!("../icons/dock-schwarz.png");
const WEISS: &[u8] = include_bytes!("../icons/dock-weiss.png");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Variante {
    Schwarz,
    Weiss,
}

pub fn setzen(app: &AppHandle, v: Variante) {
    let bytes = if v == Variante::Weiss { WEISS } else { SCHWARZ };
    #[cfg(target_os = "macos")]
    {
        use objc2::{AllocAnyThread, MainThreadMarker};
        use objc2_app_kit::{NSApplication, NSImage};
        use objc2_foundation::NSData;
        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(mtm) = MainThreadMarker::new() else { return };
            let ns = NSApplication::sharedApplication(mtm);
            if v == Variante::Schwarz {
                // Standard: das mitgelieferte Symbol (icon.icns, alle Auflösungen) —
                // ein zur Laufzeit gesetztes Bild wirkt im Dock unschärfer.
                unsafe { ns.setApplicationIconImage(None) };
                return;
            }
            let daten = NSData::with_bytes(bytes);
            if let Some(bild) = NSImage::initWithData(NSImage::alloc(), &daten) {
                unsafe { ns.setApplicationIconImage(Some(&bild)) };
            }
            let _ = app2;
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::Manager;
        if let Ok(bild) = tauri::image::Image::from_bytes(bytes) {
            for w in app.webview_windows().values() {
                let _ = w.set_icon(bild.clone());
            }
        }
    }
}
