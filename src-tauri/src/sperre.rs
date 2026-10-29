//! Touch ID: App-Sperre und Prüfung für Atlas.
//!
//! App-Sperre (ab 0.3.9 Standard): Beim Start, nach Abwesenheit und nach
//! Inaktivität ist Atlas gesperrt, bis Touch ID bzw. das Mac-Passwort
//! bestätigt (LocalAuthentication, `deviceOwnerAuthentication`). Atlas meldet
//! dann nicht mehr ab — die Sitzung bleibt bestehen.
//!
//! Der Zustand liegt hier, einmal für alle Fenster. `sperren` meldet ihn jedem
//! Atlas-Fenster (`main`, `neben-*`) über `window.__atlasDesktopSperre(…)`;
//! Atlas zeigt darauf seinen Sperrbildschirm (Inhalt verdeckt, nicht
//! bedienbar). Entsperren geht nur über `entsperren` (Touch ID hier in der App)
//! und gilt für alle Fenster.
//!
//! Abwesenheit: Ein Fokusverlust zählt erst, wenn die App kurz danach wirklich
//! nicht mehr aktiv ist (Fensterwechsel, Dateiauswahl und Blätter zählen nicht)
//! und keine Touch-ID-Abfrage läuft. Der Touch-ID-Dialog nimmt den Fokus weg
//! und darf nie selbst eine Sperre auslösen — dieselbe Falle wie „inactive“ in
//! der Mobil-App. Bildschirmsperre und Ruhezustand zählen als Abwesenheit.
//! Zeiten als Uhrzeit (`SystemTime`): `Instant` bleibt unter macOS im
//! Ruhezustand stehen, die Nacht zählte sonst nicht.
//!
//! Windows: Windows Hello folgt — dort gibt es vorerst keine Sperre.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

use tauri::{AppHandle, Manager};

use crate::{
    einstellungen::{self, Einstellungen},
    fenster,
    texte::t,
};

#[derive(Default)]
pub struct Zustand {
    pub gesperrt: AtomicBool,
    /// Seit wann Atlas nicht mehr die aktive App ist (bzw. seit Bildschirmsperre
    /// oder Ruhezustand).
    weg_seit: Mutex<Option<SystemTime>>,
}

impl Zustand {
    fn weg(&self) -> std::sync::MutexGuard<'_, Option<SystemTime>> {
        self.weg_seit.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Laufende Touch-ID-Abfragen (Entsperren, Tresor, Einstellungen).
static PRUEFUNGEN: AtomicUsize = AtomicUsize::new(0);
/// Ende der letzten Abfrage — der Fokus kommt erst kurz danach zurück.
static PRUEFUNG_ENDE: Mutex<Option<Instant>> = Mutex::new(None);
/// Nur ein Entsperr-Dialog zugleich, auch wenn mehrere Fenster fragen.
static ENTSPERRUNG_LAEUFT: AtomicBool = AtomicBool::new(false);

/// So lange nach einer Touch-ID-Abfrage zählt ein Fokusverlust nicht.
const NACHLAUF: Duration = Duration::from_secs(2);
/// So lange nach einem Fokusverlust wird nachgesehen, ob Atlas noch aktiv ist.
const FOKUS_PRUEFEN_NACH: Duration = Duration::from_millis(700);
/// Takt, in dem eine laufende Abwesenheit geprüft wird (Fenster sichtbar im Hintergrund).
const TAKT: Duration = Duration::from_secs(30);

/// Läuft gerade eine Touch-ID-Abfrage? Dann verliert z. B. das Passwort-Fenster
/// der Menüleiste kurz den Fokus — und soll dabei nicht zugehen.
pub fn pruefung_laeuft() -> bool {
    PRUEFUNGEN.load(Ordering::SeqCst) > 0
}

fn kuerzlich_geprueft() -> bool {
    pruefung_laeuft()
        || PRUEFUNG_ENDE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|t| t.elapsed() < NACHLAUF)
}

