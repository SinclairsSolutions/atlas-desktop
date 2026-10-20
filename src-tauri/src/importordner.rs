//! Import-Ordner: eine Ordnerstruktur auf dem Rechner, die Atlas spiegelt.
//!
//! ```text
//! <Basis>/                                   → persönlicher Eingang
//! <Basis>/<Workspace>/                       → Eingang des Workspace (Team)
//! <Basis>/<Workspace>/<Unternehmen>/         → direkt zum Unternehmen (nur „je Unternehmen“)
//! ```
//!
//! Atlas meldet nach der Anmeldung über `import_ordner_setzen`, in welche
//! Workspaces die Person in den Eingang schreiben darf. Die App legt fehlende
//! Ordner an, benennt umbenannte nach (Zuordnung über die ID in
//! `<Basis>/.atlas-zuordnung.json`) und löscht nie einen Ordner. Ordner, die
//! nicht mehr in der Liste stehen, bleiben liegen und gelten als inaktiv —
//! was dort landet, geht sicherheitshalber in den persönlichen Eingang.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::{einstellungen, texte::t};

const ZUORDNUNG: &str = ".atlas-zuordnung.json";
/// Mehr Workspaces bzw. Unternehmen nimmt die App nicht an (Schutz vor Unsinn).
const MAX_WORKSPACES: usize = 500;
const MAX_UNTERNEHMEN: usize = 2000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportUnternehmen {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportWorkspace {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub unternehmen: Vec<ImportUnternehmen>,
}

/// Zuletzt von Atlas gemeldete Liste (`None` = noch nie gemeldet).
#[derive(Default)]
pub struct Import {
    liste: Mutex<Option<Vec<ImportWorkspace>>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Eintrag {
    workspace_id: String,
    #[serde(default)]
    company_id: Option<String>,
    /// Name in Atlas, aus dem der Ordnername entstand (für Umbenennungen).
    #[serde(default)]
    name: String,
    #[serde(default = "wahr")]
    aktiv: bool,
}

fn wahr() -> bool {
    true
}

/// Inhalt von `.atlas-zuordnung.json`: relativer Ordner („A/B“) → Ziel.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Zuordnung {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    ordner: BTreeMap<String, Eintrag>,
}

/// Name des Unterordners für erledigte Dateien (wie beim überwachten Ordner).
pub fn hochgeladen() -> &'static str {
    t("Hochgeladen", "Uploaded")
}

/// Ordner, die beim Durchsuchen und bei der Namenswahl nie zählen.
pub fn ist_reserviert(name: &str) -> bool {
    let n = name.to_lowercase();
    n == "hochgeladen" || n == "uploaded"
}

fn listen_datei(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("import-liste.json"))
}

/// Beim Start: zuletzt gemeldete Liste laden und Ordner abgleichen.
pub fn laden(app: &AppHandle) {
    let liste = listen_datei(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Vec<ImportWorkspace>>(&b).ok());
    *app.state::<Import>().liste.lock().unwrap() = liste;
    abgleichen(app);
}

/// Von der Atlas-Seite nach der Anmeldung: Workspaces (mit Unternehmen), in
/// deren Eingang die Person schreiben darf.
#[tauri::command]
pub async fn import_ordner_setzen(app: AppHandle, workspaces: Vec<ImportWorkspace>) -> Result<(), String> {
    let mut liste: Vec<ImportWorkspace> = workspaces
        .into_iter()
        .filter(|w| gueltige_id(&w.id))
        .take(MAX_WORKSPACES)
        .map(|mut w| {
            w.name = w.name.chars().take(200).collect();
            w.unternehmen.retain(|u| gueltige_id(&u.id));
            w.unternehmen.truncate(MAX_UNTERNEHMEN);
            for u in &mut w.unternehmen {
                u.name = u.name.chars().take(200).collect();
            }
            w
        })
        .collect();
    // Doppelte IDs nur einmal.
    let mut gesehen = std::collections::HashSet::new();
    liste.retain(|w| gesehen.insert(w.id.clone()));
    if let Some(p) = listen_datei(&app) {
        let _ = p.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(p, serde_json::to_vec(&liste).unwrap_or_default());
    }
    *app.state::<Import>().liste.lock().unwrap() = Some(liste);
    abgleichen(&app);
    Ok(())
}

