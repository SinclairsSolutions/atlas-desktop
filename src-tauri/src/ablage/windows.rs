//! Windows: kommt mit der Cloud-Files-Anbindung (Phase 3). Bis dahin aus.

use super::Zugang;
use tauri::AppHandle;

pub fn eingebaut() -> bool {
    false
}
pub fn verbinden(_: &AppHandle, _: &Zugang) -> Result<(), String> {
    Err("Unter Windows noch nicht verfügbar.".into())
}
pub fn trennen(_: &AppHandle) -> Result<(), String> {
    Ok(())
}
pub fn anstossen(_: &AppHandle) {}
pub fn zeigen(_: &AppHandle) -> Result<(), String> {
    Err("Unter Windows noch nicht verfügbar.".into())
}
pub fn token_speichern(_: &str) -> Result<(), String> {
    Err("Unter Windows noch nicht verfügbar.".into())
}
pub fn token_lesen() -> Option<String> {
    None
}
pub fn token_loeschen() {}
pub fn verschluesselung() -> Option<bool> {
    None
}
