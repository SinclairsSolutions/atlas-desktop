fn main() {
    // Eigene Befehle bekommen Rechte („allow-…“), die nur die Capabilities in
    // `capabilities/` vergeben. Ohne Eintrag dort darf keine Seite sie aufrufen.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "mitteilung",
            "mitteilungen_erlauben",
            "kennzahl",
            "extern_oeffnen",
            "drucken",
            "verbinden",
            "datei_speichern",
        ]),
    ))
    .expect("tauri-build fehlgeschlagen");
}
