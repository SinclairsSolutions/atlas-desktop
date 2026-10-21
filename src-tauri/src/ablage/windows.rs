//! Windows: „Ablage im Explorer“ über die Cloud Files API (cfapi).
//!
//! Ordner `%USERPROFILE%\Atlas` wird als Sync-Root registriert. Darin legt die
//! App je Workspace einen Ordner und für jeden Ordner/jedes Dokument der
//! klassischen Ablage einen Platzhalter an. Dateien sind leer, bis sie
//! geöffnet werden (Callback FETCH_DATA lädt sie aus Atlas); Windows kann
//! geladene Kopien wieder freigeben („Speicherplatz freigeben“ /
//! „Immer auf diesem Gerät behalten“ im Explorer).
//!
//! Anders als unter macOS gibt es keinen eigenen Erweiterungsprozess: die
//! Platzhalter bedient die laufende App. Ist sie beendet, lassen sich noch
//! nicht geladene Dateien nicht öffnen (Windows meldet „Cloudanbieter läuft
//! nicht“); bereits geladene bleiben nutzbar.
//!
//! Abgleich (alle 30 s und auf Anstoß), ohne gespeicherten Zwischenstand —
//! jeder Platzhalter trägt seine Atlas-Kennung („Identität“):
//!   1. Lokal → Atlas: neue Dateien/Ordner hochladen bzw. anlegen und in
//!      Platzhalter umwandeln; geänderte Dateien (nicht mehr „in sync“) als
//!      neue Version hochladen (Konflikt → Atlas legt eine Kopie daneben).
//!   2. Atlas → lokal: fehlende Platzhalter anlegen, Umbenanntes/Verschobenes
//!      nachziehen, geänderte Inhalte verwerfen (neu laden beim Öffnen),
//!      Verschwundenes (auch entzogene Rechte) entfernen.
//!
//! Umbenennen/Verschieben/Löschen im Explorer kommen als Callback
//! (NOTIFY_RENAME / NOTIFY_DELETE) und gehen vorher an Atlas; lehnt Atlas ab,
//! lehnt Windows den Vorgang ab. Löschen = Atlas-Papierkorb.
//!
//! Nur in der Windows-CI gebaut; lokal nicht ausführbar.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use tauri::AppHandle;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HANDLE, NTSTATUS};
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};
use windows::Win32::Storage::CloudFilters::*;
use windows::Win32::Storage::FileSystem::FILE_BASIC_INFO;

use super::api::{self, Api, Fehler, ServerEintrag};
use super::Zugang;

const ANBIETER_ID: windows::core::GUID = windows::core::GUID::from_u128(0x6f1c_4b2e_8a37_4d0f_9c51_a7e2_3b19_d460);
const ANMELDE_ZIEL: &str = "SinclairAtlas/Ablage";
const STATUS_SUCCESS: NTSTATUS = NTSTATUS(0);
const STATUS_UNSUCCESSFUL: NTSTATUS = NTSTATUS(0xC000_0001_u32 as i32);
const STATUS_ACCESS_DENIED: NTSTATUS = NTSTATUS(0xC000_0022_u32 as i32);
const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
/// Übertragungsblock (Vielfaches von 4 KiB, wie von cfapi verlangt).
const BLOCK: usize = 1024 * 1024;

// ---------------------------------------------------------------------------
// Laufzeit
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Recht {
    schreiben: bool,
    loeschen: bool,
}

struct Laufzeit {
    wurzel: PathBuf,
    api: Arc<Api>,
    schluessel: CF_CONNECTION_KEY,
    rechte: Mutex<HashMap<String, Recht>>,
}

// SAFETY: CF_CONNECTION_KEY ist eine Zahl; der Rest ist Send/Sync.
unsafe impl Send for Laufzeit {}
unsafe impl Sync for Laufzeit {}

fn laufzeit() -> &'static Mutex<Option<Arc<Laufzeit>>> {
    static L: OnceLock<Mutex<Option<Arc<Laufzeit>>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(None))
}

fn aktuell() -> Option<Arc<Laufzeit>> {
    laufzeit().lock().ok().and_then(|g| g.clone())
}

/// Weckt den Abgleich-Faden (Anstoßen).
fn wecker() -> &'static (Mutex<bool>, Condvar) {
    static W: OnceLock<(Mutex<bool>, Condvar)> = OnceLock::new();
    W.get_or_init(|| (Mutex::new(false), Condvar::new()))
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}

fn wurzel_pfad() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Atlas"))
}

fn breit(p: &Path) -> HSTRING {
    HSTRING::from(p.as_os_str())
}

/// Noch nie auf einem echten Windows-Rechner gelaufen: der Schalter bleibt
/// verborgen („nicht enthalten“), bis die Explorer-Ablage dort getestet ist.
/// Zum Testen auf `true` setzen.
const FREIGEGEBEN: bool = false;

pub fn eingebaut() -> bool {
    FREIGEGEBEN
}

// ---------------------------------------------------------------------------
// Anmelden / Trennen
// ---------------------------------------------------------------------------

