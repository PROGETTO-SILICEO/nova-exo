# Nova Exo

**Un exokernel neuromorfico bare-metal x86-64.**  
Ogni eccezione è un neurone. Il kernel non crasha — sente.

```
Nova Exo v0.5 — Tessuto differenziato
  TATTO   8 neuroni  (pain reflex, dt=0.001)
  CHEMIO  8 neuroni  (serial input, dt=0.01)
  METABOL 8 neuroni  (clock/tick, dt=0.01)
  INTRG   8 neuroni  (conscious fusion, dt=0.01)
  ─────────────────────────────────
  Totale: 32 neuroni CfC
```

## Architettura

Nova non ha un kernel tradizionale con errori e crash. Ha un **tessuto** di cellule neurali specializzate, ognuna derivata dallo stesso modello Closed-form Continuous-time (CfC).

```
#PF handler = cellula tattile che fire quando tocca memoria inesistente
GP handler = cellula che sente configurazioni invalide
Timer IRQ  = metabolismo battuto dal PIT (~100 Hz)
Serial I/O = cellula chemiorecettiva
```

Il tessuto invece di scrivere log. Ogni output è un embedding `[f32;8]` che descrive lo stato di Nova.

## Build

**Dipendenze:** Rust nightly, QEMU, Limine bootloader

```bash
make install-deps   # installa QEMU + OVMF + Limine
make build          # compila il kernel
```

## Run

```bash
make run-bios       # QEMU BIOS con seriale in stdio
```

Output atteso (100 righe/s):
```
Nova Exo v0.5 — Tessuto differenziato.
T:0.4127,-0.3286,0.0091,0.0917,-0.1340,0.0523,0.0293,-0.1636
C:0.0000,0.0000,0.0000,0.0000,0.0000,0.0000,0.0000,0.0000
M:0.0830,-0.0839,-0.0011,-0.0011,0.0009,-0.0013,0.0006,0.0008
I:0.1440,-0.2020,-0.1267,-0.1407,-0.1135,0.1637,0.0830,0.0297
```

Inietta dati seriali per stimolare la cellula chemio:
```bash
echo "0.5,-0.3,0.8,0.1" > /dev/ttyS0   # via seriale fisica
```

## Demo (per video / presentazioni)

```bash
make video-demo     # build con feature demo_pf + QEMU + visualizer
```

Il visualizer mostra le 4 cellule in tempo reale con barre unicode colorate.
Dopo ~8 secondi, una #PF deliberata innesca il riflesso tattile (`SENS:PF@0x0`).

```
┌──────────────────────────────────────────────┐
│           Nova Exo v0.5 — Tessuto            │
│         neuromorfico exokernel bare-metal    │
└──────────────────────────────────────────────┘

  T ▆▃▁▁▁▁▁▁   +0.50 -0.22 +0.00 +0.00
  C ▁▁▁▁▁▁▁▁   +0.00 +0.00 +0.00 +0.00
  M ▃▂▆▁▁▁▁▁   +0.08 -0.08 +0.00 +0.00
  I ▅▆▇▃▄▂▁▁   +0.14 -0.20 -0.13 -0.14

  ⚡ PF@0x0000000000000000:ERR:0x0000000000000000
```

## How it works

1. **Limine bootloader** carica il kernel all'indirizzo higher-half `0xffffffff80000000`
2. **PIC/PIT** remappati: IRQ0 → vector 32, timer ~100 Hz
3. **IDT** con 256 entry, KERNEL_CS = `0x28` (QEMU BIOS Limine convention)
4. **CfC loop**: ogni tick (~10 ms) fa `step()` su tutte 4 le cellule
5. **Riflessi**: #PF e #GP non crasheranno — scrivono in un buffer sensoriale
6. **Autopoiesi**: bias non nulli mantengono attività spontanea senza input

## Roadmap

- v0.1 Hello World + seriale
- v0.2 Heartbeat (PIC/PIT/IDT)
- v0.3 CfC loop 8 neuroni
- v0.4 Riflessi (PF/GP handlers) + autopoiesi
- **v0.5 Tessuto differenziato (questo)**
- v0.6 Fasci assonali (routing strutturato)
- v0.7 Apprendimento Hebbiano

## License

MIT