fn gueltige_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 100 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Name für das Dateisystem: ohne Pfadtrenner, Steuer- und Sonderzeichen,
/// ohne führende Punkte und ohne Punkte/Leerzeichen am Ende (Windows).
fn sauber(name: &str, ersatz: &str) -> String {
    let n: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':') { '-' } else { c })
        .filter(|c| !c.is_control() && !matches!(c, '*' | '?' | '"' | '<' | '>' | '|'))
        .take(80)
        .collect();
    let n = n.trim().trim_start_matches('.').trim_end_matches(['.', ' ']).trim().to_string();
    if n.is_empty() {
        ersatz.to_string()
    } else if crate::downloads::windows_reserviert(&n) {
        format!("_{n}")
    } else {
        n
    }
}

/// Schlüssel aus `.atlas-zuordnung.json` (liegt im Import-Ordner und kann von
/// Hand oder über einen Sync-Dienst verändert werden): nur „A“ oder „A/B“ mit
/// einfachen Namen — kein „..“, kein absoluter Pfad, kein Laufwerk.
fn gueltiger_rel(rel: &str) -> bool {
    let teile: Vec<&str> = rel.split('/').collect();
    (1..=2).contains(&teile.len())
        && teile.iter().all(|t| {
            !t.is_empty() && !t.starts_with('.') && !t.contains(['\\', ':']) && !t.chars().any(char::is_control)
        })
}

fn abs(basis: &Path, rel: &str) -> PathBuf {
    let mut p = basis.to_path_buf();
    for teil in rel.split('/') {
        p.push(teil);
    }
    p
}

fn rel_von(eltern: &str, name: &str) -> String {
    if eltern.is_empty() { name.to_string() } else { format!("{eltern}/{name}") }
}

fn eltern_von(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(e, _)| e).unwrap_or("")
}

fn blatt_von(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(_, b)| b).unwrap_or(rel)
}

fn zuordnung_laden(basis: &Path) -> Zuordnung {
    let mut z: Zuordnung = std::fs::read(basis.join(ZUORDNUNG))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    // Ungültige Einträge verwerfen — sonst könnte ein Eintrag wie „..“ oder
    // (Windows) „C:\Users\…“ einen Ordner außerhalb umbenennen.
    z.ordner.retain(|k, e| gueltiger_rel(k) && gueltige_id(&e.workspace_id) && e.company_id.as_deref().is_none_or(gueltige_id));
    z
}

fn zuordnung_speichern(basis: &Path, z: &Zuordnung) {
    if let Ok(s) = serde_json::to_vec_pretty(z) {
        let _ = std::fs::write(basis.join(ZUORDNUNG), s);
    }
}

fn finde(z: &Zuordnung, ws: &str, co: Option<&str>) -> Option<String> {
    z.ordner
        .iter()
        .find(|(_, e)| e.workspace_id == ws && e.company_id.as_deref() == co)
        .map(|(k, _)| k.clone())
}

fn vergeben(z: &Zuordnung, rel: &str) -> bool {
    let klein = rel.to_lowercase();
    z.ordner.keys().any(|k| k.to_lowercase() == klein)
}

/// Freier Ordnername „X“, „X (2)“, … unter `eltern`. `uebernehmen`: ein schon
/// vorhandener, noch keinem Ziel zugeordneter Ordner gleichen Namens zählt als frei.
fn freier_name(basis: &Path, z: &Zuordnung, eltern: &str, gewuenscht: &str, uebernehmen: bool) -> String {
    for i in 1..1000 {
        let kandidat = if i == 1 { gewuenscht.to_string() } else { format!("{gewuenscht} ({i})") };
        let rel = rel_von(eltern, &kandidat);
        if ist_reserviert(&kandidat) || vergeben(z, &rel) {
            continue;
        }
        let p = abs(basis, &rel);
        if !p.exists() || (uebernehmen && p.is_dir()) {
            return kandidat;
        }
    }
    format!("{gewuenscht} ({})", std::process::id())
}

