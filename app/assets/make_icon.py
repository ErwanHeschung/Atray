"""Génère atray.ico (16 → 256 px) et atray-256.png depuis logo.png (vortex violet sur fond noir).
`python3 make_icon.py` depuis ce dossier ; demande ffmpeg (redimensionnement).

Le logo est lumineux sur du noir : la luminosité devient la transparence (alpha = canal le plus fort,
couleur divisée par alpha), donc la lueur reste douce et le fond disparaît sur toute barre des tâches.
Le cadrage se resserre autour du vortex pour qu'il reste lisible à 16 px."""
import struct, subprocess, zlib

SRC, SIZES = "logo.png", [16, 20, 24, 32, 40, 48, 64, 256]
THRESHOLD, MARGIN = 170, 0.0  # cadre = cœur lumineux du vortex (la lueur pâle déborde et se fond)
BOOST, HAZE = 2.2, 30  # alpha = (luminosité − HAZE) × BOOST : brume coupée, cœur bien opaque
FADE = 0.1  # largeur (fraction du côté) sur laquelle la lueur s'éteint vers le bord

def raw(vf, n):
    cmd = ["ffmpeg", "-loglevel", "error", "-i", SRC, "-vf", vf, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]
    data = subprocess.run(cmd, check=True, capture_output=True).stdout
    assert len(data) == n * n * 3, (len(data), n)
    return data

def frame():
    """Carré centré sur les pixels allumés, en pixels de l'image source (1254 px)."""
    w = int(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "stream=width", "-of", "csv=p=0", SRC],
                           check=True, capture_output=True, text=True).stdout)
    n = 256
    d = raw(f"scale={n}:{n}", n)
    on = [(i % n, i // n) for i in range(n * n) if max(d[3 * i:3 * i + 3]) > THRESHOLD]
    xs, ys = [p[0] for p in on], [p[1] for p in on]
    k = w / n
    cx, cy = (min(xs) + max(xs)) / 2 * k, (min(ys) + max(ys)) / 2 * k
    side = max(max(xs) - min(xs), max(ys) - min(ys)) * k * (1 + 2 * MARGIN)
    side = min(side, w)
    x0 = max(0, min(w - side, cx - side / 2))
    y0 = max(0, min(w - side, cy - side / 2))
    return int(x0), int(y0), int(side)

def rgba(d, n):
    """Fond noir → transparence ; la lueur s'éteint sur les bords (FADE) au lieu de dessiner un carré."""
    out = bytearray()
    for i in range(0, len(d), 3):
        r, g, b = d[i], d[i + 1], d[i + 2]
        m = max(r, g, b)
        x, y = (i // 3) % n, (i // 3) // n
        edge = min(x + 0.5, y + 0.5, n - x - 0.5, n - y - 0.5) / (FADE * n)
        a = min(255, round(max(0, m - HAZE) * BOOST * min(1.0, edge)))
        out += bytes((r * 255 // m, g * 255 // m, b * 255 // m, a) if m else (0, 0, 0, 0))
    return bytes(out)

def png(n, px):
    rows = b"".join(b"\0" + px[y * n * 4:(y + 1) * n * 4] for y in range(n))
    chunk = lambda t, c: struct.pack(">I", len(c)) + t + c + struct.pack(">I", zlib.crc32(t + c) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")

x0, y0, side = frame()
images = {n: png(n, rgba(raw(f"crop={side}:{side}:{x0}:{y0},scale={n}:{n}:flags=lanczos", n), n)) for n in SIZES}

# ICO : en-tête, une entrée par taille (images PNG, acceptées depuis Windows Vista), puis les données.
offset = 6 + 16 * len(SIZES)
head, body = struct.pack("<HHH", 0, 1, len(SIZES)), b""
for n in SIZES:
    data = images[n]
    head += struct.pack("<BBBBHHII", n % 256, n % 256, 0, 0, 1, 32, len(data), offset + len(body))
    body += data
open("atray.ico", "wb").write(head + body)
open("atray-256.png", "wb").write(images[256])
print(f"cadre {side}px à ({x0},{y0}) ; atray.ico ({len(head + body)} o), atray-256.png")
