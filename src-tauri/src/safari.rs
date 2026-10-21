//! Safari-Erweiterung „Atlas Passwort-Safe“, die in der Mac-App steckt
//! (`Contents/PlugIns/*.appex`, vom Release-Workflow eingebaut).
//!
//! Safari schaltet Erweiterungen nur selbst ein — die App kann den Zustand
//! abfragen und die passende Stelle in Safaris Einstellungen öffnen.

/// Bundle-ID der Erweiterung. Muss mit der ID der App beginnen und mit
/// `ERWEITERUNG_ID` in `.github/workflows/bauen.yml` übereinstimmen.
#[cfg(target_os = "macos")]
const ERWEITERUNG_ID: &str = "com.sinclairs-solutions.atlas.passwort-safe";

/// Steckt die Erweiterung in diesem App-Bundle? (Lokale Builds ohne
/// Workflow haben keine.)
#[cfg(target_os = "macos")]
fn eingebaut() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    // …/Sinclair Atlas.app/Contents/MacOS/Sinclair Atlas → …/Contents/PlugIns
    let Some(plugins) = exe.parent().and_then(|p| p.parent()).map(|c| c.join("PlugIns")) else {
        return false;
    };
    std::fs::read_dir(plugins)
        .map(|it| it.flatten().any(|e| e.path().extension().is_some_and(|x| x == "appex")))
        .unwrap_or(false)
}

/// Zustand für die Einstellungen: `"an"`, `"aus"`, `"unbekannt"` (Safari
/// kennt die Erweiterung noch nicht, etwa weil die App nicht unter
/// „Programme“ liegt), `"fehlt"` (nicht eingebaut) oder `"nicht_mac"`.
#[tauri::command]
pub async fn safari_erweiterung_status() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        if !eingebaut() {
            return "fehlt";
        }
        let Some(rx) = zustand_anfordern() else { return "unbekannt" };
        match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
            Ok(Ok(Some(true))) => "an",
            Ok(Ok(Some(false))) => "aus",
            _ => "unbekannt",
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        "nicht_mac"
    }
}

#[cfg(target_os = "macos")]
fn zustand_anfordern() -> Option<tokio::sync::oneshot::Receiver<Option<bool>>> {
    use block2::RcBlock;
    use objc2_foundation::{NSError, NSString};
    use objc2_safari_services::{SFSafariExtensionManager, SFSafariExtensionState};
    use std::sync::Mutex;

    let (tx, rx) = tokio::sync::oneshot::channel::<Option<bool>>();
    let tx = Mutex::new(Some(tx));
    let id = NSString::from_str(ERWEITERUNG_ID);
    let block = RcBlock::new(move |zustand: *mut SFSafariExtensionState, fehler: *mut NSError| {
        let wert = if !fehler.is_null() || zustand.is_null() {
            None
        } else {
            // SAFETY: Safari übergibt ein gültiges Objekt, solange der Block läuft.
            Some(unsafe { (*zustand).isEnabled() })
        };
        if let Some(s) = tx.lock().unwrap().take() {
            let _ = s.send(wert);
        }
    });
    unsafe {
        SFSafariExtensionManager::getStateOfSafariExtensionWithIdentifier_completionHandler(&id, &block);
    }
    Some(rx)
}

/// Öffnet Safaris Einstellungen bei „Erweiterungen“ mit unserer Erweiterung.
/// Klappt das nicht (Safari kennt sie noch nicht), wird Safari geöffnet und
/// der Fehler mit einer Anleitung zurückgegeben.
#[tauri::command]
pub async fn safari_erweiterung_oeffnen() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use crate::texte::t;
        if !eingebaut() {
            return Err(t(
                "Diese App enthält die Safari-Erweiterung nicht. Bitte die aktuelle Version von sinclair-atlas.com laden.",
                "This app does not include the Safari extension. Please download the current version from sinclair-atlas.com.",
            )
            .into());
        }
        let rx = einstellungen_anfordern();
        let ok = matches!(
            tokio::time::timeout(std::time::Duration::from_secs(10), rx).await,
            Ok(Ok(true))
        );
        if ok {
            return Ok(());
        }
        let _ = std::process::Command::new("/usr/bin/open").args(["-b", "com.apple.Safari"]).status();
        Err(t(
            "Safari kennt die Erweiterung noch nicht. Liegt Sinclair Atlas unter „Programme“? Dann in Safari → Einstellungen → Erweiterungen „Atlas Passwort-Safe“ einschalten.",
            "Safari doesn’t know the extension yet. Is Sinclair Atlas in “Applications”? Then turn on “Atlas Passwort-Safe” in Safari → Settings → Extensions.",
        )
        .into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Nur auf dem Mac.".into())
    }
}

#[cfg(target_os = "macos")]
fn einstellungen_anfordern() -> tokio::sync::oneshot::Receiver<bool> {
    use block2::RcBlock;
    use objc2_foundation::{NSError, NSString};
    use objc2_safari_services::SFSafariApplication;
    use std::sync::Mutex;

    let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
    let tx = Mutex::new(Some(tx));
    let id = NSString::from_str(ERWEITERUNG_ID);
    let block = RcBlock::new(move |fehler: *mut NSError| {
        if let Some(s) = tx.lock().unwrap().take() {
            let _ = s.send(fehler.is_null());
        }
    });
    unsafe {
        SFSafariApplication::showPreferencesForExtensionWithIdentifier_completionHandler(&id, Some(&block));
    }
    rx
}
