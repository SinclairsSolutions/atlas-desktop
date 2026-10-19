//! Ist Atlas erreichbar? Vor jedem Laden, das die App selbst auslöst (Start,
//! Neu laden, Deep-Link, „Erneut versuchen“), damit statt einer weißen Seite
//! die Offline-Ansicht erscheint.

use std::{sync::OnceLock, time::Duration};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ergebnis {
    Erreichbar,
    /// Keine Netzverbindung oder DNS/TLS-Fehler.
    KeinNetz,
    /// Server antwortet, aber mit 5xx (Wartung, Ausfall).
    Stoerung,
}

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("AtlasDesktop/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP-Client lässt sich bauen")
    })
}

pub async fn pruefen(url: &Url) -> Ergebnis {
    let mut ziel = url.clone();
    ziel.set_path("/");
    ziel.set_query(None);
    ziel.set_fragment(None);
    match client().head(ziel).send().await {
        Ok(r) if r.status().is_server_error() => Ergebnis::Stoerung,
        Ok(_) => Ergebnis::Erreichbar,
        Err(e) => {
            log::warn!("Atlas nicht erreichbar: {e}");
            Ergebnis::KeinNetz
        }
    }
}