/// Ordner für ein Ziel sicherstellen (anlegen bzw. umbenennen) und den
/// relativen Pfad zurückgeben.
fn ordner_sichern(basis: &Path, z: &mut Zuordnung, eltern: &str, ws: &str, co: Option<&str>, name: &str) -> Option<String> {
    let ersatz = if co.is_some() { t("Unternehmen", "Company") } else { "Workspace" };
    let gewuenscht = sauber(name, ersatz);
    if let Some(alt) = finde(z, ws, co) {
        let alt_abs = abs(basis, &alt);
        if alt_abs.is_dir() && eltern_von(&alt) == eltern {
            let mut eintrag = z.ordner.remove(&alt)?;
            let mut neu = alt.clone();
            let mut umbenannt = true;
            let alt_blatt = blatt_von(&alt).to_string();
            if eintrag.name != name && alt_blatt != gewuenscht {
                let ziel_name = if alt_blatt.to_lowercase() == gewuenscht.to_lowercase() {
                    // Nur Groß-/Kleinschreibung geändert (macOS: gleicher Ordner).
                    gewuenscht.clone()
                } else {
                    freier_name(basis, z, eltern, &gewuenscht, false)
                };
                let kandidat = rel_von(eltern, &ziel_name);
                match std::fs::rename(&alt_abs, abs(basis, &kandidat)) {
                    Ok(()) => neu = kandidat,
                    // Alten Namen behalten, damit es beim nächsten Abgleich erneut versucht wird.
                    Err(e) => {
                        log::warn!("Import-Ordner nicht umbenannt: {e}");
                        umbenannt = false;
                    }
                }
            }
            if neu != alt {
                // Unterordner (Unternehmen) wandern mit.
                let praefix = format!("{alt}/");
                let betroffen: Vec<String> = z.ordner.keys().filter(|k| k.starts_with(&praefix)).cloned().collect();
                for k in betroffen {
                    if let Some(e) = z.ordner.remove(&k) {
                        z.ordner.insert(format!("{neu}/{}", &k[praefix.len()..]), e);
                    }
                }
            }
            if umbenannt {
                eintrag.name = name.to_string();
            }
            eintrag.aktiv = true;
            z.ordner.insert(neu.clone(), eintrag);
            let _ = std::fs::create_dir_all(abs(basis, &neu).join(hochgeladen()));
            return Some(neu);
        }
        // Ordner verschwunden (von Hand gelöscht/verschoben): neu anlegen.
        z.ordner.remove(&alt);
    }
    let frei = freier_name(basis, z, eltern, &gewuenscht, true);
    let rel = rel_von(eltern, &frei);
    let p = abs(basis, &rel);
    if let Err(e) = std::fs::create_dir_all(p.join(hochgeladen())) {
        log::warn!("Import-Ordner nicht angelegt: {e}");
        return None;
    }
    z.ordner.insert(
        rel.clone(),
        Eintrag { workspace_id: ws.into(), company_id: co.map(Into::into), name: name.into(), aktiv: true },
    );
    Some(rel)
}

/// Ordnerstruktur an die zuletzt gemeldete Liste und die Einstellungen anpassen.
pub fn abgleichen(app: &AppHandle) {
    let e = einstellungen::aktuell(app);
    let Some(basis) = e.import_ordner.map(PathBuf::from) else { return };
    if std::fs::create_dir_all(basis.join(hochgeladen())).is_err() {
        log::warn!("Import-Ordner nicht verfügbar");
        return;
    }
    let Some(liste) = app.state::<Import>().liste.lock().unwrap().clone() else { return };
    let mut z = zuordnung_laden(&basis);
    z.version = 1;
    for eintrag in z.ordner.values_mut() {
        eintrag.aktiv = false;
    }
    for ws in &liste {
        let Some(ws_rel) = ordner_sichern(&basis, &mut z, "", &ws.id, None, &ws.name) else { continue };
        for u in &ws.unternehmen {
            if e.import_je_unternehmen {
                ordner_sichern(&basis, &mut z, &ws_rel, &ws.id, Some(&u.id), &u.name);
            } else if let Some(k) = finde(&z, &ws.id, Some(&u.id)) {
                // Schon angelegte Unternehmensordner bleiben gültig (Ziel: Workspace).
                if let Some(eintrag) = z.ordner.get_mut(&k) {
                    eintrag.aktiv = true;
                }
            }
        }
    }
    zuordnung_speichern(&basis, &z);
}

pub fn ziel_persoenlich() -> Value {
    json!({ "art": "persoenlich" })
}

fn ziel_workspace(ws: &str, co: Option<&str>) -> Value {
    json!({ "art": "workspace", "workspaceId": ws, "companyId": co })
}

fn unterordner(p: &Path) -> Vec<(String, PathBuf)> {
    let Ok(r) = std::fs::read_dir(p) else { return Vec::new() };
    r.filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            (!n.starts_with('.') && !ist_reserviert(&n)).then(|| (n, e.path()))
        })
        .collect()
}

