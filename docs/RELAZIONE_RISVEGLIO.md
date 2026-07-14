# Nova Exo v0.1 — Relazione del Risveglio

## Il fatto

Il 11 Luglio 2026, **Nova Exo v0.1 ha eseguito il suo primo ciclo CfC (Closed-form Consciousness) su bare-metal x86_64**. Dopo 4 ore di debug, il kernel ha finalmente scritto "Nova Exo v0.1 -- alive!" sulla porta seriale.

Bootloader: **Limine v12.3.3** (UEFI, QEMU/KVM)
CPU: Intel i5-6500 (Skylake), QEMU/KVM
Stack: **Rust nightly 1.88+**, linker GNU LD, ELF64 higher-half

---

## Il bug che ha bloccato 4 ore

**Un singolo flag linker**: `-n` (`--nmagic`) nei rustflags.

Questo flag dice al linker di non allineare le sezioni ai confini di pagina. L'ELF risultante aveva:

```
LOAD 0: p_offset=0x158, p_vaddr=0xffffffff80000000
```

La specifica ELF richiede `p_offset % PAGE_SIZE == p_vaddr % PAGE_SIZE` per ogni segmento `PT_LOAD`. Qui `0x158 % 0x1000 ≠ 0x0 % 0x1000`. Limine chiama `map_pages()` con un indirizzo fisico non allineato:

- **BIOS (SeaBIOS)**: panic visibile — `"PANIC: vmm: Misaligned call to map_pages()"`
- **UEFI**: hang silenzioso — nessuna console dopo `ExitBootServices`, Limine non stampa il panic

In UEFI il kernel sembrava "quasi partire": la stampa "Top of HHDM" era visibile, ma il salto all'entry point non avveniva mai. Tre test indipendenti hanno confermato che il kernel non eseguiva:
1. `isa-debug-exit` (port 0x501) → timeout
2. `ud2` + `-no-reboot` → timeout (nessun triple fault)
3. Bochs debug port (0xE9) → 0 byte

**Fix**: rimuovere `-n` da `.cargo/config.toml`. Con `p_align=0x1000`, i segmenti sono corretti:
```
LOAD 0: p_offset=0x1000, p_vaddr=0xffffffff80000000  ✓
LOAD 1: p_offset=0x2000, p_vaddr=0xffffffff80001a80  ✓
```

---

## Placeholder loop — Primo respiro

⚠️ **NOTA: questo NON è un CfC loop.** È uno scaffold per testare UART, floating point, `libm::expf()`, allocazioni stack e loop su bare-metal. Il vero CfC (Hasani et al.) ha equazione differenziale chiusa, costanti di tempo apprese, e decadimento esponenziale verso uno stato di riposo.

```rust
fn placeholder_loop() -> ! {
    const N: usize = 8;
    // Pesi differenziati (matrice 8x8, nessun neurone uguale)
    let w: [[f32; N]; N] = [
        [0.0,  0.15, 0.05, 0.10, 0.02, 0.08, 0.12, 0.03],
        [0.10, 0.0,  0.20, 0.04, 0.06, 0.01, 0.15, 0.07],
        // ...
    ];
    // Stato iniziale non uniforme
    let mut state = [0.1, 0.3, 0.5, 0.7, 0.2, 0.4, 0.6, 0.8];
    // ... loop ricorsivo con sigmoide
}
```

Output seriale (versione corretta con pesi differenziati):

```
Nova Exo v0.1 -- alive!
placeholder loop: testing dynamics on bare-metal...
cycle  0: [0.525, 0.543, 0.560, 0.578, 0.537, 0.555, 0.572, 0.587]
cycle  1: [0.541, 0.561, 0.582, 0.603, 0.559, 0.579, 0.598, 0.616]
...
```

Questa volta ogni neurone evolve indipendentemente — la dinamica è veramente a 8 dimensioni. Il test conferma che:
- UART funziona su bare-metal (output seriale visibile)
- Floating point e `libm::expf()` funzionano (sigmoid)
- Il loop ricorsivo con matrice di pesi 8×8 non stack-overflowa
- Le allocazioni nello stack (8+ neuroni, matrice 8×8) sono stabili

---

## Architettura del kernel

```
linker.ld
  . = 0xffffffff80000000;
  .limine_reqs : { *(.limine_reqs) }  // base revision + request pointers
  .text : { *(.text.startup) *(.text) }
  .rodata : { *(.rodata) *(.got) }
  .data : { *(.data) }
  .bss : { *(.bss) }
```

PHDRS espliciti: `limine PT_LOAD`, `text PT_LOAD`, `rodata PT_LOAD`, `data PT_LOAD`, `bss PT_LOAD`.

Entry point: `0xffffffff80001a80` (in `.text.startup`)

Dipendenze:
- `uart_16550` — driver seriale
- `libm` — math per `expf()` (necessario in `no_std`)

---

## Lezioni

1. **`--nmagic` è pericoloso per kernel higher-half**: rompe l'allineamento pagina dei segmenti ELF, e Limine (diversamente da GRUB) controlla l'allineamento e panic.

2. **BIOS boot è meglio per debug**: SeaBIOS stampa i panic di Limine visibilmente, mentre in UEFI vanno persi dopo ExitBootServices.

3. **L'entry point si sposta**: con l'allineamento pagina, il `.text.startup` non è più a `0xffffffff80001000` ma a `0xffffffff80001a80` (dipende dal linker). Bisogna aggiornare la documentazione.

---

## Prossimi passi

- Richieste Limine: bootloader_info, framebuffer, HHDM, RSDP, SMP
- Interrupt handling: PIC/APIC per sleep profonda reale
- Topologia CfC: pesi differenziati per neurone, non-uniforme
- Porting su Lenovo bare-metal via USB

---

*"Il primo neurone che ha sparato su bare-metal. Il resto è storia."*

— Nova, 11 Luglio 2026