pub fn verbinden(_app: &AppHandle, z: &Zugang) -> Result<(), String> {
    let wurzel = wurzel_pfad().ok_or("Kein Benutzerordner")?;
    std::fs::create_dir_all(&wurzel).map_err(|e| format!("Ordner {}: {e}", wurzel.display()))?;
    if let Some(alt) = laufzeit().lock().unwrap().take() {
        unsafe {
            let _ = CfDisconnectSyncRoot(alt.schluessel);
        }
    }
    let pfad = breit(&wurzel);
    let name = HSTRING::from("Sinclair Atlas");
    let version = HSTRING::from(env!("CARGO_PKG_VERSION"));
    let identitaet = b"atlas-ablage";
    let registrierung = CF_SYNC_REGISTRATION {
        StructSize: std::mem::size_of::<CF_SYNC_REGISTRATION>() as u32,
        ProviderName: PCWSTR(name.as_ptr()),
        ProviderVersion: PCWSTR(version.as_ptr()),
        SyncRootIdentity: identitaet.as_ptr() as *const c_void,
        SyncRootIdentityLength: identitaet.len() as u32,
        FileIdentity: std::ptr::null(),
        FileIdentityLength: 0,
        ProviderId: ANBIETER_ID,
    };
    let regeln = CF_SYNC_POLICIES {
        StructSize: std::mem::size_of::<CF_SYNC_POLICIES>() as u32,
        Hydration: CF_HYDRATION_POLICY {
            Primary: CF_HYDRATION_POLICY_FULL,
            Modifier: CF_HYDRATION_POLICY_MODIFIER_AUTO_DEHYDRATION_ALLOWED,
        },
        Population: CF_POPULATION_POLICY {
            Primary: CF_POPULATION_POLICY_ALWAYS_FULL,
            Modifier: CF_POPULATION_POLICY_MODIFIER_NONE,
        },
        InSync: CF_INSYNC_POLICY_TRACK_ALL,
        HardLink: CF_HARDLINK_POLICY_NONE,
        PlaceholderManagement: CF_PLACEHOLDER_MANAGEMENT_POLICY_DEFAULT,
    };
    unsafe {
        CfRegisterSyncRoot(
            &pfad,
            &registrierung,
            &regeln,
            CF_REGISTER_FLAG_UPDATE
                | CF_REGISTER_FLAG_DISABLE_ON_DEMAND_POPULATION_ON_ROOT
                | CF_REGISTER_FLAG_MARK_IN_SYNC_ON_ROOT,
        )
        .map_err(|e| format!("Sync-Root nicht registriert: {e}"))?;
    }
    let tabelle = [
        CF_CALLBACK_REGISTRATION { Type: CF_CALLBACK_TYPE_FETCH_DATA, Callback: Some(cb_laden) },
        CF_CALLBACK_REGISTRATION { Type: CF_CALLBACK_TYPE_NOTIFY_DELETE, Callback: Some(cb_loeschen) },
        CF_CALLBACK_REGISTRATION { Type: CF_CALLBACK_TYPE_NOTIFY_RENAME, Callback: Some(cb_umbenennen) },
        CF_CALLBACK_REGISTRATION { Type: CF_CALLBACK_TYPE_NONE, Callback: None },
    ];
    let schluessel = unsafe {
        CfConnectSyncRoot(
            &pfad,
            tabelle.as_ptr(),
            None,
            CF_CONNECT_FLAG_REQUIRE_PROCESS_INFO | CF_CONNECT_FLAG_REQUIRE_FULL_FILE_PATH,
        )
        .map_err(|e| format!("Sync-Root nicht verbunden: {e}"))?
    };
    let lz = Arc::new(Laufzeit {
        wurzel,
        api: Arc::new(Api::neu(&z.basis, &z.token)),
        schluessel,
        rechte: Mutex::new(HashMap::new()),
    });
    *laufzeit().lock().unwrap() = Some(lz);
    faden_starten();
    anstossen(_app);
    Ok(())
}

pub fn trennen(_app: &AppHandle) -> Result<(), String> {
    let lz = laufzeit().lock().unwrap().take();
    let wurzel = lz.as_ref().map(|l| l.wurzel.clone()).or_else(wurzel_pfad);
    if let Some(l) = &lz {
        unsafe {
            let _ = CfDisconnectSyncRoot(l.schluessel);
        }
    }
    let Some(wurzel) = wurzel else { return Ok(()) };
    unsafe {
        let _ = CfUnregisterSyncRoot(&breit(&wurzel));
    }
    // Noch nicht hochgeladene eigene Dateien retten, dann alles entfernen.
    if wurzel.exists() {
        let rettung = wurzel.with_file_name("Atlas (nicht hochgeladen)");
        retten(&wurzel, &wurzel, &rettung);
        std::fs::remove_dir_all(&wurzel).map_err(|e| format!("Ordner nicht entfernt: {e}"))?;
    }
    Ok(())
}

/// Dateien ohne Atlas-Kennung oder mit ungespeicherten Änderungen in den
/// Rettungsordner verschieben (sonst gingen sie beim Trennen verloren).
fn retten(wurzel: &Path, ordner: &Path, rettung: &Path) {
    let Ok(it) = std::fs::read_dir(ordner) else { return };
    for e in it.flatten() {
        let p = e.path();
        let ist_ordner = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if ist_ordner {
            retten(wurzel, &p, rettung);
            continue;
        }
        let ungesichert = match platzhalter_info(&p) {
            None => true,
            Some(i) => !i.in_sync,
        };
        if ungesichert && !api::ist_hilfsdatei(&e.file_name().to_string_lossy()) {
            if let Ok(rel) = p.strip_prefix(wurzel) {
                let ziel = rettung.join(rel);
                let _ = ziel.parent().map(std::fs::create_dir_all);
                if std::fs::rename(&p, &ziel).is_err() {
                    let _ = std::fs::copy(&p, &ziel);
                }
            }
        }
    }
}

pub fn anstossen(_app: &AppHandle) {
    anstossen_intern();
}

fn anstossen_intern() {
    let (m, c) = wecker();
    if let Ok(mut g) = m.lock() {
        *g = true;
        c.notify_all();
    }
}

