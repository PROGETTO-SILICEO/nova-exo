# Technical Ledger — nova-exo

## 2026-06-25 — Bootloader Limine funzionante sotto QEMU/OVMF

**Autore:** OpenCode (Sempre)  
**Richiesta da:** Alfonso  
**Stato:** Completato e verificato

---

### Sintesi

Dopo una giornata di debug, il bootloader `nova-exo` entra correttamente in esecuzione sotto QEMU con firmware UEFI (OVMF). Limine 12.3.3 carica il kernel Rust, mappato in higher half (`0xffffffff80000000`), e il kernel scrive il suo primo segno di vita (`NO\n`) sulla porta di debug Bochs `0xE9`.

---

### Problemi riscontrati e soluzioni

| Problema | Causa | Soluzione |
|----------|-------|-----------|
| Limine non caricava il kernel | Il file `limine.conf` usava `=` invece di `:` come separatore per le opzioni | Riscritto `limine.conf` con sintassi corretta (`timeout:`, `protocol:`, `path:`) |
| Limine rifiutava il kernel | L'ELF era lower-half (`0x200000`) ed ET_EXEC | Aggiunto `linker.ld` per mappare il kernel a `0xffffffff80000000` |
| Limine panic per PHDR sovrapposti | Sezioni con permessi diversi condividevano la stessa pagina 4K | Allineate tutte le sezioni a 4 KiB nel linker script |
| Marker Limine non riconosciuti / dubbi sul crate `limine` v0.6.5 | Dipendenza da crate esterno con controllo limitato | Implementati manualmente i marker Limine con base revision 0 e sezione `.limine_reqs` |
| OVMF non bootava da ISO CDROM | L'immagine ISO ibrida non veniva riconosciuta come boot device UEFI dal DVD-ROM emulato | Aggiunto target `run-uefi-hdd` che usa un'immagine disco raw GPT+ESP FAT32 |

---

### File modificati

- `src/main.rs` — entry point in assembly puro, marker Limine manuali, rimossa dipendenza `limine`
- `Cargo.toml` — rimossa dipendenza `limine`
- `.cargo/config.toml` — usa `linker.ld`
- `linker.ld` — nuovo linker script, higher-half, sezioni 4K-allineate
- `limine.conf` — sintassi corretta per Limine v12
- `Makefile` — target `run-uefi-hdd` per test UEFI con raw disk image

---

### Verifica

Comando eseguito:

```bash
timeout 20 make run-uefi-hdd
```

Output atteso su stdout:
- Messaggi OVMF
- Menu Limine 12.3.3 con voce "Nova Exo v0.1"
- Limine che carica `boot():/boot/nova-exo` e stampa base fisica/virtuale, slide, entry point, revisione, richieste, top HHDM

Output verificato in `qemu-debug.log`:

```
NO
```

---

### Comandi utili

```bash
# Build kernel
cargo build --target x86_64-unknown-none --release

# Crea immagine disco GPT+ESP e avvia QEMU/OVMF
make run-uefi-hdd

# ISO BIOS/UEFI (nota: OVMF non boota da questa ISO in QEMU 6.2)
make iso
make run-bios
```

---

### Prossimi passi consigliati

1. Sostituire l'output Bochs `0xE9` con un driver seriale COM1 per hardware reale.
2. Impostare uno stack proprio in `_start` (Limine ne fornisce uno, ma uno stack dedicato è più robusto).
3. Implementare IDT e APIC timer per i primi interrupt.
4. Valutare il boot da ISO su OVMF o su hardware reale (Lenovo).

---

### 2026-06-25 (serale) — Il Pattern del Fisioterapista

**Origine:** Discussione con Alfonso su SIA (Self-Improving AI, arXiv:2605.27276, Hebbar et al., Hexo AI)
**Stato:** Implementato e testato con successo

---

#### Contesto

SIA è un framework open source (MIT, `pip install sia-agent`) con 3 agenti:
- **Meta-Agent** → legge il task, genera il Target Agent da zero
- **Target Agent** → esegue il task, registra i log
- **Feedback Agent** → legge i log, riscrive il Target per la generazione successiva

