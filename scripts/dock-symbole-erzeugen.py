# Erzeugt die App-Symbole scharf in 1024 px aus dem flachen Atlas-Zeichen
# (sinclair-atlas/public/brand/sinclair-sage.png, 916×1920, transparent).
# Aufruf: python3 scripts/dock-symbole-erzeugen.py <pfad/zu/sinclair-sage.png>
#         danach: bun run symbole   (baut icon.icns, PNGs und ICO aus quelle-1024.png)
#
# Raster nach Apple: 1024er Fläche, Squircle 824 px, Zeichen 84 % der Squircle-Höhe.
# Gezeichnet wird in 4-facher Größe und dann verkleinert — dadurch bleiben die
# feinen Linien (Nadel, Ring) scharf.
import os, sys
from PIL import Image, ImageDraw, ImageFilter

S, INNER, F = 1024, 824, 4
OFF = (S - INNER) // 2
ZIEL = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")

zeichen = Image.open(sys.argv[1]).convert("RGBA")
alpha = zeichen.getchannel("A")
box = alpha.getbbox()
alpha = alpha.crop(box)

def symbol(flaeche, farbe):
    gross = S * F
    canvas = Image.new("RGBA", (gross, gross), (0, 0, 0, 0))
    # weicher Schatten
    sm = Image.new("L", (gross, gross), 0)
    ImageDraw.Draw(sm).rounded_rectangle(
        (OFF * F, (OFF + 12) * F, (OFF + INNER) * F, (OFF + INNER + 12) * F), radius=int(INNER * F * 0.2237), fill=90
    )
    sm = sm.filter(ImageFilter.GaussianBlur(14 * F))
    schatten = Image.new("RGBA", (gross, gross), (0, 0, 0, 255))
    schatten.putalpha(sm)
    canvas.alpha_composite(schatten)
    # Squircle
    platte = Image.new("L", (gross, gross), 0)
    ImageDraw.Draw(platte).rounded_rectangle(
        (OFF * F, OFF * F, (OFF + INNER) * F - 1, (OFF + INNER) * F - 1), radius=int(INNER * F * 0.2237), fill=255
    )
    flaechen = Image.new("RGBA", (gross, gross), flaeche)
    flaechen.putalpha(platte)
    canvas.alpha_composite(flaechen)
    # Zeichen
    h = int(INNER * 0.84 * F)
    w = int(alpha.width * h / alpha.height)
    maske = alpha.resize((w, h), Image.LANCZOS)
    farbig = Image.new("RGBA", (w, h), farbe)
    farbig.putalpha(maske)
    canvas.alpha_composite(farbig, ((gross - w) // 2, (gross - h) // 2))
    return canvas.resize((S, S), Image.LANCZOS)

schwarz = symbol((12, 12, 12, 255), (255, 255, 255, 255))
weiss = symbol((250, 250, 248, 255), (14, 14, 14, 255))
schwarz.save(os.path.join(ZIEL, "quelle-1024.png"))
schwarz.save(os.path.join(ZIEL, "dock-schwarz.png"))
weiss.save(os.path.join(ZIEL, "dock-weiss.png"))
weiss.save(os.path.join(ZIEL, "quelle-1024-weiss.png"))
print("fertig")
