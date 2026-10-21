//! macOS: File-Provider-Domäne „Ablage“ der Erweiterung
//! `Contents/PlugIns/AtlasAblage.appex` (finder-ablage/, gebaut vom Workflow).
//!
//! Jeder Aufruf in FileProvider läuft in `objc2::exception::catch` und die
//! Rückrufe in `catch_unwind` — eine Objective-C-Ausnahme, die durch Rust
//! läuft, würde die App beenden (wie in safari.rs).

use std::panic::AssertUnwindSafe;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_file_provider::{
    NSFileProviderDomain, NSFileProviderDomainRemovalMode, NSFileProviderManager,
    NSFileProviderRootContainerItemIdentifier, NSFileProviderWorkingSetContainerItemIdentifier,
};
use objc2_foundation::{NSError, NSString, NSURL};
use tauri::AppHandle;

use super::Zugang;

const DOMAIN_ID: &str = "atlas-ablage";
/// Muss mit finder-ablage/Ablage.entitlements und Info.plist übereinstimmen.
const APP_GRUPPE: &str = "KJ63M9QZUG.com.sinclairs-solutions.atlas";
const SCHLUESSELBUND_DIENST: &str = "com.sinclairs-solutions.atlas.ablage-app";
const SCHLUESSELBUND_KONTO: &str = "geraet";
const APPEX: &str = "AtlasAblage.appex";

fn domain() -> Option<Retained<NSFileProviderDomain>> {
    objc2::exception::catch(AssertUnwindSafe(|| unsafe {
        NSFileProviderDomain::initWithIdentifier_displayName(
            NSFileProviderDomain::alloc(),
            &NSString::from_str(DOMAIN_ID),
            &NSString::from_str("Ablage"),
        )
    }))
    .ok()
}

fn fehlertext(e: *mut NSError) -> Option<String> {
    if e.is_null() {
        return None;
    }
    // SAFETY: FileProvider übergibt ein gültiges NSError, solange der Block läuft.
    let t = objc2::exception::catch(AssertUnwindSafe(|| unsafe { (*e).localizedDescription().to_string() }));
    Some(t.unwrap_or_else(|_| "Unbekannter Fehler".into()))
}

/// Ruft eine FileProvider-Funktion mit Fehler-Rückruf auf und wartet (max. 20 s).
fn mit_rueckruf(f: impl FnOnce(&block2::DynBlock<dyn Fn(*mut NSError)>)) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    let tx = std::sync::Mutex::new(Some(tx));
    let block = RcBlock::new(move |e: *mut NSError| {
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            if let Some(s) = tx.lock().ok().and_then(|mut g| g.take()) {
                let _ = s.send(fehlertext(e));
            }
        }));
    });
    objc2::exception::catch(AssertUnwindSafe(|| f(&block))).map_err(|e| format!("FileProvider: {e:?}"))?;
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(None) => Ok(()),
        Ok(Some(f)) => Err(f),
        Err(_) => Err("FileProvider antwortet nicht.".into()),
    }
}

pub fn eingebaut() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    exe.parent()
        .and_then(|p| p.parent())
        .map(|c| c.join("PlugIns").join(APPEX).join("Contents/Info.plist").exists())
        .unwrap_or(false)
}

/// Ordner der App-Gruppe. Über NSFileManager, damit macOS ihn als
/// Gruppen-Container anlegt (Container-Schutz ab macOS 15). Rückfall auf den
/// festen Pfad, falls die App ohne Gruppen-Recht signiert ist (lokaler Bau).
fn gruppen_ordner() -> Option<std::path::PathBuf> {
    let ueber_system = objc2::exception::catch(AssertUnwindSafe(|| {
        objc2_foundation::NSFileManager::defaultManager()
            .containerURLForSecurityApplicationGroupIdentifier(&NSString::from_str(APP_GRUPPE))
            .and_then(|u| u.path())
            .map(|p| std::path::PathBuf::from(p.to_string()))
    }))
    .ok()
    .flatten();
    ueber_system.or_else(|| {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Library/Group Containers").join(APP_GRUPPE))
    })
}

/// Zugang an die Erweiterung übergeben (sie übernimmt ihn in ihren Schlüsselbund
/// und löscht die Datei) und die Domäne beim System anmelden.
pub fn verbinden(_app: &AppHandle, z: &Zugang) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let ordner = gruppen_ordner().ok_or("Kein Benutzerordner")?.join("Ablage");
    std::fs::create_dir_all(&ordner).map_err(|e| format!("App-Gruppe: {e}"))?;
    let _ = std::fs::set_permissions(&ordner, std::fs::Permissions::from_mode(0o700));
    let ziel = ordner.join("zugang.json");
    let tmp = ordner.join(".zugang.json.neu");
    let inhalt = serde_json::to_vec(z).map_err(|e| e.to_string())?;
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| format!("Übergabe: {e}"))?;
        f.write_all(&inhalt).map_err(|e| format!("Übergabe: {e}"))?;
    }
    std::fs::rename(&tmp, &ziel).map_err(|e| format!("Übergabe: {e}"))?;

    let d = domain().ok_or("Domäne nicht anlegbar")?;
    mit_rueckruf(|b| unsafe { NSFileProviderManager::addDomain_completionHandler(&d, b) })
        .map_err(|e| format!("Ablage im Finder nicht angemeldet: {e}"))?;
    anstossen(_app);
    Ok(())
}

