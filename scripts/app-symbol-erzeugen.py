# Erzeugt die App-Symbole für macOS 26 (Tahoe) im Icon-Composer-Format:
#   src-tauri/icons/AppIcon.icon        schwarz mit weißem Zeichen (Standard)
#   src-tauri/icons/AppIconWeiss.icon   weiß mit schwarzem Zeichen (Einstellung „weiß“)
# aus dem flachen Atlas-Zeichen (sinclair-atlas/public/brand/sinclair-sage.png,
# 916×1920, transparent).
# Aufruf: python3 scripts/app-symbol-erzeugen.py <pfad/zu/sinclair-sage.png>
#
# Warum: macOS 26 zeigt Symbole, die nur als .icns vorliegen, verkleinert auf
# einer eigenen Kachel („Squircle-Gefängnis“) und skaliert sie dafür um — das
# Dock-Symbol wirkt dann kleiner und unscharf. Symbole aus dem neuen System
# (.icon → Assets.car, CFBundleIconName) zeichnet das System selbst in jeder
# Größe und Darstellung (hell, dunkel, getönt, klar).
#
# Apples Raster für das neue System: quadratische Fläche 1024×1024 bis zum Rand
# („full bleed“) — die Rundung (Maske) setzt das System, nicht wir. Fläche und
# Zeichen sind getrennt: die Fläche ist die Füllung in icon.json, das Zeichen
# eine Ebene (Assets/zeichen.png, 1024×1024, transparent).
#
# Strichstärke: anders als beim .icns gibt es nur eine Vorlage für alle
# Größen. Nadel und Ring werden deshalb einmal kräftiger gezeichnet (wie in
# symbol-aus-vektor.swift / dock-symbole-erzeugen.py für kleine Größen), damit
# sie im Dock nicht zu Haarlinien werden.
#
# Gebaut wird daraus in der CI mit `xcrun actool` (Xcode 26) — siehe bauen.yml.
import json, os, shutil, sys
from PIL import Image, ImageFilter

ZIEL = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")
CANVAS = 1024
F = 4  # Überabtastung
ANTEIL = 0.86  # Höhe des Zeichens relativ zur Fläche
NADEL_QUELLE = 26  # ungefähre Breite der Nadel in der Vorlage (px)
NADEL_SOLL = 0.018  # Mindestbreite der Nadel relativ zur Fläche

zeichen = Image.open(sys.argv[1]).convert("RGBA")
alpha = zeichen.getchannel("A")
alpha = alpha.crop(alpha.getbbox())


def zeichen_maske():
    g = CANVAS * F
    h = round(CANVAS * ANTEIL * F)
    w = round(alpha.width * h / alpha.height)
    m = alpha.resize((w, h), Image.LANCZOS)
    nadel = NADEL_QUELLE * (h / F) / alpha.height
    extra = max(0.0, NADEL_SOLL * CANVAS - nadel) / 2  # je Seite, in Zielpixeln
    k = int(round(extra * F)) * 2 + 1
    if k > 1:
        m = m.filter(ImageFilter.MaxFilter(min(k, 61)))
    voll = Image.new("L", (g, g), 0)
    voll.paste(m, ((g - w) // 2, (g - h) // 2))
    return voll.resize((CANVAS, CANVAS), Image.LANCZOS)


def farbe(rgb):
    return "srgb:" + ",".join(f"{c / 255:.5f}" for c in rgb) + ",1.00000"


def icon(name, flaeche, zeichenfarbe):
    pfad = os.path.join(ZIEL, name)
    shutil.rmtree(pfad, ignore_errors=True)
    os.makedirs(os.path.join(pfad, "Assets"))
    ebene = Image.new("RGBA", (CANVAS, CANVAS), zeichenfarbe + (255,))
    ebene.putalpha(MASKE)
    ebene.save(os.path.join(pfad, "Assets", "zeichen.png"))
    fl = {"solid": farbe(flaeche)}
    daten = {
        # Hell und dunkel gleich: die Kachel bleibt in beiden Darstellungen,
        # wie sie gewählt ist. „Getönt“ und „klar“ leitet das System ab.
        "fill-specializations": [
            {"value": fl},
            {"appearance": "dark", "value": fl},
        ],
        "groups": [
            {
                "name": "Zeichen",
                "layers": [
                    {
                        "name": "Zeichen",
                        "image-name": "zeichen.png",
                        # Ohne Glas: das Zeichen bleibt flach und randscharf.
                        "glass": False,
                        "fill-specializations": [
                            # Getönt: einfarbiges Zeichen, das System färbt es ein.
                            {"appearance": "tinted", "value": {"solid": "extended-gray:1.00000,1.00000"}},
                        ],
                    }
                ],
                "shadow": {"kind": "none", "opacity": 0.5},
                "specular": False,
                "translucency": {"enabled": False, "value": 0.5},
            }
        ],
        "supported-platforms": {"squares": "shared"},
    }
    with open(os.path.join(pfad, "icon.json"), "w") as f:
        json.dump(daten, f, indent=2, ensure_ascii=False)
        f.write("\n")


MASKE = zeichen_maske()
icon("AppIcon.icon", (12, 12, 12), (255, 255, 255))
icon("AppIconWeiss.icon", (250, 250, 248), (14, 14, 14))
print("fertig")
