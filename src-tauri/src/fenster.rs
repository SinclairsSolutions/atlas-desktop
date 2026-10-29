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

/// Hauptfenster zeigen (Dock-Klick, zweiter Start, Deep-Link). Ist Atlas
/// gesperrt, zeigt es den Sperrbildschirm (siehe sperre.rs).
pub fn hauptfenster_zeigen(app: &AppHandle) {
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
    ausstatten(app, titelleiste(b, crate::erscheinung::hintergrund(app, HAUPT)), HAUPT.to_string()).build()
}

/// Titelleiste (macOS): kein grauer Balken, kein Titeltext — Atlas reicht bis
/// unter die Fensterknöpfe.
fn titelleiste<'a>(
    b: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
    hintergrund: tauri::window::Color,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    #[cfg(target_os = "macos")]
    {
        // Overlay: Atlas reicht bis unter die Fensterknöpfe (Platz dafür lässt
        // Atlas selbst, siehe styles.css „atlas-desktop-mac“).
        // Knöpfe senkrecht mittig auf der 56 px hohen Atlas-Titelleiste.
        b.title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(tauri::LogicalPosition::new(22.0, 21.0))
            // Zuletzt bekannte Darstellung von Atlas (Farbe je Fenster), siehe erscheinung.rs.
            .background_color(hintergrund)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = hintergrund;
        b
    }
}

/// Eigene Fenster (Mail, Kommunikation, Passwörter): schmale, ruhige Titelzeile
/// von Atlas (40 px, Fenstername in der Mitte) statt der grauen macOS-Leiste;
/// die Knöpfe sitzen mittig darin.
fn eigene_titelleiste<'a>(
    b: WebviewWindowBuilder<'a, tauri::Wry, AppHandle>,
    hintergrund: tauri::window::Color,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    #[cfg(target_os = "macos")]
    {
        b.title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(tauri::LogicalPosition::new(16.0, 13.0))
            // Zuletzt bekannte Darstellung von Atlas (Farbe je Fenster), siehe erscheinung.rs.
            .background_color(hintergrund)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = hintergrund;
        b
    }
}

/// Weiteres Atlas-Fenster (Menü „Neues Fenster“) — z. B. zwei Mandanten nebeneinander.
pub fn weiteres_fenster(app: &AppHandle, url: Url) {
    atlas_fenster(app, url, None);
}

/// Eigenständiges Atlas-Fenster mit eigener WebKit-Konfiguration — und damit
/// eigener, vollständiger Brücke (IPC). Für ⌘N, „Eigenes Fenster“ aus Atlas
/// (Mail, Kommunikation …) und Atlas-Links mit Ziel `_blank`.
fn atlas_fenster(app: &AppHandle, url: Url, groesse: Option<tauri::LogicalSize<f64>>) -> Option<WebviewWindow> {
    // Gesperrt: kein neues Fenster an der Sperre vorbei (⌘N, Menüleiste).
    if crate::sperre::ist_gesperrt(app) {
        crate::sperre::zur_sperre(app);
        return None;
    }
    // Eigene Fenster (Mail, Passwörter) haben keine Seitenleiste — dort bleibt die
    // normale Titelleiste, sonst lägen die Fensterknöpfe über dem Inhalt.
    let eigenes = ist_eigenes_fenster(&url);
    let n = app.state::<Zustand>().fenster_zaehler.fetch_add(1, Ordering::Relaxed);
    let label = format!("{NEBEN_PREFIX}{n}");
    let (breite, hoehe) = groesse
        .map(|g| (g.width.clamp(720.0, 2400.0), g.height.clamp(480.0, 1600.0)))
        .unwrap_or((1280.0, 840.0));
    let b = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title("Sinclair Atlas")
        .inner_size(breite, hoehe)
        .min_inner_size(720.0, 480.0);
    let farbe = crate::erscheinung::hintergrund(app, &label);
    let b = if eigenes { eigene_titelleiste(b, farbe) } else { titelleiste(b, farbe) };
    match ausstatten(app, b, label).build() {
        Ok(w) => Some(w),
        Err(e) => {
            log::error!("Neues Fenster ließ sich nicht öffnen: {e}");
            None
        }
    }
}

fn ist_eigenes_fenster(url: &Url) -> bool {
    url.query().is_some_and(|q| q.split('&').any(|p| p == "fenster=1"))
}