// ---------------------------------------------------------------------------
// Touch ID / Mac-Passwort
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ergebnis {
    Ok,
    Abgebrochen,
    NichtVerfuegbar,
}

impl Ergebnis {
    pub fn text(self) -> &'static str {
        match self {
            Ergebnis::Ok => "ok",
            Ergebnis::Abgebrochen => "abgebrochen",
            Ergebnis::NichtVerfuegbar => "nicht_verfuegbar",
        }
    }

    /// LAError: kein Gerätepasswort (-5), keine Biometrie (-6/-7),
    /// nicht interaktiv (-1004) → nicht verfügbar; alles andere (Abbruch,
    /// falscher Finger, System bricht ab) → abgebrochen.
    fn aus_fehlercode(code: isize) -> Self {
        match code {
            -5 | -6 | -7 | -1004 => Ergebnis::NichtVerfuegbar,
            _ => Ergebnis::Abgebrochen,
        }
    }
}

/// Touch ID (mit Mac-Passwort als Ausweg).
pub async fn pruefen_ergebnis(grund: &str) -> Ergebnis {
    PRUEFUNGEN.fetch_add(1, Ordering::SeqCst);
    let r = match anfordern(grund) {
        Ok(rx) => rx.await.unwrap_or(Ergebnis::Abgebrochen),
        Err(e) => e,
    };
    // Erst das Ende merken, dann den Zähler senken — sonst gäbe es eine Lücke,
    // in der ein Fokusverlust durch den Dialog als Abwesenheit zählte.
    *PRUEFUNG_ENDE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    PRUEFUNGEN.fetch_sub(1, Ordering::SeqCst);
    r
}

/// Touch ID (mit Mac-Passwort als Ausweg). `true` = bestätigt.
pub async fn pruefen(grund: &str) -> bool {
    pruefen_ergebnis(grund).await == Ergebnis::Ok
}

/// Startet die Abfrage synchron (Objective-C-Objekte bleiben in diesem
/// Aufruf) und liefert den Empfänger für die Antwort.
fn anfordern(grund: &str) -> Result<tokio::sync::oneshot::Receiver<Ergebnis>, Ergebnis> {
    #[cfg(target_os = "macos")]
    {
        use block2::RcBlock;
        use objc2::runtime::Bool;
        use objc2_foundation::{NSError, NSString};
        use objc2_local_authentication::{LAContext, LAPolicy};

        let (tx, rx) = tokio::sync::oneshot::channel::<Ergebnis>();
        let tx = Mutex::new(Some(tx));
        let grund = NSString::from_str(grund);
        unsafe {
            let ctx = LAContext::new();
            if ctx.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthentication).is_err() {
                return Err(Ergebnis::NichtVerfuegbar);
            }
            // Der Block hält den Kontext bis zur Antwort am Leben.
            let halten = ctx.clone();
            let block = RcBlock::new(move |ok: Bool, err: *mut NSError| {
                let _ = &halten;
                let r = if ok.as_bool() {
                    Ergebnis::Ok
                } else {
                    // LocalAuthentication übergibt bei Misserfolg ein NSError oder nil.
                    err.as_ref().map_or(Ergebnis::Abgebrochen, |e| Ergebnis::aus_fehlercode(e.code()))
                };
                if let Some(s) = tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = s.send(r);
                }
            });
            ctx.evaluatePolicy_localizedReason_reply(LAPolicy::DeviceOwnerAuthentication, &grund, &block);
        }
        Ok(rx)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = grund;
        Err(Ergebnis::NichtVerfuegbar)
    }
}

#[cfg(target_os = "macos")]
fn kann(policy: objc2_local_authentication::LAPolicy) -> bool {
    unsafe { objc2_local_authentication::LAContext::new().canEvaluatePolicy_error(policy).is_ok() }
}

