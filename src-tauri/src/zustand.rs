use std::{
    sync::{atomic::AtomicU32, Mutex},
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
    /// Laufende Nummer für Nebenfenster-Labels.
    pub fenster_zaehler: AtomicU32,
    /// Zoomstufe je Fensterlabel (1.0 = 100 %).
    pub zoom: Mutex<std::collections::HashMap<String, f64>>,
}