/// Passwörter-Schnellzugriff aus der Menüleiste (wie Safari): kleines Fenster
/// ohne Rahmen unter dem Symbol, geht beim Wegklicken zu. Fängt mit „neben-“
/// an, damit es dieselben Rechte wie die anderen Atlas-Fenster hat.
pub const PASSWORT_SCHNELL: &str = "neben-passwoerter-schnell";
const PW_BREITE: f64 = 380.0;
const PW_HOEHE: f64 = 500.0;

/// Ein-/ausblenden; `unter` = Rechteck des Menüleisten-Symbols (physisch).
pub fn passwort_schnell(app: &AppHandle, unter: Option<(f64, f64, f64, f64)>) {
    if crate::sperre::ist_gesperrt(app) {
        crate::sperre::zur_sperre(app);
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
                    if !crate::sperre::pruefung_laeuft() {
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
        // Kamera/Mikrofon nur, wenn das Fenster Atlas zeigt. Ohne diese Regel
        // gibt WebKit jeder Seite (auch fremden) den Zugriff ohne Rückfrage.
        .on_permission_request(|w, _art| {
            use tauri::webview::PermissionResponse;
            if !w.url().is_ok_and(|u| ziel::ist_atlas(&u)) {
                PermissionResponse::Deny
            } else if cfg!(target_os = "macos") {
                // macOS: wie bisher ohne Rückfrage (die Systemfreigabe gilt ohnehin).
                PermissionResponse::Allow
            } else {
                // Windows: WebView2 fragt wie bisher selbst.
                PermissionResponse::Default
            }
        })
        .on_document_title_changed(move |w, titel| {
            // Das Hauptfenster heißt immer „Sinclair Atlas“; Nebenfenster tragen
            // den Seitentitel (sonst sind sie im Fenster-Menü nicht unterscheidbar).
            if w.label() != HAUPT && !titel.trim().is_empty() {
                let _ = w.set_title(&titel);
            }
        })
}

fn navigation_erlaubt(app: &AppHandle, label: &str, url: &Url) -> bool {
    // Meldekanal von `bruecke.js`, der ohne IPC funktioniert (siehe dort).
    if url.scheme() == ziel::SCHEMA && url.host_str() == Some("bruecke") {
        if url.path() == "/gestoert" {
            let auf_atlas = app
                .get_webview_window(label)
                .and_then(|w| w.url().ok())
                .is_some_and(|u| ziel::ist_atlas(&u));
            if auf_atlas {
                bruecke_gestoert(app, label, url);
            }
        }
        return false;
    }
    if ziel::ist_atlas(url) || ziel::ist_lokal(url) || ziel::ist_intern(url) {
        return true;
    }
    // macOS: fremde Blob-Adresse kann ein iframe sein (Vorschauen) — der
    // Hauptframe wird in `seite_geladen` geprüft.
    if url.scheme() == "blob" && !cfg!(windows) {
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
    // Gesperrt: keine Fenster an der Sperre vorbei (Skripte im Hintergrund).
    if crate::sperre::ist_gesperrt(app) {
        return NewWindowResponse::Deny;
    }
    if ziel::ist_atlas(&url) {
        // Atlas-Seiten (Mail, Kommunikation, Passwörter, Links mit Ziel) bekommen
        // ein eigenständiges Fenster. NICHT über `NewWindowResponse::Create`:
        // Unter macOS müsste das neue WKWebView die Konfiguration des öffnenden
        // Fensters übernehmen — und teilt sich damit dessen
        // WKUserContentController. Folgen (lokal nachgestellt mit 0.3.6):
        //  - Die Brücke des neuen Fensters (Handler „ipc“) lässt sich nicht
        //    anmelden; seine Aufrufe landen beim öffnenden Fenster (Ziehen
        //    bewegt das falsche Fenster bzw. nichts, Aufrufe hängen).
        //  - Beim Schließen entfernt wry den Handler „ipc“ — den des
        //    Hauptfensters. Danach fehlt dort `window.webkit.messageHandlers`
        //    („Ablage nicht verbunden …“, Eingangs-Bestätigungen gehen verloren),
        //    auch nach dem Neuladen.
        // Gleiche Adresse schon offen (Atlas hält je Bereich ein Fenster): nach vorn holen.
        let schluessel = fensterschluessel(&url);
        if vorhandenes_nach_vorn(app, &schluessel) {
            return NewWindowResponse::Deny;
        }
        let a = app.clone();
        let groesse = features.size();
        // Erst nach dem WebKit-Rückruf bauen (nicht mitten in der Delegate-Methode).
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = atlas_fenster(&a, url, groesse) {
                ziel_merken(&a, schluessel, &w);
            }
        });
        return NewWindowResponse::Deny;
    }
    if ziel::ist_intern(&url) {
        // Leere Fenster, Blob/Data (TIFF-Ansicht, PDF-Vorschau, Bank-Login über
        // about:blank): Atlas schreibt hinein und braucht dafür die Verbindung zum
        // öffnenden Fenster — hier geht es nur mit der geteilten Konfiguration.
        // Solche Fenster brauchen selbst keine Brücke; damit beim Schließen nicht
        // die Brücke des öffnenden Fensters verschwindet, werden sie nur
        // ausgeblendet (siehe `geteiltes_fenster_schliessen`).
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
        let farbe = crate::erscheinung::hintergrund(app, &label);
        #[cfg(target_os = "macos")]
        let b = b.background_color(farbe);
        #[cfg(not(target_os = "macos"))]
        let _ = farbe;
        return match ausstatten(app, b, label.clone()).build() {
            Ok(window) => {
                app.state::<Zustand>().geteilte_fenster.lock().unwrap().insert(label);
                NewWindowResponse::Create { window }
            }
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

/// Eigenes Fenster eines Bereichs (Menü/Kürzel ⇧⌘M, ⇧⌘K, ⇧⌘P, Menüleiste):
/// Ist es schon offen — auch wenn Atlas es per `window.open` geöffnet hat —,
/// kommt es nach vorn, sonst entsteht es neu. Vorher entstand bei jedem Druck
/// ein weiteres Fenster genau über dem vorhandenen (gleiche Größe und Lage),
/// sodass das Kürzel scheinbar nichts tat.
pub fn bereichsfenster(app: &AppHandle, url: Url) {
    let schluessel = fensterschluessel(&url);
    if vorhandenes_nach_vorn(app, &schluessel) {
        return;
    }
    if let Some(w) = atlas_fenster(app, url, None) {
        ziel_merken(app, schluessel, &w);
    }
}

/// Fenster zu dieser Adresse (ohne Sprungmarke) nach vorn holen, falls offen.
fn vorhandenes_nach_vorn(app: &AppHandle, schluessel: &str) -> bool {
    let vorhanden = app
        .state::<Zustand>()
        .fenster_ziele
        .lock()
        .unwrap()
        .get(schluessel)
        .and_then(|l| app.get_webview_window(l))
        .filter(|w| w.is_visible().unwrap_or(false) || w.is_minimized().unwrap_or(false));
    let Some(w) = vorhanden else {
        log::info!("Bereichsfenster {schluessel}: keins offen, öffne neu");
        return false;
    };
    log::info!("Bereichsfenster {schluessel}: {} nach vorn", w.label());
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
    true
}

fn ziel_merken(app: &AppHandle, schluessel: String, w: &WebviewWindow) {
    app.state::<Zustand>()
        .fenster_ziele
        .lock()
        .unwrap()
        .insert(schluessel, w.label().to_string());
}

/// Adresse ohne Sprungmarke — gleiche Adresse = gleiches Fenster.
fn fensterschluessel(url: &Url) -> String {
    let mut u = url.clone();
    u.set_fragment(None);
    u.to_string()
}

/// Fenster mit geteilter WebKit-Konfiguration (siehe `neues_fenster`) nicht
/// zerstören, sondern ausblenden und leeren: Beim Zerstören würde wry den
/// IPC-Handler des öffnenden Fensters mit entfernen. Gibt `true` zurück, wenn
/// das Schließen damit erledigt ist.
pub fn geteiltes_fenster_schliessen(w: &tauri::Window) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let app = w.app_handle();
    if !app.state::<Zustand>().geteilte_fenster.lock().unwrap().contains(w.label()) {
        return false;
    }
    let _ = w.hide();
    if let Some(wv) = app.get_webview_window(w.label()) {
        let _ = wv.navigate("about:blank".parse().expect("about:blank"));
    }
    true
}

/// Meldung von `bruecke.js`: In diesem Fenster fehlt die Brücke (IPC). Kommt
/// ohne IPC an — als Navigation auf `sinclair-atlas://bruecke/gestoert`, die
/// `navigation_erlaubt` abfängt.
fn bruecke_gestoert(app: &AppHandle, label: &str, meldung: &Url) {
    let grund: String = meldung
        .query_pairs()
        .find(|(k, _)| k == "grund")
        .map(|(_, v)| v.chars().take(200).collect())
        .unwrap_or_default();
    log::error!("Brücke gestört in Fenster {label}: {grund}");
    let app = app.clone();
    let label = label.to_string();
    // Nicht mitten im Navigations-Rückruf Fenster umbauen.
    let _ = app.clone().run_on_main_thread(move || bruecke_heilen(&app, &label));
}

/// Fenster mit gestörter Brücke neu aufbauen (eigene WebKit-Konfiguration,
/// frische IPC) — je Fenster höchstens alle 10 Minuten, damit nichts kreist.
fn bruecke_heilen(app: &AppHandle, label: &str) {
    let Some(w) = app.get_webview_window(label) else { return };
    let Some(url) = w.url().ok().filter(ziel::ist_atlas) else { return };
    {
        let zustand = app.state::<Zustand>();
        let mut h = zustand.heilung.lock().unwrap();
        if h.get(label).is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(600)) {
            log::warn!("Brücke in {label}: schon kürzlich neu aufgebaut — kein weiterer Versuch");
            return;
        }
        h.insert(label.to_string(), Instant::now());
    }
    if label == HAUPT {
        log::warn!("Hauptfenster wird neu aufgebaut (Brücke fehlte)");
        *app.state::<Zustand>().neuaufbau.lock().unwrap() = Some(url);
        // Weiter in `nach_zerstoerung`, sobald das Fenster weg ist (Label frei).
        let _ = w.destroy();
        return;
    }
    let groesse = w
        .inner_size()
        .ok()
        .map(|g| g.to_logical::<f64>(w.scale_factor().unwrap_or(1.0)));
    let lage = w.outer_position().ok();
    if let Some(neu) = atlas_fenster(app, url, groesse) {
        if let Some(p) = lage {
            let _ = neu.set_position(p);
        }
        let _ = w.close();
    }
}

/// Ein Fenster ist zerstört. War es das Hauptfenster im Neuaufbau, gleich wieder
/// bauen und die Seite von vorher laden.
pub fn nach_zerstoerung(app: &AppHandle, label: &str) {
    app.state::<Zustand>().geteilte_fenster.lock().unwrap().remove(label);
    if label != HAUPT {
        return;
    }
    let Some(url) = app.state::<Zustand>().neuaufbau.lock().unwrap().take() else { return };
    match hauptfenster_bauen(app) {
        Ok(w) => {
            let _ = w.show();
            let _ = w.set_focus();
            laden(app, url);
        }
        Err(e) => log::error!("Hauptfenster ließ sich nicht neu aufbauen: {e}"),
    }
}

/// Läuft gerade ein Neuaufbau des Hauptfensters? (Dann die App nicht beenden,
/// nur weil kurz kein Fenster offen ist.)
pub fn neuaufbau_laeuft(app: &AppHandle) -> bool {
    app.state::<Zustand>().neuaufbau.lock().unwrap().is_some()
}

fn seite_geladen(app: &AppHandle, w: WebviewWindow, p: PageLoadPayload<'_>) {
    let url = p.url().clone();
    match p.event() {
        PageLoadEvent::Started => {
            if w.label() == HAUPT {
                // Neue Seite: Atlas meldet erst wieder, wenn es Dateien annimmt.
                crate::eingang::seite_wechselt(app);
            }
            let web = matches!(url.scheme(), "http" | "https");
            let fremd = (web && !ziel::ist_atlas(&url) && !ziel::ist_lokal(&url))
                // Blob-Seite eines fremden Ursprungs (aus einem iframe heraus).
                || (url.scheme() == "blob" && !ziel::ist_intern(&url));
            if !fremd {
                return;
            }
            log::info!("Fenster {} auf fremder Seite gelandet", w.label());
            if web {
                extern_oeffnen(app, &url);
            }
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
        // Nur Schema und Host ins Protokoll — der Rest kann Kennungen enthalten.
        log::warn!("Deep-Link verworfen: {}://{}", link.scheme(), link.host_str().unwrap_or_default());
        return;
    };
    hauptfenster_zeigen(app);
    // Kommt der Link als Abschluss einer Anmeldung, nicht zusätzlich neu laden.
    app.state::<Zustand>().anmeldung_seit.lock().unwrap().take();
    laden(app, ziel_url);
}
