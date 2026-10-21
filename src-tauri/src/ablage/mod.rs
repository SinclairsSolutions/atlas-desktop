//! „Ablage im Finder“ (macOS, File Provider) bzw. „im Explorer“ (Windows,
//! Cloud Files): die klassische Atlas-Ablage als Ordner „Atlas“ auf dem
//! Rechner. Dateien sind Platzhalter und werden erst beim Öffnen geladen.
//!
//! Ablauf:
//! 1. Einstellungen → Ablage → einschalten (`ablage_schalten`). Die App merkt
//!    sich „gewünscht“ und bittet die angemeldete Atlas-Seite, einen
//!    Gerätezugang zu holen (`window.__atlasDesktopAblage`).
//! 2. Atlas ruft `ablage_verbunden` mit dem Token auf. Die App legt ihn in den
//!    Schlüsselbund, übergibt ihn der Erweiterung und meldet die Ablage beim
//!    System an.
//! 3. Alle 30 Sekunden (solange die App läuft) stößt die App einen Abgleich an.
//! 4. Ausschalten oder Abmelden in Atlas: Ablage samt lokaler Kopien entfernen,
//!    Gerätezugang beim Server widerrufen, Token löschen.
//!
//! Der Gerätezugang gilt nur für `/api/public/desktop/ablage/*` und ist im
//! Profil (Apps) jederzeit widerrufbar.

mod api;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
use mac as plattform;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows as plattform;

#[cfg(not(any(target_os = "macos", windows)))]
mod plattform {
    use super::Zugang;
    use tauri::AppHandle;
    pub fn eingebaut() -> bool {
        false
    }
    pub fn verbinden(_: &AppHandle, _: &Zugang) -> Result<(), String> {
        Err("Nicht unterstützt.".into())
    }
    pub fn trennen(_: &AppHandle) -> Result<(), String> {
        Ok(())
    }
    pub fn anstossen(_: &AppHandle) {}
    pub fn zeigen(_: &AppHandle) -> Result<(), String> {
        Err("Nicht unterstützt.".into())
    }
    pub fn token_speichern(_: &str) -> Result<(), String> {
        Err("Nicht unterstützt.".into())
    }
    pub fn token_lesen() -> Option<String> {
        None
    }
    pub fn token_loeschen() {}
    pub fn verschluesselung() -> Option<bool> {
        None
    }
}

use serde::{Deserialize, Serialize};
use std::sync::{atomic::AtomicBool, atomic::Ordering, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::{fenster, texte::t, ziel};

pub const TOKEN_PRAEFIX: &str = "atlas_abl_";

/// Erhöhen, wenn die Ablage auf den Rechnern einmal komplett neu aufgebaut
/// werden muss. 1 = 0.3.0-beta.7: bis beta.6 blieben Dateien ohne bekannte
/// Größe nach dem Laden leer; die alte Domäne behielt diese Fassungen.
const DOMAENEN_EPOCHE: u32 = 1;

/// Was die Erweiterung zum Verbinden braucht.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Zugang {
    pub token: String,
    pub basis: String,
    pub geraet_id: String,
}

/// Gespeichert in `ablage.json` (App-Konfigurationsordner) — ohne Token.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Gespeichert {
    /// Stand der Domäne (siehe DOMAENEN_EPOCHE).
    domaenen_epoche: u32,
    gewuenscht: bool,
    geraet_id: Option<String>,
    geraet_name: Option<String>,
}

#[derive(Default)]
pub struct Ablage {
    stand: Mutex<Option<Gespeichert>>,
    fehler: Mutex<Option<String>>,
    takt_laeuft: AtomicBool,
}

fn datei(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("ablage.json"))
}

fn laden(app: &AppHandle) -> Gespeichert {
    let z = app.state::<Ablage>();
    let mut g = z.stand.lock().unwrap();
    if g.is_none() {
        *g = Some(
            datei(app)
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
        );
    }
    g.clone().unwrap_or_default()
}

fn speichern(app: &AppHandle, neu: &Gespeichert) {
    *app.state::<Ablage>().stand.lock().unwrap() = Some(neu.clone());
    if let Some(p) = datei(app) {
        let _ = p.parent().map(std::fs::create_dir_all);
        if let Ok(s) = serde_json::to_string_pretty(neu) {
            let _ = std::fs::write(p, s);
        }
    }
}

fn fehler_setzen(app: &AppHandle, f: Option<String>) {
    *app.state::<Ablage>().fehler.lock().unwrap() = f;
    let _ = app.emit("ablage-geaendert", ());
}

fn verbunden(g: &Gespeichert) -> bool {
    g.geraet_id.is_some() && plattform::token_lesen().is_some()
}

