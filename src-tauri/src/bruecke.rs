//! Brücke zwischen der Atlas-Seite und dem System.
//!
//! `bruecke.js` wird in jede Seite eingespielt, wirkt aber nur auf Atlas-Seiten.
//! Es ersetzt `Notification` und `navigator.setAppBadge` durch Aufrufe der
//! Befehle unten. Die Atlas-Seite darf über die Capability `atlas-remote`
//! ausschließlich diese fünf Befehle aufrufen — keinen Dateizugriff, keine
//! Shell, keine anderen Plugins.

use std::sync::atomic::Ordering;

use tauri::{AppHandle, Manager, Webview};
use tauri_plugin_notification::{NotificationExt, PermissionState};
use url::Url;

use crate::{erreichbarkeit, fenster, ziel, zustand::Zustand};

const SKRIPT: &str = include_str!("bruecke.js");

pub fn skript() -> String {
    let hosts: Vec<String> = ziel::ATLAS_HOSTS.iter().map(|h| h.to_string()).collect();
    let mut origins: Vec<String> = Vec::new();
    if ziel::ist_uebersteuert() {
        origins.push(ziel::start_url().origin().ascii_serialization());
    }
    SKRIPT
        .replace("__ATLAS_HOSTS__", &serde_json::to_string(&hosts).unwrap())
        .replace("__ATLAS_ORIGINS__", &serde_json::to_string(&origins).unwrap())
        .replace("__VERSION__", env!("CARGO_PKG_VERSION"))
        .replace("__PLATTFORM__", std::env::consts::OS)
        .replace("__SCHEMA__", ziel::SCHEMA)
}

/// Web-Mitteilung als System-Mitteilung.
#[tauri::command]
pub fn mitteilung(app: AppHandle, titel: String, text: Option<String>) -> Result<(), String> {
    let titel: String = titel.chars().take(200).collect();
    let text: String = text.unwrap_or_default().chars().take(1000).collect();
    app.notification()
        .builder()
        .title(titel)
        .body(text)
        .show()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mitteilungen_erlauben(app: AppHandle) -> String {
    match app.notification().request_permission() {
        Ok(PermissionState::Granted) => "granted",
        Ok(PermissionState::Denied) => "denied",
        _ => "default",
    }
    .into()
}

/// Zahl am Dock- bzw. Taskleistensymbol. `None` oder 0 entfernt sie,
/// `punkt = true` (setAppBadge ohne Zahl) zeigt nur einen Punkt.
#[tauri::command]
pub fn kennzahl(app: AppHandle, anzahl: Option<i64>, punkt: Option<bool>) {
    let Some(w) = fenster::hauptfenster(&app) else { return };
    // Einstellung „Zahl am App-Symbol“ aus: immer leeren.
    let aus = !crate::einstellungen::aktuell(&app).dock_zahl;
    let anzahl = anzahl.filter(|n| *n > 0 && !aus);
    let punkt = punkt.filter(|_| !aus);
    #[cfg(target_os = "macos")]
    {
        if punkt.unwrap_or(false) && anzahl.is_none() {
            let _ = w.set_badge_label(Some("•".into()));
        } else {
            let _ = w.set_badge_count(anzahl);
        }
    }
    #[cfg(windows)]
    {
        let _ = punkt;
        crate::taskleiste::zahl_setzen(&w, anzahl);
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (punkt, w.set_badge_count(anzahl));
    }
}

/// `window.print()` — WebKit führt es in eingebetteten Ansichten unter macOS
/// nicht aus; hier öffnet sich der normale Druckdialog für das Fenster.
#[tauri::command]
pub fn drucken(webview: Webview) -> Result<(), String> {
    webview.print().map_err(|e| e.to_string())
}

/// Fremde Adresse im Standardbrowser öffnen (für Klicks, die `bruecke.js`
/// abfängt). Atlas-Adressen bleiben in der App.
#[tauri::command]
pub fn extern_oeffnen(app: AppHandle, webview: Webview, url: String) -> Result<(), String> {
    let url = Url::parse(&url).map_err(|e| e.to_string())?;
    if ziel::ist_atlas(&url) {
        return webview.navigate(url).map_err(|e| e.to_string());
    }
    if !ziel::darf_extern(&url) {
        return Err("Adresse nicht erlaubt".into());
    }
    fenster::extern_oeffnen(&app, &url);
    Ok(())
}

/// Von der Lade-/Offline-Seite: Atlas prüfen und bei Erfolg laden. Schlägt es
/// fehl, versucht die App es im Hintergrund alle 15 Sekunden weiter — in Rust,
/// weil WebKit die Timer verdeckter Fenster drosselt.
#[tauri::command]
pub async fn verbinden(
    app: AppHandle,
    webview: Webview,
    zustand: tauri::State<'_, Zustand>,
    ziel_url: Option<String>,
) -> Result<erreichbarkeit::Ergebnis, String> {
    let ziel_url = ziel_url
        .and_then(|z| Url::parse(&z).ok())
        .filter(ziel::ist_atlas)
        .or_else(|| zustand.start_ziel.lock().unwrap().take())
        .unwrap_or_else(|| ziel::start_url().clone());
    let ergebnis = erreichbarkeit::pruefen(&ziel_url).await;
    log::info!("Verbindungsversuch: {ergebnis:?}");
    if ergebnis == erreichbarkeit::Ergebnis::Erreichbar {
        webview.navigate(ziel_url).map_err(|e| e.to_string())?;
    } else if !zustand.wiederholung_laeuft.swap(true, Ordering::SeqCst) {
        tauri::async_runtime::spawn(wiederholen(app, webview, ziel_url));
    }
    Ok(ergebnis)
}

async fn wiederholen(app: AppHandle, webview: Webview, ziel_url: Url) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        // Inzwischen anders weitergegangen (Knopf, Deep-Link)? Dann aufhören.
        let noch_offline = webview.url().map(|u| ziel::ist_lokal(&u)).unwrap_or(false);
        if !noch_offline {
            break;
        }
        if erreichbarkeit::pruefen(&ziel_url).await == erreichbarkeit::Ergebnis::Erreichbar {
            log::info!("Atlas wieder erreichbar");
            let _ = webview.navigate(ziel_url);
            break;
        }
    }
    app.state::<Zustand>().wiederholung_laeuft.store(false, Ordering::SeqCst);
}
