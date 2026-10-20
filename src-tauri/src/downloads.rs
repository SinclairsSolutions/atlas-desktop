//! Downloads landen im Download-Ordner (gleiche Namen bekommen „(1)“, „(2)“ …,
//! das erledigt WebKit bzw. WebView2). Nach Abschluss eine System-Mitteilung.

use tauri::{webview::DownloadEvent, AppHandle, Manager};

use crate::{texte::t, zustand::Zustand};

pub fn ereignis(app: &AppHandle, event: DownloadEvent<'_>) -> bool {
    match event {
        DownloadEvent::Requested { url, destination } => {
            // Nur Debug: Zielordner umleiten (Tests ohne Zugriff auf ~/Downloads).
            if cfg!(debug_assertions) {
                if let (Ok(dir), Some(name)) =
                    (std::env::var("ATLAS_DESKTOP_DOWNLOADS"), destination.file_name())
                {
                    *destination = std::path::Path::new(&dir).join(name);
                }
            }
            log::info!("Download gestartet: {} → {}", url.scheme(), destination.display());
            app.state::<Zustand>()
                .downloads
                .lock()
                .unwrap()
                .insert(url.to_string(), destination.clone());
            true
        }
        DownloadEvent::Finished { url, path, success } => {
            let gemerkt = app.state::<Zustand>().downloads.lock().unwrap().remove(url.as_str());
            match path.or(gemerkt) {
                Some(p) if success => {
                    herkunft_markieren(&p, false);
                    gespeichert(app, &p)
                }
                _ => {
                    let _ = crate::mitteilung::zeigen(
                        app,
                        t("Download fehlgeschlagen", "Download failed"),
                        t("Bitte erneut versuchen.", "Please try again."),
                    );
                }
            }
            true
        }
        _ => true,
    }
}

/// Download-Ordner im Finder bzw. Explorer öffnen (Menü).
pub fn ordner_oeffnen(app: &AppHandle) {
    use tauri_plugin_opener::OpenerExt;
    if let Ok(dir) = app.path().download_dir() {
        let _ = app.opener().open_path(dir.to_string_lossy(), None::<&str>);
    }
}

/// Höchstgröße für Downloads, die die Seite selbst übergibt (Blob/Data-URL).
const MAX_BYTES: usize = 200 * 1024 * 1024;

/// Dateiname ohne Pfadanteile und Steuerzeichen, höchstens 200 Byte (Grenze
/// der Dateisysteme: 255 Byte), ohne Punkte/Leerzeichen am Ende und ohne
/// reservierte Windows-Namen (CON, NUL, COM1 …).
pub(crate) fn sauberer_name(name: &str) -> String {
    let mut n = String::new();
    for c in name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        if n.len() + c.len_utf8() > 200 {
            break;
        }
        n.push(c);
    }
    let n = n.trim().trim_start_matches('.').trim_end_matches(['.', ' ']).to_string();
    if n.is_empty() {
        return "Download".to_string();
    }
    if windows_reserviert(&n) { format!("_{n}") } else { n }
}

/// CON, PRN, AUX, NUL, COM1–9, LPT1–9 (auch mit Endung) sind unter Windows Geräte.
pub(crate) fn windows_reserviert(name: &str) -> bool {
    let stamm = name.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    matches!(stamm.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stamm.starts_with("COM") || stamm.starts_with("LPT"))
            && stamm.len() == 4
            && stamm.as_bytes()[3].is_ascii_digit())
}

