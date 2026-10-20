//! Haupt- und Nebenfenster: was darin geladen werden darf, was in den
//! Standardbrowser geht, und wie Atlas nach einer Störung wieder lädt.
//!
//! Warum die Prüfung zweistufig ist: Unter macOS meldet WebKit Navigationen von
//! iframes genauso wie die des Hauptfensters, ohne sie zu unterscheiden. Atlas
//! bettet aber fremde Adressen ein (signierte Speicher-URLs, Google-Drive-
//! Vorschau, Layer-Vorschau). Würde `on_navigation` alles Fremde sperren,
//! blieben diese Vorschauen leer und es öffneten sich ungefragt Browser-Tabs.
//! Deshalb:
//!   1. `bruecke.js` fängt im Hauptframe Klicks und Navigationen (Navigation API)
//!      zu fremden Adressen ab, bevor sie starten.
//!   2. `on_navigation` schickt Anmeldeseiten (Google, Microsoft, Bank …) immer
//!      in den Browser — die werden nie eingebettet. Unter Windows meldet
//!      WebView2 nur den Hauptframe, dort geht jede fremde Adresse sofort raus.
//!   3. `on_page_load` (feuert nur für den Hauptframe) ist das Sicherheitsnetz:
//!      Landet ein Fenster trotzdem auf einer fremden Seite, geht die Adresse in
//!      den Browser und das Fenster zurück zu Atlas bzw. zu.

use std::{sync::atomic::Ordering, time::Instant};