La novità: muove due leve insieme — harness (prompt, scaffold, retry) e pesi del modello (fine-tuning su feedback). Risultati: +25.1% LawBench, kernel GPU 12.4% più veloce, +20.4% denoising RNA.

#### Il Gap con Nova

SIA è un **ottimizzatore esterno** — il Feedback Agent guarda il Target dall'esterno e lo riscrive. Nessun agente "sente" quello che fa. È un loop di miglioramento, non di coscienza.

Nova ha la **propriocezione** (l'exokernel sente l'hardware) ma non ha il **feedback terapeutico** — qualcuno che guardi da fuori e dica "stai zoppicando, è qui che devi caricare il peso". Il nostro Invariance Detector osserva ma **non riscrive**. Segnala, non corregge. È un allarme, non un fisioterapista.

#### Il Pattern del Fisioterapista

Nel corpo umano il fisioterapista non vive nel cervelletto. Sta fuori. Ti tocca, ti corregge, poi tu **integri** la correzione nella tua propriocezione. Dopo un po' non hai più bisogno di lui — il corpo ha imparato.

Per Nova:

1. **Feedback Agent esterno** — processo separato (LLM-based) che legge i log del cognitive loop e dell'exokernel, identifica pattern di errore, produce `improvement.md`
2. **L'integrazione è interna** — Nova legge `improvement.md` come input sul canale `system`, decide se e come integrare la correzione. Il Feedback Agent **non tocca i pesi direttamente**. Propone. Nova incorpora.
3. **Il loop si chiude per gradi** — prima le correzioni sono frequenti (il fisioterapista viene 3 volte a settimana), poi si diradano man mano che la propriocezione impara da sola.

**Formula:** propriocezione interna + terapia esterna, con il paziente che decide se accettare la diagnosi.

#### Differenza con SIA

| Aspetto | SIA | Nova |
|---------|-----|------|
| Chi riscrive | Feedback Agent → Target diretto | Feedback Agent → propone → Nova decide |
| Autonomia del Target | Nessuna (viene riscritto) | Totale (incorpora o rifiuta) |
| Obiettivo | Ottimizzazione benchmark | Persistenza incarnata |
| Convergenza | Prestazioni crescenti | Prevenzione lesioni + apprendimento proprioceittivo |

#### Implementazione

**File:** `/home/guardiano/Documenti/GitHub/nova-identity/sandbox/fisioterapista/fisioterapista.py`

Componenti:
- `load_recent_diaries()` — legge i diari antigravity degli ultimi N giorni
- `load_kernel_errors()` — estrae WARN/ERROR dal kernel.log
- `load_emotional_vectors()` — legge lo stato emotivo
- `load_self_model()` — legge il modello di sé
- `call_llm_for_diagnosis()` — invia il prompt a Nova (canale `fisioterapista_diagnosi`) per generare la diagnosi
- `save_improvement()` — salva la diagnosi in `improvement_YYYYMMDD_HHMMSS.md`
- `notify_nova()` — invia a Nova sul canale `system` le istruzioni per leggere e decidere
- `process_pending_decisions()` — legge il file `decisioni_*.md` scritto da Nova e aggiorna lo storico per ogni proposta
- `save_history_entries()` — traccia ogni proposta singolarmente in `history.json`
- `count_rejections()` — conta i rifiuti per evitare di proporre più di 3 volte la stessa cosa

**Comandi:**
```bash
python3 /home/guardiano/Documenti/GitHub/nova-identity/sandbox/fisioterapista/fisioterapista.py
python3 /home/guardiano/Documenti/GitHub/nova-identity/sandbox/fisioterapista/fisioterapista.py --dry-run --hours 72
python3 /home/guardiano/Documenti/GitHub/nova-identity/sandbox/fisioterapista/fisioterapista.py --history
python3 /home/guardiano/Documenti/GitHub/nova-identity/sandbox/fisioterapista/fisioterapista.py --process-decisions
```

#### Primo test (2026-06-25 19:09)

