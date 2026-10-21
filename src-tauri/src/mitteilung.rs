//! Mitteilungen (Benachrichtigungen) der App.
//!
//! macOS: über `UNUserNotificationCenter` — das alte Benachrichtigungs-API, das
//! das Plugin nutzt, zeigt unter aktuellem macOS nichts mehr an. Dazu gehört:
//!   - einmal um Erlaubnis fragen (Systemdialog; danach unter
//!     Systemeinstellungen → Mitteilungen → Sinclair Atlas einstellbar),
//!   - ein Delegate, damit Mitteilungen auch erscheinen, wenn Atlas vorne ist,
//!   - Klick auf eine Mitteilung holt das Atlas-Fenster nach vorn.
//! Ohne App-Paket (Entwicklungs-Binary) gibt es keinen Mitteilungs-Dienst —
//! dann nur ins Log. Windows/Linux: weiter über das Plugin.

use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::OnceLock;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, msg_send, AllocAnyThread};
    use objc2_foundation::{NSBundle, NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
        UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
        UNNotificationSettings, UNNotificationSound, UNUserNotificationCenter,
        UNUserNotificationCenterDelegate,
    };
    use tauri::AppHandle;

    static APP: OnceLock<AppHandle> = OnceLock::new();

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "AtlasMitteilungDelegat"]
        struct Delegat;

        unsafe impl NSObjectProtocol for Delegat {}

        unsafe impl UNUserNotificationCenterDelegate for Delegat {
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn vorne_zeigen(
                &self,
                _center: &UNUserNotificationCenter,
                _n: &UNNotification,
                fertig: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                fertig.call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List
                    | UNNotificationPresentationOptions::Sound,));
            }

            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn angeklickt(
                &self,
                _center: &UNUserNotificationCenter,
                _r: &UNNotificationResponse,
                fertig: &block2::DynBlock<dyn Fn()>,
            ) {
                if let Some(app) = APP.get() {
                    let a = app.clone();
                    let _ = app.run_on_main_thread(move || crate::fenster::hauptfenster_zeigen(&a));
                }
                fertig.call(());
            }
        }
    );

    /// Läuft die App als Paket (.app)? Nur dann gibt es den Mitteilungs-Dienst.
    fn im_paket() -> bool {
        NSBundle::mainBundle().bundleIdentifier().is_some()
    }

    pub fn einrichten(app: &AppHandle) {
        let _ = APP.set(app.clone());
        if !im_paket() {
            log::warn!("Mitteilungen: kein App-Paket — nur Log");
            return;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegat: Retained<Delegat> = unsafe { msg_send![Delegat::alloc(), init] };
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegat)));
        // Das Zentrum hält den Delegate nur schwach — für die Laufzeit behalten.
        std::mem::forget(delegat);
        erlaubnis_anfragen();
    }

    pub fn erlaubnis_anfragen() {
        if !im_paket() {
            return;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let block = RcBlock::new(|ok: Bool, _e: *mut NSError| {
            log::info!("Mitteilungen erlaubt: {}", ok.as_bool());
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge,
            &block,
        );
    }

    pub fn zeigen(titel: &str, text: &str) -> Result<(), String> {
        if !im_paket() {
            log::info!("Mitteilung (ohne Paket): {titel} — {text}");
            return Ok(());
        }
        let inhalt = UNMutableNotificationContent::new();
        inhalt.setTitle(&NSString::from_str(titel));
        inhalt.setBody(&NSString::from_str(text));
        inhalt.setSound(Some(&UNNotificationSound::defaultSound()));
        let id = NSString::from_str(&format!(
            "atlas-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let anfrage = UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &inhalt, None);
        let block = RcBlock::new(|e: *mut NSError| {
            if let Some(e) = unsafe { e.as_ref() } {
                log::warn!("Mitteilung nicht gezeigt: {}", e.localizedDescription());
            }
        });
        UNUserNotificationCenter::currentNotificationCenter()
            .addNotificationRequest_withCompletionHandler(&anfrage, Some(&block));
        Ok(())
    }

    /// Probe mit Rückmeldung: Fehler beim Einreihen und die Einstellungen,
    /// die macOS für Atlas meldet (für den Knopf „Test“).
    fn probe_absenden(titel: &str, text: &str) -> tokio::sync::oneshot::Receiver<Option<String>> {
        use std::panic::AssertUnwindSafe;
        use std::sync::Mutex;
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let inhalt = UNMutableNotificationContent::new();
        inhalt.setTitle(&NSString::from_str(titel));
        inhalt.setBody(&NSString::from_str(text));
        inhalt.setSound(Some(&UNNotificationSound::defaultSound()));
        let id = NSString::from_str(&format!("atlas-probe-{}", std::process::id()));
        let anfrage = UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &inhalt, None);
        let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
        let tx = Mutex::new(Some(tx));
        let block = RcBlock::new(move |e: *mut NSError| {
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let f = unsafe { e.as_ref() }.map(|e| e.localizedDescription().to_string());
                if let Some(s) = tx.lock().ok().and_then(|mut g| g.take()) {
                    let _ = s.send(f);
                }
            }));
        });
        center.addNotificationRequest_withCompletionHandler(&anfrage, Some(&block));
        rx
    }

    fn probe_einstellungen() -> tokio::sync::oneshot::Receiver<(isize, isize, isize)> {
        use std::panic::AssertUnwindSafe;
        use std::sync::Mutex;
        let (tx2, rx2) = tokio::sync::oneshot::channel::<(isize, isize, isize)>();
        let tx2 = Mutex::new(Some(tx2));
        let block2 = RcBlock::new(move |e: std::ptr::NonNull<UNNotificationSettings>| {
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let e = unsafe { e.as_ref() };
                let werte = (e.authorizationStatus().0, e.alertSetting().0, e.alertStyle().0);
                if let Some(s) = tx2.lock().ok().and_then(|mut g| g.take()) {
                    let _ = s.send(werte);
                }
            }));
        });
        UNUserNotificationCenter::currentNotificationCenter().getNotificationSettingsWithCompletionHandler(&block2);
        rx2
    }

    pub async fn probe(titel: &str, text: &str) -> Result<String, String> {
        use std::time::Duration;
        if !im_paket() {
            return Err("Ohne App-Paket gibt es keine Mitteilungen.".into());
        }
        let rx = probe_absenden(titel, text);
        let fehler = tokio::time::timeout(Duration::from_secs(5), rx).await.ok().and_then(|r| r.ok()).flatten();
        let rx2 = probe_einstellungen();
        let stand = tokio::time::timeout(Duration::from_secs(5), rx2).await.ok().and_then(|r| r.ok());
        log::info!("Mitteilungs-Probe: Fehler={fehler:?} Einstellungen={stand:?}");
        use crate::texte::t;
        if let Some(f) = fehler {
            return Err(format!("{} {f}", t("macOS hat die Mitteilung abgelehnt:", "macOS rejected the notification:")));
        }
        match stand {
            // authorizationStatus: 0 offen, 1 verweigert, 2 erlaubt, 3 vorläufig
            Some((1, _, _)) => Err(t(
                "macOS verweigert Atlas Mitteilungen. Systemeinstellungen → Mitteilungen → Sinclair Atlas → „Mitteilungen erlauben“.",
                "macOS denies Atlas notifications. System Settings → Notifications → Sinclair Atlas → “Allow notifications”.",
            ).into()),
            Some((0, _, _)) => Err(t(
                "macOS hat noch nicht gefragt. Bitte „Erlauben“ drücken.",
                "macOS hasn’t asked yet. Please press “Allow”.",
            ).into()),
            // alertStyle 0 = keine Banner
            Some((_, _, 0)) => Ok(t(
                "Gesendet — Banner sind für Atlas ausgeschaltet; die Mitteilung liegt nur in der Mitteilungszentrale.",
                "Sent — banners are off for Atlas; the notification is only in Notification Centre.",
            ).into()),
            Some(_) => Ok(t(
                "Gesendet, macOS erlaubt Atlas Banner. Erscheint nichts, ist ein Fokus (z. B. „Nicht stören“ oder „Schlafen“) aktiv.",
                "Sent, macOS allows Atlas banners. If nothing appears, a Focus (e.g. Do Not Disturb or Sleep) is on.",
            ).into()),
            None => Ok(t("Gesendet.", "Sent.").into()),
        }
    }

    #[allow(dead_code)]
    fn _nutzen(_: &NSObject) {}
}

/// Beim Start: Delegate setzen und (einmal) um Erlaubnis fragen.
pub fn einrichten(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    mac::einrichten(app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

pub fn erlaubnis_anfragen(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        mac::erlaubnis_anfragen();
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri_plugin_notification::NotificationExt;
        let _ = app.notification().request_permission();
    }
}

/// Probe-Mitteilung mit Rückmeldung für die Einstellungen.
pub async fn probe(app: &AppHandle, titel: &str, text: &str) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        mac::probe(titel, text).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        zeigen(app, titel, text).map(|_| String::new())
    }
}

/// Mitteilung zeigen (Titel + Text).
pub fn zeigen(app: &AppHandle, titel: &str, text: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        mac::zeigen(titel, text)
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri_plugin_notification::NotificationExt;
        app.notification().builder().title(titel).body(text).show().map_err(|e| e.to_string())
    }
}