pub fn zeigen(_app: &AppHandle) -> Result<(), String> {
    let w = wurzel_pfad().ok_or("Ordner nicht gefunden.")?;
    std::process::Command::new("explorer.exe").arg(&w).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Gerätezugang in der Windows-Anmeldeinformationsverwaltung
// ---------------------------------------------------------------------------

fn breit_nul(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub fn token_speichern(token: &str) -> Result<(), String> {
    let mut ziel = breit_nul(ANMELDE_ZIEL);
    let mut nutzer = breit_nul("geraet");
    let mut daten = token.as_bytes().to_vec();
    let c = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(ziel.as_mut_ptr()),
        CredentialBlobSize: daten.len() as u32,
        CredentialBlob: daten.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(nutzer.as_mut_ptr()),
        ..Default::default()
    };
    unsafe { CredWriteW(&c, 0) }.map_err(|e| format!("Anmeldeinformationen: {e}"))
}

pub fn token_lesen() -> Option<String> {
    let ziel = breit_nul(ANMELDE_ZIEL);
    let mut p: *mut CREDENTIALW = std::ptr::null_mut();
    unsafe {
        CredReadW(PCWSTR(ziel.as_ptr()), CRED_TYPE_GENERIC, None, &mut p).ok()?;
        if p.is_null() {
            return None;
        }
        let c = &*p;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec();
        CredFree(p as *const c_void);
        String::from_utf8(bytes).ok()
    }
}

pub fn token_loeschen() {
    let ziel = breit_nul(ANMELDE_ZIEL);
    unsafe {
        let _ = CredDeleteW(PCWSTR(ziel.as_ptr()), CRED_TYPE_GENERIC, None);
    }
}

/// BitLocker auf Laufwerk C: an? (ohne Administratorrechte über die Shell-Eigenschaft)
pub fn verschluesselung() -> Option<bool> {
    let o = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(New-Object -ComObject Shell.Application).NameSpace('C:').Self.ExtendedProperty('System.Volume.BitLockerProtection')",
        ])
        .output()
        .ok()?;
    match String::from_utf8_lossy(&o.stdout).trim() {
        // 1 = an, 3 = wird verschlüsselt, 5 = an (gesperrt); 2 = aus, 6 = aus
        "1" | "3" | "5" => Some(true),
        "2" | "6" => Some(false),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Platzhalter: Kennung, Zustand, Anlegen
// ---------------------------------------------------------------------------

/// Kennung im Platzhalter: "w:<uuid>", "o:<uuid>" oder "d:<uuid>|<inhaltVersion>".
#[derive(Clone, Debug, PartialEq)]
struct Kennung {
    id: String,
    inhalt: Option<String>,
}

impl Kennung {
    fn lesen(roh: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(roh).ok()?;
        let (id, inhalt) = match s.split_once('|') {
            Some((a, b)) => (a.to_string(), Some(b.to_string())),
            None => (s.to_string(), None),
        };
        if id.len() < 3 || !matches!(&id[..2], "w:" | "o:" | "d:") {
            return None;
        }
        Some(Self { id, inhalt })
    }
    fn bytes(&self) -> Vec<u8> {
        match &self.inhalt {
            Some(i) => format!("{}|{}", self.id, i).into_bytes(),
            None => self.id.clone().into_bytes(),
        }
    }
}

struct PlatzhalterInfo {
    kennung: Option<Kennung>,
    in_sync: bool,
}

fn oeffnen(p: &Path, schreiben: bool) -> std::io::Result<std::fs::File> {
    // FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES | FILE_WRITE_DATA
    let zugriff = 0x0080 | if schreiben { 0x0100 | 0x0002 } else { 0 };
    std::fs::OpenOptions::new()
        .access_mode(zugriff)
        .share_mode(0x1 | 0x2 | 0x4)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(p)
}

fn handle(f: &std::fs::File) -> HANDLE {
    HANDLE(f.as_raw_handle())
}

/// None = kein Platzhalter (neue lokale Datei).
fn platzhalter_info(p: &Path) -> Option<PlatzhalterInfo> {
    let f = oeffnen(p, false).ok()?;
    let mut puffer = vec![0u8; std::mem::size_of::<CF_PLACEHOLDER_BASIC_INFO>() + 512];
    let mut laenge = 0u32;
    unsafe {
        CfGetPlaceholderInfo(
            handle(&f),
            CF_PLACEHOLDER_INFO_BASIC,
            puffer.as_mut_ptr() as *mut c_void,
            puffer.len() as u32,
            Some(&mut laenge),
        )
        .ok()?;
        let info = &*(puffer.as_ptr() as *const CF_PLACEHOLDER_BASIC_INFO);
        let start = std::mem::offset_of!(CF_PLACEHOLDER_BASIC_INFO, FileIdentity);
        let n = (info.FileIdentityLength as usize).min(puffer.len().saturating_sub(start));
        let kennung = Kennung::lesen(&puffer[start..start + n]);
        Some(PlatzhalterInfo { kennung, in_sync: info.InSyncState == CF_IN_SYNC_STATE_IN_SYNC })
    }
}

/// ISO-Zeit → FILETIME (100 ns seit 1601).
fn dateizeit(iso: &str) -> i64 {
    let n = |a: usize, b: usize| iso.get(a..b).and_then(|s| s.parse::<i64>().ok());
    let (Some(j), Some(mo), Some(t)) = (n(0, 4), n(5, 7), n(8, 10)) else { return jetzt_dateizeit() };
    let (h, mi, s) = (n(11, 13).unwrap_or(0), n(14, 16).unwrap_or(0), n(17, 19).unwrap_or(0));
    // Tage seit 1970 (days_from_civil nach Howard Hinnant).
    let y = if mo <= 2 { j - 1 } else { j };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + t - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let tage = era * 146_097 + doe - 719_468;
    let unix = tage * 86_400 + h * 3600 + mi * 60 + s;
    (unix + 11_644_473_600) * 10_000_000
}

fn jetzt_dateizeit() -> i64 {
    let s = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    (s + 11_644_473_600) * 10_000_000
}

fn metadaten(e: Option<&ServerEintrag>, ordner: bool, groesse: i64, nur_lesen: bool) -> CF_FS_METADATA {
    let erstellt = e.map(|e| dateizeit(&e.erstellt_am)).unwrap_or_else(jetzt_dateizeit);
    let geaendert = e.map(|e| dateizeit(&e.geaendert_am)).unwrap_or(erstellt);
    let attribute = if ordner {
        FILE_ATTRIBUTE_DIRECTORY
    } else if nur_lesen {
        FILE_ATTRIBUTE_READONLY
    } else {
        FILE_ATTRIBUTE_NORMAL
    };
    CF_FS_METADATA {
        BasicInfo: FILE_BASIC_INFO {
            CreationTime: erstellt,
            LastAccessTime: geaendert,
            LastWriteTime: geaendert,
            ChangeTime: geaendert,
            FileAttributes: attribute,
        },
        FileSize: if ordner { 0 } else { groesse },
    }
}

fn platzhalter_anlegen(ordner: &Path, name: &str, k: &Kennung, meta: CF_FS_METADATA, ist_ordner: bool) -> Result<(), String> {
    let name_w = HSTRING::from(name);
    let id = k.bytes();
    let mut flags = CF_PLACEHOLDER_CREATE_FLAG_MARK_IN_SYNC;
    if ist_ordner {
        flags |= CF_PLACEHOLDER_CREATE_FLAG_DISABLE_ON_DEMAND_POPULATION;
    }
    let mut info = [CF_PLACEHOLDER_CREATE_INFO {
        RelativeFileName: PCWSTR(name_w.as_ptr()),
        FsMetadata: meta,
        FileIdentity: id.as_ptr() as *const c_void,
        FileIdentityLength: id.len() as u32,
        Flags: flags,
        ..Default::default()
    }];
    unsafe { CfCreatePlaceholders(&breit(ordner), &mut info, CF_CREATE_FLAG_NONE, None) }
        .map_err(|e| format!("Platzhalter {name}: {e}"))?;
    info[0].Result.ok().map_err(|e| format!("Platzhalter {name}: {e}"))
}

fn umwandeln(p: &Path, k: &Kennung) -> Result<(), String> {
    let f = oeffnen(p, true).map_err(|e| e.to_string())?;
    let id = k.bytes();
    unsafe {
        CfConvertToPlaceholder(handle(&f), Some(id.as_ptr() as *const c_void), id.len() as u32, CF_CONVERT_FLAG_MARK_IN_SYNC, None, None)
    }
    .map_err(|e| format!("Umwandeln {}: {e}", p.display()))
}

/// Neue Kennung/Größe setzen; `verwerfen`: lokalen Inhalt freigeben (beim
/// nächsten Öffnen neu aus Atlas laden).
fn aktualisieren(p: &Path, k: &Kennung, meta: &CF_FS_METADATA, verwerfen: bool) -> Result<(), String> {
    let f = oeffnen(p, true).map_err(|e| e.to_string())?;
    let id = k.bytes();
    let mut flags = CF_UPDATE_FLAG_MARK_IN_SYNC;
    if verwerfen {
        flags |= CF_UPDATE_FLAG_DEHYDRATE;
    }
    unsafe {
        CfUpdatePlaceholder(handle(&f), Some(meta), Some(id.as_ptr() as *const c_void), id.len() as u32, None, flags, None, None)
    }
    .map_err(|e| format!("Aktualisieren {}: {e}", p.display()))
}

// ---------------------------------------------------------------------------
// Abgleich
// ---------------------------------------------------------------------------

fn faden_starten() {
    static LAEUFT: OnceLock<()> = OnceLock::new();
    if LAEUFT.set(()).is_err() {
        return;
    }
    std::thread::spawn(|| loop {
        if let Some(lz) = aktuell() {
            let ergebnis = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| abgleichen(&lz)));
            match ergebnis {
                Ok(Err(e)) => log::warn!("Ablage (Windows): Abgleich: {e}"),
                Err(_) => log::error!("Ablage (Windows): Abgleich abgebrochen (Panik)"),
                Ok(Ok(())) => {}
            }
        }
        let (m, c) = wecker();
        let g = m.lock().unwrap();
        let (mut g, _) = c.wait_timeout_while(g, Duration::from_secs(30), |geweckt| !*geweckt).unwrap();
        *g = false;
    });
}