/// Herkunft wie bei einem Browser-Download markieren: macOS-Quarantäne bzw.
/// Windows „Mark of the Web“. Ohne sie prüft Gatekeeper/SmartScreen eine aus
/// Atlas gespeicherte Datei (z. B. „.command“, „.exe“) beim Öffnen nicht.
/// `neu`: Datei hat die App selbst geschrieben (sonst schon vorhandene Marke behalten).
fn herkunft_markieren(pfad: &std::path::Path, neu: bool) {
    #[cfg(target_os = "macos")]
    {
        let xattr = "/usr/bin/xattr";
        let vorhanden = !neu
            && std::process::Command::new(xattr)
                .args(["-p", "com.apple.quarantine"])
                .arg(pfad)
                .output()
                .is_ok_and(|o| o.status.success());
        if !vorhanden {
            let zeit = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let wert = format!("0081;{zeit:x};Sinclair Atlas;");
            if let Err(e) = std::process::Command::new(xattr)
                .args(["-w", "com.apple.quarantine", &wert])
                .arg(pfad)
                .status()
            {
                log::warn!("Quarantäne-Marke nicht gesetzt: {e}");
            }
        }
    }
    #[cfg(windows)]
    {
        // WebView2 markiert eigene Downloads selbst.
        if neu {
            let mut strom = pfad.as_os_str().to_owned();
            strom.push(":Zone.Identifier");
            if let Err(e) = std::fs::write(&strom, "[ZoneTransfer]\r\nZoneId=3\r\n") {
                log::warn!("Mark of the Web nicht gesetzt: {e}");
            }
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = (pfad, neu);
}

/// Freier Name im Ordner: „Datei.pdf“, „Datei (1).pdf“, …
pub(crate) fn freier_pfad(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stamm, endung) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for i in 1..1000 {
        let k = dir.join(format!("{stamm} ({i}){endung}"));
        if !k.exists() {
            return k;
        }
    }
    dir.join(format!("{stamm}-{}{endung}", std::process::id()))
}

/// Downloads aus Blob- oder Data-Adressen (WebKit meldet sie nicht als
/// Download): Die Seite liest die Datei und übergibt sie hier. Gespeichert
/// wird ausschließlich im Download-Ordner.
#[tauri::command]
pub fn datei_speichern(
    app: AppHandle,
    name: String,
    inhalt: String,
    mandant: Option<String>,
) -> Result<String, String> {
    use base64::Engine;
    let daten = base64::engine::general_purpose::STANDARD
        .decode(inhalt.as_bytes())
        .map_err(|e| format!("Ungültige Daten: {e}"))?;
    if daten.len() > MAX_BYTES {
        return Err("Datei zu groß.".into());
    }
    let dir = if cfg!(debug_assertions) {
        std::env::var("ATLAS_DESKTOP_DOWNLOADS").map(std::path::PathBuf::from).ok()
    } else {
        None
    }
    .or_else(|| export_ordner(&app, mandant.as_deref()))
    .or_else(|| app.path().download_dir().ok())
    .ok_or("Download-Ordner nicht gefunden.")?;
    let ziel = freier_pfad(&dir, &sauberer_name(&name));
    // Nur neu anlegen (folgt keinem Symlink, überschreibt nichts).
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&ziel)
            .map_err(|e| e.to_string())?;
        f.write_all(&daten).map_err(|e| e.to_string())?;
    }
    herkunft_markieren(&ziel, true);
    let anzeige = ziel.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    log::info!("Download gespeichert: {} Bytes", daten.len());
    gespeichert(&app, &ziel);
    Ok(anzeige)
}

/// Zuletzt gespeicherte Dateien (für „Im Finder zeigen“ — die Seite bekommt
/// nur den Dateinamen, nie den vollen Pfad).
static ZULETZT: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Nach dem Speichern: Downloads-Stapel im Dock hüpfen lassen (wie Safari)
/// und Atlas Bescheid geben, damit dort „Gespeichert · Im Finder zeigen“ steht.
fn gespeichert(app: &AppHandle, pfad: &std::path::Path) {
    {
        let mut z = ZULETZT.lock().unwrap();
        z.push(pfad.to_path_buf());
        let n = z.len();
        if n > 30 {
            z.drain(0..n - 30);
        }
    }
    #[cfg(target_os = "macos")]
    dock_huepfen(pfad);
    let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ordner = pfad
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let js = format!(
        "window.__atlasDesktopGespeichert && window.__atlasDesktopGespeichert({})",
        serde_json::json!({ "name": name, "ordner": ordner })
    );
    if let Some(w) = crate::fenster::aktives_fenster(app) {
        let _ = w.eval(&js);
    }
}

/// Dock: Downloads-Stapel hüpfen lassen (gleiche Meldung, die Safari schickt).
#[cfg(target_os = "macos")]
fn dock_huepfen(pfad: &std::path::Path) {
    use objc2_foundation::{NSDistributedNotificationCenter, NSString};
    let center = NSDistributedNotificationCenter::defaultCenter();
    let name = NSString::from_str("com.apple.DownloadFileFinished");
    let objekt = NSString::from_str(&pfad.to_string_lossy());
    unsafe {
        center.postNotificationName_object(&name, Some(&objekt));
    }
}

/// „Im Finder zeigen“ für eine zuletzt gespeicherte Datei (nach Namen).
#[tauri::command]
pub fn im_finder_zeigen(name: String) -> Result<(), String> {
    let pfad = ZULETZT
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|p| p.file_name().map(|n| n.to_string_lossy() == name.as_str()).unwrap_or(false))
        .cloned()
        .ok_or("Datei nicht gefunden.")?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("/usr/bin/open").arg("-R").arg(&pfad).spawn().map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    {
        // Voller Pfad statt Suche über PATH bzw. den Programmordner.
        let explorer = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))
            .join("explorer.exe");
        std::process::Command::new(explorer).arg(format!("/select,{}", pfad.display())).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Export-Ordner aus den Einstellungen, auf Wunsch mit Unterordner je Mandant.
fn export_ordner(app: &AppHandle, mandant: Option<&str>) -> Option<std::path::PathBuf> {
    let e = crate::einstellungen::aktuell(app);
    let basis = std::path::PathBuf::from(e.export_ordner?);
    if !basis.is_dir() {
        return None;
    }
    let ziel = match mandant.map(sauberer_name).filter(|m| m != "Download") {
        Some(m) if e.export_je_mandant => basis.join(m),
        _ => basis,
    };
    std::fs::create_dir_all(&ziel).ok()?;
    Some(ziel)
}
