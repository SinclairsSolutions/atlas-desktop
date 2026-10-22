//! Fensterhintergrund passend zur Darstellung von Atlas (hell/dunkel).
//!
//! Atlas meldet über `__ATLAS_DESKTOP__.erscheinungGeaendert({ dunkel, fenster })`,
//! ob die Seite gerade dunkel ist. Die App färbt daraufhin den Hintergrund
//! genau dieses Fensters (sichtbar beim Laden, Vergrößern, Überscrollen) und
//! merkt sich den Stand, damit neue Fenster gleich richtig anfangen statt hell
//! aufzublitzen.
//!
//! Bewusst kein `set_theme`: Das Erscheinungsbild des Fensters bestimmt auch
//! `prefers-color-scheme` der Seite. Stünde das Fenster fest auf „dunkel“,
//! läse Atlas im Modus „wie System“ danach den falschen Systemwert.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{window::Color, AppHandle, Manager, WebviewWindow};

/// Hell: wie bisher.
pub const HELL: Color = Color(247, 248, 246, 255);
/// Dunkel: `--background` aus `.dark` in Atlas (`oklch(0.175 0.022 162)`), tiefes Waldgrün.
pub const DUNKEL: Color = Color(7, 20, 13, 255);

#[derive(Default)]
pub struct Erscheinung {
    dunkel: AtomicBool,
}

fn datei(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("erscheinung.json"))
}

/// Beim Start: zuletzt bekannten Stand laden.
pub fn laden(app: &AppHandle) {
    let dunkel = datei(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("dunkel").and_then(|d| d.as_bool()))
        .unwrap_or(false);
    app.state::<Erscheinung>().dunkel.store(dunkel, Ordering::Relaxed);
}

/// Hintergrund für ein neues Fenster (zuletzt bekannter Stand).
pub fn hintergrund(app: &AppHandle) -> Color {
    if app.state::<Erscheinung>().dunkel.load(Ordering::Relaxed) {
        DUNKEL
    } else {
        HELL
    }
}

/// Atlas meldet seine aktuelle Darstellung (`fenster` = Fensterart in Atlas,
/// nur fürs Protokoll).
#[tauri::command]
pub fn erscheinung_geaendert(app: AppHandle, window: WebviewWindow, dunkel: bool, fenster: Option<String>) {
    let farbe = if dunkel { DUNKEL } else { HELL };
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        // Eine Objective-C-Ausnahme aus AppKit darf nicht durch Rust laufen (Absturz).
        let ergebnis = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(target_os = "macos")]
            {
                objc2::exception::catch(std::panic::AssertUnwindSafe(|| w.set_background_color(Some(farbe))))
                    .map_err(|e| format!("{e:?}"))
                    .and_then(|r| r.map_err(|e| e.to_string()))
            }
            #[cfg(not(target_os = "macos"))]
            {
                w.set_background_color(Some(farbe)).map_err(|e| e.to_string())
            }
        }));
        match ergebnis {
            Ok(Err(e)) => log::warn!("Fensterhintergrund nicht gesetzt: {e}"),
            Err(_) => log::warn!("Fensterhintergrund nicht gesetzt (Abbruch abgefangen)"),
            Ok(Ok(())) => {}
        }
    });
    let alt = app.state::<Erscheinung>().dunkel.swap(dunkel, Ordering::Relaxed);
    if alt != dunkel {
        log::info!(
            "Darstellung: {} ({})",
            if dunkel { "dunkel" } else { "hell" },
            fenster.as_deref().unwrap_or("-").chars().take(40).collect::<String>()
        );
        if let Some(p) = datei(&app) {
            let _ = p.parent().map(std::fs::create_dir_all);
            let _ = std::fs::write(p, serde_json::json!({ "dunkel": dunkel }).to_string());
        }
    }
}
