//! Menüleiste. Kürzel über `CmdOrCtrl` — unter macOS ⌘, unter Windows Strg.
//! Wo sich die Gepflogenheiten unterscheiden (Zurück/Vor, Vollbild, Beenden),
//! bekommt jede Plattform ihre eigene Belegung.

use tauri::{
    menu::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Manager, Runtime,
};
use url::Url;

use crate::{aktualisierung, downloads, fenster, texte::t, ziel, zustand::Zustand};

const ZOOM_STUFEN: &[f64] = &[0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

#[cfg(target_os = "macos")]
const ZURUECK: &str = "CmdOrCtrl+[";
#[cfg(not(target_os = "macos"))]
const ZURUECK: &str = "Alt+Left";
#[cfg(target_os = "macos")]
const VOR: &str = "CmdOrCtrl+]";
#[cfg(not(target_os = "macos"))]
const VOR: &str = "Alt+Right";

pub fn erstellen<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let pkg = app.package_info();
    let ueber = AboutMetadata {
        name: Some("Sinclair Atlas".into()),
        version: Some(pkg.version.to_string()),
        copyright: Some("© Sinclairs Solutions".into()),
        website: Some(ziel::ATLAS_URL.into()),
        website_label: Some("sinclair-atlas.com".into()),
        ..Default::default()
    };
    let updates = MenuItem::with_id(
        app,
        "updates",
        t("Nach Updates suchen …", "Check for Updates …"),
        true,
        None::<&str>,
    )?;

    let ablage = Submenu::with_items(
        app,
        t("Ablage", "File"),
        true,
        &[
            &MenuItem::with_id(app, "startseite", t("Startseite", "Home"), true, Some("CmdOrCtrl+Shift+H"))?,
            &MenuItem::with_id(app, "im-browser", t("Im Browser öffnen", "Open in Browser"), true, Some("CmdOrCtrl+Shift+O"))?,
            &MenuItem::with_id(app, "downloads", t("Download-Ordner öffnen", "Open Downloads Folder"), true, Some("CmdOrCtrl+Alt+L"))?,
            &PredefinedMenuItem::separator(app)?,
            #[cfg(not(target_os = "macos"))]
            &updates,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some(t("Fenster schließen", "Close Window")))?,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::quit(app, Some(t("Beenden", "Exit")))?,
        ],
    )?;

    let bearbeiten = Submenu::with_items(
        app,
        t("Bearbeiten", "Edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(t("Widerrufen", "Undo")))?,
            &PredefinedMenuItem::redo(app, Some(t("Wiederholen", "Redo")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some(t("Ausschneiden", "Cut")))?,
            &PredefinedMenuItem::copy(app, Some(t("Kopieren", "Copy")))?,
            &PredefinedMenuItem::paste(app, Some(t("Einsetzen", "Paste")))?,
            &PredefinedMenuItem::select_all(app, Some(t("Alles auswählen", "Select All")))?,
        ],
    )?;

    let darstellung = Submenu::with_items(
        app,
        t("Darstellung", "View"),
        true,
        &[
            &MenuItem::with_id(app, "neu-laden", t("Neu laden", "Reload"), true, Some("CmdOrCtrl+R"))?,
            #[cfg(not(target_os = "macos"))]
            &MenuItem::with_id(app, "neu-laden-f5", t("Neu laden", "Reload"), true, Some("F5"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "zurueck", t("Zurück", "Back"), true, Some(ZURUECK))?,
            &MenuItem::with_id(app, "vor", t("Vorwärts", "Forward"), true, Some(VOR))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "zoom-null", t("Originalgröße", "Actual Size"), true, Some("CmdOrCtrl+0"))?,
            &MenuItem::with_id(app, "zoom-plus", t("Vergrößern", "Zoom In"), true, Some("CmdOrCtrl+="))?,
            &MenuItem::with_id(app, "zoom-minus", t("Verkleinern", "Zoom Out"), true, Some("CmdOrCtrl+-"))?,
            &PredefinedMenuItem::separator(app)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::fullscreen(app, Some(t("Vollbild", "Enter Full Screen")))?,
            #[cfg(not(target_os = "macos"))]
            &MenuItem::with_id(app, "vollbild", t("Vollbild", "Full Screen"), true, Some("F11"))?,
        ],
    )?;

    let fenster_menue = Submenu::with_items(
        app,
        t("Fenster", "Window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(t("Im Dock ablegen", "Minimize")))?,
            &PredefinedMenuItem::maximize(app, Some(t("Zoomen", "Zoom")))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "hauptfenster", "Sinclair Atlas", true, Some("CmdOrCtrl+1"))?,
        ],
    )?;

    let hilfe = Submenu::with_items(
        app,
        t("Hilfe", "Help"),
        true,
        &[
            &MenuItem::with_id(app, "website", t("Atlas im Browser öffnen", "Open Atlas in Browser"), true, None::<&str>)?,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::separator(app)?,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(app, Some(t("Über Sinclair Atlas", "About Sinclair Atlas")), Some(ueber.clone()))?,
        ],
    )?;

    #[cfg(target_os = "macos")]
    {
        let app_menue = Submenu::with_items(
            app,
            "Sinclair Atlas",
            true,
            &[
                &PredefinedMenuItem::about(app, Some(t("Über Sinclair Atlas", "About Sinclair Atlas")), Some(ueber))?,
                &PredefinedMenuItem::separator(app)?,
                &updates,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::services(app, Some(t("Dienste", "Services")))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, Some(t("Sinclair Atlas ausblenden", "Hide Sinclair Atlas")))?,
                &PredefinedMenuItem::hide_others(app, Some(t("Andere ausblenden", "Hide Others")))?,
                &PredefinedMenuItem::show_all(app, Some(t("Alle einblenden", "Show All")))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, Some(t("Sinclair Atlas beenden", "Quit Sinclair Atlas")))?,
            ],
        )?;
        let menue = Menu::with_items(app, &[&app_menue, &ablage, &bearbeiten, &darstellung, &fenster_menue, &hilfe])?;
        // macOS hängt hier die Fensterliste an und setzt die Hilfe-Suche ein.
        let _ = fenster_menue.set_as_windows_menu_for_nsapp();
        let _ = hilfe.set_as_help_menu_for_nsapp();
        Ok(menue)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = updates;
        Menu::with_items(app, &[&ablage, &bearbeiten, &darstellung, &fenster_menue, &hilfe])
    }
}