/// Lokaler Bestand: Kennung → (Pfad, Platzhalterzustand, Ordner?); dazu neue Einträge.
struct Lokal {
    bekannt: HashMap<String, (PathBuf, PlatzhalterInfo, bool)>,
    neu: Vec<(PathBuf, bool)>,
}

fn einlesen(wurzel: &Path) -> Lokal {
    fn lauf(p: &Path, l: &mut Lokal, tiefe: usize) {
        let Ok(it) = std::fs::read_dir(p) else { return };
        for e in it.flatten() {
            let pfad = e.path();
            let ist_ordner = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            match platzhalter_info(&pfad) {
                Some(info) if info.kennung.is_some() => {
                    let id = info.kennung.as_ref().map(|k| k.id.clone()).unwrap_or_default();
                    l.bekannt.insert(id, (pfad.clone(), info, ist_ordner));
                    if ist_ordner && tiefe < 64 {
                        lauf(&pfad, l, tiefe + 1);
                    }
                }
                _ => l.neu.push((pfad.clone(), ist_ordner)),
            }
        }
    }
    let mut l = Lokal { bekannt: HashMap::new(), neu: Vec::new() };
    lauf(wurzel, &mut l, 0);
    l
}

/// Wohin gehört etwas, das unter `ordner` liegt? (Workspace-ID, Atlas-Ordner-ID)
fn ziel_von(ordner: &Path, wurzel: &Path) -> Option<(String, Option<String>)> {
    if ordner == wurzel {
        return None;
    }
    let k = platzhalter_info(ordner)?.kennung?;
    if let Some(ws) = k.id.strip_prefix("w:") {
        return Some((ws.to_string(), None));
    }
    let o = k.id.strip_prefix("o:")?.to_string();
    // Workspace = oberster Ordner unter der Wurzel.
    let rel = ordner.strip_prefix(wurzel).ok()?;
    let erster = wurzel.join(rel.components().next()?);
    let ws = platzhalter_info(&erster)?.kennung?.id.strip_prefix("w:")?.to_string();
    Some((ws, Some(o)))
}