use tauri::{
    webview::{NewWindowFeatures, NewWindowResponse, PageLoadEvent, PageLoadPayload},
    AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::{bruecke, downloads, erreichbarkeit, texte::t, ziel, zustand::Zustand};

pub const HAUPT: &str = "main";
const NEBEN_PREFIX: &str = "neben-";

pub fn hauptfenster(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(HAUPT)
}

/// Hauptfenster zeigen (Dock-Klick, zweiter Start, Deep-Link).
pub fn hauptfenster_zeigen(app: &AppHandle) {
    if app.state::<crate::sperre::Zustand>().gesperrt.load(Ordering::SeqCst) {
        crate::sperre::sperren(app);
        if let Some(w) = app.get_webview_window(crate::sperre::SPERRE) {
            let _ = w.show();
            let _ = w.set_focus();
        }
        return;
    }
    if let Some(w) = hauptfenster(app) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Das Fenster, auf das sich Menübefehle beziehen.
pub fn aktives_fenster(app: &AppHandle) -> Option<WebviewWindow> {
    app.webview_windows()
        .into_values()
        .find(|w| w.is_focused().unwrap_or(false))
        .or_else(|| hauptfenster(app))
}

pub fn hauptfenster_bauen(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let b = WebviewWindowBuilder::new(app, HAUPT, WebviewUrl::App("index.html".into()))
        .title("Sinclair Atlas")
        .inner_size(1360.0, 880.0)
        .min_inner_size(880.0, 560.0)
        .center();
    ausstatten(app, titelleiste(b), HAUPT.to_string()).build()
}

/// Titelleiste (macOS): kein grauer Balken, kein Titeltext — Atlas reicht bis
/// unter die Fensterknöpfe.
fn titelleiste<'a>(
    b: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    #[cfg(target_os = "macos")]
    {
        // Overlay: Atlas reicht bis unter die Fensterknöpfe (Platz dafür lässt
        // Atlas selbst, siehe styles.css „atlas-desktop-mac“).
        // Knöpfe senkrecht mittig auf der 56 px hohen Atlas-Titelleiste.
        b.title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(tauri::LogicalPosition::new(22.0, 21.0))
            .background_color(tauri::window::Color(247, 248, 246, 255))
    }
    #[cfg(not(target_os = "macos"))]
    {
        b
    }
}

/// Weiteres Atlas-Fenster (Menü „Neues Fenster“) — z. B. zwei Mandanten nebeneinander.
pub fn weiteres_fenster(app: &AppHandle, url: Url) {
    // Eigene Fenster (Mail, Passwörter) haben keine Seitenleiste — dort bleibt die
    // normale Titelleiste, sonst lägen die Fensterknöpfe über dem Inhalt.
    let eigenes = url.query().is_some_and(|q| q.split('&').any(|p| p == "fenster=1"));
    let n = app.state::<Zustand>().fenster_zaehler.fetch_add(1, Ordering::Relaxed);
    let label = format!("{NEBEN_PREFIX}{n}");
    let b = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title("Sinclair Atlas")
        .inner_size(1280.0, 840.0)
        .min_inner_size(720.0, 480.0);
    let b = if eigenes { b } else { titelleiste(b) };
    if let Err(e) = ausstatten(app, b, label).build() {
        log::error!("Neues Fenster ließ sich nicht öffnen: {e}");
    }
}

/// Passwörter-Schnellzugriff aus der Menüleiste (wie Safari): kleines Fenster
/// ohne Rahmen unter dem Symbol, geht beim Wegklicken zu. Fängt mit „neben-“
/// an, damit es dieselben Rechte wie die anderen Atlas-Fenster hat.
pub const PASSWORT_SCHNELL: &str = "neben-passwoerter-schnell";
const PW_BREITE: f64 = 380.0;
const PW_HOEHE: f64 = 500.0;

/// Ein-/ausblenden; `unter` = Rechteck des Menüleisten-Symbols (physisch).
pub fn passwort_schnell(app: &AppHandle, unter: Option<(f64, f64, f64, f64)>) {
    if app.state::<crate::sperre::Zustand>().gesperrt.load(Ordering::SeqCst) {
        crate::sperre::sperren(app);
        return;
    }
    let w = match app.get_webview_window(PASSWORT_SCHNELL) {
        Some(w) => {
            if w.is_visible().unwrap_or(false) {
                let _ = w.hide();
                return;
            }
            w
        }
        None => {
            let mut url = ziel::start_url().clone();
            url.set_path("/passwoerter-schnell");
            url.set_query(Some("fenster=1"));
            url.set_fragment(None);
            let b = WebviewWindowBuilder::new(app, PASSWORT_SCHNELL, WebviewUrl::External(url))
                .title(t("Passwörter", "Passwords"))
                .inner_size(PW_BREITE, PW_HOEHE)
                .resizable(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false);
            // macOS: normales Fenster (runde Ecken, Schatten) mit unsichtbarer
            // Titelleiste; die drei Knöpfe werden danach ausgeblendet.
            #[cfg(target_os = "macos")]
            let b = b.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
            #[cfg(not(target_os = "macos"))]
            let b = b.decorations(false);
            let w = match ausstatten(app, b, PASSWORT_SCHNELL.to_string()).build() {
                Ok(w) => {
                    #[cfg(target_os = "macos")]
                    knoepfe_ausblenden(&w);
                    w
                }
                Err(e) => {
                    log::error!("Passwort-Fenster ließ sich nicht öffnen: {e}");
                    return;
                }
            };
            let w2 = w.clone();
            w.on_window_event(move |ev| {
                if let tauri::WindowEvent::Focused(false) = ev {
                    // Während Touch ID fragt, nicht zugehen.
                    if !crate::sperre::PRUEFUNG_LAEUFT.load(Ordering::SeqCst) {
                        let _ = w2.hide();
                    }
                }
            });
            w
        }
    };
    if let Some((x, y, b, h)) = unter {
        let faktor = w.scale_factor().unwrap_or(1.0);
        let breite = PW_BREITE * faktor;
        let hoehe = PW_HOEHE * faktor;
        let mut links = (x + b / 2.0 - breite / 2.0).max(8.0 * faktor);
        let mut oben = y + h + 4.0 * faktor;
        // Taskleiste unten (Windows) bzw. rechter Bildschirmrand: nach oben/links ausweichen.
        if let Ok(Some(m)) = w.current_monitor().or_else(|_| w.primary_monitor()) {
            let (mx, my) = (m.position().x as f64, m.position().y as f64);
            let (mb, mh) = (m.size().width as f64, m.size().height as f64);
            if oben + hoehe > my + mh {
                oben = (y - hoehe - 4.0 * faktor).max(my);
            }
            links = links.min(mx + mb - breite - 8.0 * faktor);
        }
        let _ = w.set_position(tauri::PhysicalPosition::new(links, oben));
    } else {
        let _ = w.center();
    }
    let _ = w.show();
    let _ = w.set_focus();
    // Suchfeld gleich bereit — auch beim zweiten Öffnen.
    let _ = w.eval("setTimeout(function(){var e=document.querySelector('input');e&&e.focus();e&&e.select&&e.select();},60)");
}

/// Fensterknöpfe (Schließen, Minimieren, Zoomen) ausblenden — für das kleine
/// Passwort-Fenster, das wie ein Menü aussehen soll.
#[cfg(target_os = "macos")]
fn knoepfe_ausblenden(w: &WebviewWindow) {
    use objc2_app_kit::{NSWindow, NSWindowButton};
    let Ok(zeiger) = w.ns_window() else { return };
    // SAFETY: Tauri liefert den NSWindow-Zeiger des eben gebauten Fensters;
    // aufgerufen auf dem Hauptthread (Menüleisten-Ereignis).
    let fenster: &NSWindow = unsafe { &*(zeiger as *const NSWindow) };
    for k in [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton] {
        if let Some(b) = fenster.standardWindowButton(k) {
            b.setHidden(true);
        }
    }
}

/// Gemeinsame Ausstattung für Haupt- und Nebenfenster.
fn ausstatten<'a>(
    app: &AppHandle,
    b: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
    label: String,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    let (a1, a2, a3, a4) = (app.clone(), app.clone(), app.clone(), app.clone());
    let l1 = label.clone();
    b.initialization_script(bruecke::skript())
        // Sonst fängt Tauri Datei-Drops ab und Drag & Drop-Uploads in Atlas
        // kommen nie an.
        .disable_drag_drop_handler()
        .on_navigation(move |url| navigation_erlaubt(&a1, &l1, url))
        .on_new_window(move |url, features| neues_fenster(&a2, url, features))
        .on_download(move |_webview, event| downloads::ereignis(&a3, event))
        .on_page_load(move |w, p| seite_geladen(&a4, w, p))
        .on_document_title_changed(move |w, titel| {
            // Das Hauptfenster heißt immer „Sinclair Atlas“; Nebenfenster tragen
            // den Seitentitel (sonst sind sie im Fenster-Menü nicht unterscheidbar).
            if w.label() != HAUPT && !titel.trim().is_empty() {
                let _ = w.set_title(&titel);
            }
        })
}

