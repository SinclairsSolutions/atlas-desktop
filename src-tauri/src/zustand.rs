use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32},
        Mutex,
    },
    time::Instant,
};
use url::Url;

/// Laufzeitzustand der Hülle (nicht gespeichert — Fenstergröße und -lage
/// sichert das Window-State-Plugin).
#[derive(Default)]
pub struct Zustand {
    /// Zuletzt vollständig geladene Atlas-Seite im Hauptfenster. Ziel, wenn das
    /// Hauptfenster versehentlich auf einer fremden Seite landet.
    pub letzte_atlas_url: Mutex<Option<Url>>,
    /// Deep-Link, mit dem die App gestartet wurde — wird beim ersten Verbinden
    /// statt der Startseite geladen.
    pub start_ziel: Mutex<Option<Url>>,
    /// Seit wann eine Anmeldung im System-Browser läuft. Kehrt die Person in die
    /// App zurück, lädt das Hauptfenster neu, damit die neue Verbindung erscheint.
    pub anmeldung_seit: Mutex<Option<Instant>>,
    /// Läuft die automatische Wiederverbindung der Offline-Seite schon?
    pub wiederholung_laeuft: AtomicBool,
    /// Laufende Nummer für Nebenfenster-Labels.
    pub fenster_zaehler: AtomicU32,
    /// Laufende Downloads: Adresse → Zieldatei (macOS meldet beim Abschluss
    /// keinen Pfad mit).
    pub downloads: Mutex<std::collections::HashMap<String, std::path::PathBuf>>,
    /// Zoomstufe je Fensterlabel (1.0 = 100 %).
    pub zoom: Mutex<std::collections::HashMap<String, f64>>,
}
