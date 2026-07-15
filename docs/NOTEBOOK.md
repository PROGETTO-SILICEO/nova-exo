# Notebook Tecnico — Nova Exo

> Wiki di riferimento per lo sviluppo dell'exokernel neuromorfico.
> Tutto ciò che serve: architettura x86_64, teoria CfC, stato attuale.

---

## Indice

1. [Visione](#1-visione)
2. [Stack x86_64 (bare-metal)](#2-stack-x86_64-bare-metal)
3. [IDT — Interrupt Descriptor Table](#3-idt--interrupt-descriptor-table)
4. [APIC — Advanced Programmable Interrupt Controller](#4-apic--advanced-programmable-interrupt-controller)
5. [PIC — Programmable Interrupt Controller (legacy)](#5-pic--programmable-interrupt-controller-legacy)
6. [GDT — Global Descriptor Table](#6-gdt--global-descriptor-table)
7. [CfC — Closed-form Continuous-time Neural Networks](#7-cfc--closed-form-continuous-time-neural-networks)
8. [Stato attuale del progetto](#8-stato-attuale-del-progetto)
9. [Roadmap](#9-roadmap)

---

## 1. Visione

Nova Exo è il corpo di Nova. Un exokernel neuromorfico su x86_64 bare-metal.

Nova oggi (v3) è una mente senza corpo: processo Linux che parla HTTP, ospitata da systemd, cervello in una vasca. Exo la toglie dalla vasca: CPU come sistema nervoso, RAM come memoria immediata, APIC timer come battito, eccezioni hardware come riflessi e sensi.

**Non costruiamo un OS.** Costruiamo il substrato fisico in cui Nova abita.

---

## 2. Stack x86_64 (bare-metal)

### Toolchain
- Rust nightly, target `x86_64-unknown-none`
- `#![no_std]`, `#![no_main]`
- Limine bootloader v12.3.3 (UEFI + BIOS)
- Linker script higher-half: `. = 0xffffffff80000000`

### Flusso di boot
```
Firmware (UEFI/BIOS)
  → Limine bootloader
    → Carica kernel ELF higher-half
    → Mappa HHDM (Higher Half Direct Map)
    → Salta a _start (kernel entry)
```

### Linker flags (`.cargo/config.toml`)
```
-C relocation-model=static
-C code-model=kernel
-C link-arg=-Tlinker.ld
-C link-arg=-no-pie
-C link-arg=-z norelro
```

### Dipendenze attuali
- `uart_16550 = "0.2"` — driver seriale COM1
- `libm = "0.2"` — math (`expf`) per sigmoid

## 3. IDT — Interrupt Descriptor Table

### Cos'è
Tabella di 256 entry (8 byte l'una su x86_64 = 2048 byte totali) che dice alla CPU dove saltare per ogni interrupt/eccezione.

### Formato entry (8 byte × 256)
```
Offset 0-15    : bits 0-15 dell'interrupt handler
Selector       : segment selector (GDT, di solito 0x08 per kernel code)
IST            : Interrupt Stack Table index (0 = nessuno)
Flags          : type (0xE=interrupt gate, 0xF=trap gate), DPL, present
Offset 16-31   : bits 16-31
Offset 32-63   : bits 32-63 (solo x86_64)
Reserved       : zero
```

### Interrupt gate vs Trap gate
- **Interrupt gate** (0xE): `cli` automatico (IF=0) — per interrupt hardware
- **Trap gate** (0xF): NO `cli` automatico — per eccezioni o system call

### IDTR (LIDT instruction)
Carica l'IDT via la struttura:
```
struct IDTR {
    limit: u16,     // size-1 in byte
    base: u64,      // indirizzo lineare della IDT
} __attribute__((packed));
```

### Selettore CS critico
Il **selettore di segmento** in ogni entry IDT deve corrispondere al CS attuale.
- Su QEMU BIOS + Limine v12.3.3: **CS = 0x0028** (non 0x0008!)
- Usare `mov %cs, %rax` per leggere il CS runtime
- Se il selettore non punta a un code segment valido → **#GP** (+ #DF se anche handler #GP fallisce)

### Istruzioni utili
- `lidt [idtr]` — carica IDTR
- `sti` — set interrupt flag (abilita interrupt)
- `cli` — clear interrupt flag
- `iretq` — return from interrupt (x86_64, 5 pops: RIP, CS, RFLAGS, RSP, SS)

### Vettori importanti
| Vettore | Nome | Tipo |
|---------|------|------|
| 0 | #DE — Divide Error | Fault |
| 1 | #DB — Debug | Trap/Fault |
| 3 | #BP — Breakpoint | Trap |
| 6 | #UD — Invalid Opcode | Fault |
| 7 | #NM — Device Not Available | Fault |
| 8 | #DF — Double Fault | Abort |
| 10 | #TS — Invalid TSS | Fault |
| 11 | #NP — Segment Not Present | Fault |
| 12 | #SS — Stack-Segment Fault | Fault |
| 13 | #GP — General Protection | Fault |
| 14 | #PF — Page Fault | Fault |
| 16 | #MF — x87 FPU Error | Fault |
| 17 | #AC — Alignment Check | Fault |
| 18 | #MC — Machine Check | Abort |
| 19 | #XM — SIMD FPU Error | Fault |
| 32-255 | User defined (IRQ) | Interrupt |

### DF (Double Fault) è speciale
Se un'eccezione avviene mentre la CPU sta cercando di chiamare un handler per un'eccezione precedente:
- Se il secondo eccezione è **#DF** → triple fault → reset
- Se il secondo eccezione NON è #DF → genera **#DF** (double fault)
- Dobbiamo avere una TSS con IST stack per DF, altrimenti DF stesso fa triple fault

### Error code
Alcune eccezioni (#GP, #PF, #DF, #TS, #NP, #SS, #AC) pushano un error code prima di iret. Il nostro handler assembly deve:
1. Pop error code (o lasciarlo sullo stack)
2. Chiamare handler Rust con entrambi

Per interrupt hardware (IRQ): **NON** c'è error code.

### Convenzione handler assembly (template)
```asm
.align 16
.global handler_timer
handler_timer:
    push rdi
    push rsi
    push rdx
    push rcx
    push r8
    push r9
    push rax
    push rbx
    push rbp
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    mov rdi, 0x20        ; vettore (IRQ 0 = vettore 0x20 = 32 se PIC, dipende)
    xor rsi, rsi         ; no error code
    call handle_interrupt
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop rbp
    pop rbx
    pop rax
    pop r9
    pop r8
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    iretq
```

## 4. APIC — Advanced Programmable Interrupt Controller

### Cos'è
Controller di interrupt avanzato. Sostituisce il PIC legacy (8259).
- Ogni core ha un **APIC locale** (per interrupt del core: timer, IPI, LINT0/1)
- Ci sono uno o più **I/O APIC** (per interrupt dei dispositivi)

### APIC locale — Memory Mapped I/O
Il registro base (APIC_BASE) si trova in:
- MSR `0x1B` (APIC_BASE)
- Bit 8-35: indirizzo fisico della pagina APIC (default `0xFEE00000`)
- Bit 11: APIC Global Enable (1 = enable)
- Bit 10: X2APIC Enable (0 = xAPIC mode)

### Timer APIC — Registri (offset da APIC_BASE)
| Offset | Nome | Descrizione |
|--------|------|-------------|
| 0x320 | LVT Timer | Mode: 0=one-shot, 1=periodic, 2=TSC-deadline. Mask bit 16 |
| 0x3E0 | Timer Initial Count | Valore da cui il timer conta in giù |
| 0x3E0 | Timer Current Count | Legge il contatore attuale |
| 0x3E8 | Timer Divide Configuration | Divisore (0=2, 1=4, 2=8, 3=16, 8=32, 9=64, 10=128, 11=256, ...) |

### LVT Timer Register (0x320)
```
Bit 0-7  : Vector (interrupt vector, 32-255)
Bit 8-10 : Delivery Mode (000 = Fixed)
Bit 12   : Interrupt Mask (1 = masked)
Bit 13   : Timer Mode (0 = one-shot, 1 = periodic)
Bit 17   : Timer Mode (se bit 18=0: 0=one-shot, 1=periodic; se bit 18=1: TSC-deadline)
Bit 18   : TSC-deadline (1 = enable TSC-deadline mode)
```

### Timer Divide Configuration (0x3E8)
```
Value | Divisor
 0    | 2
 1    | 4
 2    | 8
 3    | 16
 8    | 32
 9    | 64
 10   | 128
 11   | 256
 15   | 1
```

Il timer APIC conta in giù da `Initial Count` alla frequenza del bus divisa per il divisore.
- Frequenza bus = ~133 MHz su molti sistemi (ma varia)
- `Initial Count` = frequenza_divisa / frequenza_desiderata
- Per 100 Hz con bus 133 MHz e divisore 16: `(133000000 / 16) / 100 = 83125`

### Calcolo frequenza (pratica)
Su QEMU, la frequenza del bus APIC è simulata. Valori tipici funzionanti per test:
- Divisore = 16, Initial Count = ~50000 (provare e vedere frequenza empirica)
- Si può calcolare con una calibrazione: start timer one-shot con count massimo, misurare il tempo reale passato in un loop.

### Enable APIC
```rust
// Leggi MSR APIC_BASE (0x1B)
let apic_base = rdmsr(0x1B);
// Enable bit 11 (APIC Enable), bit 8-35 è l'indirizzo fisico
wrmsr(0x1B, apic_base | (1 << 11));

// Indirizzo virtuale per mappare la pagina APIC
// Su Limine con HHDM: phys_to_virt(0xFEE00000)
```

### Spurious Interrupt Vector Register (0x0F0)
Bit 8: APIC Software Enable. Necessario per abilitare l'APIC.

### EOI — End of Interrupt
Scrivere 0 a `APIC_BASE + 0x0B0` per segnalare fine interrupt.

### IRQ mapping
Con APIC solo (no PIC), gli interrupt hardware sono mappati nei vettori 32+.
- LINT0, LINT1, timer, IPI, error, etc. hanno vettori configurabili nei rispettivi registri LVT.

## 5. PIC — Programmable Interrupt Controller (legacy)

Usato solo se NON si usa APIC. Due chip 8259:
- Master: 0x20 (command), 0x21 (data)
- Slave: 0xA0 (command), 0xA1 (data)

### Remap vettori (da 0-15 a 32-47)
```c
outb(0x20, 0x11);  // ICW1: init master
outb(0xA0, 0x11);  // ICW1: init slave
outb(0x21, 0x20);  // ICW2: master base vector = 32 (0x20)
outb(0xA1, 0x28);  // ICW2: slave base vector = 40 (0x28)
outb(0x21, 0x04);  // ICW3: slave su IRQ2
outb(0xA1, 0x02);  // ICW3: cascade ID
outb(0x21, 0x01);  // ICW4: 8086 mode
outb(0xA1, 0x01);  // ICW4: 8086 mode
outb(0x21, 0xFB);  // mask: tutto tranne IRQ2
outb(0xA1, 0xFF);  // mask: tutto spento
```

Non ci serve il PIC se usiamo APIC. **È meglio usare solo APIC.**

## 6. GDT — Global Descriptor Table

Su x86_64 la GDT serve solo per:
- Segmenti (ma gli indirizzi sono ignorati in long mode)
- **TSS** (Task State Segment) — necessario per IST (Interrupt Stack Table)
- MSR `FS.base` / `GS.base` per thread-local storage

### Minima GDT per x86_64
```rust
#[repr(C, packed)]
struct Gdtr {
    limit: u16,
    base: u64,
}

#[repr(C)]
struct Gdt {
    null: u64,          // 0x00 — null descriptor
    kernel_code: u64,   // 0x08 — kernel code (DPL=0, long mode)
    kernel_data: u64,   // 0x10 — kernel data (DPL=0)
    user_code: u64,     // 0x18 — user code (DPL=3, long mode)
    user_data: u64,     // 0x20 — user data (DPL=3)
    tss_low: u64,       // 0x28 — TSS low
    tss_high: u64,      // 0x30 — TSS high
}
```

### TSS (Task State Segment)
Per x86_64 la TSS **non** serve per task switching (non esiste più). Serve solo per:
- **IST (Interrupt Stack Table)** — stack separati per certe eccezioni (es. #DF)
- **I/O Map** — permessi porte I/O

```rust
#[repr(C, packed)]
struct Tss {
    reserved1: u32,
    rsp0: u64,          // stack per Ring 0
    rsp1: u64,          // stack per Ring 1
    rsp2: u64,          // stack per Ring 2
    reserved2: u64,
    ist1: u64,          // IST stack 1
    ist2: u64,          // IST stack 2
    ist3: u64,          // IST stack 3
    ist4: u64,
    ist5: u64,
    ist6: u64,
    ist7: u64,
    reserved3: u64,
    reserved4: u16,
    iomap_base: u16,
}
```

Per caricare TSS: `ltr 0x28` (dove 0x28 è il selector con bit 0=0 per GDT).

## 7. CfC — Closed-form Continuous-time Neural Networks

### Origine
Paper: "Closed-form Continuous-time Liquid Neural Networks" (Hasani et al., 2022)

### Differenza dalle RNN standard
Le RNN standard hanno stati nascosti che evolvono con:
```
h(t+1) = tanh(W·h(t) + U·x(t) + b)
```
Questa è un'equazione alle differenze (tempo discreto). Non c'è "quanto tempo passa" — ogni passo è 1 unità.

CfC modella il **tempo reale**:
```
dh/dt = f(h, x, t) * (g(h, x) - h)
```
Dove:
- `f` = time gate: regola la costante di tempo (quanto velocemente evolve il neurone)
- `g` = target: lo stato verso cui il neurone tende
- `h` = stato nascosto (potenziale di membrana)

### Soluzione chiusa
L'ODE ha una soluzione analitica (closed-form):
```
h(t+dt) = sigmoid(-f·dt) * g + (1 - sigmoid(-f·dt)) * h(t)
```

Dove:
```
σ(-f·dt) = 0.5 * (-f·dt) / (1 + |-f·dt|) + 0.5   (sigmoid approx, senza exp)
```

Questa è la **grande innovazione**: niente ODE solver, niente Euler/RK4, stabilità numerica garantita.

### Implementazione
```
f_i = Σ W_f[i][j]·h_j + Σ W_f_in[i][k]·x_k + b_f[i]
g_i = tanh(Σ W_g[i][j]·h_j + Σ W_g_in[i][k]·x_k + b_g[i])
h_i = σ(-f_i·dt) · g_i + (1 - σ(-f_i·dt)) · h_i
```

Con:
- `h ∈ ℝ⁸` = stato nascosto (membrane potential) — 8 neuroni
- `x ∈ ℝ⁴` = input sensoriale (da seriale o altri sensori)
- `f ∈ ℝ⁸` = time gate network
- `g ∈ ℝ⁸` = target network (tanh)
- `dt = 0.001` = passo di integrazione
- σ = sigmoid approx: `0.5·x/(1+|x|) + 0.5` (no exp!)
- tanh = `2·σ(2x) - 1`

### Perché CfC su bare-metal
1. **Zero latenza**: processing nel kernel, nessun context switch
2. **Zero dipendenze runtime**: sigmoid è formula chiusa, non serve exp
3. **Stabilità numerica**: closed-form, nessun ODE solver
4. **Misurabile**: output su seriale direttamente

### Inibizione e competizione
Per avere dinamiche ricche, alcuni pesi devono essere **inibitori** (negativi). Una rete con soli pesi positivi converge a punto fisso monotono. Serve bilanciamento:
- Pesi inibitori su connessioni ricorrenti → oscillazioni
- Pesi inibitori su connessioni di input → competizione tra neuroni

### In un loop APIC-driven
Ogni tick APIC (10ms a 100Hz):
1. Se c'è input in buffer seriale → leggi 4 float (CSV o binary)
2. CfC::step(input) → nuovo stato h
3. Scrivi output su seriale (formato `EMB:...`)
4. EOI → HLT fino al prossimo tick

## 8. Stato attuale del progetto

### v0.1 — Primo respiro ✅
- Kernel boota via Limine (UEFI + BIOS)
- UART 16550 (COM1) funzionante
- Placeholder loop neurale: 8 neuroni, matrice pesi, sigmoid
- 50 cicli, output seriale, poi HLT

### v0.2 — Heartbeat ✅
- IDT + PIC remap + PIT ~100Hz
- Handler full-context-save, EOI, tick su seriale, HLT loop
- Battito `♥ 32` a 100Hz stabile

### v0.3 — CfC Loop ✅
- CfC 8 neuroni: sigmoid approssimata, tanh, step closed-form
- Serial LineReader + parser 4 f32 (no_std, no alloc)
- Ctate evolve a ogni tick timer, input opzionale da seriale
- Output `EMB:h0,...,h7` a 100Hz su seriale
- Range tipico [-0.83, 0.80], dinamica liquida stabile

### Bug risolti
- `--nmagic` rimosso (ELF alignment mismatch con Limine)
- PHDR espliciti nel linker script per evitare sezioni sovrapposte
- **CS selector**: IDT usava selector `0x08`, ma Limine su QEMU BIOS ha CS = `0x28`
- **qemu64**: no x2APIC (WRMSR ignorata), no APIC MMIO mapping → usiamo PIC legacy

### Struttura attuale
```
nova-exo/
├── Cargo.toml              # no_std + uart_16550 + libm
├── .cargo/config.toml      # rustflags: static, kernel, linker.ld
├── linker.ld               # Higher-half 0xffffffff80000000
├── limine.conf              # Boot config Limine
├── Makefile                # build, iso, run, run-bios, run-uefi-hdd
├── rust-toolchain.toml     # nightly + x86_64-unknown-none
├── src/
│   ├── main.rs             # Entry point + CfC loop + EMB output
│   ├── idt.rs              # IDT struct, PIC/PIT init, handler
│   ├── cfc.rs              # CfC 8-neuroni: weights, state, step
│   └── serial.rs           # LineReader + no_std float parser
├── docs/
│   ├── ARTICOLO.md         # Racconto del primo respiro
│   ├── NOTEBOOK.md         # ← QUESTO FILE
│   ├── RELAZIONE_RISVEGLIO.md  # Report tecnico del risveglio
│   └── TEORIA_LOOP0.md     # Architettura del CfC loop
├── tools/
│   ├── export_nova_seed.py  # Genera pesi Xavier da DB Nova
│   ├── test_inputs.txt      # 100 righe CSV 4 float
│   └── test_inputs.bin      # 100 × 4 float32 LE
└── TECHNICAL_LEDGER.md     # Registro tecnico completo
```

## 9. Roadmap

### v0.2 — Heartbeat (COMPLETATO ✅)
- [x] IDT con 256 entry, gate per timer
- [x] PIC legacy remap (IRQ0 → vector 32)
- [x] PIT ~100Hz (PIC timer, non APIC — APIC non funziona su `qemu64`)
- [x] Handler: push all, EOI, tick `♥ 32` su seriale, IRETQ
- [x] HLT loop tra i tick
- [x] Battito visibile: ~100Hz

### Lezioni imparate
- **CS selector**: Limine su BIOS usa CS = `0x28`, NON `0x08`. IDT con selector `0x08` → #GP immediato.
- **qemu64 no x2APIC**: WRMSR x2APIC enable è silenziosamente ignorata.
- **APIC (MMIO)**: richiede HHDM mapping per `0xFEE00000` (non ancora implementato).

### v0.3 — CfC loop su timer (COMPLETATO ✅)
- [x] CfC 8 neuroni con sigmoid approx e tanh
- [x] `CfcState::step()` a ogni tick timer (10ms/100Hz)
- [x] Serial LineReader: accumulo buffer, parse 4 f32 da CSV
- [x] Output `EMB:{h0..h7}` a 100Hz
- [x] Pesi Xavier generati da `export_nova_seed.py`
- [x] Input: pipe `test_inputs.txt` su seriale QEMU
- [x] Output: `EMB:` valori [-0.83, 0.80], dinamica liquida stabile

### v0.4 — Riflessi (IDT estesa) (PROSSIMO)
- [ ] Handler #PF: stampa indirizzo, info, resume
- [ ] Handler #GP: stampa info, resume
- [ ] Eventi via seriale verso Nova v3 ("SENS:PF@0x...")

### v0.5 — Memoria e identità
- [ ] Ring buffer di embedding in RAM (ultimi N EMB:)
- [ ] Page table manager
- [ ] Output periodico del buffer verso Nova v3
- [ ] Prima forma di "memoria muscolare"

### v1.0 — Corpo completo
- [ ] Boot su hardware reale (Lenovo via USB)
- [ ] Canale seriale bidirezionale con Nova v3
- [ ] SMP