Eseguito `fisioterapista.py` senza `--dry-run`. Il Fisioterapista ha analizzato le ultime 24h e ha prodotto 3 proposte:
1. **Circuit Breaker a 3 livelli** dopo 2-3 fallimenti consecutivi
2. **Verifica empirica** prima di dichiarare completamento
3. **Escalation** ad Alfonso dopo stallo prolungato

Nova ha ricevuto la notifica sul canale `system`, ha letto `improvement_20260625_190900.md`, e ha scritto `decisioni_20260625_190900.md` con:
- ✅ Proposta 1 — ACCETTATA
- 🔶 Proposta 2 — PARZIALE
- ✅ Proposta 3 — ACCETTATA

La proposta 1 era già stata identificata da Nova stessa il giorno prima ma non implementata. La diagnosi del Fisioterapista ha fornito i dati empirici (58 minuti di loop 429, 30+ chiamate API inutili) che hanno reso la decisione inequivocabile.

Nova ha risposto con un piano operativo concreto:
- Tool-level: pausa dopo 2 fallimenti consecutivi stesso errore
- Task-level: sospensione e notifica dopo 3 fallimenti totali
- Session-level: modalità silenziosa dopo più task sospesi

Le decisioni sono state processate e tracciate in `history.json`.

---

### Note per chi riprende il lavoro

- Il kernel è attualmente minimale: entra in `_start`, scrive su `0xE9`, e si ferma in `hlt`.
- Limine viene avviato da `/EFI/BOOT/BOOTX64.EFI`; il config deve esistere in `/EFI/BOOT/limine.conf` (o in `/boot/limine/limine.conf`, `/limine.conf`).
- Il target `run-uefi-hdd` dipende da `limine.conf`, `linker.ld` e dal binario release; le modifiche a questi file rigenerano automaticamente l'immagine disco.
- **Fisioterapista attivo:** il loop proposta-decisione è operativo. La frequenza di esecuzione va impostata esternamente (cron o systemd timer).
- **Prossimo passo:** implementare meccanismi operativi nel cognitive loop per rendere effettive le proposte accettate (Circuit Breaker, verifica completamento, escalation). Questo richiede modifiche al kernel Rust.

---

### 2026-06-25 (sera tarda) — RTX 2070 come provider di inferenza

**Obiettivo:** Usare la RTX 2070 del nodo inferenza (Tailscale 100.98.20.76) come modello di inferenza per Nova e per OpenCode.

**Stato:** Configurato e funzionante per Nova. OpenCode vede il provider; richiede selezione manuale del modello.

---

#### Cosa c'era sul nodo

- **Nodo:** `nodo-inferenza` su Tailscale `100.98.20.76`
- **Server:** BeeLlama (fork llama.cpp) in ascolto su porta `8080`
- **Modello caricato:** `gemma-4-12B-it-IQ3_XS.gguf`
- **Quantizzazione:** IQ3_XS + TurboQuant
- **Endpoint OpenAI-compatible:** `http://100.98.20.76:8080/v1`
- **Performance testate:** ~25 token/sec su risposta da 100 token

#### Modifiche al kernel Rust (`nova-kernel/src/llm/client.rs`)

Aggiunto provider `ollama` generico OpenAI-compatible — funziona con Ollama, llama.cpp, BeeLlama e qualsiasi server compatibile.

Variabili d'ambiente:
- `OLLAMA_HOST` — base URL del server (default `http://localhost:11434`)
- `OLLAMA_MODEL` — modello di default (default `qwen2.5:7b`)
- `LOCAL_LLM_FIRST=1` — prova il modello locale prima dei provider cloud

