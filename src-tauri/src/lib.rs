mod ablage;
mod aktualisierung;
mod bruecke;
mod downloads;
mod eingang;
mod einstellungen;
mod erreichbarkeit;
mod fenster;
mod importordner;
mod menue;
mod mitteilung;
mod safari;
mod sperre;
mod symbol;
#[cfg(windows)]
mod taskleiste;
mod texte;
mod werkzeuge;
mod ziel;
mod zustand;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_window_state::StateFlags;

pub fn run() {
    // reqwest ist ohne eingebauten TLS-Anbieter gebaut (wie im Updater-Plugin).
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
        // Muss als erstes Plugin stehen: ein zweiter Start (auch per Deep-Link
        // unter Windows) holt nur das laufende Fenster nach vorn.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            fenster::hauptfenster_zeigen(app);
        }))
        .plugin(
            // Protokoll unter ~/Library/Logs/com.sinclairs-solutions.atlas bzw.
            // %LOCALAPPDATA%\com.sinclairs-solutions.atlas\logs — keine Seiteninhalte.
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000)
                .build(),
        )
        .plugin(tauri_plugin_deep_link::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .with_filter(|label| label == fenster::HAUPT)
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(einstellungen::Speicher::default())
        .manage(eingang::Eingang::default())
        .manage(importordner::Import::default())
        .manage(sperre::Zustand::default())
        .manage(zustand::Zustand::default())
        .manage(ablage::Ablage::default())
        .invoke_handler(tauri::generate_handler![
            bruecke::mitteilung,
            bruecke::mitteilungen_erlauben,
            bruecke::kennzahl,
            bruecke::extern_oeffnen,
            bruecke::drucken,
            bruecke::verbinden,
            downloads::datei_speichern,
            downloads::im_finder_zeigen,
            eingang::dateien_erledigt,
            importordner::import_ordner_setzen,
            werkzeuge::import_ordner_anlegen,
            werkzeuge::nach_updates_suchen,
            sperre::touch_id_pruefen,
            sperre::entsperren,
            sperre::geraet_geheimnis,
            werkzeuge::einstellungen_lesen,
            werkzeuge::einstellungen_setzen,
            werkzeuge::autostart_setzen,
            werkzeuge::ordner_waehlen,
            werkzeuge::mitteilung_testen,
            werkzeuge::ordner_zeigen,
            werkzeuge::warteschlange_senden,
            werkzeuge::notiz_senden,
            werkzeuge::beleg_auswaehlen,
            werkzeuge::bildschirmfoto,
            safari::safari_erweiterung_status,
            safari::safari_erweiterung_oeffnen,
            ablage::ablage_status,
            ablage::ablage_verbunden,
            ablage::ablage_fehler,
            ablage::ablage_abgemeldet,
            ablage::ablage_info,
            ablage::ablage_schalten,
            ablage::ablage_zeigen,
        ])
        .menu(menue::erstellen)
        .on_menu_event(menue::ereignis)
        .setup(|app| {
            let handle = app.handle().clone();
            *app.state::<einstellungen::Speicher>().0.lock().unwrap() = einstellungen::laden(&handle);
            mitteilung::einrichten(&handle);
            importordner::laden(&handle);

            if ziel::ist_uebersteuert() {
                // Nur Debug: lokale Atlas-Instanz bekommt dieselben Brückenrechte.
                use tauri::ipc::CapabilityBuilder;
                app.add_capability(
                    CapabilityBuilder::new("atlas-entwicklung")
                        .remote(ziel::start_url().origin().ascii_serialization())
                        .windows(["main", "neben-*"])
                        .permission("allow-mitteilung")
                        .permission("allow-mitteilungen-erlauben")
                        .permission("allow-kennzahl")
                        .permission("allow-im-finder-zeigen")
                        .permission("allow-extern-oeffnen")
                        .permission("allow-drucken")
                        .permission("allow-datei-speichern")
                        .permission("allow-dateien-erledigt")
                        .permission("allow-import-ordner-setzen")
                        .permission("allow-touch-id-pruefen")
                        .permission("allow-geraet-geheimnis")
                        .permission("allow-ablage-status")
                        .permission("allow-ablage-verbunden")
                        .permission("allow-ablage-fehler")
                        .permission("allow-ablage-abgemeldet")
                        .permission("core:window:allow-start-dragging"),
                )?;
                log::warn!("Atlas Desktop lädt {}", ziel::start_url());
            }

            // Deep-Link beim Start merken (Windows übergibt ihn als Argument),
            // spätere Links direkt öffnen.
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                if let Some(z) = urls.iter().find_map(ziel::deep_link_ziel) {
                    *app.state::<zustand::Zustand>().start_ziel.lock().unwrap() = Some(z);
                }
            }
            let h = handle.clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    fenster::deep_link_oeffnen(&h, &url);
                }
            });
            #[cfg(windows)]
            {
                // Schema im Benutzerprofil registrieren (der Installer tut es auch;
                // so funktioniert es auch nach manuellem Kopieren).
                let _ = app.deep_link().register_all();
            }

            fenster::hauptfenster_bauen(&handle)?;
            werkzeuge::anwenden(&handle, None);
            sperre::beim_start(&handle);
            eingang::starten(handle.clone());
            ablage::starten(&handle);
            aktualisierung::starten(handle);
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // macOS: Schließen blendet nur aus, die App läuft weiter (Dock-Zahl,
            // Mitteilungen). Windows: ebenso, solange das Symbol im Infobereich
            // da ist (Einstellung „Menüleiste“, Standard an) — dort holt „Atlas
            // öffnen“ das Fenster zurück, „Beenden“ beendet. Ohne Symbol beendet
            // Schließen die App (sonst liefe sie unsichtbar weiter).
            WindowEvent::CloseRequested { api, .. }
                if window.label() == fenster::HAUPT
                    && (cfg!(target_os = "macos")
                        || window.app_handle().tray_by_id(werkzeuge::TRAY_ID).is_some()) =>
            {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(fokus) => {
                sperre::fokus(window.app_handle(), window.label(), *fokus);
                if *fokus && window.label() == fenster::HAUPT {
                    fenster::nach_anmeldung_neu_laden(window.app_handle());
                    // Seite geladen und im Vordergrund: wartende Belege übergeben.
                    eingang::abarbeiten(window.app_handle());
                    ablage::anstossen(window.app_handle());
                }
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("Atlas Desktop konnte nicht starten")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let RunEvent::Reopen { .. } = event {
                fenster::hauptfenster_zeigen(app);
            }
            // Dateien aufs Dock-Symbol gezogen bzw. „Öffnen mit → Sinclair Atlas“.
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            if let RunEvent::Opened { urls } = &event {
                eingang::geoeffnet(app, urls);
            }
            let _ = (app, event);
        });
}
