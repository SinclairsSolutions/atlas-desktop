//! Einstellungsfenster, Menüleisten-Symbol, Schnellnotiz und globales Kürzel.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::{
    eingang, einstellungen::{self, Einstellungen}, fenster, importordner, sperre, symbol, texte::t,
};

pub const EINSTELLUNGEN: &str = "einstellungen";
pub const NOTIZ: &str = "notiz";
const TRAY_ID: &str = "atlas-menueleiste";
const TRAY_PW: &str = "atlas-passwoerter";
/// Schnellerfassung: ⌥⌘A (Windows: Strg+Alt+A).
const KUERZEL: &str = "CmdOrCtrl+Alt+A";

/// `min`: Fenster in der Größe veränderbar, nicht kleiner als (Breite, Höhe).
fn lokales_fenster(app: &AppHandle, label: &str, seite: &str, titel: &str, b: f64, h: f64, min: Option<(f64, f64)>) {
    // Aus einem globalen Tastenkürzel heraus ist Atlas oft nicht das aktive
    // Programm — ohne Aktivieren öffnete sich das Fenster unsichtbar dahinter.
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let bld = WebviewWindowBuilder::new(app, label, WebviewUrl::App(seite.into()))
        .title(titel)
        .inner_size(b, h)
        .resizable(min.is_some())
        .center();
    let bld = match min {
        Some((mb, mh)) => bld.min_inner_size(mb, mh),
        None => bld,
    };
    #[cfg(target_os = "macos")]
    let bld = bld.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    if let Err(e) = bld.build() {
        log::error!("Fenster {label} ließ sich nicht öffnen: {e}");
    }
}

pub fn einstellungen_oeffnen(app: &AppHandle) {
    // Gesperrt: Einstellungen erst nach dem Entsperren (sonst ließe sich dort
    // die Sperre abschalten).
    if sperre::ist_gesperrt(app) {
        sperre::sperren(app);
        return;
    }
    lokales_fenster(app, EINSTELLUNGEN, "einstellungen.html", &t("Einstellungen", "Settings"), 820.0, 640.0, Some((680.0, 500.0)));
}

pub fn notiz_oeffnen(app: &AppHandle) {
    if app.state::<sperre::Zustand>().gesperrt.load(std::sync::atomic::Ordering::SeqCst) {
        sperre::sperren(app);
        return;
    }
    lokales_fenster(app, NOTIZ, "notiz.html", &t("Schnellerfassung", "Quick Capture"), 460.0, 300.0, None);
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
    /// Import-Ordner: zuletzt von Atlas gemeldete Workspaces und ihre Ordner.
    import: importordner::Anzeige,
    /// Vorschlag für den Import-Ordner („~/Atlas Import“).
    import_vorschlag: Option<String>,
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
        import: importordner::anzeige(&app),
        import_vorschlag: import_vorschlag(&app).map(|p| p.to_string_lossy().into_owned()),
    }
}

#[tauri::command]
pub async fn einstellungen_setzen(app: AppHandle, neu: Einstellungen) -> Result<(), String> {
    let alt = einstellungen::aktuell(&app);
    if sperre::ist_gesperrt(&app) {
        return Err(t("Atlas ist gesperrt.", "Atlas is locked.").into());
    }
    // Touch-ID-Sperre ein- und ausschalten bzw. lockern nur nach erfolgreicher
    // Prüfung — sonst ließe sich die Sperre ohne Touch ID abstellen.
    let einschalten = neu.touch_id_sperre && !alt.touch_id_sperre;
    let lockern = alt.touch_id_sperre
        && (!neu.touch_id_sperre
            || (alt.sperre_nach_minuten > 0
                && (neu.sperre_nach_minuten == 0 || neu.sperre_nach_minuten > alt.sperre_nach_minuten)));
    let grund = if einschalten {
        Some(t("Touch-ID-Sperre einschalten", "Turn on Touch ID lock"))
    } else if lockern {
        Some(t("Touch-ID-Sperre ändern", "Change Touch ID lock"))
    } else {
        None
    };
    if let Some(g) = grund {
        if !sperre::pruefen(g).await {
            return Err(t("Touch ID wurde nicht bestätigt.", "Touch ID was not confirmed.").into());
        }
    }
    einstellungen::speichern(&app, &neu);
    if alt.import_ordner != neu.import_ordner || alt.import_je_unternehmen != neu.import_je_unternehmen {
        importordner::abgleichen(&app);
    }
    // Menüleisten-Symbole, Dock-Symbol und Kürzel nur auf dem Hauptthread
    // anfassen — sonst stürzt AppKit beim Entfernen eines Symbols ab.
    let a = app.clone();
    app.run_on_main_thread(move || anwenden(&a, Some(&alt)))
        .map_err(|e| e.to_string())?;
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
    passwort_symbol(app, e.passwort_symbol);
    kuerzel(app, e.schnell_kuerzel);
}

