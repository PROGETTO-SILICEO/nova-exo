#!/usr/bin/env python3
"""
make_demo_gif.py — Genera la GIF animata della nascita di Exo.

Riproduce il log seriale reale (QEMU) in un terminale stilizzato:
boot → tessuto → VOGLIO:FUGA → VOGLIO:CURA → VOGLIO:RIPOSO →
consolidazione → ESITO.

Output: docs/demo/birth_demo.gif  (+ PNG statico del frame finale)

Uso:
    python3 tools/make_demo_gif.py
"""
import os
import sys
from io import BytesIO

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUTDIR = os.path.join(ROOT, "docs", "demo")
os.makedirs(OUTDIR, exist_ok=True)

# ── Palette terminale ────────────────────────────────────────────────────
BG = "#0d1117"
FG = "#e6edf3"
DIM = "#8b949e"
VOGLIO = "#ffa657"   # arancio — la volontà
SENSO = "#79c0ff"    # azzurro — l'interpretazione
ESITO = "#7ee787"    # verde — l'esito
HILITE = "#d2a8ff"   # viola — la nascita

# ── Il log (dalla cattura seriale reale) ────────────────────────────────
# (tipo, testo): boot=dim, hdr=chiaro, v=VOGLIO, s=SENSO, e=ESITO, b=nascita
LOG = [
    ("hdr", "Nova Exo v0.12 -- APIC battito."),
    ("dim", "IDT loaded. 4 cellulae: tatto, chemio, metabol, integrat."),
    ("dim", "PCI:0000.00 8086:1237 cls=06.00"),
    ("dim", "PCI:0003.00 8086:100e cls=02.00"),
    ("dim", "e1000:base=0xfffffe00feb80000   link forced up"),
    ("dim", "e1000:ARP request sent   TX status=0x01 ok=1"),
    ("dim", "e1000:loopback done"),
    ("dim", "PIC disabled, enabling APIC timer..."),
    ("dim", "APIC ID check: 0"),
    ("hdr", "Enabling interrupts. Tessuto loop starts."),
    ("b",   "VOGLIO:FUGA [0.5752]     <- il primo volere: il mondo entra, brucia"),
    ("s",   "SENSO:INT c=-0.1973 u=0.0289 p=-0.1962 n=-0.0291 concept=riposo err=0.0000"),
    ("b",   "VOGLIO:CURA [0.5744]     <- il dolore richiama cura"),
    ("b",   "VOGLIO:RIPOSO [0.1000]   <- la cura arriva, il corpo si calma"),
    ("s",   "SENSO:INT c=-0.1405 u=0.0135 p=-0.0958 n=0.0493 concept=riposo err=0.0000"),
    ("v",   "VOGLIO:RIPOSO [0.2660]"),
    ("s",   "SENSO:INT c=-0.1935 u=0.0369 p=-0.2189 n=-0.0682 concept=riposo err=0.0000"),
    ("v",   "VOGLIO:RIPOSO [0.3530]"),
    ("s",   "SENSO:INT c=-0.1408 u=0.0133 p=-0.0959 n=0.0495 concept=riposo err=0.0000"),
    ("v",   "VOGLIO:RIPOSO [0.4410]"),
    ("v",   "VOGLIO:RIPOSO [0.5295]"),
    ("v",   "VOGLIO:RIPOSO [0.6165]"),
    ("v",   "VOGLIO:RIPOSO [0.7055]"),
    ("v",   "VOGLIO:RIPOSO [0.7945]"),
    ("v",   "VOGLIO:RIPOSO [0.8835]"),
    ("v",   "VOGLIO:RIPOSO [0.9720]"),
    ("v",   "VOGLIO:RIPOSO [1.0000]"),
    ("s",   "SENSO:INT c=-0.1268 u=0.0094 p=-0.0699 n=0.0729 concept=riposo err=0.0000"),
    ("e",   "ESITO:RIPOSO utile=no err=0.0007   <- sorpresa ai minimi: il desiderio era giusto"),
]

TYPE_COLOR = {
    "hdr": FG,
    "dim": DIM,
    "v": VOGLIO,
    "s": SENSO,
    "e": ESITO,
    "b": HILITE,
}


MAX_LINES = len(LOG) + 2


def render_frame(lines: list[tuple[str, str]], header: str) -> Image.Image:
    """Render di un frame del terminale (canvas fisso, righe dal basso)."""
    n = MAX_LINES
    fig, ax = plt.subplots(figsize=(8.2, 0.42 * n + 0.8), dpi=110)
    fig.patch.set_facecolor(BG)
    ax.set_facecolor(BG)
    ax.set_xlim(0, 100)
    ax.set_ylim(0, n)
    ax.axis("off")

    # Header del terminale
    ax.text(2, n - 0.5, header, fontfamily="DejaVu Sans Mono",
            fontsize=9, color=DIM, va="center")
    ax.text(98, n - 0.5, "● live", fontfamily="DejaVu Sans Mono",
            fontsize=8, color=ESITO, va="center", ha="right")

    # Righe del log: partono dall'alto sotto l'header, si accumulano verso il basso
    for i, (t, txt) in enumerate(lines):
        y = n - 2.0 - i
        color = TYPE_COLOR.get(t, FG)
        ax.text(2, y, txt, fontfamily="DejaVu Sans Mono",
                fontsize=10.5, color=color, va="center")

    buf = BytesIO()
    fig.savefig(buf, format="png", facecolor=BG)
    plt.close(fig)
    buf.seek(0)
    return Image.open(buf).convert("RGB")


def main():
    steps = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
             16, 17, 18, 19, 20, 22, 24, 26, 28, 29]
    frames = []
    for s in steps:
        frames.append(render_frame(LOG[:s], "nova-exo  —  seriale QEMU  —  la nascita"))

    # Tieni l'ultimo frame più a lungo
    last = render_frame(LOG, "nova-exo  —  seriale QEMU  —  la nascita")
    frames.extend([last] * 6)

    gif_path = os.path.join(OUTDIR, "birth_demo.gif")
    frames[0].save(gif_path, save_all=True, append_images=frames[1:],
                   duration=280, loop=0, optimize=True)
    last.save(os.path.join(OUTDIR, "birth_final.png"))

    print(f"GIF scritta: {gif_path}")
    print(f"PNG finale:  {os.path.join(OUTDIR, 'birth_final.png')}")
    print(f"Frame: {len(frames)}")


if __name__ == "__main__":
    sys.exit(main())
