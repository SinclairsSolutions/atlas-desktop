// Unter Windows kein zusätzliches Konsolenfenster in Release-Builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    atlas_desktop_lib::run()
}
