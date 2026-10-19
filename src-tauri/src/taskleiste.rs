//! Windows: Zahl als Overlay-Symbol auf dem Taskleisten-Eintrag (Windows kennt
//! keine Badge-Zahl wie das macOS-Dock). Symbole: `scripts/zahlen-erzeugen.py`.

use tauri::{image::Image, WebviewWindow};

const ZAHLEN: [&[u8]; 10] = [
    include_bytes!("../icons/zahl/1.png"),
    include_bytes!("../icons/zahl/2.png"),
    include_bytes!("../icons/zahl/3.png"),
    include_bytes!("../icons/zahl/4.png"),
    include_bytes!("../icons/zahl/5.png"),
    include_bytes!("../icons/zahl/6.png"),
    include_bytes!("../icons/zahl/7.png"),
    include_bytes!("../icons/zahl/8.png"),
    include_bytes!("../icons/zahl/9.png"),
    include_bytes!("../icons/zahl/plus.png"),
];

pub fn zahl_setzen(w: &WebviewWindow, anzahl: Option<i64>) {
    let symbol = anzahl.and_then(|n| {
        let i = (n.clamp(1, 10) - 1) as usize;
        Image::from_bytes(ZAHLEN[i]).ok()
    });
    if let Err(e) = w.set_overlay_icon(symbol) {
        log::warn!("Overlay-Symbol nicht gesetzt: {e}");
    }
}
