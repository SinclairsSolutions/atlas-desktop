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
            "dateien_erledigt",
            "touch_id_pruefen",
            "entsperren",
            "geraet_geheimnis",
            "einstellungen_lesen",
            "einstellungen_setzen",
            "autostart_setzen",
            "ordner_waehlen",
            "mitteilung_testen",
            "ordner_zeigen",
            "warteschlange_senden",
            "notiz_senden",
            "beleg_auswaehlen",
            "bildschirmfoto",
        ]),
    ))
    .expect("tauri-build fehlgeschlagen");
}