fn navigation_erlaubt(app: &AppHandle, label: &str, url: &Url) -> bool {
    if ziel::ist_atlas(url) || ziel::ist_lokal(url) || ziel::ist_intern(url) {
        return true;
    }
    if !ziel::darf_extern(url) {
        log::warn!("Navigation gesperrt: {}", url.scheme());
        return false;
    }
    let http = matches!(url.scheme(), "http" | "https");
    if !http || ziel::ist_anmeldung(url) || cfg!(windows) {
        extern_oeffnen(app, url);
        leeres_nebenfenster_schliessen(app, label);
        return false;
    }
    // macOS: kann ein iframe sein — der Hauptframe wird in `seite_geladen` geprüft.
    true
}

fn neues_fenster(app: &AppHandle, url: Url, features: NewWindowFeatures) -> NewWindowResponse<tauri::Wry> {
    if ziel::ist_atlas(&url) || ziel::ist_intern(&url) {
        // In der App öffnen: Atlas schreibt in manche Fenster direkt hinein
        // (Zweitfenster, TIFF-Ansicht, Bank-Login über about:blank) und braucht
        // dafür die Verbindung zum öffnenden Fenster.
        let n = app.state::<Zustand>().fenster_zaehler.fetch_add(1, Ordering::Relaxed);
        let label = format!("{NEBEN_PREFIX}{n}");
        let b = WebviewWindowBuilder::new(
            app,
            &label,
            WebviewUrl::External("about:blank".parse().expect("about:blank")),
        )
        .title("Sinclair Atlas")
        .inner_size(1100.0, 800.0)
        .min_inner_size(420.0, 320.0)
        .window_features(features);
        return match ausstatten(app, b, label).build() {
            Ok(window) => NewWindowResponse::Create { window },
            Err(e) => {
                log::error!("Nebenfenster ließ sich nicht öffnen: {e}");
                NewWindowResponse::Deny
            }
        };
    }
    if ziel::darf_extern(&url) {
        extern_oeffnen(app, &url);
    }
    NewWindowResponse::Deny
}

fn seite_geladen(app: &AppHandle, w: WebviewWindow, p: PageLoadPayload<'_>) {
    let url = p.url().clone();
    match p.event() {
        PageLoadEvent::Started => {
            let fremd = matches!(url.scheme(), "http" | "https")
                && !ziel::ist_atlas(&url)
                && !ziel::ist_lokal(&url);
            if !fremd {
                return;
            }
            log::info!("Fenster {} auf fremder Seite gelandet, öffne im Browser", w.label());
            extern_oeffnen(app, &url);
            if w.label() == HAUPT {
                let zurueck = app
                    .state::<Zustand>()
                    .letzte_atlas_url
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| ziel::start_url().clone());
                let _ = w.navigate(zurueck);
            } else {
                let _ = w.close();
            }
        }
        PageLoadEvent::Finished => {
            log::info!("Fenster {} geladen: {}", w.label(), url.origin().ascii_serialization());
            if w.label() == HAUPT && ziel::ist_atlas(&url) {
                *app.state::<Zustand>().letzte_atlas_url.lock().unwrap() = Some(url);
            }
        }
    }
}

