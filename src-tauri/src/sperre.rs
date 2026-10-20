//! Touch ID: App-Sperre und Prüfung für Atlas (z. B. Passwort-Safe).
//!
//! - App-Sperre (Einstellungen): beim Start und nach Abwesenheit sind alle
//!   Atlas-Fenster verborgen, bis Touch ID (oder das Mac-Passwort) bestätigt.
//! - `touch_id_pruefen`: Atlas kann eine Bestätigung anfordern
//!   (`window.__ATLAS_DESKTOP__.touchId("Tresor entsperren")`).
//! Windows: Windows Hello folgt — dort gibt es vorerst keine Sperre.

use std::{
    sync::{atomic::Ordering, Mutex},
    time::Instant,
};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{einstellungen, fenster, texte::t};

pub const SPERRE: &str = "sperre";

#[derive(Default)]
pub struct Zustand {
    pub gesperrt: std::sync::atomic::AtomicBool,
    /// Seit wann keines der Atlas-Fenster mehr im Vordergrund ist.
    pub weg_seit: Mutex<Option<Instant>>,
}

/// Läuft gerade eine Touch-ID-Abfrage? Dann verliert das Passwort-Fenster der
/// Menüleiste kurz den Fokus — und soll dabei nicht zugehen.
pub static PRUEFUNG_LAEUFT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Touch ID (mit Mac-Passwort als Ausweg). `true` = bestätigt.
pub async fn pruefen(grund: &str) -> bool {
    PRUEFUNG_LAEUFT.store(true, Ordering::SeqCst);
    let ok = match anfordern(grund) {
        Some(rx) => rx.await.unwrap_or(false),
        None => false,
    };
    PRUEFUNG_LAEUFT.store(false, Ordering::SeqCst);
    ok
}

/// Startet die Abfrage synchron (Objective-C-Objekte bleiben in diesem
/// Aufruf) und liefert den Empfänger für die Antwort.
fn anfordern(grund: &str) -> Option<tokio::sync::oneshot::Receiver<bool>> {
    #[cfg(target_os = "macos")]
    {
        use block2::RcBlock;
        use objc2::runtime::Bool;
        use objc2_foundation::{NSError, NSString};
        use objc2_local_authentication::{LAContext, LAPolicy};

        let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
        let tx = Mutex::new(Some(tx));
        let grund = NSString::from_str(grund);
        unsafe {
            let ctx = LAContext::new();
            if ctx.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthentication).is_err() {
                return None;
            }
            // Der Block hält den Kontext bis zur Antwort am Leben.
            let halten = ctx.clone();
            let block = RcBlock::new(move |ok: Bool, _err: *mut NSError| {
                let _ = &halten;
                if let Some(s) = tx.lock().unwrap().take() {
                    let _ = s.send(ok.as_bool());
                }
            });
            ctx.evaluatePolicy_localizedReason_reply(LAPolicy::DeviceOwnerAuthentication, &grund, &block);
        }
        Some(rx)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = grund;
        None
    }
}

/// Für die Einstellungen: Gibt es Touch ID bzw. eine Geräteanmeldung?
pub fn verfuegbar() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc2_local_authentication::{LAContext, LAPolicy};
        LAContext::new().canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthentication).is_ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[tauri::command]
pub async fn touch_id_pruefen(grund: Option<String>) -> bool {
    let g: String = grund.unwrap_or_default().chars().take(120).collect();
    let g = if g.trim().is_empty() { t("Atlas bestätigen", "Confirm Atlas").to_string() } else { g };
    pruefen(&g).await
}

pub fn ist_gesperrt(app: &AppHandle) -> bool {
    app.state::<Zustand>().gesperrt.load(Ordering::SeqCst)
}

/// Alle Atlas-Fenster verbergen und das Sperrfenster zeigen.
///
/// Auch bei schon gesperrter App: Fenster erneut verbergen und das
/// Sperrfenster neu anlegen, falls es von Hand geschlossen wurde — sonst
/// bliebe die App ohne Weg zum Entsperren.
pub fn sperren(app: &AppHandle) {
    let z = app.state::<Zustand>();
    z.gesperrt.store(true, Ordering::SeqCst);
    for (label, w) in app.webview_windows() {
        if label != SPERRE {
            let _ = w.hide();
        }
    }
    if let Some(w) = app.get_webview_window(SPERRE) {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let b = WebviewWindowBuilder::new(app, SPERRE, WebviewUrl::App("sperre.html".into()))
        .title("Sinclair Atlas")
        .inner_size(420.0, 360.0)
        .resizable(false)
        .center()
        .always_on_top(true);
    #[cfg(target_os = "macos")]
    let b = b.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    let _ = b.build();
}

/// Sperrfenster: Touch ID anfordern; bei Erfolg Fenster zurückholen.
#[tauri::command]
pub async fn entsperren(app: AppHandle) -> bool {
    if !pruefen(t("Atlas entsperren", "Unlock Atlas")).await {
        return false;
    }
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        a.state::<Zustand>().gesperrt.store(false, Ordering::SeqCst);
        *a.state::<Zustand>().weg_seit.lock().unwrap() = None;
        if let Some(w) = a.get_webview_window(SPERRE) {
            let _ = w.close();
        }
        fenster::hauptfenster_zeigen(&a);
        for (label, w) in a.webview_windows() {
            if label.starts_with("neben-") && label != fenster::PASSWORT_SCHNELL {
                let _ = w.show();
            }
        }
    });
    true
}

