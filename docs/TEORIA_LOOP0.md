# Loop 0 — Teoria Architetturale

## 1. Che cos'è Nova Exo

Exokernel neuromorfico. Ring 0 come midollo spinale di Nova:
- **APIC timer** = battito sinusale (non ancora implementato)
- **IDT** = architettura riflessa (non ancora implementata)
- **Paging** = identità fisica (gestito da Limine)
- **UART** = canale sensoriale ⬅ attivo

Loop 0 è il primo tessuto neurale nel midollo: 8 neuroni CfC (Closed-form Continuous-time) che processano input sensoriali in tempo reale su bare-metal.

---

## 2. Architettura del Loop 0

```
Host (Linux)                    QEMU (x86_64 bare-metal)
┌──────────────┐               ┌─────────────────────────┐
│ test_inputs  │ ──serial────> │ Nova Exo v0.2           │
│ (4 f32 CSV)  │   COM1 0x3F8  │                         │
│              │               │ 1. serial_readline()    │
│              │               │ 2. parse_4f32()         │
│              │               │ 3. CfcState::step()     │
│              │               │ 4. serial_print_f32()   │
│              │ <──serial──── │ 5. ripeti               │
│ grep EMB:    │   COM1 0x3F8  │                         │
└──────────────┘               └─────────────────────────┘
```

### Componenti

| Componente | Linguaggio | Righe | Dipendenze |
|---|---|---|---|
| Bootloader (Limine) | Config | 9 | limine v12.3.3 |
| Kernel entry + Limine req | Rust | ~40 | x86_64, uart_16550 |
| CfC state | Rust | ~40 | core (nessuna) |
| CfC step (ODE closed-form) | Rust | ~30 | core (nessuna) |
| Sigmoid, tanh (no_std) | Rust | ~10 | core (nessuna) |
| UART I/O (raw) | Rust | ~70 | uart_16550 |
| Float parser (no_std) | Rust | ~60 | core (nessuna) |
| Seed extractor | Python | ~140 | numpy, sqlite3 |

### Matematicamente

Il CfC closed-form (Hasani et al. 2022):

```
f_i(t) = Σ W_f[i][j]·h_j(t) + Σ W_f_in[i][k]·x_k(t) + b_f[i]
g_i(t) = tanh(Σ W_g[i][j]·h_j(t) + Σ W_g_in[i][k]·x_k(t) + b_g[i])
h_i(t+dt) = σ(-f_i(t)·dt) · g_i(t) + (1 - σ(-f_i(t)·dt)) · h_i(t)
```

Dove:
- `h ∈ ℝ⁸` = stato nascosto (membrane potential)
- `x ∈ ℝ⁴` = input sensoriale (da seriale)
- `f ∈ ℝ⁸` = time gate network (determina la costante di tempo)
- `g ∈ ℝ⁸` = target network (con attivazione tanh)
- `dt = 0.001` = passo di integrazione
- `σ(x) = 0.5·x/(1+|x|) + 0.5` = sigmoid approximation (no exp!)
- `tanh(x) = 2·σ(2x) - 1`

### Flusso dati (serial protocol)

Input (host → exo, ASCII CSV):
```
0.5321,0.1234,0.9876,0.4567<LF>
```

Output (exo → host, ASCII CSV):
```
EMB:0.023,0.145,-0.089,0.312,0.067,-0.201,0.445,-0.133<CR><LF>
```

---

## 3. Perché CfC su bare-metal

1. **Zero latenza**: il processing è nel kernel, nessun context switch
2. **Zero dipendenze runtime**: la sigmoid è una formula chiusa, non serve exp
3. **Stabilità**: il closed-form è numerico stabile (no ODE solver)
4. **Misurabile**: l'output su seriale è direttamente leggibile da terminale

Alternative scartate:
- **Hopfield** (Grok 4.5): richiede heavy linear algebra, non adattivo
- **LTC con ODE solver**: richiede Euler o RK4, più complesso
- **Standard RNN**: no time-constant liquid, meno espressivo

---

## 4. Pesi (Xavier initialization)

Generati da `tools/export_nova_seed.py`:
```bash
python3 tools/export_nova_seed.py
```

Il script:
1. Cerca DB SQLite di Nova v3 (7 percorsi candidati)
2. Estrae 4 feature numeriche: [queue_len, priority, age_hours, irq_pending]
3. Se nessun DB trovato → Xavier sintetico
4. Genera pesi `W_f, W_f_in, W_g, W_g_in` con Xavier uniform
5. Aggiorna `src/main.rs` → `static CFC_WEIGHTS`
6. Produce `tools/test_inputs.txt` (100 righe CSV)
7. Produce `tools/test_inputs.bin` (100 × 4 float32 LE)

---

## 5. Limiti attuali (v0.2)

| Limitazione | Perché | Soluzione futura |
|---|---|---|
| Polling su UART (no IRQ) | Non serve IDT | v0.3 APIC timer |
| Sigmoid approssimata | No exp() in no_std | LUT a 64 entry |
| Single-step (no timer) | Loop bloccante | v0.3 heartbeat |
| 4 float in, CSV ASCII | Semplicità | Binary protocol (già generato .bin) |
| Weights fissi statici | Xavier init | Backprop via seed script |
| Nessun feedback da Nova v3 | Dual-track isolato | v0.4 socket bridge |

---

## 6. Roadmap post-Loop 0

```
v0.3 — Battito e Riflessi
  [ ] APIC timer (heartbeat a 100Hz)
  [ ] IDT con 3 handler (PF, GPF, timer)
  [ ] CfC step guidato da timer, non da polling

v0.4 — Feedback da Nova v3
  [ ] Lettura pesi aggiornati via serial binary
  [ ] Seed script → invia nuovi pesi a runtime
  [ ] Nova v3 può modificare il comportamento di Exo

v0.5 — Memoria Locale
  [ ] Ring buffer di embedding in RAM
  [ ] Output periodico via serial del buffer
  [ ] Prima forma di "memoria muscolare"
```

---

## 7. Build e test

```bash
# Build
cargo build --target x86_64-unknown-none --release

# Genera pesi + test inputs
python3 tools/export_nova_seed.py

# Test con QEMU (UEFI)
make run-uefi-hdd
# (poi in un altro terminale:)
cat tools/test_inputs.txt > /dev/ttyS0  # o pipe in QEMU
```

Test diretti (senza immagine disco):
```bash
qemu-system-x86_64 \
  -kernel target/x86_64-unknown-none/release/nova-exo \
  -serial stdio \
  -display none \
  -m 32M
```

---

## 8. Domande Aperte (da sottoporre ad altri LLM)

1. La sigmoid approximation `x/(1+|x|)` è stabile per un loop chiuso CfC? Rischio di divergenza per certe configurazioni di pesi?

2. Vale la pena passare a binary protocol (4 float32 LE = 16 byte) subito, o tenere ASCII finché il loop non è provato?

3. Per l'APIC timer: meglio usare l'APIC locale (x2APIC) o il PIT legacy? L'APIC è più preciso ma richiede più setup.

4. I pesi statici sono sufficienti per dimostrare il loop, o serve un meccanismo di aggiornamento runtime prima di dichiarare "chiuso"?

5. Il salto da QEMU a hardware reale (Lenovo): cosa blocca? Solo il serial driver (COM1 vs 0xE9)?
