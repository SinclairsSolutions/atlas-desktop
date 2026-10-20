//! Einstellungen der Desktop-App (Fenster „Einstellungen…“, ⌘,).
//! Gespeichert als JSON im App-Konfigurationsordner — nur Gerät, kein Konto.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Einstellungen {
    /// App-Symbol: "schwarz" | "weiss".
    pub symbol: String,
    /// Zahl ungelesener Mitteilungen am Dock-/Taskleisten-Symbol.
    pub dock_zahl: bool,
    /// Überwachter Ordner: neue Belege darin gehen automatisch in den Eingang.
    pub ueberwachter_ordner: Option<String>,
    /// Exporte (PDF, Excel, DATEV …) statt in „Downloads“ in diesen Ordner.
    pub export_ordner: Option<String>,
    /// Unterordner je Mandant (Workspace) im Export-Ordner.
    pub export_je_mandant: bool,
    /// App beim Öffnen und nach Abwesenheit mit Touch ID entsperren.
    pub touch_id_sperre: bool,
    /// Nach so vielen Minuten im Hintergrund wieder sperren (0 = nur beim Start).
    pub sperre_nach_minuten: u32,
    /// Globales Tastenkürzel für die Schnellerfassung.
    pub schnell_kuerzel: bool,
    /// Symbol in der Menüleiste.
    pub menueleiste: bool,
    /// Zweites Symbol in der Menüleiste: Passwörter-Schnellzugriff (wie Safari).
    pub passwort_symbol: bool,
}

impl Default for Einstellungen {
    fn default() -> Self {
        Self {
            symbol: "schwarz".into(),
            dock_zahl: true,
            ueberwachter_ordner: None,
            export_ordner: None,
            export_je_mandant: true,
            touch_id_sperre: false,
            sperre_nach_minuten: 15,
            schnell_kuerzel: true,
            menueleiste: true,
            passwort_symbol: true,
        }
    }
}

#[derive(Default)]
pub struct Speicher(pub Mutex<Einstellungen>);

fn datei(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("einstellungen.json"))
}

pub fn laden(app: &AppHandle) -> Einstellungen {
    let mut e: Einstellungen = datei(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    // Frühere Symbol-Wahl (symbol.txt aus 0.1.1) übernehmen.
    if let Some(alt) = app
        .path()
        .app_config_dir()
        .ok()
        .and_then(|d| std::fs::read_to_string(d.join("symbol.txt")).ok())
    {
        if alt.trim() == "weiss" {
            e.symbol = "weiss".into();
        }
    }
    e
}

pub fn aktuell(app: &AppHandle) -> Einstellungen {
    app.state::<Speicher>().0.lock().unwrap().clone()
}

pub fn speichern(app: &AppHandle, e: &Einstellungen) {
    *app.state::<Speicher>().0.lock().unwrap() = e.clone();
    if let Some(p) = datei(app) {
        let _ = p.parent().map(std::fs::create_dir_all);
        if let Ok(s) = serde_json::to_string_pretty(e) {
            let _ = std::fs::write(p, s);
        }
    }
}
