//! Automatische Updates über den Tauri-Updater: 20 s nach dem Start, danach
//! alle 6 Stunden, und auf Zuruf über das Menü. Gefragt wird immer — Atlas
//! kann ungespeicherte Eingaben enthalten, daher kein stiller Neustart.

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

use crate::texte::t;

static LAEUFT: AtomicBool = AtomicBool::new(false);

pub fn starten(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let warten = if cfg!(debug_assertions) && std::env::var("ATLAS_DESKTOP_UPDATE_OHNE_FRAGE").is_ok() {
            3
        } else {
            20
        };
        tokio::time::sleep(Duration::from_secs(warten)).await;
        loop {
            pruefen(&app, false).await;
            tokio::time::sleep(Duration::from_secs(6 * 60 * 60)).await;
        }
    });
}

/// Ist ein echter Updater-Schlüssel eingetragen? Solange in `tauri.conf.json`
/// der Platzhalter steht, wird gar nicht erst gesucht.
fn eingerichtet(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .map(|k| !k.is_empty() && !k.starts_with("ERSETZEN"))
        .unwrap_or(false)
}

fn meldung(app: &AppHandle, titel: &str, text: String, art: MessageDialogKind) {
    app.dialog()
        .message(text)
        .title(titel)
        .kind(art)
        .buttons(MessageDialogButtons::Ok)
        .show(|_| {});
}

pub async fn pruefen(app: &AppHandle, manuell: bool) {
    if !eingerichtet(app) {
        if manuell {
            meldung(
                app,
                t("Updates", "Updates"),
                t(
                    "Automatische Updates sind in dieser Version noch nicht eingerichtet.",
                    "Automatic updates are not set up in this build yet.",
                )
                .into(),
                MessageDialogKind::Info,
            );
        }
        return;
    }
    if LAEUFT.swap(true, Ordering::SeqCst) {
        return;
    }
    let ergebnis = suchen_und_anbieten(app, manuell).await;
    LAEUFT.store(false, Ordering::SeqCst);
    if let Err(e) = ergebnis {
        log::warn!("Update-Prüfung fehlgeschlagen: {e}");
        if manuell {
            meldung(
                app,
                t("Updates", "Updates"),
                format!(
                    "{}\n\n{e}",
                    t("Die Suche nach Updates ist fehlgeschlagen.", "Checking for updates failed.")
                ),
                MessageDialogKind::Warning,
            );
        }
    }
}

async fn suchen_und_anbieten(app: &AppHandle, manuell: bool) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        if manuell {
            meldung(
                app,
                t("Updates", "Updates"),
                format!(
                    "{} ({})",
                    t("Sinclair Atlas ist auf dem neuesten Stand.", "Sinclair Atlas is up to date."),
                    app.package_info().version
                ),
                MessageDialogKind::Info,
            );
        }
        return Ok(());
    };

    let frage = format!(
        "{} {} {}\n\n{}",
        t("Version", "Version"),
        update.version,
        t("ist verfügbar.", "is available."),
        t(
            "Jetzt installieren? Sinclair Atlas startet danach neu — vorher bitte offene Eingaben speichern.",
            "Install now? Sinclair Atlas will restart — please save any open edits first.",
        ),
    );
    // Nur Debug: automatischer Test ohne Klick auf den Dialog.
    if cfg!(debug_assertions) && std::env::var("ATLAS_DESKTOP_UPDATE_OHNE_FRAGE").is_ok() {
        log::warn!("Update {} wird ohne Rückfrage installiert (Test)", update.version);
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| e.to_string())?;
        app.restart();
    }

    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .message(frage)
        .title(t("Update verfügbar", "Update available"))
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            t("Installieren und neu starten", "Install and Restart").into(),
            t("Später", "Later").into(),
        ))
        .show(move |ja| {
            let _ = tx.send(ja);
        });
    let ja = tauri::async_runtime::spawn_blocking(move || rx.recv().unwrap_or(false))
        .await
        .unwrap_or(false);
    if !ja {
        return Ok(());
    }

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}