/// Gibt es eine Geräteanmeldung (Touch ID oder Mac-Passwort)?
pub fn verfuegbar() -> bool {
    #[cfg(target_os = "macos")]
    {
        kann(objc2_local_authentication::LAPolicy::DeviceOwnerAuthentication)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Ist Touch ID eingerichtet (sonst fragt macOS nach dem Mac-Passwort)?
pub fn touch_id() -> bool {
    #[cfg(target_os = "macos")]
    {
        kann(objc2_local_authentication::LAPolicy::DeviceOwnerAuthenticationWithBiometrics)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn grund_text(grund: Option<String>) -> String {
    let g: String = grund.unwrap_or_default().chars().take(120).collect();
    if g.trim().is_empty() {
        t("Atlas bestätigen", "Confirm Atlas").to_string()
    } else {
        g
    }
}

#[tauri::command]
pub async fn touch_id_pruefen(grund: Option<String>) -> bool {
    pruefen(&grund_text(grund)).await
}

/// Touch ID bzw. Mac-Passwort: "ok" | "abgebrochen" | "nicht_verfuegbar".
/// Bei gesperrter App immer "abgebrochen" — eine Abfrage von dort sähe aus
/// wie die Entsperr-Abfrage, entsperrte aber nichts.
#[tauri::command]
pub async fn biometrie_pruefen(app: AppHandle, grund: Option<String>) -> &'static str {
    if ist_gesperrt(&app) {
        return Ergebnis::Abgebrochen.text();
    }
    pruefen_ergebnis(&grund_text(grund)).await.text()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verfuegbarkeit {
    /// Touch ID oder Mac-Passwort lassen sich abfragen.
    verfuegbar: bool,
    /// Touch ID ist eingerichtet.
    touch_id: bool,
}

#[tauri::command]
pub fn biometrie_verfuegbar() -> Verfuegbarkeit {
    Verfuegbarkeit { verfuegbar: verfuegbar(), touch_id: touch_id() }
}

// ---------------------------------------------------------------------------
// Regeln (rein, getestet)
// ---------------------------------------------------------------------------

/// Sperre in dieser App wirksam? (Windows: noch nicht.)
pub fn aktiv(e: &Einstellungen) -> bool {
    cfg!(target_os = "macos") && e.touch_id_sperre
}

/// Wie lange Atlas weg sein darf, bevor es sperrt. `None` = nie nach
/// Abwesenheit (aus bzw. „nur beim Start“). „Sofort“ = 0: Ein Fokusverlust
/// zählt ohnehin erst nach echter Abwesenheit (siehe `fokus`).
pub fn schwelle(e: &Einstellungen) -> Option<Duration> {
    if !e.touch_id_sperre {
        None
    } else if e.sperre_sofort {
        Some(Duration::ZERO)
    } else if e.sperre_nach_minuten == 0 {
        None
    } else {
        Some(Duration::from_secs(u64::from(e.sperre_nach_minuten) * 60))
    }
}

/// Je kleiner, desto strenger.
fn strenge(e: &Einstellungen) -> u64 {
    if !e.touch_id_sperre {
        u64::MAX
    } else {
        schwelle(e).map_or(u64::MAX - 1, |d| d.as_secs())
    }
}

/// Einschalten und Lockern nur nach Touch ID — sonst ließe sich die Sperre
/// ohne Bestätigung abstellen. Verschärfen geht ohne.
pub fn bestaetigung_noetig(alt: &Einstellungen, neu: &Einstellungen) -> Option<&'static str> {
    if neu.touch_id_sperre && !alt.touch_id_sperre {
        Some(t("Touch-ID-Sperre einschalten", "Turn on Touch ID lock"))
    } else if strenge(neu) > strenge(alt) {
        Some(t("Touch-ID-Sperre ändern", "Change Touch ID lock"))
    } else {
        None
    }
}

/// Lange genug weg, um zu sperren?
pub fn abwesend_genug(e: &Einstellungen, weg_seit: SystemTime, jetzt: SystemTime) -> bool {
    match schwelle(e) {
        None => false,
        Some(s) => jetzt.duration_since(weg_seit).unwrap_or(Duration::ZERO) >= s,
    }
}

// ---------------------------------------------------------------------------
// Sperren und Entsperren (alle Fenster)
// ---------------------------------------------------------------------------

pub fn ist_gesperrt(app: &AppHandle) -> bool {
    app.state::<Zustand>().gesperrt.load(Ordering::SeqCst)
}

fn ist_atlas_fenster(label: &str) -> bool {
    label == fenster::HAUPT || label.starts_with("neben-")
}

/// Allen Atlas-Fenstern den Zustand melden. Seiten ohne Atlas (Lade- und
/// Offline-Seite) ignorieren den Aufruf.
fn melden(app: &AppHandle, gesperrt: bool) {
    let js = format!("window.__atlasDesktopSperre && window.__atlasDesktopSperre({gesperrt})");
    for (label, w) in app.webview_windows() {
        if ist_atlas_fenster(&label) {
            let _ = w.eval(&js);
        }
    }
}

/// Alle Atlas-Fenster sperren. Eigene Fenster ohne Atlas-Seite
/// (Einstellungen, Schnellerfassung) und der Passwort-Schnellzugriff gehen zu.
pub fn sperren(app: &AppHandle) {
    let z = app.state::<Zustand>();
    if !z.gesperrt.swap(true, Ordering::SeqCst) {
        log::info!("Sperre: Atlas gesperrt");
    }
    melden(app, true);
    for (label, w) in app.webview_windows() {
        if !ist_atlas_fenster(&label) || label == fenster::PASSWORT_SCHNELL {
            let _ = w.hide();
        }
    }
}

/// Gesperrt und jemand will etwas öffnen (Dock, Menü, Kürzel): Hauptfenster
/// mit dem Sperrbildschirm nach vorn.
pub fn zur_sperre(app: &AppHandle) {
    sperren(app);
    fenster::hauptfenster_zeigen(app);
}

fn aufheben(app: &AppHandle, grund: &str) {
    let z = app.state::<Zustand>();
    *z.weg() = None;
    if z.gesperrt.swap(false, Ordering::SeqCst) {
        log::info!("Sperre: aufgehoben ({grund})");
    }
    melden(app, false);
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Sperre eingeschaltet (sonst gilt in Atlas die Abmeldung nach 12 Stunden).
    aktiv: bool,
    gesperrt: bool,
    /// 0 = nur beim Start.
    nach_minuten: u32,
    sofort: bool,
    verfuegbar: bool,
    touch_id: bool,
}

fn status(app: &AppHandle) -> Status {
    let e = einstellungen::aktuell(app);
    Status {
        aktiv: aktiv(&e),
        gesperrt: ist_gesperrt(app),
        nach_minuten: e.sperre_nach_minuten,
        sofort: e.sperre_sofort,
        verfuegbar: verfuegbar(),
        touch_id: touch_id(),
    }
}

/// Atlas fragt beim Laden, ob gesperrt ist und wie die Sperre eingestellt ist.
#[tauri::command]
pub fn sperre_status(app: AppHandle) -> Status {
    status(&app)
}

/// Atlas meldet Inaktivität (keine Eingabe seit dem Zeitlimit): sperren.
#[tauri::command]
pub fn sperre_anfordern(app: AppHandle) -> bool {
    if aktiv(&einstellungen::aktuell(&app)) && verfuegbar() {
        sperren(&app);
    }
    ist_gesperrt(&app)
}

/// Sperrbildschirm: Touch ID anfordern; bei Erfolg alle Fenster entsperren.
/// "ok" | "abgebrochen" | "nicht_verfuegbar" | "laeuft" (anderes Fenster fragt schon).
#[tauri::command]
pub async fn entsperren(app: AppHandle) -> &'static str {
    if !ist_gesperrt(&app) {
        melden(&app, false);
        return Ergebnis::Ok.text();
    }
    if ENTSPERRUNG_LAEUFT.swap(true, Ordering::SeqCst) {
        return "laeuft";
    }
    let r = pruefen_ergebnis(t("Atlas entsperren", "Unlock Atlas")).await;
    ENTSPERRUNG_LAEUFT.store(false, Ordering::SeqCst);
    if r == Ergebnis::Ok {
        aufheben(&app, "Touch ID");
    }
    r.text()
}

/// Atlas hat keine Sitzung (Anmeldeseite) bzw. wurde vom Sperrbildschirm aus
/// abgemeldet: Es gibt nichts zu schützen — die Sperre fällt, damit nach der
/// nächsten Anmeldung nicht gleich wieder Touch ID kommt.
#[tauri::command]
pub fn sperre_ohne_sitzung(app: AppHandle) {
    if ist_gesperrt(&app) {
        aufheben(&app, "keine Sitzung");
    }
}

/// Einstellungen aus Atlas (Profil → Apps): an/aus und Zeitlimit.
#[tauri::command]
pub async fn sperre_einstellen(app: AppHandle, aktiv: bool, nach_minuten: u32, sofort: bool) -> Result<Status, String> {
    if !matches!(nach_minuten, 0 | 5 | 15 | 60) {
        return Err(t("Ungültiges Zeitlimit.", "Invalid time limit.").into());
    }
    if ist_gesperrt(&app) {
        return Err(t("Atlas ist gesperrt.", "Atlas is locked.").into());
    }
    let setzen = |e: &mut Einstellungen| {
        e.touch_id_sperre = aktiv;
        e.sperre_nach_minuten = nach_minuten;
        e.sperre_sofort = sofort;
    };
    let alt = einstellungen::aktuell(&app);
    let mut neu = alt.clone();
    setzen(&mut neu);
    if let Some(g) = bestaetigung_noetig(&alt, &neu) {
        if !pruefen(g).await {
            return Err(t("Touch ID wurde nicht bestätigt.", "Touch ID was not confirmed.").into());
        }
    }
    // Erst nach der Abfrage frisch lesen: Inzwischen Geändertes bleibt erhalten.
    let mut e = einstellungen::aktuell(&app);
    setzen(&mut e);
    einstellungen::speichern(&app, &e);
    Ok(status(&app))
}

// ---------------------------------------------------------------------------
// Abwesenheit
// ---------------------------------------------------------------------------

/// Ist Atlas die aktive App? (Hauptthread.)
fn app_aktiv(app: &AppHandle) -> bool {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        match objc2::MainThreadMarker::new() {
            Some(m) => objc2_app_kit::NSApplication::sharedApplication(m).isActive(),
            None => true,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        app.webview_windows().values().any(|w| w.is_focused().unwrap_or(false))
    }
}

fn weg_merken(app: &AppHandle, seit: SystemTime) {
    app.state::<Zustand>().weg().get_or_insert(seit);
}

/// Atlas ist wieder vorn: lange genug weg → sperren.
fn zurueck(app: &AppHandle) {
    let Some(seit) = app.state::<Zustand>().weg().take() else { return };
    let e = einstellungen::aktuell(app);
    if aktiv(&e) && abwesend_genug(&e, seit, SystemTime::now()) {
        sperren(app);
    }
}

/// Während der Abwesenheit (Takt, Aufwachen): schon sperren, wenn die Zeit um
/// ist — ein sichtbares Fenster im Hintergrund soll nicht offen bleiben.
fn auswerten(app: &AppHandle) {
    let Some(seit) = *app.state::<Zustand>().weg() else { return };
    let e = einstellungen::aktuell(app);
    if !ist_gesperrt(app) && aktiv(&e) && abwesend_genug(&e, seit, SystemTime::now()) {
        sperren(app);
    }
}

/// Fokuswechsel eines Fensters (Hauptthread).
pub fn fokus(app: &AppHandle, hat_fokus: bool) {
    if hat_fokus {
        zurueck(app);
        return;
    }
    // Fensterwechsel, Dateiauswahl oder Touch-ID-Dialog nehmen den Fokus nur
    // kurz bzw. innerhalb der App weg. Erst nachsehen, ob Atlas danach
    // wirklich nicht mehr aktiv ist.
    let verloren = SystemTime::now();
    let a = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FOKUS_PRUEFEN_NACH).await;
        let b = a.clone();
        let _ = a.run_on_main_thread(move || {
            if !kuerzlich_geprueft() && !app_aktiv(&b) {
                weg_merken(&b, verloren);
            }
        });
    });
}