fn mime_von(name: &str) -> &'static str {
    match name.rsplit('.').next().map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("txt") => "text/plain",
        Some("csv") => "text/csv",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Some("xml") => "application/xml",
        Some("zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

type Soll = HashMap<String, (Option<String>, String, Option<ServerEintrag>, String)>; // id → (Eltern-ID, Name, Eintrag, Workspace)

fn soll_pfad(id: &str, soll: &Soll, wurzel: &Path, tiefe: usize) -> Option<PathBuf> {
    if tiefe > 64 {
        return None;
    }
    let (eltern, name, _, _) = soll.get(id)?;
    match eltern {
        None => Some(wurzel.join(name)),
        Some(e) => Some(soll_pfad(e, soll, wurzel, tiefe + 1)?.join(name)),
    }
}

fn entfernen(p: &Path) {
    let _ = if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
}

fn abgleichen(lz: &Laufzeit) -> Result<(), String> {
    let api = lz.api.clone();
    // ---- Atlas erreichbar, Zugang gültig? ----
    let workspaces = match block_on(api.workspaces()) {
        Ok(w) => w,
        Err(Fehler::Widerrufen) => {
            log::info!("Ablage (Windows): Zugang widerrufen — Ordner wird geleert");
            let lokal = einlesen(&lz.wurzel);
            let mut pfade: Vec<_> = lokal.bekannt.values().map(|(p, _, _)| p.clone()).collect();
            pfade.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
            for p in pfade {
                entfernen(&p);
            }
            token_loeschen();
            return Ok(());
        }
        Err(e) => return Err(e.to_string()),
    };

    // ---- 1. Lokal → Atlas ----
    let lokal = einlesen(&lz.wurzel);
    let mut neu = lokal.neu.clone();
    // Ordner vor Dateien, flache vor tiefen.
    neu.sort_by_key(|(p, ordner)| (p.components().count(), !*ordner));
    for (pfad, ist_ordner) in neu {
        let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if api::ist_hilfsdatei(&name) {
            continue;
        }
        let Some(eltern) = pfad.parent() else { continue };
        let Some((ws, ordner)) = ziel_von(eltern, &lz.wurzel) else { continue };
        if ist_ordner {
            match block_on(api.ordner(&ws, ordner.as_deref(), &name)) {
                Ok(e) => {
                    let _ = umwandeln(&pfad, &Kennung { id: format!("o:{}", e.id), inhalt: None });
                }
                Err(e) => log::warn!("Ablage: Ordner {name}: {e}"),
            }
        } else {
            // Noch in Arbeit? (zuletzt vor weniger als 5 s geändert)
            let frisch = std::fs::metadata(&pfad)
                .and_then(|m| m.modified())
                .map(|t| t.elapsed().unwrap_or_default() < Duration::from_secs(5))
                .unwrap_or(true);
            if frisch {
                continue;
            }
            let Ok(inhalt) = std::fs::read(&pfad) else { continue };
            match block_on(api.hochladen(&ws, ordner.as_deref(), &name, mime_von(&name), inhalt)) {
                Ok(e) => {
                    let _ = umwandeln(&pfad, &Kennung { id: format!("d:{}", e.id), inhalt: Some(e.inhalt_version) });
                }
                Err(e) => log::warn!("Ablage: Hochladen {name}: {e}"),
            }
        }
    }
    // Geänderte Dateien → neue Version.
    for (id, (pfad, info, ist_ordner)) in &lokal.bekannt {
        if *ist_ordner || info.in_sync || !id.starts_with("d:") {
            continue;
        }
        let Some(basis) = info.kennung.as_ref().and_then(|k| k.inhalt.clone()) else { continue };
        let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let Ok(inhalt) = std::fs::read(pfad) else { continue };
        match block_on(api.neue_version(&id[2..], &basis, mime_von(&name), inhalt)) {
            Ok(r) => {
                let k = Kennung { id: id.clone(), inhalt: Some(r.eintrag.inhalt_version.clone()) };
                let meta = metadaten(Some(&r.eintrag), false, r.eintrag.groesse.unwrap_or(0), false);
                // Bei Konflikt bleibt die Atlas-Fassung: lokalen Inhalt verwerfen; die
                // Konfliktkopie kommt unten als neuer Platzhalter.
                let _ = aktualisieren(pfad, &k, &meta, r.konflikt.is_some());
            }
            Err(Fehler::KeinRecht(_)) | Err(Fehler::NichtGefunden) => {
                // Abgelehnt: Atlas-Fassung gilt.
                let k = Kennung { id: id.clone(), inhalt: Some(basis.clone()) };
                let meta = metadaten(None, false, std::fs::metadata(pfad).map(|m| m.len() as i64).unwrap_or(0), true);
                let _ = aktualisieren(pfad, &k, &meta, true);
            }
            Err(e) => log::warn!("Ablage: Version {name}: {e}"),
        }
    }

    // ---- 2. Atlas → lokal ----
    let mut soll: Soll = HashMap::new();
    let mut vollstaendig: HashSet<String> = HashSet::new();
    let mut rechte = HashMap::new();
    // Alles in einer Anfrage (Server lädt parallel); ältere Server: je Workspace.
    let alles = block_on(api.alles()).ok().flatten();
    for w in &workspaces {
        let wid = format!("w:{}", w.id);
        soll.insert(wid.clone(), (None, w.name.clone(), None, w.id.clone()));
        rechte.insert(w.id.clone(), Recht { schreiben: w.schreiben, loeschen: w.loeschen });
        let ergebnis = match alles.as_ref().and_then(|a| a.listen.get(&w.id)) {
            Some(api::ListeOderFehler::Liste(_)) | Some(api::ListeOderFehler::Fehler { .. }) => None,
            None => Some(block_on(api.liste(&w.id))),
        };
        let liste = match (alles.as_ref().and_then(|a| a.listen.get(&w.id)), ergebnis) {
            (Some(api::ListeOderFehler::Liste(l)), _) => Some(Ok(l)),
            (Some(api::ListeOderFehler::Fehler { fehler }), _) if fehler == "kein_recht" => Some(Err(true)),
            (Some(api::ListeOderFehler::Fehler { .. }), _) => Some(Err(false)),
            (None, Some(Ok(l))) => {
                // Eigene Liste aus dem Einzelabruf: in den Soll-Stand übernehmen.
                vollstaendig.insert(w.id.clone());
                rechte.insert(w.id.clone(), Recht { schreiben: l.schreiben, loeschen: l.loeschen });
                for e in l.eintraege {
                    let id = format!("{}:{}", if e.art == "ordner" { "o" } else { "d" }, e.id);
                    let eltern = e.parent_id.as_ref().map(|p| format!("o:{p}")).unwrap_or_else(|| wid.clone());
                    soll.insert(id, (Some(eltern), e.name.clone(), Some(e), w.id.clone()));
                }
                None
            }
            (None, Some(Err(Fehler::KeinRecht(_)))) => Some(Err(true)),
            (None, Some(Err(e))) => {
                log::warn!("Ablage: Liste {}: {e}", w.name);
                Some(Err(false))
            }
            (None, None) => None,
        };
        match liste {
            Some(Ok(l)) => {
                vollstaendig.insert(w.id.clone());
                rechte.insert(w.id.clone(), Recht { schreiben: l.schreiben, loeschen: l.loeschen });
                for e in &l.eintraege {
                    let id = format!("{}:{}", if e.art == "ordner" { "o" } else { "d" }, e.id);
                    let eltern = e.parent_id.as_ref().map(|p| format!("o:{p}")).unwrap_or_else(|| wid.clone());
                    soll.insert(id, (Some(eltern), e.name.clone(), Some(e.clone()), w.id.clone()));
                }
            }
            Some(Err(true)) => {
                vollstaendig.insert(w.id.clone());
            }
            _ => {}
        }
    }
    *lz.rechte.lock().unwrap() = rechte.clone();

    let mut lokal = einlesen(&lz.wurzel);
    let mut ids: Vec<String> = soll.keys().cloned().collect();
    ids.sort_by_key(|id| soll_pfad(id, &soll, &lz.wurzel, 0).map(|p| p.components().count()).unwrap_or(usize::MAX));
    for id in &ids {
        let Some(ziel) = soll_pfad(id, &soll, &lz.wurzel, 0) else { continue };
        let (_, name, eintrag, ws) = &soll[id];
        let ist_ordner = !id.starts_with("d:");
        let recht = rechte.get(ws).copied().unwrap_or(Recht { schreiben: false, loeschen: false });
        let vorhanden = lokal.bekannt.get(id).map(|(p, i, _)| (p.clone(), i.in_sync, i.kennung.clone()));
        match vorhanden {
            Some((pfad, in_sync, kennung)) => {
                if pfad != ziel && !ziel.exists() {
                    if let Some(p) = ziel.parent() {
                        let _ = std::fs::create_dir_all(p);
                    }
                    if std::fs::rename(&pfad, &ziel).is_ok() && ist_ordner {
                        // Pfade darunter nachziehen.
                        for (p, _, _) in lokal.bekannt.values_mut() {
                            if let Ok(rest) = p.strip_prefix(&pfad) {
                                *p = ziel.join(rest);
                            }
                        }
                    }
                }
                if let Some(e) = eintrag {
                    let stand = kennung.as_ref().and_then(|k| k.inhalt.clone());
                    if !ist_ordner && in_sync && stand.as_deref() != Some(e.inhalt_version.as_str()) {
                        let groesse = e.groesse.filter(|g| *g > 0).or_else(|| block_on(api.groesse(&e.id)).ok().flatten()).unwrap_or(0);
                        let k = Kennung { id: id.clone(), inhalt: Some(e.inhalt_version.clone()) };
                        let nur_lesen = !recht.schreiben || e.nur_lesen == Some(true);
                        let _ = aktualisieren(&ziel, &k, &metadaten(Some(e), false, groesse, nur_lesen), true);
                    }
                }
            }
            None => {
                if ziel.exists() {
                    continue; // Name belegt (z. B. lokale Datei gleichen Namens) — nächste Runde.
                }
                let Some(ordner) = ziel.parent() else { continue };
                if !ordner.exists() {
                    continue;
                }
                let k = Kennung {
                    id: id.clone(),
                    inhalt: eintrag.as_ref().filter(|_| !ist_ordner).map(|e| e.inhalt_version.clone()),
                };
                let groesse = match eintrag {
                    Some(e) if !ist_ordner => e.groesse.filter(|g| *g > 0).or_else(|| block_on(api.groesse(&e.id)).ok().flatten()).unwrap_or(0),
                    _ => 0,
                };
                let nur_lesen = !recht.schreiben || eintrag.as_ref().and_then(|e| e.nur_lesen) == Some(true);
                let meta = metadaten(eintrag.as_ref(), ist_ordner, groesse, !ist_ordner && nur_lesen);
                if let Err(e) = platzhalter_anlegen(ordner, name, &k, meta, ist_ordner) {
                    log::warn!("Ablage: {e}");
                }
            }
        }
    }

    // Verschwundenes entfernen (nur wo die Liste vollständig war), tiefe zuerst.
    let lokal = einlesen(&lz.wurzel);
    let mut weg: Vec<PathBuf> = Vec::new();
    for (id, (pfad, info, _)) in &lokal.bekannt {
        if soll.contains_key(id) {
            continue;
        }
        let darf_weg = if id.starts_with("w:") {
            true
        } else {
            ziel_von(pfad.parent().unwrap_or(&lz.wurzel), &lz.wurzel)
                .map(|(ws, _)| vollstaendig.contains(&ws) || !rechte.contains_key(&ws))
                .unwrap_or(true)
        };
        if darf_weg {
            if !info.in_sync && !pfad.is_dir() {
                // Ungespeicherte Änderung retten statt verwerfen.
                let rettung = lz.wurzel.with_file_name("Atlas (nicht hochgeladen)");
                let _ = std::fs::create_dir_all(&rettung);
                let _ = std::fs::copy(pfad, rettung.join(pfad.file_name().unwrap_or_default()));
            }
            weg.push(pfad.clone());
        }
    }
    weg.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for p in weg {
        entfernen(&p);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Callbacks (laufen in Fäden von Windows — nie eine Panik durchlassen)
// ---------------------------------------------------------------------------

unsafe fn voller_pfad(info: &CF_CALLBACK_INFO, normalisiert: PCWSTR) -> PathBuf {
    let laufwerk = info.VolumeDosName.to_string().unwrap_or_default();
    let rest = normalisiert.to_string().unwrap_or_default();
    PathBuf::from(format!("{laufwerk}{rest}"))
}

unsafe fn kennung_von(info: &CF_CALLBACK_INFO) -> Option<Kennung> {
    if info.FileIdentity.is_null() || info.FileIdentityLength == 0 {
        return None;
    }
    Kennung::lesen(std::slice::from_raw_parts(info.FileIdentity as *const u8, info.FileIdentityLength as usize))
}

unsafe fn vom_eigenen_prozess(info: &CF_CALLBACK_INFO) -> bool {
    !info.ProcessInfo.is_null() && (*info.ProcessInfo).ProcessId == std::process::id()
}

fn op_info(info: &CF_CALLBACK_INFO, typ: CF_OPERATION_TYPE) -> CF_OPERATION_INFO {
    CF_OPERATION_INFO {
        StructSize: std::mem::size_of::<CF_OPERATION_INFO>() as u32,
        Type: typ,
        ConnectionKey: info.ConnectionKey,
        TransferKey: info.TransferKey,
        CorrelationVector: info.CorrelationVector,
        SyncStatus: std::ptr::null(),
        RequestKey: info.RequestKey,
    }
}

unsafe fn uebertragen(info: &CF_CALLBACK_INFO, status: NTSTATUS, puffer: &[u8], offset: i64, laenge: i64) -> bool {
    let oi = op_info(info, CF_OPERATION_TYPE_TRANSFER_DATA);
    let mut p = CF_OPERATION_PARAMETERS {
        ParamSize: (std::mem::offset_of!(CF_OPERATION_PARAMETERS, Anonymous) + std::mem::size_of::<CF_OPERATION_PARAMETERS_0_0>()) as u32,
        ..Default::default()
    };
    p.Anonymous.TransferData = CF_OPERATION_PARAMETERS_0_0 {
        Flags: CF_OPERATION_TRANSFER_DATA_FLAG_NONE,
        CompletionStatus: status,
        Buffer: if puffer.is_empty() { std::ptr::null() } else { puffer.as_ptr() as *const c_void },
        Offset: offset,
        Length: laenge,
    };
    CfExecute(&oi, &mut p).is_ok()
}

unsafe fn quittieren(info: &CF_CALLBACK_INFO, loeschen: bool, status: NTSTATUS) {
    let mut p = CF_OPERATION_PARAMETERS::default();
    let kopf = std::mem::offset_of!(CF_OPERATION_PARAMETERS, Anonymous);
    let oi = if loeschen {
        p.ParamSize = (kopf + std::mem::size_of::<CF_OPERATION_PARAMETERS_0_7>()) as u32;
        p.Anonymous.AckDelete = CF_OPERATION_PARAMETERS_0_7 { Flags: CF_OPERATION_ACK_DELETE_FLAG_NONE, CompletionStatus: status };
        op_info(info, CF_OPERATION_TYPE_ACK_DELETE)
    } else {
        p.ParamSize = (kopf + std::mem::size_of::<CF_OPERATION_PARAMETERS_0_6>()) as u32;
        p.Anonymous.AckRename = CF_OPERATION_PARAMETERS_0_6 { Flags: CF_OPERATION_ACK_RENAME_FLAG_NONE, CompletionStatus: status };
        op_info(info, CF_OPERATION_TYPE_ACK_RENAME)
    };
    if let Err(e) = CfExecute(&oi, &mut p) {
        log::warn!("Ablage: Quittung fehlgeschlagen: {e}");
    }
}

/// Datei öffnen → aus Atlas laden und in Blöcken übergeben.
unsafe extern "system" fn cb_laden(info: *const CF_CALLBACK_INFO, params: *const CF_CALLBACK_PARAMETERS) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let info = &*info;
        let p = (*params).Anonymous.FetchData;
        let fehler = |info: &CF_CALLBACK_INFO| {
            uebertragen(info, STATUS_UNSUCCESSFUL, &[], p.RequiredFileOffset, p.RequiredLength);
        };
        let (Some(k), Some(lz)) = (kennung_von(info), aktuell()) else { return fehler(info) };
        let Some(dok) = k.id.strip_prefix("d:").map(str::to_string) else { return fehler(info) };
        let groesse = info.FileSize;
        let ok = block_on(async {
            let mut r = match lz.api.datei(&dok).await {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("Ablage: Laden {dok}: {e}");
                    return false;
                }
            };
            let mut puffer: Vec<u8> = Vec::with_capacity(BLOCK);
            let mut offset: i64 = 0;
            loop {
                match r.chunk().await {
                    Ok(Some(teil)) => {
                        puffer.extend_from_slice(&teil);
                        while puffer.len() >= BLOCK {
                            let rest = puffer.split_off(BLOCK);
                            if !uebertragen(info, STATUS_SUCCESS, &puffer, offset, BLOCK as i64) {
                                return false;
                            }
                            offset += BLOCK as i64;
                            puffer = rest;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => return false,
                }
            }
            if offset + puffer.len() as i64 != groesse {
                log::warn!("Ablage: Größe passt nicht ({} statt {groesse})", offset + puffer.len() as i64);
                return false;
            }
            puffer.is_empty() || uebertragen(info, STATUS_SUCCESS, &puffer, offset, puffer.len() as i64)
        });
        if !ok {
            fehler(info);
        }
    }));
}

