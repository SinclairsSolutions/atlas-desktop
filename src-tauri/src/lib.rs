mod aktualisierung;
mod bruecke;
mod downloads;
mod erreichbarkeit;
mod fenster;
mod menue;
#[cfg(windows)]
mod taskleiste;
mod texte;
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
        .manage(zustand::Zustand::default())
        .invoke_handler(tauri::generate_handler![
            bruecke::mitteilung,
            bruecke::mitteilungen_erlauben,
            bruecke::kennzahl,
            bruecke::extern_oeffnen,
            bruecke::verbinden,
        ])
        .menu(menue::erstellen)
        .on_menu_event(menue::ereignis)
        .setup(|app| {
            let handle = app.handle().clone();

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
                        .permission("allow-extern-oeffnen"),
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
            aktualisierung::starten(handle);
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // macOS: Schließen blendet nur aus, die App läuft weiter (Dock-Zahl,
            // Mitteilungen). Windows: Schließen beendet.
            WindowEvent::CloseRequested { api, .. }
                if cfg!(target_os = "macos") && window.label() == fenster::HAUPT =>
            {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(true) if window.label() == fenster::HAUPT => {
                fenster::nach_anmeldung_neu_laden(window.app_handle());
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
            let _ = (app, event);
        });
}