/// Fokuswechsel: Abwesenheit merken bzw. nach Ablauf sperren.
pub fn fokus(app: &AppHandle, label: &str, hat_fokus: bool) {
    if label == SPERRE || label == "einstellungen" || label == "notiz" {
        return;
    }
    let e = einstellungen::aktuell(app);
    if !e.touch_id_sperre {
        return;
    }
    let z = app.state::<Zustand>();
    if !hat_fokus {
        z.weg_seit.lock().unwrap().get_or_insert_with(Instant::now);
        return;
    }
    let weg = z.weg_seit.lock().unwrap().take();
    if let Some(seit) = weg {
        if e.sperre_nach_minuten > 0 && seit.elapsed().as_secs() >= e.sperre_nach_minuten as u64 * 60 {
            sperren(app);
        }
    }
}

/// Beim Start: sperren, wenn eingeschaltet.
pub fn beim_start(app: &AppHandle) {
    if einstellungen::aktuell(app).touch_id_sperre && verfuegbar() {
        sperren(app);
    }
}

/// Schlüsselbund-Dienst für Gerätegeheimnisse des Passwort-Tresors.
#[cfg(target_os = "macos")]
const TRESOR_DIENST: &str = "com.sinclairs-solutions.atlas.tresor";

/// Gerätegeheimnis für „Tresor mit Touch ID entsperren“ (32 Byte, base64).
///
/// Atlas verschlüsselt damit den privaten Tresor-Schlüssel — wie beim
/// Passkey-Weg (WebAuthn PRF), der in der App erst mit der Domain-Freigabe
/// bei Apple geht. Das Geheimnis liegt im macOS-Schlüsselbund dieses Macs und
/// wird nur nach Touch ID bzw. Mac-Passwort herausgegeben.
#[tauri::command]
pub async fn geraet_geheimnis(app: AppHandle, id: String, anlegen: bool) -> Result<String, String> {
    if id.len() < 8 || id.len() > 80 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Ungültige Kennung.".into());
    }
    // Bei gesperrter App sind alle Atlas-Fenster verborgen — eine Touch-ID-
    // Abfrage von dort sähe aus wie die Entsperr-Abfrage. Deshalb ablehnen.
    if ist_gesperrt(&app) {
        return Err(t("Atlas ist gesperrt.", "Atlas is locked.").into());
    }
    let grund = if anlegen {
        t("Touch ID für den Passwort-Tresor einrichten", "Set up Touch ID for the password vault")
    } else {
        t("Passwort-Tresor entsperren", "Unlock password vault")
    };
    if !pruefen(grund).await {
        log::warn!("Tresor: Touch ID nicht bestätigt (anlegen={anlegen})");
        return Err(t("Touch ID nicht bestätigt.", "Touch ID not confirmed.").into());
    }
    #[cfg(target_os = "macos")]
    {
        use base64::Engine;
        use security_framework::passwords::{get_generic_password, set_generic_password};
        if anlegen {
            let mut roh = [0u8; 32];
            ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut roh)
                .map_err(|_| "Zufall nicht verfügbar.".to_string())?;
            set_generic_password(TRESOR_DIENST, &id, &roh).map_err(|e| {
                log::error!("Tresor: Schlüsselbund schreiben fehlgeschlagen: {e}");
                format!("Schlüsselbund: {e}")
            })?;
            log::info!("Tresor: Gerätegeheimnis angelegt");
            return Ok(base64::engine::general_purpose::STANDARD.encode(roh));
        }
        let roh = get_generic_password(TRESOR_DIENST, &id)
            .map_err(|_| t("Auf diesem Mac ist Touch ID für den Tresor nicht eingerichtet.", "Touch ID for the vault is not set up on this Mac.").to_string())?;
        Ok(base64::engine::general_purpose::STANDARD.encode(roh))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Nur unter macOS.".into())
    }
}