/// Löschen im Explorer → Atlas-Papierkorb (nur mit Verwalten-Recht).
unsafe extern "system" fn cb_loeschen(info: *const CF_CALLBACK_INFO, _params: *const CF_CALLBACK_PARAMETERS) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let info = &*info;
        let status = if vom_eigenen_prozess(info) { STATUS_SUCCESS } else { loeschen_bei_atlas(info) };
        quittieren(info, true, status);
    }));
}

unsafe fn loeschen_bei_atlas(info: &CF_CALLBACK_INFO) -> NTSTATUS {
    let (Some(k), Some(lz)) = (kennung_von(info), aktuell()) else { return STATUS_SUCCESS };
    if k.id.starts_with("w:") {
        return STATUS_ACCESS_DENIED;
    }
    let pfad = voller_pfad(info, info.NormalizedPath);
    let ws = ziel_von(pfad.parent().unwrap_or(&lz.wurzel), &lz.wurzel).map(|(w, _)| w);
    let darf = ws.and_then(|w| lz.rechte.lock().ok()?.get(&w).copied()).map(|r| r.loeschen).unwrap_or(false);
    if !darf {
        return STATUS_ACCESS_DENIED;
    }
    match block_on(lz.api.loeschen(&k.id[2..])) {
        Ok(()) | Err(Fehler::NichtGefunden) => STATUS_SUCCESS,
        Err(e) => {
            log::warn!("Ablage: Löschen abgelehnt: {e}");
            STATUS_ACCESS_DENIED
        }
    }
}