/// Bildschirm entsperrt bzw. aufgewacht (Hauptthread).
#[cfg(target_os = "macos")]
fn system_zurueck(app: &AppHandle) {
    if app_aktiv(app) {
        zurueck(app);
    } else {
        auswerten(app);
    }
}

/// Bildschirmsperre und Ruhezustand beobachten (macOS).
#[cfg(target_os = "macos")]
fn systemereignisse(app: &AppHandle) {
    use block2::RcBlock;
    use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification, NSWorkspaceWillSleepNotification};
    use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSString};
    use std::ptr::NonNull;

    let a = app.clone();
    let weg: RcBlock<dyn Fn(NonNull<NSNotification>)> =
        RcBlock::new(move |_n: NonNull<NSNotification>| weg_merken(&a, SystemTime::now()));
    let a = app.clone();
    let da: RcBlock<dyn Fn(NonNull<NSNotification>)> = RcBlock::new(move |_n: NonNull<NSNotification>| {
        let b = a.clone();
        let _ = a.run_on_main_thread(move || system_zurueck(&b));
    });
    // Die Beobachter laufen so lange wie die App — Rückgabewerte bewusst behalten.
    unsafe {
        let verteilt = NSDistributedNotificationCenter::defaultCenter();
        for (name, block) in [("com.apple.screenIsLocked", &weg), ("com.apple.screenIsUnlocked", &da)] {
            let name = NSString::from_str(name);
            std::mem::forget(verteilt.addObserverForName_object_queue_usingBlock(Some(&name), None, None, block));
        }
        let arbeitsplatz = NSWorkspace::sharedWorkspace().notificationCenter();
        std::mem::forget(arbeitsplatz.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceWillSleepNotification),
            None,
            None,
            &weg,
        ));
        std::mem::forget(arbeitsplatz.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidWakeNotification),
            None,
            None,
            &da,
        ));
    }
}

