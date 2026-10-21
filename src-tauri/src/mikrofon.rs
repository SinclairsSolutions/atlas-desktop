//! Mikrofon-Erlaubnis von macOS für die Atlas-Seite.
//!
//! WebKit in der App fragt macOS nicht immer selbst nach dem Mikrofon; ohne
//! Freigabe liefert getUserMedia dann nur Stille. Die Seite ruft vor einer
//! Aufnahme `mikrofon_erlaubnis` auf: ist noch nichts entschieden, erscheint
//! die Systemabfrage; bei „verweigert“ führt `mikrofon_einstellungen` direkt
//! zu Systemeinstellungen → Datenschutz & Sicherheit → Mikrofon.
//! (Die Seite selbst bekommt Kamera/Mikrofon nur auf Atlas-Adressen, siehe
//! `on_permission_request` in fenster.rs.)

/// "erlaubt" | "verweigert" | "eingeschraenkt" | "offen" (unbekannt/nicht Mac).
#[tauri::command]
pub async fn mikrofon_erlaubnis() -> String {
    #[cfg(target_os = "macos")]
    {
        match status() {
            Some(3) => "erlaubt".into(),
            Some(2) => "verweigert".into(),
            Some(1) => "eingeschraenkt".into(),
            Some(0) => {
                let Some(rx) = anfrage_starten() else { return "offen".into() };
                // Die Person entscheidet im Systemdialog — großzügig warten.
                match tokio::time::timeout(std::time::Duration::from_secs(120), rx).await {
                    Ok(Ok(true)) => "erlaubt".into(),
                    Ok(Ok(false)) => "verweigert".into(),
                    _ => "offen".into(),
                }
            }
            _ => "offen".into(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        "offen".into()
    }
}

#[tauri::command]
pub fn mikrofon_einstellungen() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            .status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "ms-settings:privacy-microphone"])
            .status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn status() -> Option<isize> {
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    use std::panic::AssertUnwindSafe;
    objc2::exception::catch(AssertUnwindSafe(|| unsafe {
        let typ = AVMediaTypeAudio?;
        Some(AVCaptureDevice::authorizationStatusForMediaType(typ).0)
    }))
    .ok()
    .flatten()
}

#[cfg(target_os = "macos")]
fn anfrage_starten() -> Option<tokio::sync::oneshot::Receiver<bool>> {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    use std::panic::AssertUnwindSafe;
    use std::sync::Mutex;

    let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
    let tx = Mutex::new(Some(tx));
    let block = RcBlock::new(move |ok: Bool| {
        // Ein Panic darf nicht über die Objective-C-Grenze laufen.
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            if let Some(s) = tx.lock().ok().and_then(|mut g| g.take()) {
                let _ = s.send(ok.as_bool());
            }
        }));
    });
    let gestartet = objc2::exception::catch(AssertUnwindSafe(|| unsafe {
        let Some(typ) = AVMediaTypeAudio else { return false };
        AVCaptureDevice::requestAccessForMediaType_completionHandler(typ, &block);
        true
    }));
    if !matches!(gestartet, Ok(true)) {
        log::warn!("Mikrofon-Abfrage nicht möglich: {gestartet:?}");
        return None;
    }
    Some(rx)
}