fn basis() -> String {
    ziel::start_url().origin().ascii_serialization()
}

fn geraet_name() -> String {
    let host = std::process::Command::new(if cfg!(target_os = "macos") { "/usr/sbin/scutil" } else { "hostname" })
        .args(if cfg!(target_os = "macos") { &["--get", "ComputerName"][..] } else { &[][..] })
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    host.unwrap_or_else(|| if cfg!(target_os = "macos") { "Mac".into() } else { "Windows-PC".into() })
        .chars()
        .take(100)
        .collect()
}

// ---------------------------------------------------------------------------
// Beim Start
// ---------------------------------------------------------------------------

pub fn starten(app: &AppHandle) {
    // Nicht auf dem Hauptthread warten: FileProvider antwortet asynchron.
    let app = app.clone();
    std::thread::spawn(move || starten_jetzt(&app));
}

fn starten_jetzt(app: &AppHandle) {
    let g = laden(app);
    if g.gewuenscht && verbunden(&g) {
        // Domäne sicherstellen (bleibt sonst über Neustarts bestehen) und Takt starten.
        if let Some(token) = plattform::token_lesen() {
            let z = Zugang { token, basis: basis(), geraet_id: g.geraet_id.clone().unwrap_or_default() };
            if cfg!(target_os = "macos") && g.domaenen_epoche < DOMAENEN_EPOCHE {
                // Einmal sauber neu aufbauen: Domäne entfernen (eigene, noch nicht
                // hochgeladene Dateien sichert macOS dabei) und neu anmelden.
                log::info!("Ablage: Domäne wird neu aufgebaut (Epoche {DOMAENEN_EPOCHE})");
                if let Err(e) = plattform::trennen(app) {
                    log::warn!("Ablage: Entfernen vor dem Neuaufbau: {e}");
                }
                let mut neu = laden(app);
                neu.domaenen_epoche = DOMAENEN_EPOCHE;
                speichern(app, &neu);
            }
            if let Err(e) = plattform::verbinden(app, &z) {
                log::warn!("Ablage: Anmelden beim System fehlgeschlagen: {e}");
                fehler_setzen(app, Some(e));
            }
        }
        takt(app);
    }
}

/// Alle 30 Sekunden einen Abgleich anstoßen (der Finder fragt dann Atlas).
fn takt(app: &AppHandle) {
    if app.state::<Ablage>().takt_laeuft.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let g = laden(&app);
            if !(g.gewuenscht && g.geraet_id.is_some()) {
                break;
            }
            plattform::anstossen(&app);
        }
        app.state::<Ablage>().takt_laeuft.store(false, Ordering::SeqCst);
    });
}

/// Sofort abgleichen (z. B. wenn das Hauptfenster aktiv wird).
pub fn anstossen(app: &AppHandle) {
    let g = laden(app);
    if g.gewuenscht && g.geraet_id.is_some() {
        plattform::anstossen(app);
    }
}

// ---------------------------------------------------------------------------
// Trennen
// ---------------------------------------------------------------------------

/// Ablage entfernen (samt lokaler Kopien), Zugang widerrufen, Token löschen.
fn trennen(app: &AppHandle, gewuenscht_behalten: bool) {
    if let Err(e) = plattform::trennen(app) {
        log::warn!("Ablage: Entfernen fehlgeschlagen: {e}");
    }
    if let Some(token) = plattform::token_lesen() {
        // Widerruf beim Server: der Token gilt danach nirgends mehr.
        let url = format!("{}/api/public/desktop/ablage/abmelden", basis());
        tauri::async_runtime::spawn(async move {
            let r = reqwest::Client::new()
                .post(url)
                .bearer_auth(token)
                .timeout(Duration::from_secs(15))
                .send()
                .await;
            if let Err(e) = r {
                log::warn!("Ablage: Widerruf nicht erreicht ({e}) — der Zugang bleibt im Profil sichtbar und lässt sich dort trennen.");
            }
        });
    }
    plattform::token_loeschen();
    let mut g = laden(app);
    g.geraet_id = None;
    g.gewuenscht = g.gewuenscht && gewuenscht_behalten;
    speichern(app, &g);
    fehler_setzen(app, None);
}

// ---------------------------------------------------------------------------
// Befehle der Atlas-Seite (Capability atlas-remote)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    gewuenscht: bool,
    verbunden: bool,
    geraet_id: Option<String>,
    geraet_name: Option<String>,
}

