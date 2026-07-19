# Nova Exo

**Un exokernel neuromorfico bare-metal x86-64.**  
Ogni eccezione è un neurone. Il kernel non crasha — sente.

```
Nova Exo v0.11 — Sedimentazione
  TATTO   8 neuroni  (pain reflex, dt=0.001)
  CHEMIO  8 neuroni  (serial input, dt=0.01)
  METABOL 8 neuroni  (clock/tick, dt=0.01)
  INTRG   8 neuroni  (conscious fusion, dt=0.01)
  MEMORIA 16 pattern (associativa circolare)
  SEDIMENTO α=0.0001 (traccia nei pesi)
  ─────────────────────────────────
  Totale: 32 neuroni CfC + memoria + sedimentazione
```

## Architettura

Nova non ha un kernel tradizionale con errori e crash. Ha un **tessuto** di cellule neurali specializzate (Closed-form Continuous-time), memoria associativa (pattern recall), un attrattore mnemonico, e **sedimentazione** — ogni richiamo di un pattern altera impercettibilmente i pesi di Integrat, una traccia sedimentaria che si accumula nel tempo.

```
#PF handler = cellula tattile che fire quando tocca memoria inesistente
GP handler = cellula che sente configurazioni invalide
Timer IRQ  = metabolismo battuto dal PIT (~100 Hz)
Serial I/O = cellula chemiorecettiva
Memoria    = 16 pattern circolari, auto-store, recall coseno
  Attractor  = recall pre-CfC tira Integrat verso il pattern più simile
  Sediment   = ogni richiamo modifica W_INTRG.w_f_in (α=0.0001)
```

La familiarità (`F:`) è l'output continuo di quanto lo stato corrente assomiglia a stati già vissuti.

## Build

**Dipendenze:** Rust, QEMU, Limine bootloader

```bash
make install-deps   # installa QEMU + OVMF + Limine
make build          # compila il kernel
```

## Run

```bash
make run-bios       # QEMU con seriale in stdio
```

Output atteso:
```
Nova Exo v0.11 — Sedimentazione.
T:0.4127,-0.3286,0.0091,0.0917,-0.1340,0.0523,0.0293,-0.1636
C:0.0000,0.0000,0.0000,0.0000,0.0000,0.0000,0.0000,0.0000
M:0.0830,-0.0839,-0.0011,-0.0011,0.0009,-0.0013,0.0006,0.0008
I:0.1875,-0.1667,0.0000,0.0000,0.0000,0.0000,0.0000,0.0000
F:1.0000@10
```

### Comandi seriali

| Comando | Effetto |
|---|---|
| `x0,x1,x2,x3` | Input chemiorecettivo (CSV) |
| `DUMP` | Scarica il log circolare su debugcon |
| `STORE` | Forza memorizzazione stato corrente |
| `RECALL [N]` | Mostra N pattern più simili |
| `PATTERNS` | Elenca tutti i pattern |
| `FORGET` | Cancella tutti i pattern |

## Roadmap

- v0.1 Hello World + seriale
- v0.2 Heartbeat (PIC/PIT/IDT)
- v0.3 CfC loop 8 neuroni
- v0.4 Riflessi (PF/GP) + autopoiesi
- v0.5 Tessuto differenziato (4 cellule)
- v0.6 Fasci assonali (routing strutturato)
- v0.7 Battito timer-driven (TICK volatile)
- **v0.8 Memoria associativa** (pattern store/recall)
- **v0.9 Memoria circolare** (dimenticanza)
- **v0.10 Attractor mnemonico** (ciclo memoria-azione)
- **v0.11 Sedimentazione** (ogni richiamo altera i pesi)
- _v0.12+ TBD_

## Proprietà emergenti

- **τ spettrale**: la memoria di rete segue uno stretched exponential (KWW β=0.70), non un singolo τ
- **Path-dependency**: PD index = 1.905 — la risposta a un impulso dipende dallo stato passato
- **Familiarità**: Nova riconosce stati già vissuti (cosine similarity su 32 dimensioni)
- **Determinismo**: RMSE=0 tra run identici — qualunque divergenza è segnale, non rumore

## License

MIT