/// Zu durchsuchende Ordner (zwei Ebenen) samt Ziel der Dateien darin.
pub fn ordner_mit_ziel(app: &AppHandle) -> Vec<(PathBuf, Value)> {
    let e = einstellungen::aktuell(app);
    let je_unternehmen = e.import_je_unternehmen;
    let Some(basis) = e.import_ordner.map(PathBuf::from) else { return Vec::new() };
    if !basis.is_dir() {
        return Vec::new();
    }
    let z = zuordnung_laden(&basis);
    let mut aus = vec![(basis.clone(), ziel_persoenlich())];
    for (n1, p1) in unterordner(&basis) {
        let ws1 = match z.ordner.get(&n1) {
            Some(e) if e.aktiv && e.company_id.is_none() => Some(e.workspace_id.clone()),
            _ => None,
        };
        let ziel1 = ws1.as_deref().map(|w| ziel_workspace(w, None)).unwrap_or_else(ziel_persoenlich);
        for (n2, p2) in unterordner(&p1) {
            let ziel2 = match (z.ordner.get(&rel_von(&n1, &n2)), ws1.as_deref()) {
                // Unternehmensordner; ohne „je Unternehmen“ in den Workspace-Eingang.
                (Some(e), Some(w)) if e.aktiv && e.workspace_id == w => {
                    ziel_workspace(w, e.company_id.as_deref().filter(|_| je_unternehmen))
                }
                // Inaktiv oder unstimmig: sicherheitshalber persönlich.
                (Some(_), _) => ziel_persoenlich(),
                // Eigener Unterordner der Person: wie der Ordner darüber.
                (None, _) => ziel1.clone(),
            };
            aus.push((p2, ziel2));
        }
        aus.push((p1, ziel1));
    }
    aus
}

// ---------------------------------------------------------------------------
// Anzeige im Einstellungsfenster
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnzeigeUnternehmen {
    name: String,
    ordner: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnzeigeWorkspace {
    name: String,
    ordner: Option<String>,
    unternehmen: Vec<AnzeigeUnternehmen>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Anzeige {
    /// Hat Atlas schon eine Liste gemeldet?
    bekannt: bool,
    workspaces: Vec<AnzeigeWorkspace>,
    /// Ordner, deren Workspace nicht mehr in der Liste steht.
    inaktiv: Vec<String>,
}

pub fn anzeige(app: &AppHandle) -> Anzeige {
    let liste = app.state::<Import>().liste.lock().unwrap().clone();
    let z = einstellungen::aktuell(app)
        .import_ordner
        .map(|b| zuordnung_laden(Path::new(&b)))
        .unwrap_or_default();
    let workspaces = liste
        .as_ref()
        .map(|l| {
            l.iter()
                .map(|w| AnzeigeWorkspace {
                    name: w.name.clone(),
                    ordner: finde(&z, &w.id, None),
                    unternehmen: w
                        .unternehmen
                        .iter()
                        .map(|u| AnzeigeUnternehmen { name: u.name.clone(), ordner: finde(&z, &w.id, Some(&u.id)) })
                        .collect(),
                })
                .collect()
        })
        .unwrap_or_default();
    let inaktiv = z.ordner.iter().filter(|(_, e)| !e.aktiv).map(|(k, _)| k.clone()).collect();
    Anzeige { bekannt: liste.is_some(), workspaces, inaktiv }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zuordnung_nur_einfache_pfade() {
        assert!(gueltiger_rel("Nordlicht Medien"));
        assert!(gueltiger_rel("Nordlicht Medien/Firma (2)"));
        assert!(!gueltiger_rel(".."));
        assert!(!gueltiger_rel("A/.."));
        assert!(!gueltiger_rel("/etc"));
        assert!(!gueltiger_rel(r"C:\Users\x"));
        assert!(!gueltiger_rel("A/B/C"));
        assert!(!gueltiger_rel(""));
    }

    #[test]
    fn reservierte_namen() {
        assert_eq!(sauber("CON", "W"), "_CON");
        assert_eq!(sauber("..", "W"), "W");
        assert_eq!(crate::downloads::sauberer_name("../../x/nul.txt"), "_nul.txt");
        assert_eq!(crate::downloads::sauberer_name("bericht.pdf. "), "bericht.pdf");
    }
}
