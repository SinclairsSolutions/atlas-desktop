//! App-Symbol: schwarz mit weißem Logo (Standard) oder weiß mit schwarzem Logo.
//! Die Wahl steht in den Einstellungen (einstellungen.rs) und gilt ab dem Start.
//! macOS: Dock-Symbol zur Laufzeit (NSApplication), Windows: Fenstersymbol.
//! Das Symbol im Finder bzw. Startmenü bleibt das mitgelieferte (schwarz).
//!
//! Windows bekommt eigene, randlose Symbole (scripts/windows-symbole-erzeugen.py):
//! das Mac-Symbol hat Rand und Schatten nach Apple-Vorgabe und wirkte in der
//! Taskleiste klein und gerahmt.

use tauri::AppHandle;

#[cfg(target_os = "macos")]
const SCHWARZ: &[u8] = include_bytes!("../icons/dock-schwarz.png");
// Weiß als .icns (alle Größen, für kleine Größen kräftiger gezeichnet).
#[cfg(target_os = "macos")]
const WEISS: &[u8] = include_bytes!("../icons/dock-weiss.icns");
#[cfg(not(target_os = "macos"))]
const SCHWARZ: &[u8] = include_bytes!("../icons/windows-schwarz.png");
#[cfg(not(target_os = "macos"))]
const WEISS: &[u8] = include_bytes!("../icons/windows-weiss.png");

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
        // Schwarz: das Symbol aus der Programmdatei (icon.ico, alle Größen) —
        // ein einzelnes zur Laufzeit gesetztes Bild skaliert Windows schlechter.
        let standard = if v == Variante::Schwarz { app.default_window_icon().cloned() } else { None };
        if let Some(bild) = standard.or_else(|| tauri::image::Image::from_bytes(bytes).ok()) {
            for w in app.webview_windows().values() {
                let _ = w.set_icon(bild.clone());
            }
        }
    }
}
