//! App-Symbol: schwarz mit weißem Logo (Standard) oder weiß mit schwarzem Logo.
//! Die Wahl steht in den Einstellungen (einstellungen.rs) und gilt ab dem Start.
//! macOS: Dock-Symbol zur Laufzeit (NSApplication), Windows: Fenstersymbol.
//! Das Symbol im Finder bzw. Startmenü bleibt das mitgelieferte (schwarz).
//!
//! macOS 26 (Tahoe): Das mitgelieferte Symbol kommt aus Assets.car
//! (CFBundleIconName „AppIcon“, gebaut aus icons/AppIcon.icon in der CI). Nur
//! so zeichnet das System es selbst — groß wie die übrigen Apps, scharf und in
//! jeder Darstellung (hell, dunkel, getönt, klar). Ein .icns allein steckt
//! macOS 26 verkleinert auf eine eigene Kachel.
//!
//! Grenze beim weißen Symbol: macOS kennt (anders als iOS) keinen Wechsel auf
//! ein alternatives App-Symbol. Es bleibt nur `setApplicationIconImage`, und
//! ein so gesetztes Bild ist ein fertiges Bitmap — das System passt es nicht
//! mehr an Dunkel/Getönt/Klar an, und nach dem Beenden zeigt das Dock wieder
//! das mitgelieferte (schwarze). Damit es wenigstens genauso scharf und groß
//! ist wie das Standard-Symbol, nehmen wir die vom System vorgerenderte Fassung
//! von „AppIconWeiss“ aus Assets.car (1024 px, Tahoe-Raster samt Maske und
//! Schatten). Nur wenn die fehlt (lokaler Build ohne Assets.car), greift das
//! alte dock-weiss.icns.

use tauri::AppHandle;

const SCHWARZ: &[u8] = include_bytes!("../icons/dock-schwarz.png");
// Weiß als .icns (alle Größen, für kleine Größen kräftiger gezeichnet).
#[cfg(target_os = "macos")]
const WEISS: &[u8] = include_bytes!("../icons/dock-weiss.icns");
#[cfg(not(target_os = "macos"))]
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
            // Vom System gerenderte Fassung aus Assets.car (siehe oben).
            let bild = NSImage::imageNamed(objc2_foundation::ns_string!("AppIconWeiss"))
                .or_else(|| NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(bytes)));
            if let Some(bild) = bild {
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
