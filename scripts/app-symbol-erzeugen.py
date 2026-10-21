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
#
# Strichstärke und Größe einstellen (Dock auf Bildschirmen ohne Retina):
#   python3 scripts/app-symbol-erzeugen.py <sinclair-sage.png> [--staerke F] [--groesse A] [--versatz X] [--ziel ORDNER]
#   --staerke F  Strichstärke von Nadel und Ring als Vielfaches der bisherigen
#                (bisher ≈ 1,8 % der Zeichenhöhe); 1.8 = 80 % kräftiger.
#                Verbreitert wird rund nach außen (wie „Pfad verschieben“),
#                abgestuft nach örtlicher Stärke: Nadel/Ring voll, Haarstriche
#                und Endstriche der S gut zur Hälfte, S-Bögen und Kugel nicht.
#                Ohne Angabe: bisheriges Verfahren (gleichmäßige Verbreiterung,
#                Ergebnis unverändert). Braucht numpy und scipy.
#   --groesse A  Höhe des Zeichens relativ zur Fläche (Standard 0.86).
#                0.92 = Zeichen füllt die Kachel deutlich mehr aus.
#   --versatz X  Zeichen um X Punkte (1024er Raster) nach rechts schieben.
#                Die Mitte der Fläche fällt bei geraden Kachelgrößen genau auf
#                eine Pixelgrenze; die Nadel wird dann zu zwei grauen Spalten.
#                12 = ein halbes Pixel bei 52-px-Dock (0,6 px bei 64 px), dort
#                liegt die Nadel dann auf einer Pixelspalte. Auf Retina
#                unsichtbar (1 px bei 104 px).
#   --ziel O     Ausgabeordner statt src-tauri/icons (für Vorschauen).
# Empfehlung (Vergleich 22.10.2026): --staerke 1.8 --groesse 0.92 --versatz 12
import argparse, json, os, shutil, sys
from PIL import Image, ImageFilter

ZIEL = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")
CANVAS = 1024
F = 4  # Überabtastung
ANTEIL = 0.86  # Höhe des Zeichens relativ zur Fläche
NADEL_QUELLE = 26  # ungefähre Breite der Nadel in der Vorlage (px)
NADEL_SOLL = 0.018  # Mindestbreite der Nadel relativ zur Fläche

# Für --staerke (alle Werte relativ zur Zeichenhöhe):
STRICH_BISHER = 0.018  # wirksame Breite von Ring/Nadel im bisherigen Verfahren
RING_QUELLE = 0.011  # Breite von Ring/Nadel oben in der Vorlage
DUENN_BIS = 0.013  # bis zu dieser Breite volle Verbreiterung
KRAEFTIG_AB = 0.032  # ab dieser Breite keine Verbreiterung mehr

arg = argparse.ArgumentParser()
arg.add_argument("vorlage")
arg.add_argument("--staerke", type=float, default=None)
arg.add_argument("--groesse", type=float, default=ANTEIL)
arg.add_argument("--versatz", type=float, default=0.0)
arg.add_argument("--ziel", default=ZIEL)
arg = arg.parse_args()
ANTEIL = arg.groesse
ZIEL = arg.ziel

zeichen = Image.open(arg.vorlage).convert("RGBA")
alpha = zeichen.getchannel("A")
alpha = alpha.crop(alpha.getbbox())