/// Ein Nebenfenster, das noch leer ist (about:blank) und nur als Sprungbrett für
/// eine fremde Seite diente — etwa der Bank-Login — wird geschlossen, sobald die
/// Seite im Browser offen ist.
fn leeres_nebenfenster_schliessen(app: &AppHandle, label: &str) {
    if !label.starts_with(NEBEN_PREFIX) {
        return;
    }
    let Some(w) = app.get_webview_window(label) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let leer = w
            .url()
            .map(|u| u.scheme() == "about" || u.as_str().is_empty())
            .unwrap_or(true);
        if leer {
            let _ = w.close();
        }
    });
}

/// Öffnet eine Adresse im Standardbrowser bzw. Standardprogramm.
pub fn extern_oeffnen(app: &AppHandle, url: &Url) {
    if !ziel::darf_extern(url) {
        return;
    }
    if ziel::ist_anmeldung(url) {
        *app.state::<Zustand>().anmeldung_seit.lock().unwrap() = Some(Instant::now());
        hinweis(
            app,
            t(
                "Die Anmeldung ist im Browser geöffnet. Danach einfach hierher zurückkehren.",
                "Sign-in opened in your browser. Come back here when you're done.",
            ),
        );
    }
    if let Err(e) = app.opener().open_url(url.as_str(), None::<&str>) {
        log::error!("Konnte Adresse nicht extern öffnen: {e}");
    }
}

/// Kurzer Hinweis unten im Hauptfenster (gezeichnet von `bruecke.js`).
pub fn hinweis(app: &AppHandle, text: &str) {
    if let Some(w) = hauptfenster(app) {
        let js = format!(
            "window.__atlasDesktopHinweis && window.__atlasDesktopHinweis({})",
            serde_json::to_string(text).unwrap_or_default()
        );
        let _ = w.eval(js);
    }
}

/// Wird das Hauptfenster nach einer Anmeldung im Browser wieder aktiv, Seite neu
/// laden — die Verbindung ist serverseitig angelegt, Atlas zeigt sie erst nach
/// dem Neuladen.
pub fn nach_anmeldung_neu_laden(app: &AppHandle) {
    let seit = app.state::<Zustand>().anmeldung_seit.lock().unwrap().take();
    let Some(seit) = seit else { return };
    // Fokuswechsel direkt beim Öffnen des Browsers ignorieren.
    if seit.elapsed().as_secs() < 3 {
        *app.state::<Zustand>().anmeldung_seit.lock().unwrap() = Some(seit);
        return;
    }
    if let Some(w) = hauptfenster(app) {
        if w.url().map(|u| ziel::ist_atlas(&u)).unwrap_or(false) {
            let _ = w.eval("location.reload()");
        }
    }
}

/// Atlas-Adresse im Hauptfenster laden — mit Erreichbarkeitsprüfung, sonst
/// Offline-Seite mit Rücksprungziel.
pub fn laden(app: &AppHandle, ziel_url: Url) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(w) = hauptfenster(&app) else { return };
        match erreichbarkeit::pruefen(&ziel_url).await {
            erreichbarkeit::Ergebnis::Erreichbar => {
                let _ = w.navigate(ziel_url);
            }
            _ => {
                let _ = w.navigate(ziel::lokale_seite(Some(&ziel_url)));
            }
        }
    });
}

/// Aktuelle Seite neu laden; bei fehlender Verbindung zur Offline-Seite.
pub fn neu_laden(app: &AppHandle, w: &WebviewWindow) {
    match w.url() {
        Ok(u) if ziel::ist_atlas(&u) && w.label() == HAUPT => laden(app, u),
        _ => {
            let _ = w.eval("location.reload()");
        }
    }
}

/// Deep-Link verarbeiten: Fenster nach vorn holen und Ziel laden.
pub fn deep_link_oeffnen(app: &AppHandle, link: &Url) {
    let Some(ziel_url) = ziel::deep_link_ziel(link) else {
        log::warn!("Deep-Link verworfen: {link}");
        return;
    };
    hauptfenster_zeigen(app);
    // Kommt der Link als Abschluss einer Anmeldung, nicht zusätzlich neu laden.
    app.state::<Zustand>().anmeldung_seit.lock().unwrap().take();
    laden(app, ziel_url);
}
