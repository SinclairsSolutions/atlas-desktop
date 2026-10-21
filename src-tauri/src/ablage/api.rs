//! Schnittstelle zu Atlas für die Ablage (`/api/public/desktop/ablage/*`) —
//! dieselbe, die die macOS-Erweiterung in Swift spricht (finder-ablage/Quellen/Server.swift).
//! Unter Windows nutzt sie die App selbst (Cloud Files laufen im App-Prozess).

#![cfg_attr(not(windows), allow(dead_code))]

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, Deserialize)]
pub struct WorkspaceInfo {
    pub id: String,
    pub name: String,
    pub schreiben: bool,
    pub loeschen: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerEintrag {
    pub id: String,
    /// "ordner" | "dokument"
    pub art: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub groesse: Option<i64>,
    pub mime: Option<String>,
    pub version: i64,
    pub inhalt_version: String,
    pub meta_version: String,
    pub erstellt_am: String,
    pub geaendert_am: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Liste {
    pub workspace_id: String,
    pub schreiben: bool,
    pub loeschen: bool,
    pub fingerabdruck: String,
    pub eintraege: Vec<ServerEintrag>,
}

#[derive(Debug, Deserialize)]
pub struct VersionErgebnis {
    pub eintrag: ServerEintrag,
    pub konflikt: Option<ServerEintrag>,
}

#[derive(Debug)]
pub enum Fehler {
    NichtVerbunden,
    /// Zugang widerrufen — alles entfernen.
    Widerrufen,
    KeinRecht(String),
    NichtGefunden,
    Abgelehnt(String),
    Netz(String),
    Server(u16, String),
}

impl std::fmt::Display for Fehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fehler::NichtVerbunden => write!(f, "nicht verbunden"),
            Fehler::Widerrufen => write!(f, "Zugang widerrufen"),
            Fehler::KeinRecht(m) | Fehler::Abgelehnt(m) => write!(f, "{m}"),
            Fehler::NichtGefunden => write!(f, "nicht gefunden"),
            Fehler::Netz(m) => write!(f, "Netz: {m}"),
            Fehler::Server(s, m) => write!(f, "Server {s}: {m}"),
        }
    }
}

