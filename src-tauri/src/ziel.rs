//! Welche Adressen gehören zu Atlas, welche gehen in den Standardbrowser?
//!
//! Die einzige Stelle, an der die Atlas-Adresse steht. Alles andere fragt hier.

use std::sync::OnceLock;
use url::Url;

/// Die Live-Seite, die die App lädt.
pub const ATLAS_URL: &str = "https://sinclair-atlas.com";

/// Hosts, die als „Atlas“ gelten und in der App bleiben.
pub const ATLAS_HOSTS: &[&str] = &["sinclair-atlas.com", "www.sinclair-atlas.com"];

/// URL-Schema für Deep-Links (`sinclair-atlas://open/<pfad>`). Muss zu
/// `plugins.deep-link.desktop.schemes` in `tauri.conf.json` passen.
pub const SCHEMA: &str = "sinclair-atlas";

/// Anmelde- und Verbindungsseiten fremder Anbieter. Sie laufen grundsätzlich im
/// System-Browser (dort sind Passwort-Manager, Passkeys und bestehende Sitzungen),
/// nie in der eingebetteten Ansicht — Google blockiert eingebettete Logins ohnehin.
const ANMELDE_HOSTS: &[&str] = &[
    "accounts.google.com",
    "login.microsoftonline.com",
    "login.microsoft.com",
    "login.live.com",
    "appleid.apple.com",
    "connect.stripe.com",
    "slack.com",
    "www.paypal.com",
    "api.sumup.com",
    "login.datev.de",
];

/// Endungen von Anmelde-Hosts (finAPI-Webformular der Banken u. ä.).
const ANMELDE_ENDUNGEN: &[&str] = &[".finapi.io"];

/// Startadresse. In Debug-Builds per `ATLAS_DESKTOP_URL` übersteuerbar
/// (z. B. `http://localhost:8080` gegen den lokalen Atlas-Dev-Server).
pub fn start_url() -> &'static Url {
    static START: OnceLock<Url> = OnceLock::new();
    START.get_or_init(|| {
        if cfg!(debug_assertions) {
            if let Some(u) = std::env::var("ATLAS_DESKTOP_URL")
                .ok()
                .and_then(|v| Url::parse(&v).ok())
            {
                return u;
            }
        }
        Url::parse(ATLAS_URL).expect("ATLAS_URL ist gültig")
    })
}

/// Liegt die Startadresse nicht auf der Live-Domain (nur Debug)?
pub fn ist_uebersteuert() -> bool {
    start_url().as_str().trim_end_matches('/') != ATLAS_URL
}

pub fn ist_atlas(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    if url.scheme() == "https" && ATLAS_HOSTS.contains(&host) {
        return true;
    }
    ist_uebersteuert() && url.origin() == start_url().origin()
}

/// Die eigene, mitgelieferte Seite (Lade- und Offline-Ansicht).
pub fn ist_lokal(url: &Url) -> bool {
    match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" | "https" => url.host_str() == Some("tauri.localhost"),
        _ => false,
    }
}

/// Adressen ohne eigenen Server: leere Fenster, Blob- und Daten-URLs, die Atlas
/// selbst erzeugt (PDF-Vorschau, TIFF-Ansicht, Zweitfenster).
pub fn ist_intern(url: &Url) -> bool {
    matches!(url.scheme(), "about" | "blob" | "data")
}

pub fn ist_anmeldung(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    ANMELDE_HOSTS.contains(&host) || ANMELDE_ENDUNGEN.iter().any(|e| host.ends_with(e))
}

/// Darf diese Adresse an das Betriebssystem (Browser, Mail-Programm …) gehen?
pub fn darf_extern(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "mailto" | "tel" | "sms" | "facetime")
}

/// Adresse der mitgelieferten Lade-/Offline-Seite, optional mit Rücksprungziel.
pub fn lokale_seite(ziel: Option<&Url>) -> Url {
    let basis = if cfg!(windows) {
        "http://tauri.localhost/index.html"
    } else {
        "tauri://localhost/index.html"
    };
    let mut u = Url::parse(basis).expect("lokale Adresse ist gültig");
    if let Some(z) = ziel {
        u.query_pairs_mut().append_pair("ziel", z.as_str());
    }
    u
}

/// Übersetzt einen Deep-Link in eine Atlas-Adresse.
///
/// `sinclair-atlas://open/workspaces/123/mail?verbunden=1` → `https://sinclair-atlas.com/workspaces/123/mail?verbunden=1`
/// `sinclair-atlas://open` → Startseite. Alles, was nicht auf Atlas zeigt, wird verworfen.
pub fn deep_link_ziel(link: &Url) -> Option<Url> {
    if link.scheme() != SCHEMA {
        return None;
    }
    // Erlaubt sind „sinclair-atlas://open/…“ und „sinclair-atlas:///…“.
    match link.host_str() {
        None | Some("") | Some("open") => {}
        _ => return None,
    }
    let pfad = link.path();
    let pfad = if pfad.is_empty() { "/" } else { pfad };
    // „//evil.example“ würde sonst als fremder Host gelesen.
    if !pfad.starts_with('/') || pfad.starts_with("//") {
        return None;
    }
    let mut ziel = start_url().join(pfad).ok()?;
    ziel.set_query(link.query());
    ziel.set_fragment(link.fragment());
    ist_atlas(&ziel).then_some(ziel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn atlas_und_fremde_hosts() {
        assert!(ist_atlas(&u("https://sinclair-atlas.com/workspaces/1")));
        assert!(ist_atlas(&u("https://www.sinclair-atlas.com/")));
        assert!(!ist_atlas(&u("http://sinclair-atlas.com/")));
        assert!(!ist_atlas(&u("https://sinclair-atlas.com.evil.example/")));
        assert!(!ist_atlas(&u("https://accounts.google.com/o/oauth2")));
    }

    #[test]
    fn anmeldungen() {
        assert!(ist_anmeldung(&u("https://accounts.google.com/o/oauth2/v2/auth?x=1")));
        assert!(ist_anmeldung(&u("https://login.microsoftonline.com/common/oauth2")));
        assert!(ist_anmeldung(&u("https://webform-live.finapi.io/abc")));
        assert!(!ist_anmeldung(&u("https://drive.google.com/file/d/1")));
    }

    #[test]
    fn deep_links() {
        assert_eq!(
            deep_link_ziel(&u("sinclair-atlas://open/workspaces/1/mail?verbunden=1#x"))
                .unwrap()
                .as_str(),
            "https://sinclair-atlas.com/workspaces/1/mail?verbunden=1#x"
        );
        assert_eq!(
            deep_link_ziel(&u("sinclair-atlas://open")).unwrap().as_str(),
            "https://sinclair-atlas.com/"
        );
        assert!(deep_link_ziel(&u("sinclair-atlas://evil.example/x")).is_none());
        assert!(deep_link_ziel(&u("sinclair-atlas://open//evil.example/x")).is_none());
        assert!(deep_link_ziel(&u("https://sinclair-atlas.com/")).is_none());
    }

    #[test]
    fn lokale_seite_mit_ziel() {
        let z = u("https://sinclair-atlas.com/a?b=1");
        let l = lokale_seite(Some(&z));
        assert!(ist_lokal(&l));
        assert_eq!(
            l.query_pairs().find(|(k, _)| k == "ziel").unwrap().1,
            "https://sinclair-atlas.com/a?b=1"
        );
    }
}