fn import_vorschlag(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().home_dir().ok().map(|h| h.join("Atlas Import"))
}

/// Import-Ordner am vorgeschlagenen Ort anlegen und einschalten.
#[tauri::command]
pub fn import_ordner_anlegen(app: AppHandle) -> Result<String, String> {
    let p = import_vorschlag(&app).ok_or(t("Benutzerordner nicht gefunden.", "Home folder not found."))?;
    std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    let mut e = einstellungen::aktuell(&app);
    e.import_ordner = Some(p.to_string_lossy().into_owned());
    einstellungen::speichern(&app, &e);
    importordner::abgleichen(&app);
    Ok(p.to_string_lossy().into_owned())
}

/// „Nach Updates suchen“ aus dem Einstellungsfenster (wie im App-Menü).
#[tauri::command]
pub fn nach_updates_suchen(app: AppHandle) {
    tauri::async_runtime::spawn(async move { crate::aktualisierung::pruefen(&app, true).await });
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
    crate::mitteilung::erlaubnis_anfragen(&app);
    crate::mitteilung::zeigen(
        &app,
        "Sinclair Atlas",
        t("So sehen Mitteilungen von Atlas aus.", "This is how Atlas notifications look."),
    )
}

#[tauri::command]
pub fn ordner_zeigen(app: AppHandle, pfad: String) {
    use tauri_plugin_opener::OpenerExt;
    // Nur die eingestellten Ordner — `open` auf einen beliebigen „Ordner“
    // würde z. B. ein .app-Paket starten.
    let e = einstellungen::aktuell(&app);
    let erlaubt = [e.ueberwachter_ordner, e.export_ordner, e.import_ordner];
    if !erlaubt.iter().flatten().any(|o| *o == pfad) {
        return;
    }
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
    // Nur an die Atlas-Seite übergeben, nie an eine andere Seite im Hauptfenster.
    if !w.url().is_ok_and(|u| crate::ziel::ist_atlas(&u)) {
        return Err(t("Atlas ist nicht geöffnet.", "Atlas is not open.").into());
    }
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

/// Schlüssel-Symbol: Linksklick öffnet den Passwörter-Schnellzugriff,
/// Rechtsklick ein kleines Menü.
fn passwort_symbol(app: &AppHandle, an: bool) {
    if !an {
        if let Some(t) = app.remove_tray_by_id(TRAY_PW) {
            drop(t);
        }
        if let Some(w) = app.get_webview_window(fenster::PASSWORT_SCHNELL) {
            let _ = w.close();
        }
        return;
    }
    if app.tray_by_id(TRAY_PW).is_some() {
        return;
    }
    let bauen = || -> tauri::Result<()> {
        let menue = Menu::with_items(
            app,
            &[
                &MenuItem::with_id(app, "pw-schnell", t("Passwörter durchsuchen", "Search Passwords"), true, None::<&str>)?,
                &MenuItem::with_id(app, "pw-fenster", t("Passwörter in eigenem Fenster", "Passwords in Own Window"), true, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "pw-einstellungen", t("Einstellungen …", "Settings …"), true, None::<&str>)?,
            ],
        )?;
        let bild = tauri::image::Image::from_bytes(include_bytes!("../icons/passwoerter-menueleiste.png"))?;
        TrayIconBuilder::with_id(TRAY_PW)
            .icon(bild)
            .icon_as_template(true)
            .tooltip(t("Atlas Passwörter", "Atlas Passwords"))
            .menu(&menue)
            .show_menu_on_left_click(false)
            .on_tray_icon_event(|tray, ev| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = ev {
                    let app = tray.app_handle();
                    let faktor = app
                        .primary_monitor()
                        .ok()
                        .flatten()
                        .map(|m| m.scale_factor())
                        .unwrap_or(1.0);
                    let pos = rect.position.to_physical::<f64>(faktor);
                    let groesse = rect.size.to_physical::<f64>(faktor);
                    fenster::passwort_schnell(app, Some((pos.x, pos.y, groesse.width, groesse.height)));
                }
            })
            .on_menu_event(|app, ev| match ev.id().as_ref() {
                "pw-schnell" => fenster::passwort_schnell(app, None),
                "pw-fenster" => crate::menue::eigenes_fenster(app, "fenster-passwoerter"),
                "pw-einstellungen" => einstellungen_oeffnen(app),
                _ => {}
            })
            .build(app)?;
        Ok(())
    };
    if let Err(e) = bauen() {
        log::error!("Passwort-Symbol: {e}");
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
