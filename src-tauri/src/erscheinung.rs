//! Fensterhintergrund passend zur Darstellung von Atlas (hell/dunkel, Farbton).
//!
//! Atlas meldet über `__ATLAS_DESKTOP__.erscheinungGeaendert({ dunkel, fenster, farbe })`,
//! ob die Seite gerade dunkel ist und welche Hintergrundfarbe das Fenster hat
//! (`farbe` als `#rrggbb`: im Dunkelmodus der gewählte Dunkel-Ton, hell `#fcfcfc`).
//! Die App färbt daraufhin den Hintergrund genau dieses Fensters (sichtbar beim
//! Laden, Vergrößern, Überscrollen) und merkt sich die Farbe je Fenster-Label,
//! damit neue Fenster gleich richtig anfangen statt hell aufzublitzen. Fehlt
//! `farbe` (ältere Atlas-Stände), gelten die festen Farben unten.
//!
//! Bewusst kein `set_theme`: Das Erscheinungsbild des Fensters bestimmt auch
//! `prefers-color-scheme` der Seite. Stünde das Fenster fest auf „dunkel“,
//! läse Atlas im Modus „wie System“ danach den falschen Systemwert.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use tauri::{window::Color, AppHandle, Manager, WebviewWindow};

/// Hell: wie bisher.
pub const HELL: Color = Color(247, 248, 246, 255);
/// Dunkel: `--background` aus `.dark` in Atlas (`oklch(0.175 0.022 162)`), tiefes Waldgrün.
pub const DUNKEL: Color = Color(7, 20, 13, 255);

/// Label des Hauptfensters (Rückfall für neue Fenster).
const HAUPT: &str = crate::fenster::HAUPT;
/// Höchstzahl gemerkter Fenster (Nebenfenster-Labels zählen hoch).
const MAX_FENSTER: usize = 32;

#[derive(Default)]
pub struct Erscheinung {
    dunkel: AtomicBool,
    /// Zuletzt gemeldete Farbe je Fenster-Label.
    farben: Mutex<BTreeMap<String, Color>>,
}

impl Erscheinung {
    fn farben(&self) -> MutexGuard<'_, BTreeMap<String, Color>> {
        self.farben.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn datei(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("erscheinung.json"))
}

/// Streng `#rrggbb` (genau sieben Zeichen, Hex-Ziffern).
fn farbe_lesen(s: &str) -> Option<Color> {
    let b = s.as_bytes();
    if b.len() != 7 || b[0] != b'#' || !b[1..].iter().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let teil = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some(Color(teil(1)?, teil(3)?, teil(5)?, 255))
}

fn farbe_text(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

fn fest(dunkel: bool) -> Color {
    if dunkel {
        DUNKEL
    } else {
        HELL
    }
}

/// Beim Start: zuletzt bekannten Stand laden.
pub fn laden(app: &AppHandle) {
    let wert = datei(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let dunkel = wert.as_ref().and_then(|v| v.get("dunkel")).and_then(|d| d.as_bool()).unwrap_or(false);
    let zustand = app.state::<Erscheinung>();
    zustand.dunkel.store(dunkel, Ordering::Relaxed);
    if let Some(farben) = wert.as_ref().and_then(|v| v.get("farben")).and_then(|f| f.as_object()) {
        let mut karte = zustand.farben();
        for (label, f) in farben.iter().take(MAX_FENSTER) {
            if let Some(c) = f.as_str().and_then(farbe_lesen) {
                karte.insert(label.chars().take(64).collect(), c);
            }
        }
    }
}

/// Hintergrund für ein Fenster beim Bauen: zuletzt gemeldete Farbe dieses
/// Labels, sonst die des Hauptfensters, sonst die feste Farbe zum letzten Stand.
pub fn hintergrund(app: &AppHandle, label: &str) -> Color {
    let zustand = app.state::<Erscheinung>();
    let karte = zustand.farben();
    karte
        .get(label)
        .or_else(|| karte.get(HAUPT))
        .copied()
        .unwrap_or_else(|| fest(zustand.dunkel.load(Ordering::Relaxed)))
}

fn speichern(app: &AppHandle) {
    let zustand = app.state::<Erscheinung>();
    let farben: serde_json::Map<String, serde_json::Value> = zustand
        .farben()
        .iter()
        .map(|(l, c)| (l.clone(), serde_json::Value::String(farbe_text(*c))))
        .collect();
    let inhalt = serde_json::json!({
        "dunkel": zustand.dunkel.load(Ordering::Relaxed),
        "farben": farben,
    });
    if let Some(p) = datei(app) {
        let _ = p.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(p, inhalt.to_string());
    }
}

/// Atlas meldet seine aktuelle Darstellung (`fenster` = Fensterart in Atlas,
/// nur fürs Protokoll; `farbe` = Fensterhintergrund als `#rrggbb`, optional).
#[tauri::command]
pub fn erscheinung_geaendert(
    app: AppHandle,
    window: WebviewWindow,
    dunkel: bool,
    fenster: Option<String>,
    farbe: Option<String>,
) {
    let gemeldet = farbe.as_deref().and_then(farbe_lesen);
    if farbe.is_some() && gemeldet.is_none() {
        log::warn!("Darstellung: ungültige Farbe ignoriert");
    }
    let farbe = gemeldet.unwrap_or_else(|| fest(dunkel));
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
    let zustand = app.state::<Erscheinung>();
    let alt_dunkel = zustand.dunkel.swap(dunkel, Ordering::Relaxed);
    let farbe_neu = {
        let mut karte = zustand.farben();
        let label = window.label().to_string();
        let neu = karte.get(&label) != Some(&farbe);
        if neu {
            // Platz halten: zuerst ein Nebenfenster verwerfen, nie das Hauptfenster.
            if !karte.contains_key(&label) && karte.len() >= MAX_FENSTER {
                if let Some(weg) = karte.keys().find(|l| l.as_str() != HAUPT).cloned() {
                    karte.remove(&weg);
                }
            }
            karte.insert(label, farbe);
        }
        neu
    };
    if alt_dunkel != dunkel || farbe_neu {
        log::info!(
            "Darstellung: {} {} ({})",
            if dunkel { "dunkel" } else { "hell" },
            farbe_text(farbe),
            fenster.as_deref().unwrap_or("-").chars().take(40).collect::<String>()
        );
        speichern(&app);
    }
}