pub fn ereignis(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    if id == "updates" {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { aktualisierung::pruefen(&app, true).await });
        return;
    }
    if id == "downloads" {
        downloads::ordner_oeffnen(app);
        return;
    }
    if id == "website" {
        if let Ok(u) = Url::parse(ziel::ATLAS_URL) {
            fenster::extern_oeffnen(app, &u);
        }
        return;
    }
    if id == "hauptfenster" {
        fenster::hauptfenster_zeigen(app);
        return;
    }
    let Some(w) = fenster::aktives_fenster(app) else { return };
    match id {
        "neu-laden" | "neu-laden-f5" => fenster::neu_laden(app, &w),
        "zurueck" => {
            let _ = w.eval("history.back()");
        }
        "vor" => {
            let _ = w.eval("history.forward()");
        }
        "startseite" => {
            fenster::hauptfenster_zeigen(app);
            fenster::laden(app, ziel::start_url().clone());
        }
        "im-browser" => {
            if let Ok(u) = w.url() {
                let u = if ziel::ist_atlas(&u) { u } else { ziel::start_url().clone() };
                fenster::extern_oeffnen(app, &u);
            }
        }
        "zoom-null" | "zoom-plus" | "zoom-minus" => {
            let zustand = app.state::<Zustand>();
            let mut zoom = zustand.zoom.lock().unwrap();
            let aktuell = *zoom.get(w.label()).unwrap_or(&1.0);
            let neu = match id {
                "zoom-plus" => ZOOM_STUFEN.iter().copied().find(|s| *s > aktuell + 0.001).unwrap_or(aktuell),
                "zoom-minus" => ZOOM_STUFEN.iter().rev().copied().find(|s| *s < aktuell - 0.001).unwrap_or(aktuell),
                _ => 1.0,
            };
            if w.set_zoom(neu).is_ok() {
                zoom.insert(w.label().to_string(), neu);
            }
        }
        "vollbild" => {
            let an = w.is_fullscreen().unwrap_or(false);
            let _ = w.set_fullscreen(!an);
        }
        _ => {}
    }
}