#[tauri::command]
pub fn ablage_status(app: AppHandle) -> Status {
    let g = laden(&app);
    Status {
        gewuenscht: g.gewuenscht && plattform::eingebaut(),
        verbunden: verbunden(&g),
        geraet_id: g.geraet_id.clone(),
        geraet_name: Some(g.geraet_name.clone().unwrap_or_else(geraet_name)),
    }
}

#[tauri::command]
pub async fn ablage_verbunden(app: AppHandle, token: String, geraet_id: String, name: String) -> Result<(), String> {
    let g = laden(&app);
    if !g.gewuenscht {
        return Err("Ablage ist ausgeschaltet.".into());
    }
    if !token.starts_with(TOKEN_PRAEFIX) || token.len() > 200 || geraet_id.len() != 36 {
        return Err("Ungültiger Zugang.".into());
    }
    // Älteren Zugang dieses Geräts ersetzen (Atlas hat ihn beim Koppeln widerrufen).
    plattform::token_speichern(&token)?;
    let z = Zugang { token, basis: basis(), geraet_id: geraet_id.clone() };
    if let Err(e) = plattform::verbinden(&app, &z) {
        fehler_setzen(&app, Some(e.clone()));
        return Err(e);
    }
    let mut g = g;
    g.domaenen_epoche = DOMAENEN_EPOCHE;
    g.geraet_id = Some(geraet_id);
    g.geraet_name = Some(name.chars().take(120).collect());
    speichern(&app, &g);
    fehler_setzen(&app, None);
    takt(&app);
    log::info!("Ablage verbunden");
    Ok(())
}

#[tauri::command]
pub fn ablage_fehler(app: AppHandle, meldung: String) {
    fehler_setzen(&app, Some(meldung.chars().take(300).collect()));
}

/// Abmelden in Atlas: Ablage entfernen. „Gewünscht“ bleibt, damit sie sich
/// nach der nächsten Anmeldung von selbst wieder verbindet.
#[tauri::command]
pub async fn ablage_abgemeldet(app: AppHandle) -> Result<(), String> {
    let g = laden(&app);
    if g.geraet_id.is_some() || plattform::token_lesen().is_some() {
        log::info!("Ablage: Abmeldung in Atlas — Ablage wird entfernt");
        trennen(&app, true);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Befehle des Einstellungsfensters (Capability werkzeuge)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    eingebaut: bool,
    gewuenscht: bool,
    verbunden: bool,
    geraet_name: Option<String>,
    fehler: Option<String>,
    /// FileVault (macOS) bzw. BitLocker (Windows) an? `None` = unbekannt.
    verschluesselung: Option<bool>,
    plattform: &'static str,
}

#[tauri::command]
pub fn ablage_info(app: AppHandle) -> Info {
    let g = laden(&app);
    Info {
        eingebaut: plattform::eingebaut(),
        gewuenscht: g.gewuenscht,
        verbunden: verbunden(&g),
        geraet_name: g.geraet_name.clone(),
        fehler: app.state::<Ablage>().fehler.lock().unwrap().clone(),
        verschluesselung: plattform::verschluesselung(),
        plattform: std::env::consts::OS,
    }
}

#[tauri::command]
pub async fn ablage_schalten(app: AppHandle, an: bool) -> Result<String, String> {
    if !an {
        trennen(&app, false);
        return Ok(t("Ablage entfernt.", "Files removed.").into());
    }
    if !plattform::eingebaut() {
        return Err(t(
            "Diese App enthält die Ablage-Erweiterung nicht. Bitte die aktuelle Version von sinclair-atlas.com laden.",
            "This app does not include the files extension. Please download the current version from sinclair-atlas.com.",
        )
        .into());
    }
    let mut g = laden(&app);
    g.gewuenscht = true;
    if g.geraet_name.is_none() {
        g.geraet_name = Some(geraet_name());
    }
    speichern(&app, &g);
    fehler_setzen(&app, None);
    // Die angemeldete Atlas-Seite holt den Gerätezugang.
    match fenster::hauptfenster(&app) {
        Some(w) if w.url().map(|u| ziel::ist_atlas(&u)).unwrap_or(false) => {
            let _ = w.eval("window.__atlasDesktopAblage && window.__atlasDesktopAblage({ aktion: 'koppeln' })");
            Ok(t(
                "Verbinde mit Atlas …",
                "Connecting to Atlas …",
            )
            .into())
        }
        _ => Ok(t(
            "Bitte in Atlas anmelden — die Ablage verbindet sich dann von selbst.",
            "Please sign in to Atlas — the files connect automatically afterwards.",
        )
        .into()),
    }
}

#[tauri::command]
pub async fn ablage_zeigen(app: AppHandle) -> Result<(), String> {
    plattform::zeigen(&app)
}
