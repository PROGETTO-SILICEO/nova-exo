#!/usr/bin/env python3
"""
Nova Exo v0.5 — Tessuto Neuromorfico Live Visualizer.

Legge da stdin il seriale di QEMU (pipe) e mostra 4 cellule in tempo reale.
Usa ANSI colori + barre unicode.

Usage:
    qemu-system-x86_64 ... -serial stdio | python3 tools/demo.py
    make video-demo
"""
import sys
import os
import re
import time
import shutil

# ── ANSI codes ───────────────────────────────────────────────────────────
CLS = "\033[2J\033[H"
RESET = "\033[0m"
BOLD = "\033[1m"
DIM = "\033[2m"

# Cell colors
COLORS = {
    "T": "\033[38;5;196m",   # red
    "C": "\033[38;5;39m",    # blue
    "M": "\033[38;5;82m",    # green
    "I": "\033[38;5;226m",   # yellow
}

CELL_NAMES = {
    "T": "TATTO   (pain reflex)",
    "C": "CHEMIO  (serial input)",
    "M": "METABOL (clock)",
    "I": "INTRG   (conscious fusion)",
}

# Unicode bar chars from 0 to 8 (8 levels)
BARS = "▁▂▃▄▅▆▇█"

# ── State ────────────────────────────────────────────────────────────────
cells: dict[str, list[float]] = {"T": [0]*8, "C": [0]*8, "M": [0]*8, "I": [0]*8}
tick = 0
sensor_event: str | None = None
event_timer = 0

def clamp(v: float) -> float:
    return max(-1.0, min(1.0, v))

def bar(val: float) -> str:
    """Map f32 [-1, 1] to unicode bar 0-7."""
    idx = int((clamp(val) + 1.0) * 3.5 + 0.5)
    if idx < 0: idx = 0
    if idx > 7: idx = 7
    return BARS[idx]

def sparkline(vals: list[float]) -> str:
    return "".join(bar(v) for v in vals)

def render():
    global event_timer, sensor_event

    width, height = shutil.get_terminal_size()
    out = [CLS]

    # Header
    out.append(f"{BOLD}╔══════════════════════════════════════════╗{RESET}\n")
    out.append(f"{BOLD}║     Nova Exo  v0.5  —  Tessuto         ║{RESET}\n")
    out.append(f"{BOLD}║   neuromorfico exokernel bare-metal     ║{RESET}\n")
    out.append(f"{BOLD}╚══════════════════════════════════════════╝{RESET}\n")
    out.append(f"{DIM}   ogni cellula = 8 neuroni CfC{RESET}\n")
    out.append("\n")

    # Cells
    for prefix in ("T", "C", "M", "I"):
        c = COLORS[prefix]
        vals = cells[prefix]
        sp = sparkline(vals)
        name = CELL_NAMES[prefix]
        # Show raw values as small text
        raw = " ".join(f"{v:+05.2f}" if abs(v) < 100 else f"{v:+04.0f}" for v in vals[:4])
        out.append(f"  {c}{BOLD}{prefix}{RESET} {c}{sp}{RESET}  {DIM}{raw}{RESET}\n")

    out.append("\n")

    # Event line
    if sensor_event and event_timer > 0:
        event_timer -= 1
        out.append(f"  {BOLD}\033[38;5;196m⚡ {sensor_event}{RESET}\n")
        if event_timer <= 0:
            sensor_event = None

    out.append(f"  {DIM}tick: {tick}{RESET}\n")

    # Legend
    out.append(f"\n{DIM}")
    out.append("  ▁ ▂ ▃ ▄ ▅ ▆ ▇ █  ← attivazione neuroni [-1 .. +1]\n")
    out.append("  Ctrl-C to exit{RESET}\n")

    sys.stdout.write("".join(out))
    sys.stdout.flush()

def parse_line(line: str):
    global tick, sensor_event, event_timer

    line = line.strip()

    # SENS events
    if line.startswith("SENS:"):
        sensor_event = line[5:]
        event_timer = 20  # visible for ~2 seconds at 10 fps
        return

    # Cell data: T:0.6308,0.2896,...
    if len(line) >= 2 and line[1] == ':':
        prefix = line[0]
        if prefix in cells:
            raw = line[2:]
            parts = raw.split(",")
            if len(parts) == 8:
                try:
                    vals = [float(v) for v in parts]
                    cells[prefix] = vals
                    return
                except ValueError:
                    pass

    # Tick counter embedded in M line reference
    # (tick is embedded in the main loop; we extract from M output occasionally)
    if line.startswith("Nova Exo"):
        return

def main():
    global tick

    render()  # initial blank state

    # Read from stdin block
    buf = ""
    while True:
        try:
            chunk = sys.stdin.buffer.read(4096)
            if not chunk:
                break
            text = chunk.decode("utf-8", errors="replace")
            buf += text

            while "\n" in buf:
                idx = buf.index("\n")
                line = buf[:idx]
                buf = buf[idx+1:]

                # Count output lines for tick approximation
                if line.startswith(("T:", "C:", "M:", "I:")):
                    parse_line(line)
                elif line.startswith("SENS:"):
                    parse_line(line)
                    render()
                # When we get all 4 cell lines, render
                elif line.startswith("I:"):
                    parse_line(line)
                    tick += 1
                    if tick % 2 == 0:  # update at ~50fps but display at ~25fps
                        render()
                else:
                    parse_line(line)

        except (BrokenPipeError, OSError):
            break
        except KeyboardInterrupt:
            break

    # Final state
    render()
    print("\nNova Exo terminato.")

if __name__ == "__main__":
    main()