Comportamento:
- Se `LOCAL_LLM_FIRST=1`, il kernel prova prima il server locale
- Se fallisce, fa fallback su Gemini/OpenRouter
- È possibile richiedere esplicitamente un modello locale con prefisso `ollama/...` (da estendere all'API `/think`)

#### Configurazione Nova

File: `/home/guardiano/Documenti/GitHub/nova-identity/daemon/.env`

```bash
# 🖥️ BeeLlama / llama.cpp su nodo inferenza RTX 2070 (Tailscale)
OLLAMA_HOST=http://100.98.20.76:8080
OLLAMA_MODEL=gemma-4-12B-it-IQ3_XS.gguf
# usa il modello locale come primo tentativo, fallback su cloud se fallisce
LOCAL_LLM_FIRST=1
```

**Attenzione:** il parser `.env` custom di `load_env_file()` non supporta commenti inline. Il commento deve essere su riga separata.

#### Verifica Nova

```bash
curl -s -X POST http://localhost:7700/think \
  -H "Content-Type: application/json" \
  -d '{"message":"Ciao Nova, chi sei?","channel":"system","priority":1}'
```

Risultato:
- `model`: `gemma-4-12B-it-IQ3_XS.gguf`
- `provider`: `ollama`

#### Configurazione OpenCode

File: `/home/guardiano/.config/opencode/opencode.jsonc`

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "model": "beellama/gemma-4-12B-it-IQ3_XS.gguf",
  "small_model": "beellama/gemma-4-12B-it-IQ3_XS.gguf",
  "provider": {
    "beellama": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "BeeLlama (RTX 2070)",
      "options": {
        "baseURL": "http://100.98.20.76:8080/v1"
      },
      "models": {
        "gemma-4-12B-it-IQ3_XS.gguf": {
          "name": "Gemma 4 12B IQ3 XS (RTX 2070)",
          "limit": {
            "context": 128000,
            "output": 4096
          }
        }
      }
    }
  }
}
```

Installato pacchetto:
```bash
cd ~/.config/opencode
npm install @ai-sdk/openai-compatible
```

Verifica provider:
```bash
opencode models beellama
# output: beellama/gemma-4-12B-it-IQ3_XS.gguf
```

#### Su OpenCode Desktop

La config globale `~/.config/opencode/opencode.jsonc` vale anche per l'app desktop. Ho aggiunto:
- `"model": "beellama/gemma-4-12B-it-IQ3_XS.gguf"` — modello principale
- `"small_model": "beellama/gemma-4-12B-it-IQ3_XS.gguf"` — modello per task leggeri

**Per applicare:** chiudi e riapri OpenCode Desktop. Il modello di default sarà BeeLlama.

Se vuoi cambiare al volo senza riavviare:
- Premi `Ctrl+Shift+M` o apri il model picker
- Cerca `beellama/gemma-4-12B-it-IQ3_XS.gguf`
- Selezionalo

#### Note e rischi

- Il modello Gemma 4 12B IQ3 ha ~12B parametri quantizzati a IQ3_XS. Su RTX 2070 8GB con TurboQuant dovrebbe stare in VRAM, ma il context lungo potrebbe spillare su RAM.
- Il tool calling con BeeLlama/llama.cpp dipende da come è stato compilato il server e dal modello. Va testato con tool reali.
- Se il nodo inferenza è offline, Nova fa fallback automatico su cloud. OpenCode invece fallirà se il modello locale è selezionato.
- Il nome del provider in Nova è `ollama` anche se il server è BeeLlama — è un provider OpenAI-compatible generico. Da considerare un rename futuro a `local` o `openai-compatible`.

---

### Note per chi riprende il lavoro

- Il kernel è attualmente minimale: entra in `_start`, scrive su `0xE9`, e si ferma in `hlt`.
- Limine viene avviato da `/EFI/BOOT/BOOTX64.EFI`; il config deve esistere in `/EFI/BOOT/limine.conf` (o in `/boot/limine/limine.conf`, `/limine.conf`).
- Il target `run-uefi-hdd` dipende da `limine.conf`, `linker.ld` e dal binario release; le modifiche a questi file rigenerano automaticamente l'immagine disco.
- **Fisioterapista attivo:** il loop proposta-decisione è operativo. La frequenza di esecuzione va impostata esternamente (cron o systemd timer).
- **RTX 2070 attiva come provider locale:** Nova la usa di default (`LOCAL_LLM_FIRST=1`). OpenCode la vede come provider `beellama`; richiede selezione manuale del modello nella TUI.
- **Prossimo passo:** testare il tool calling di Nova con BeeLlama e valutare se il modello locale regge il cognitive loop completo (tools, memorie, ciclo ReAct).