#[derive(Deserialize)]
struct FehlerAntwort {
    error: Option<String>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct WorkspacesAntwort {
    workspaces: Vec<WorkspaceInfo>,
}

#[derive(Deserialize)]
struct EintragAntwort {
    eintrag: ServerEintrag,
}

pub struct Api {
    basis: String,
    token: String,
    client: reqwest::Client,
}

impl Api {
    pub fn neu(basis: &str, token: &str) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60 * 30))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("AtlasAblage/", env!("CARGO_PKG_VERSION"), " (Windows)"))
            .build()
            .expect("HTTP-Client");
        Self { basis: basis.trim_end_matches('/').to_string(), token: token.to_string(), client }
    }

    fn url(&self, pfad: &str) -> String {
        format!("{}/api/public/desktop/ablage/{pfad}", self.basis)
    }

    async fn pruefen(r: reqwest::Response) -> Result<reqwest::Response, Fehler> {
        let status = r.status();
        if status.is_success() {
            return Ok(r);
        }
        let f: Option<FehlerAntwort> = r.json().await.ok();
        let meldung = f.as_ref().and_then(|f| f.message.clone()).unwrap_or_else(|| format!("Fehler {status}"));
        Err(match status.as_u16() {
            401 if f.as_ref().and_then(|f| f.error.as_deref()) == Some("token_ungueltig") => Fehler::Widerrufen,
            401 => Fehler::NichtVerbunden,
            403 => Fehler::KeinRecht(meldung),
            404 => Fehler::NichtGefunden,
            400 | 409 | 413 => Fehler::Abgelehnt(meldung),
            s => Fehler::Server(s, meldung),
        })
    }

    async fn post_json<T: for<'de> Deserialize<'de>>(&self, pfad: &str, koerper: serde_json::Value) -> Result<T, Fehler> {
        let r = self
            .client
            .post(self.url(pfad))
            .bearer_auth(&self.token)
            .json(&koerper)
            .send()
            .await
            .map_err(|e| Fehler::Netz(e.to_string()))?;
        let r = Self::pruefen(r).await?;
        r.json::<T>().await.map_err(|e| Fehler::Server(200, e.to_string()))
    }

    pub async fn workspaces(&self) -> Result<Vec<WorkspaceInfo>, Fehler> {
        Ok(self.post_json::<WorkspacesAntwort>("workspaces", serde_json::json!({})).await?.workspaces)
    }

    pub async fn liste(&self, workspace_id: &str) -> Result<Liste, Fehler> {
        self.post_json("liste", serde_json::json!({ "workspaceId": workspace_id })).await
    }

    /// Datei laden; liefert die Antwort zum Streamen.
    pub async fn datei(&self, dokument_id: &str) -> Result<reqwest::Response, Fehler> {
        let r = self
            .client
            .get(self.url("datei"))
            .query(&[("id", dokument_id)])
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| Fehler::Netz(e.to_string()))?;
        Self::pruefen(r).await
    }

    /// Größe einer Datei (HEAD), für Platzhalter älterer Dokumente ohne gespeicherte Größe.
    pub async fn groesse(&self, dokument_id: &str) -> Result<Option<i64>, Fehler> {
        let r = self
            .client
            .head(self.url("datei"))
            .query(&[("id", dokument_id)])
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| Fehler::Netz(e.to_string()))?;
        let r = Self::pruefen(r).await?;
        let h = r.headers();
        let wert = h.get("x-atlas-groesse").or_else(|| h.get("content-length"));
        Ok(wert.and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<i64>().ok()))
    }

    pub async fn hochladen(&self, workspace_id: &str, ordner_id: Option<&str>, name: &str, mime: &str, inhalt: Vec<u8>) -> Result<ServerEintrag, Fehler> {
        let name_kodiert: String = url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>().replace('+', "%20");
        let r = self
            .client
            .post(self.url("hochladen"))
            .bearer_auth(&self.token)
            .header("x-atlas-workspace", workspace_id)
            .header("x-atlas-ordner", ordner_id.unwrap_or(""))
            .header("x-atlas-name", name_kodiert)
            .header("content-type", mime)
            .body(inhalt)
            .send()
            .await
            .map_err(|e| Fehler::Netz(e.to_string()))?;
        let r = Self::pruefen(r).await?;
        Ok(r.json::<EintragAntwort>().await.map_err(|e| Fehler::Server(200, e.to_string()))?.eintrag)
    }

    pub async fn neue_version(&self, dokument_id: &str, basis: &str, mime: &str, inhalt: Vec<u8>) -> Result<VersionErgebnis, Fehler> {
        let r = self
            .client
            .post(self.url("hochladen"))
            .bearer_auth(&self.token)
            .header("x-atlas-dokument", dokument_id)
            .header("x-atlas-basis", basis)
            .header("content-type", mime)
            .body(inhalt)
            .send()
            .await
            .map_err(|e| Fehler::Netz(e.to_string()))?;
        let r = Self::pruefen(r).await?;
        r.json::<VersionErgebnis>().await.map_err(|e| Fehler::Server(200, e.to_string()))
    }

    pub async fn ordner(&self, workspace_id: &str, ordner_id: Option<&str>, name: &str) -> Result<ServerEintrag, Fehler> {
        Ok(self
            .post_json::<EintragAntwort>(
                "ordner",
                serde_json::json!({ "workspaceId": workspace_id, "parentId": ordner_id, "name": name }),
            )
            .await?
            .eintrag)
    }

    /// `ordner`: None = nicht verschieben, Some(None) = oberste Ebene.
    pub async fn aendern(&self, id: &str, name: Option<&str>, ordner: Option<Option<&str>>) -> Result<ServerEintrag, Fehler> {
        let mut k = serde_json::json!({ "id": id });
        if let Some(n) = name {
            k["name"] = serde_json::Value::String(n.to_string());
        }
        if let Some(o) = ordner {
            k["parentId"] = o.map(|s| serde_json::Value::String(s.to_string())).unwrap_or(serde_json::Value::Null);
        }
        Ok(self.post_json::<EintragAntwort>("aendern", k).await?.eintrag)
    }

    pub async fn loeschen(&self, id: &str) -> Result<(), Fehler> {
        let _: serde_json::Value = self.post_json("loeschen", serde_json::json!({ "id": id })).await?;
        Ok(())
    }
}

/// Dateien, die Programme nur zwischenspeichern (gleiche Regel wie in Atlas).
pub fn ist_hilfsdatei(name: &str) -> bool {
    let n = name.trim();
    if n.is_empty() || n.starts_with('.') || n.starts_with("~$") {
        return true;
    }
    let k = n.to_lowercase();
    if [".tmp", ".temp", ".crdownload", ".part", ".download"].iter().any(|e| k.ends_with(e)) {
        return true;
    }
    matches!(k.as_str(), "thumbs.db" | "desktop.ini")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hilfsdateien() {
        assert!(ist_hilfsdatei("~$Bericht.docx"));
        assert!(ist_hilfsdatei("desktop.ini"));
        assert!(ist_hilfsdatei("x.TMP"));
        assert!(!ist_hilfsdatei("Bericht.docx"));
    }
    #[test]
    fn eintrag_lesen() {
        let e: ServerEintrag = serde_json::from_str(
            r#"{"id":"a","art":"dokument","name":"x.pdf","parentId":null,"groesse":3,"mime":"application/pdf","version":2,"inhaltVersion":"2-ab","metaVersion":"m","erstelltAm":"2026-10-21T00:00:00Z","geaendertAm":"2026-10-21T00:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(e.version, 2);
        assert_eq!(e.parent_id, None);
    }
}
