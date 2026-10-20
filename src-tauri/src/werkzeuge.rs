//! Einstellungsfenster, Menüleisten-Symbol, Schnellnotiz und globales Kürzel.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use crate::{
    eingang, einstellungen::{self, Einstellungen}, fenster, sperre, symbol, texte::t,
};

pub const EINSTELLUNGEN: &str = "einstellungen";
pub const NOTIZ: &str = "notiz";
const TRAY_ID: &str = "atlas-menueleiste";
/// Schnellerfassung: ⌥⌘A (Windows: Strg+Alt+A).
const KUERZEL: &str = "CmdOrCtrl+Alt+A";

fn lokales_fenster(app: &AppHandle, label: &str, seite: &str, titel: &str, b: f64, h: f64) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let bld = WebviewWindowBuilder::new(app, label, WebviewUrl::App(seite.into()))
        .title(titel)
        .inner_size(b, h)
        .resizable(false)
        .center();
    #[cfg(target_os = "macos")]
    let bld = bld.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    if let Err(e) = bld.build() {
        log::error!("Fenster {label} ließ sich nicht öffnen: {e}");
    }
}

pub fn einstellungen_oeffnen(app: &AppHandle) {
    lokales_fenster(app, EINSTELLUNGEN, "einstellungen.html", &t("Einstellungen", "Settings"), 560.0, 680.0);
}

pub fn notiz_oeffnen(app: &AppHandle) {
    if app.state::<sperre::Zustand>().gesperrt.load(std::sync::atomic::Ordering::SeqCst) {
        sperre::sperren(app);
        return;
    }
    lokales_fenster(app, NOTIZ, "notiz.html", &t("Schnellerfassung", "Quick Capture"), 460.0, 300.0);
    if let Some(w) = app.get_webview_window(NOTIZ) {
        let _ = w.set_always_on_top(true);
    }
}

// ---------------------------------------------------------------------------
// Befehle für die lokalen Fenster (Capability „werkzeuge“)
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stand {
    einstellungen: Einstellungen,
    touch_id: bool,
    autostart: bool,
    wartend: usize,
    version: String,
    plattform: &'static str,
}

#[tauri::command]
pub fn einstellungen_lesen(app: AppHandle) -> Stand {
    Stand {
        einstellungen: einstellungen::aktuell(&app),
        touch_id: sperre::verfuegbar(),
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        wartend: eingang::anzahl(&app),
        version: app.package_info().version.to_string(),
        plattform: std::env::consts::OS,
    }
}

#[tauri::command]
pub async fn einstellungen_setzen(app: AppHandle, neu: Einstellungen) -> Result<(), String> {
    let alt = einstellungen::aktuell(&app);
    // Touch-ID-Sperre einschalten nur nach erfolgreicher Prüfung.
    if neu.touch_id_sperre && !alt.touch_id_sperre && !sperre::pruefen(t("Touch-ID-Sperre einschalten", "Turn on Touch ID lock")).await {
        return Err(t("Touch ID wurde nicht bestätigt.", "Touch ID was not confirmed.").into());
    }
    einstellungen::speichern(&app, &neu);
    anwenden(&app, Some(&alt));
    Ok(())
}

/// Einstellungen wirksam machen (beim Start und nach Änderungen).
pub fn anwenden(app: &AppHandle, alt: Option<&Einstellungen>) {
    let e = einstellungen::aktuell(app);
    if alt.map(|a| a.symbol != e.symbol).unwrap_or(true) {
        let v = if e.symbol == "weiss" { symbol::Variante::Weiss } else { symbol::Variante::Schwarz };
        symbol::setzen(app, v);
    }
    if !e.dock_zahl {
        crate::bruecke::kennzahl(app.clone(), None, None);
    }
    menueleiste(app, e.menueleiste);
    kuerzel(app, e.schnell_kuerzel);
}

#[tauri::command]
pub fn autostart_setzen(app: AppHandle, an: bool) -> Result<bool, String> {
    let al = app.autolaunch();
    if an { al.enable() } else { al.disable() }.map_err(|e| e.to_string())?;
    Ok(al.is_enabled().unwrap_or(false))
}

#[tauri::command]
pub async fn ordner_waehlen(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |p| {
        let _ = tx.send(p.and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned()));
    });
    rx.await.ok().flatten()
}

