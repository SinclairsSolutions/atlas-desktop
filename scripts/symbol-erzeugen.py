# Erzeugt src-tauri/icons/quelle-1024.png aus dem Atlas-Kompass-Motiv.
# Aufruf: python3 scripts/symbol-erzeugen.py <pfad/zu/sinclair-atlas>/public/brand/atlas-compass.png
#         danach: bun run symbole
import os, sys
from PIL import Image, ImageDraw, ImageFilter
src = Image.open(sys.argv[1]).convert('RGB')
# Emblem mittig ausschneiden (Kompass ohne Schriftzug)
cx, cy, half = 407, 440, 345
crop = src.crop((cx-half, cy-half, cx+half, cy+half))
S = 1024; inner = 824; off = (S-inner)//2
body = crop.resize((inner, inner), Image.LANCZOS)
mask = Image.new('L', (inner*4, inner*4), 0)
ImageDraw.Draw(mask).rounded_rectangle((0,0,inner*4-1,inner*4-1), radius=int(inner*4*0.2237), fill=255)
mask = mask.resize((inner, inner), Image.LANCZOS)
canvas = Image.new('RGBA', (S,S), (0,0,0,0))
shadow = Image.new('RGBA', (S,S), (0,0,0,0))
sm = Image.new('L', (S,S), 0); sm.paste(mask, (off, off+12))
shadow.putalpha(sm.point(lambda v: int(v*0.35))); shadow = shadow.filter(ImageFilter.GaussianBlur(14))
canvas.alpha_composite(shadow)
layer = Image.new('RGBA', (inner, inner)); layer.paste(body, (0,0)); layer.putalpha(mask)
canvas.alpha_composite(layer, (off, off))
canvas.save(os.path.join(os.path.dirname(__file__), '..', 'src-tauri', 'icons', 'quelle-1024.png'))