/// Domäne entfernen. Noch nicht hochgeladene eigene Dateien sichert macOS
/// dabei in einen Wiederherstellungsordner (statt sie zu verwerfen); alles
/// aus Atlas Geladene wird gelöscht.
pub fn trennen(_app: &AppHandle) -> Result<(), String> {
    if let Some(o) = gruppen_ordner() {
        let _ = std::fs::remove_file(o.join("Ablage/zugang.json"));
    }
    let d = domain().ok_or("Domäne nicht anlegbar")?;
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    let tx = std::sync::Mutex::new(Some(tx));
    let block = RcBlock::new(move |url: *mut NSURL, e: *mut NSError| {
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            if !url.is_null() {
                // SAFETY: gültig, solange der Block läuft.
                let pfad = objc2::exception::catch(AssertUnwindSafe(|| unsafe { (*url).path().map(|p| p.to_string()) }))
                    .ok()
                    .flatten();
                log::info!("Ablage entfernt; ungesicherte eigene Dateien liegen unter {pfad:?}");
            }
            if let Some(s) = tx.lock().ok().and_then(|mut g| g.take()) {
                let _ = s.send(fehlertext(e));
            }
        }));
    });
    objc2::exception::catch(AssertUnwindSafe(|| unsafe {
        NSFileProviderManager::removeDomain_mode_completionHandler(
            &d,
            NSFileProviderDomainRemovalMode::PreserveDirtyUserData,
            &block,
        )
    }))
    .map_err(|e| format!("FileProvider: {e:?}"))?;
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(None) => Ok(()),
        Ok(Some(f)) => Err(f),
        Err(_) => Err("FileProvider antwortet nicht.".into()),
    }
}

/// Abgleich anstoßen: die Erweiterung fragt Atlas nach Änderungen.
pub fn anstossen(_app: &AppHandle) {
    let Some(d) = domain() else { return };
    let ergebnis = objc2::exception::catch(AssertUnwindSafe(|| unsafe {
        let Some(m) = NSFileProviderManager::managerForDomain(&d) else { return };
        let block = RcBlock::new(|e: *mut NSError| {
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                if let Some(f) = fehlertext(e) {
                    log::debug!("Ablage: Anstoßen: {f}");
                }
            }));
        });
        m.signalEnumeratorForContainerItemIdentifier_completionHandler(
            NSFileProviderWorkingSetContainerItemIdentifier,
            &block,
        );
    }));
    if let Err(e) = ergebnis {
        log::warn!("Ablage: Anstoßen fehlgeschlagen: {e:?}");
    }
}

/// Ordner „Atlas“ im Finder öffnen.
pub fn zeigen(_app: &AppHandle) -> Result<(), String> {
    let d = domain().ok_or("Domäne nicht anlegbar")?;
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    let tx = std::sync::Mutex::new(Some(tx));
    let block = RcBlock::new(move |url: *mut NSURL, _e: *mut NSError| {
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let pfad = if url.is_null() {
                None
            } else {
                objc2::exception::catch(AssertUnwindSafe(|| unsafe { (*url).path().map(|p| p.to_string()) }))
                    .ok()
                    .flatten()
            };
            if let Some(s) = tx.lock().ok().and_then(|mut g| g.take()) {
                let _ = s.send(pfad);
            }
        }));
    });
    objc2::exception::catch(AssertUnwindSafe(|| unsafe {
        if let Some(m) = NSFileProviderManager::managerForDomain(&d) {
            m.getUserVisibleURLForItemIdentifier_completionHandler(NSFileProviderRootContainerItemIdentifier, &block);
        }
    }))
    .map_err(|e| format!("FileProvider: {e:?}"))?;
    let pfad = rx
        .recv_timeout(Duration::from_secs(10))
        .ok()
        .flatten()
        .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/Library/CloudStorage")))
        .ok_or("Ordner nicht gefunden.")?;
    std::process::Command::new("/usr/bin/open").arg(&pfad).status().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn token_speichern(token: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password(SCHLUESSELBUND_DIENST, SCHLUESSELBUND_KONTO, token.as_bytes())
        .map_err(|e| format!("Schlüsselbund: {e}"))
}

pub fn token_lesen() -> Option<String> {
    security_framework::passwords::get_generic_password(SCHLUESSELBUND_DIENST, SCHLUESSELBUND_KONTO)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
}

pub fn token_loeschen() {
    let _ = security_framework::passwords::delete_generic_password(SCHLUESSELBUND_DIENST, SCHLUESSELBUND_KONTO);
}

/// FileVault an? (`fdesetup status` braucht keine Administratorrechte.)
pub fn verschluesselung() -> Option<bool> {
    let o = std::process::Command::new("/usr/bin/fdesetup").arg("status").output().ok()?;
    let s = String::from_utf8_lossy(&o.stdout);
    if s.contains("FileVault is On") {
        Some(true)
    } else if s.contains("FileVault is Off") {
        Some(false)
    } else {
        None
    }
}