#[tauri::command]
pub fn mitteilung_testen(app: AppHandle) -> Result<(), String> {
    app.notification()
        .builder()
        .title("Sinclair Atlas")
        .body(t("So sehen Mitteilungen von Atlas aus.", "This is how Atlas notifications look."))
        .show()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ordner_zeigen(app: AppHandle, pfad: String) {
    use tauri_plugin_opener::OpenerExt;
    let p = std::path::PathBuf::from(&pfad);
    if p.is_dir() {
        let _ = app.opener().open_path(pfad, None::<&str>);
    }
}

#[tauri::command]
pub fn warteschlange_senden(app: AppHandle) -> usize {
    eingang::abarbeiten(&app);
    eingang::anzahl(&app)
}

/// Schnellnotiz an Atlas: Atlas legt eine Aufgabe bzw. Notiz an.
#[tauri::command]
pub fn notiz_senden(app: AppHandle, text: String, art: String) -> Result<(), String> {
    let text: String = text.chars().take(5000).collect();
    if text.trim().is_empty() {
        return Err(t("Bitte etwas eingeben.", "Please enter something.").into());
    }
    let art = if art == "notiz" { "notiz" } else { "aufgabe" };
    let w = fenster::hauptfenster(&app).ok_or("Atlas ist nicht geöffnet.")?;
    let js = format!(
        "window.__atlasDesktopNotiz && window.__atlasDesktopNotiz({})",
        serde_json::json!({ "text": text, "art": art })
    );
    w.eval(&js).map_err(|e| e.to_string())?;
    if let Some(n) = app.get_webview_window(NOTIZ) {
        let _ = n.close();
    }
    Ok(())
}

#[tauri::command]
pub fn beleg_auswaehlen(app: AppHandle) {
    eingang::auswaehlen(&app);
}

#[tauri::command]
pub fn bildschirmfoto(app: AppHandle) {
    if let Some(n) = app.get_webview_window(NOTIZ) {
        let _ = n.close();
    }
    eingang::bildschirmfoto(&app);
}

// ---------------------------------------------------------------------------
// Menüleiste und Kürzel
// ---------------------------------------------------------------------------

fn menueleiste(app: &AppHandle, an: bool) {
    if !an {
        if let Some(t) = app.remove_tray_by_id(TRAY_ID) {
            drop(t);
        }
        return;
    }
    if app.tray_by_id(TRAY_ID).is_some() {
        return;
    }
    let bauen = || -> tauri::Result<()> {
        let menue = Menu::with_items(
            app,
            &[
                &MenuItem::with_id(app, "mb-oeffnen", t("Atlas öffnen", "Open Atlas"), true, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "mb-beleg", t("Belege in den Eingang …", "Receipts to Inbox …"), true, None::<&str>)?,
                &MenuItem::with_id(app, "mb-foto", t("Bildschirmfoto in den Eingang", "Screenshot to Inbox"), true, None::<&str>)?,
                &MenuItem::with_id(app, "mb-notiz", t("Schnellerfassung …", "Quick Capture …"), true, Some(KUERZEL))?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "mb-einstellungen", t("Einstellungen …", "Settings …"), true, None::<&str>)?,
            ],
        )?;
        let bild = tauri::image::Image::from_bytes(include_bytes!("../icons/menueleiste.png"))?;
        TrayIconBuilder::with_id(TRAY_ID)
            .icon(bild)
            .icon_as_template(true)
            .tooltip("Sinclair Atlas")
            .menu(&menue)
            .show_menu_on_left_click(true)
            .on_menu_event(|app, ev| match ev.id().as_ref() {
                "mb-oeffnen" => fenster::hauptfenster_zeigen(app),
                "mb-beleg" => eingang::auswaehlen(app),
                "mb-foto" => eingang::bildschirmfoto(app),
                "mb-notiz" => notiz_oeffnen(app),
                "mb-einstellungen" => einstellungen_oeffnen(app),
                _ => {}
            })
            .build(app)?;
        Ok(())
    };
    if let Err(e) = bauen() {
        log::error!("Menüleisten-Symbol: {e}");
    }
}

fn kuerzel(app: &AppHandle, an: bool) {
    let gs = app.global_shortcut();
    let Ok(k) = KUERZEL.parse::<Shortcut>() else { return };
    let schon = gs.is_registered(k);
    if an && !schon {
        if let Err(e) = gs.on_shortcut(k, |app, _k, ev| {
            if ev.state() == ShortcutState::Pressed {
                notiz_oeffnen(app);
            }
        }) {
            log::warn!("Tastenkürzel nicht registriert: {e}");
        }
    } else if !an && schon {
        let _ = gs.unregister(k);
    }
}