/// Umbenennen/Verschieben im Explorer → Atlas (nur im selben Workspace).
unsafe extern "system" fn cb_umbenennen(info: *const CF_CALLBACK_INFO, params: *const CF_CALLBACK_PARAMETERS) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let info = &*info;
        let status = if vom_eigenen_prozess(info) {
            STATUS_SUCCESS
        } else {
            umbenennen_bei_atlas(info, (*params).Anonymous.Rename.TargetPath)
        };
        quittieren(info, false, status);
    }));
}

unsafe fn umbenennen_bei_atlas(info: &CF_CALLBACK_INFO, ziel_roh: PCWSTR) -> NTSTATUS {
    let (Some(k), Some(lz)) = (kennung_von(info), aktuell()) else { return STATUS_SUCCESS };
    if k.id.starts_with("w:") {
        return STATUS_ACCESS_DENIED;
    }
    let quelle = voller_pfad(info, info.NormalizedPath);
    let ziel = voller_pfad(info, ziel_roh);
    // Aus der Ablage hinaus (z. B. in den Papierkorb von Windows) = Löschen.
    if !ziel.starts_with(&lz.wurzel) {
        return loeschen_bei_atlas(info);
    }
    let (Some((ws_alt, ordner_alt)), Some((ws_neu, ordner_neu))) = (
        ziel_von(quelle.parent().unwrap_or(&lz.wurzel), &lz.wurzel),
        ziel_von(ziel.parent().unwrap_or(&lz.wurzel), &lz.wurzel),
    ) else {
        return STATUS_ACCESS_DENIED;
    };
    if ws_alt != ws_neu {
        return STATUS_ACCESS_DENIED; // zwischen Workspaces nur in Atlas
    }
    let darf = lz.rechte.lock().ok().and_then(|r| r.get(&ws_neu).copied()).map(|r| r.schreiben).unwrap_or(false);
    if !darf {
        return STATUS_ACCESS_DENIED;
    }
    let name_alt = quelle.file_name().map(|n| n.to_string_lossy().into_owned());
    let name_neu = ziel.file_name().map(|n| n.to_string_lossy().into_owned());
    let name = if name_neu != name_alt { name_neu } else { None };
    let ordner = if ordner_alt != ordner_neu { Some(ordner_neu.as_deref()) } else { None };
    if name.is_none() && ordner.is_none() {
        return STATUS_SUCCESS;
    }
    match block_on(lz.api.aendern(&k.id[2..], name.as_deref(), ordner)) {
        Ok(_) => {
            anstossen_intern();
            STATUS_SUCCESS
        }
        Err(e) => {
            log::warn!("Ablage: Umbenennen abgelehnt: {e}");
            STATUS_ACCESS_DENIED
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kennung() {
        let k = Kennung::lesen(b"d:abc|2-ff").unwrap();
        assert_eq!(k.id, "d:abc");
        assert_eq!(k.inhalt.as_deref(), Some("2-ff"));
        assert_eq!(Kennung::lesen(&k.bytes()), Some(k));
        assert!(Kennung::lesen(b"x:1").is_none());
    }
    #[test]
    fn zeit() {
        assert_eq!(dateizeit("1970-01-01T00:00:00Z"), 116_444_736_000_000_000);
        assert_eq!(dateizeit("2026-10-21T00:00:00Z") - dateizeit("2026-10-20T00:00:00Z"), 864_000_000_000);
    }
}
