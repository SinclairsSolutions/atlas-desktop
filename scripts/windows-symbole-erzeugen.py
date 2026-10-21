# Erzeugt die Windows-Symbole aus den vorhandenen Mac-Quellen im Repo
# (src-tauri/icons/quelle-1024.png — schwarze Platte, weißes Zeichen):
#
#   src-tauri/icons/icon.ico             16, 24, 32, 48, 64, 256 px (Installer, Programmdatei,
#                                        Startmenü, Taskleiste, Fenstersymbol)
#   src-tauri/icons/windows-schwarz.png  256 px, Fenstersymbol zur Laufzeit (symbol.rs)
#   src-tauri/icons/windows-weiss.png    256 px, dasselbe in Weiß
#   src-tauri/icons/windows-infobereich.png       32 px, Symbol im Infobereich (Tray)
#   src-tauri/icons/windows-passwoerter-infobereich.png  32 px, Schlüssel im Infobereich
#
# Aufruf: python3 scripts/windows-symbole-erzeugen.py   (läuft auch nach `bun run symbole`)
#
# Unterschied zum Mac-Symbol: Apple verlangt Rand (Platte 824/1024) und Schatten.
# In der Windows-Taskleiste wirkt das klein und „gerahmt“. Windows-Symbole füllen
# die Fläche: Platte randlos, kein Schatten, kleinere Ecken, Zeichen größer. Für
# kleine Größen werden die feinen Linien (Nadel, Ring) kräftiger gezeichnet,
# sonst verschwinden sie unter einem Pixel.
import os
from PIL import Image, ImageDraw, ImageFilter

HIER = os.path.dirname(os.path.abspath(__file__))
ICONS = os.path.join(HIER, "..", "src-tauri", "icons")
F = 8  # Überabtastung

quelle = Image.open(os.path.join(ICONS, "quelle-1024.png")).convert("RGBA")
# Zeichen = helle Pixel innerhalb der Platte (Platte ist fast schwarz, Schatten schwarz).
r, g, b, a = quelle.split()
hell = Image.merge("RGB", (r, g, b)).convert("L").point(lambda v: max(0, min(255, (v - 40) * 255 // 200)))
zeichen = Image.composite(hell, Image.new("L", quelle.size, 0), a)
zeichen = zeichen.crop(zeichen.getbbox())

# Nadelbreite messen (Zeile knapp unter der Spitze, oberhalb des Rings).
zeile = round(zeichen.height * 0.04)
NADEL = max(1, sum(1 for x in range(zeichen.width) if zeichen.getpixel((x, zeile)) > 128))


def symbol(s, flaeche, farbe, anteil=0.84, rand=0.0):
    """Randloses Windows-Symbol der Kantenlänge s."""
    gross = s * F
    leinwand = Image.new("RGBA", (gross, gross), (0, 0, 0, 0))
    o = rand * gross
    platte = Image.new("L", (gross, gross), 0)
    ImageDraw.Draw(platte).rounded_rectangle(
        (o, o, gross - 1 - o, gross - 1 - o), radius=int((gross - 2 * o) * 0.19), fill=255
    )
    fl = Image.new("RGBA", (gross, gross), flaeche)
    fl.putalpha(platte)
    leinwand.alpha_composite(fl)
    h = int((gross - 2 * o) * anteil)
    w = max(1, int(zeichen.width * h / zeichen.height))
    maske = zeichen.resize((w, h), Image.LANCZOS)
    # Mindeststrichstärke im Ziel: ~1,2 px klein, ~1,2 % der Größe groß.
    nadel_ziel = NADEL * (h / F) / zeichen.height
    soll = max(1.2, s * 0.012)
    extra = max(0.0, soll - nadel_ziel) / 2
    k = int(round(extra * F)) * 2 + 1
    if k > 1:
        maske = maske.filter(ImageFilter.MaxFilter(min(k, 61)))
    fb = Image.new("RGBA", (w, h), farbe)
    fb.putalpha(maske)
    leinwand.alpha_composite(fb, ((gross - w) // 2, (gross - h) // 2))
    return leinwand.resize((s, s), Image.LANCZOS)


SCHWARZ = ((12, 12, 12, 255), (255, 255, 255, 255))
WEISS = ((250, 250, 248, 255), (14, 14, 14, 255))
GROESSEN = [16, 24, 32, 48, 64, 256]

bilder = [symbol(s, *SCHWARZ) for s in GROESSEN]
# Pillow schreibt jede Größe aus dem jeweils eigenen Bild (append_images), nicht
# aus einer verkleinerten 256er-Fassung.
bilder[-1].save(
    os.path.join(ICONS, "icon.ico"),
    format="ICO",
    sizes=[(s, s) for s in GROESSEN],
    append_images=bilder[:-1],
)
symbol(256, *SCHWARZ).save(os.path.join(ICONS, "windows-schwarz.png"))
symbol(256, *WEISS).save(os.path.join(ICONS, "windows-weiss.png"))
# Infobereich: Windows zeigt 16 px (bei 100 %) bis 32 px; ein kleiner Rand,
# damit das Symbol neben den System-Symbolen nicht größer wirkt.
symbol(32, *SCHWARZ, rand=0.03).save(os.path.join(ICONS, "windows-infobereich.png"))

# Schlüssel für den Infobereich: die Mac-Vorlage ist schwarz auf transparent
# (Template-Bild) und auf der dunklen Taskleiste unsichtbar — hier weiß auf
# der Atlas-Platte.
vorlage = Image.open(os.path.join(ICONS, "passwoerter-menueleiste.png")).convert("RGBA")
schluessel = vorlage.getchannel("A").crop(vorlage.getchannel("A").getbbox())
s = 32
gross = s * F
leinwand = Image.new("RGBA", (gross, gross), (0, 0, 0, 0))
o = 0.03 * gross
platte = Image.new("L", (gross, gross), 0)
ImageDraw.Draw(platte).rounded_rectangle((o, o, gross - 1 - o, gross - 1 - o), radius=int(gross * 0.19), fill=255)
fl = Image.new("RGBA", (gross, gross), SCHWARZ[0])
fl.putalpha(platte)
leinwand.alpha_composite(fl)
seite = int(gross * 0.68)
fak = seite / max(schluessel.size)
schluessel = schluessel.resize((max(1, int(schluessel.width * fak)), max(1, int(schluessel.height * fak))), Image.LANCZOS)
fb = Image.new("RGBA", schluessel.size, SCHWARZ[1])
fb.putalpha(schluessel)
leinwand.alpha_composite(fb, ((gross - schluessel.width) // 2, (gross - schluessel.height) // 2))
leinwand.resize((s, s), Image.LANCZOS).save(os.path.join(ICONS, "windows-passwoerter-infobereich.png"))
print("fertig")