def verstaerken(m, staerke):
    """Verbreitert dünne Linien rund nach außen („Pfad verschieben“), kräftige nicht.

    Für jeden Punkt des Zeichens wird zuerst die örtliche Strichstärke
    bestimmt (größter Kreis um die Mittellinie, der ihn enthält). Danach wird
    der Rand um einen Betrag nach außen geschoben, der von dieser Stärke
    abhängt: Nadel und Ring (≈ 1 % der Zeichenhöhe) um den vollen Betrag,
    mittlere Linien (Haarstriche der S, Endstriche ≈ 2 %) um gut die Hälfte,
    kräftige Formen ab 3,2 % (S-Bögen, Kugel) gar nicht. Innenecken bleiben
    spitz, Außenecken werden nur um den Verschiebebetrag gerundet, und die
    Lücken neben der Nadel bleiben offen.
    """
    try:
        import numpy as np
        from scipy import ndimage
    except ImportError:
        sys.exit("--staerke braucht numpy und scipy: python3 -m pip install --user numpy scipy")

    h = m.height
    r = max(0.0, (staerke * STRICH_BISHER - RING_QUELLE) / 2) * h  # je Seite, px
    d1, d2 = DUENN_BIS / 2 * h, KRAEFTIG_AB / 2 * h  # Halbbreiten, px
    rand = int(np.ceil(r)) + 4
    a = np.pad(np.asarray(m, dtype=np.float32) / 255.0, rand)
    innen = a >= 0.5
    d = ndimage.distance_transform_edt(innen)
    # Mittellinie: in mindestens einer Richtung örtlich am weitesten vom Rand.
    grat = np.zeros_like(innen)
    for dy, dx in ((0, 1), (1, 0), (1, 1), (1, -1)):
        grat |= (d >= np.roll(d, (dy, dx), (0, 1))) & (d >= np.roll(d, (-dy, -dx), (0, 1)))
    grat &= innen & (d >= 1)
    # Örtliche Halbbreite: Maximum der Mittellinien-Kreise, die den Punkt enthalten.
    t = d.copy()
    for lam in np.arange(np.floor(d1), np.ceil(d2)):
        kern = grat & (d >= lam) & (d < lam + 1)
        if kern.any():
            drin = ndimage.distance_transform_edt(~kern) <= lam + 1.5
            t[drin] = np.maximum(t[drin], lam)
    kraeftig = ndimage.distance_transform_edt(~(d >= d2)) <= d2 + 1.5  # Öffnung mit Radius d2
    t[kraeftig] = d2
    # Verschiebung je Randpunkt, dann als weiche Kreise nach außen ziehen.
    schub = r * np.clip((d2 - t) / (d2 - d1), 0.0, 1.0)
    # Spitze Ecken kräftiger Formen liegen außerhalb jedes großen Kreises und
    # sähen dünn aus; der kleinste Wert in der Umgebung hält sie unverändert.
    schub = ndimage.minimum_filter(np.where(innen, schub, np.inf), size=int(r) | 1)
    # Weich ineinander übergehen lassen, damit an Übergängen keine Stufen entstehen.
    gew = ndimage.gaussian_filter(innen.astype(np.float32), r / 2)
    schub = ndimage.gaussian_filter(np.where(innen, schub, 0.0).astype(np.float32), r / 2) / np.maximum(gew, 1e-6)
    randpunkte = innen & ~ndimage.binary_erosion(innen)
    neu = a.copy()
    for rho in np.arange(np.ceil(r), 0, -1.0):
        kern = randpunkte & (schub > rho - 1) & (schub <= rho)
        if kern.any():
            rho_echt = float(schub[kern].mean())
            abstand = ndimage.distance_transform_edt(~kern)
            neu = np.maximum(neu, np.clip(rho_echt - abstand + 0.5, 0.0, 1.0))
    neu = neu[rand:-rand, rand:-rand]
    return Image.fromarray(np.round(neu * 255).astype(np.uint8))


def zeichen_maske():
    g = CANVAS * F
    h = round(CANVAS * ANTEIL * F)
    w = round(alpha.width * h / alpha.height)
    m = alpha.resize((w, h), Image.LANCZOS)
    if arg.staerke is not None:
        m = verstaerken(m, arg.staerke)
        k = 1
    else:
        nadel = NADEL_QUELLE * (h / F) / alpha.height
        extra = max(0.0, NADEL_SOLL * CANVAS - nadel) / 2  # je Seite, in Zielpixeln
        k = int(round(extra * F)) * 2 + 1
    if k > 1:
        m = m.filter(ImageFilter.MaxFilter(min(k, 61)))
    voll = Image.new("L", (g, g), 0)
    voll.paste(m, ((g - w) // 2 + round(arg.versatz * F), (g - h) // 2))
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
