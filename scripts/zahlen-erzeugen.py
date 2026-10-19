# Erzeugt die Overlay-Symbole für die Windows-Taskleiste (1–9 und 9+).
# Aufruf: python3 scripts/zahlen-erzeugen.py
from PIL import Image, ImageDraw, ImageFont
import os
ziel = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons", "zahl")
S = 64
font = None
for kandidat in ["/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/Helvetica.ttc", "C:/Windows/Fonts/segoeuib.ttf"]:
    if os.path.exists(kandidat):
        font = kandidat; break
for text in [str(i) for i in range(1, 10)] + ["9+"]:
    img = Image.new("RGBA", (S*4, S*4), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.ellipse((0, 0, S*4-1, S*4-1), fill=(200, 56, 48, 255))
    f = ImageFont.truetype(font, 150 if len(text) == 1 else 112)
    d.text((S*2, S*2+6), text, font=f, fill="white", anchor="mm")
    img.resize((S, S), Image.LANCZOS).save(os.path.join(ziel, ("plus" if text == "9+" else text) + ".png"))
