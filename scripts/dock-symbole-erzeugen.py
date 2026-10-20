# Erzeugt die App-Symbole aus dem flachen Atlas-Zeichen
# (sinclair-atlas/public/brand/sinclair-sage.png, 916×1920, transparent):
#   src-tauri/icons/icon.icns, dock-schwarz.icns, dock-weiss.icns,
#   quelle-1024.png (für `bun run symbole`, Windows/ICO) und dock-*.png.
# Aufruf: python3 scripts/dock-symbole-erzeugen.py <pfad/zu/sinclair-sage.png>
#
# Warum je Größe eigens gezeichnet wird: Nadel und Ring des Zeichens sind sehr
# fein. Einfach verkleinert sind sie im Dock nur Bruchteile eines Pixels breit
# und verschwimmen. Wie Apple es bei eigenen Symbolen macht, werden die
# Linien für kleine Größen kräftiger gezeichnet (Mindeststrichstärke), große
# Größen bleiben fein. Raster nach Apple: Squircle 824/1024, Zeichen 84 %.
import os, subprocess, sys, tempfile
from PIL import Image, ImageDraw, ImageFilter

INNER_ANTEIL = 824 / 1024
ZIEL = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")
F = 8  # Überabtastung

zeichen = Image.open(sys.argv[1]).convert("RGBA")
alpha = zeichen.getchannel("A")
alpha = alpha.crop(alpha.getbbox())
NADEL_QUELLE = 26  # ungefähre Breite der Nadel in der Vorlage (px)

def symbol(s, flaeche, farbe):
    g = s * F
    inner = round(s * INNER_ANTEIL)
    off = (s - inner) / 2
    canvas = Image.new("RGBA", (g, g), (0, 0, 0, 0))
    if s >= 64:  # weicher Schatten nur, wo er sichtbar ist
        sm = Image.new("L", (g, g), 0)
        ImageDraw.Draw(sm).rounded_rectangle(
            (off * F, (off + s * 0.012) * F, (off + inner) * F, (off + inner + s * 0.012) * F),
            radius=int(inner * F * 0.2237), fill=90,
        )
        sm = sm.filter(ImageFilter.GaussianBlur(s * 0.014 * F))
        sch = Image.new("RGBA", (g, g), (0, 0, 0, 255)); sch.putalpha(sm)
        canvas.alpha_composite(sch)
    platte = Image.new("L", (g, g), 0)
    ImageDraw.Draw(platte).rounded_rectangle(
        (off * F, off * F, (off + inner) * F - 1, (off + inner) * F - 1), radius=int(inner * F * 0.2237), fill=255
    )
    fl = Image.new("RGBA", (g, g), flaeche); fl.putalpha(platte)
    canvas.alpha_composite(fl)
    h = int(inner * 0.84 * F)
    w = int(alpha.width * h / alpha.height)
    maske = alpha.resize((w, h), Image.LANCZOS)
    # Mindeststrichstärke: Nadel soll im Ziel mindestens ~1,4 px (klein) bzw.
    # ~1,6 % der Größe (groß) breit sein.
    nadel_ziel = NADEL_QUELLE * (h / F) / alpha.height
    soll = max(1.4, s * 0.016)
    extra = max(0.0, soll - nadel_ziel) / 2  # je Seite, in Zielpixeln
    k = int(round(extra * F)) * 2 + 1
    if k > 1:
        maske = maske.filter(ImageFilter.MaxFilter(min(k, 61)))
    fb = Image.new("RGBA", (w, h), farbe); fb.putalpha(maske)
    canvas.alpha_composite(fb, ((g - w) // 2, (g - h) // 2))
    return canvas.resize((s, s), Image.LANCZOS)

GROESSEN = [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)]

def icns(name, flaeche, farbe):
    with tempfile.TemporaryDirectory() as d:
        iset = os.path.join(d, "x.iconset"); os.mkdir(iset)
        for basis, fak in GROESSEN:
            px = basis * fak
            nm = f"icon_{basis}x{basis}{'@2x' if fak == 2 else ''}.png"
            symbol(px, flaeche, farbe).save(os.path.join(iset, nm))
        subprocess.run(["iconutil", "-c", "icns", "-o", os.path.join(ZIEL, name), iset], check=True)

SCHWARZ = ((12, 12, 12, 255), (255, 255, 255, 255))
WEISS = ((250, 250, 248, 255), (14, 14, 14, 255))
icns("icon.icns", *SCHWARZ)
icns("dock-schwarz.icns", *SCHWARZ)
icns("dock-weiss.icns", *WEISS)
symbol(1024, *SCHWARZ).save(os.path.join(ZIEL, "quelle-1024.png"))
symbol(1024, *SCHWARZ).save(os.path.join(ZIEL, "dock-schwarz.png"))
symbol(1024, *WEISS).save(os.path.join(ZIEL, "dock-weiss.png"))
symbol(1024, *WEISS).save(os.path.join(ZIEL, "quelle-1024-weiss.png"))
for s in (32, 64, 128):
    symbol(s, *SCHWARZ).save(os.path.join(ZIEL, f"{s}x{s}.png"))
symbol(256, *SCHWARZ).save(os.path.join(ZIEL, "128x128@2x.png"))
symbol(512, *SCHWARZ).save(os.path.join(ZIEL, "icon.png"))
print("fertig")