/// Beim Start: sperren, wenn eingeschaltet; Abwesenheit beobachten.
pub fn beim_start(app: &AppHandle) {
    if aktiv(&einstellungen::aktuell(app)) && verfuegbar() {
        sperren(app);
    }
    #[cfg(target_os = "macos")]
    systemereignisse(app);
    let a = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(TAKT).await;
            let b = a.clone();
            let _ = a.run_on_main_thread(move || auswerten(&b));
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn e(an: bool, minuten: u32, sofort: bool) -> Einstellungen {
        Einstellungen { touch_id_sperre: an, sperre_nach_minuten: minuten, sperre_sofort: sofort, ..Default::default() }
    }

    fn vor(sek: u64) -> (SystemTime, SystemTime) {
        let jetzt = SystemTime::now();
        (jetzt - Duration::from_secs(sek), jetzt)
    }

    #[test]
    fn zeitlimit_wird_eingehalten() {
        let (seit, jetzt) = vor(14 * 60);
        assert!(!abwesend_genug(&e(true, 15, false), seit, jetzt));
        let (seit, jetzt) = vor(15 * 60);
        assert!(abwesend_genug(&e(true, 15, false), seit, jetzt));
        let (seit, jetzt) = vor(5 * 60 + 1);
        assert!(abwesend_genug(&e(true, 5, false), seit, jetzt));
    }

    #[test]
    fn nur_beim_start_und_aus_sperren_nie_nach_abwesenheit() {
        let (seit, jetzt) = vor(10 * 3600);
        assert!(!abwesend_genug(&e(true, 0, false), seit, jetzt));
        assert!(!abwesend_genug(&e(false, 15, false), seit, jetzt));
        assert!(!abwesend_genug(&e(false, 15, true), seit, jetzt));
    }

    #[test]
    fn sofort_sperrt_nach_jeder_echten_abwesenheit() {
        let (seit, jetzt) = vor(1);
        assert!(abwesend_genug(&e(true, 15, true), seit, jetzt));
        assert!(abwesend_genug(&e(true, 0, true), seit, jetzt));
    }

    #[test]
    fn uhr_rueckwaerts_sperrt_nicht_bei_zeitlimit() {
        let jetzt = SystemTime::now();
        let seit = jetzt + Duration::from_secs(60);
        assert!(!abwesend_genug(&e(true, 5, false), seit, jetzt));
    }

    #[test]
    fn lockern_und_einschalten_brauchen_touch_id() {
        // Einschalten
        assert!(bestaetigung_noetig(&e(false, 15, false), &e(true, 15, false)).is_some());
        // Ausschalten
        assert!(bestaetigung_noetig(&e(true, 15, false), &e(false, 15, false)).is_some());
        // Länger warten bzw. nur beim Start
        assert!(bestaetigung_noetig(&e(true, 15, false), &e(true, 60, false)).is_some());
        assert!(bestaetigung_noetig(&e(true, 15, false), &e(true, 0, false)).is_some());
        assert!(bestaetigung_noetig(&e(true, 5, true), &e(true, 5, false)).is_some());
        // Verschärfen geht ohne
        assert!(bestaetigung_noetig(&e(true, 60, false), &e(true, 5, false)).is_none());
        assert!(bestaetigung_noetig(&e(true, 0, false), &e(true, 15, false)).is_none());
        assert!(bestaetigung_noetig(&e(true, 15, false), &e(true, 15, true)).is_none());
        // Unverändert bzw. aus bleibt aus
        assert!(bestaetigung_noetig(&e(true, 15, false), &e(true, 15, false)).is_none());
        assert!(bestaetigung_noetig(&e(false, 15, false), &e(false, 60, false)).is_none());
    }

    #[test]
    fn standard_ist_an_mit_15_minuten() {
        let d = Einstellungen::default();
        assert!(d.touch_id_sperre);
        assert_eq!(d.sperre_nach_minuten, 15);
        assert!(!d.sperre_sofort);
        assert_eq!(schwelle(&d), Some(Duration::from_secs(15 * 60)));
    }

    /// Lokal auf dem Mac: `cargo test biometrie_lokal -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn biometrie_lokal() {
        let v = biometrie_verfuegbar();
        println!("biometrie_verfuegbar: verfuegbar={} touch_id={}", v.verfuegbar, v.touch_id);
        assert_eq!(v.verfuegbar, cfg!(target_os = "macos"));
    }

    #[test]
    fn fehlercodes() {
        assert_eq!(Ergebnis::aus_fehlercode(-2), Ergebnis::Abgebrochen);
        assert_eq!(Ergebnis::aus_fehlercode(-1), Ergebnis::Abgebrochen);
        assert_eq!(Ergebnis::aus_fehlercode(-4), Ergebnis::Abgebrochen);
        assert_eq!(Ergebnis::aus_fehlercode(-5), Ergebnis::NichtVerfuegbar);
        assert_eq!(Ergebnis::aus_fehlercode(-1004), Ergebnis::NichtVerfuegbar);
    }

    #[test]
    fn touch_id_dialog_zaehlt_nicht_als_abwesenheit() {
        PRUEFUNGEN.fetch_add(1, Ordering::SeqCst);
        assert!(kuerzlich_geprueft());
        *PRUEFUNG_ENDE.lock().unwrap() = Some(Instant::now());
        PRUEFUNGEN.fetch_sub(1, Ordering::SeqCst);
        // Nachlauf: der Fokus kommt erst kurz nach dem Dialog zurück.
        assert!(kuerzlich_geprueft());
        *PRUEFUNG_ENDE.lock().unwrap() = Instant::now().checked_sub(NACHLAUF + Duration::from_millis(10));
        assert!(!kuerzlich_geprueft());
    }
}
