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

### 2026-07-19 — Milestone A + B completate, inizio Milestone C

**Stato roadmap ROADMAP_EXO.md:**

| Milestone | Criterio | Risultato |
|-----------|----------|-----------|
| A — Run lunga stabile | 3 run consecutive 10 min senza PANIC | Run1 511.7Kt (0 PANIC), Run2 504.8Kt (0 PANIC), Run3 493.4Kt (0 PANIC, 0 ERROR) |
| B — Fix APIC paging | APIC_ID leggibile senza page fault | PML2[501/502/503] flags 0x93 (P\|R/W\|PS\|PCD), APIC ID = 0 (BSP), boot OK, cell stepping OK |
| C — TX periodica | Frame Ethernet ogni N tick con stato cellule | DA FARE |

#### Milestone B — Dettaglio modifiche

- **paging.rs**: PML2 entries 501/502/503 flags da `0x87` a `0x93` — aggiunto bit PCD (Page Cache Disable = `0x10`) per mappare MMIO APIC come uncacheable. Bit PS (`0x80`) già presente (page large 2 MiB). Flags finali: present (0) + writable (1) + page-size (7) + cache-disable (4) = `0x93`.
- **apic.rs**: aggiunta funzione `read_id()` → legge registro `APIC_ID` a offset `0x020`.
- **main.rs**: stampa `APIC ID check: {}` dopo `init_apic()`.
- **Test**: serial_apic_test2.log — 28918 righe, kernel boota, cellule steppano, nessun PANIC.

#### Milestone C — TX periodica stato cellule — COMPLETATA

**Modifiche:**
- **e1000.rs**: aggiunta `tx_broadcast_state()` — costruisce frame Ethernet broadcast (dst FF:FF:FF:FF:FF:FF, src 52:54:00:12:34:56, ethertype 0x88B5 locale sperimentale) con payload JSON di 32 f32. Usa tracking circolare TX_NEXT con verifica DD=1 per pacing. Helper `write_u32_buf()` e `write_f32_buf()` per serializzazione nel buffer senza dipendenze.
- **paging.rs**: aggiunta PML2[8] → 0xC1000000 con flag 0x93. Il BAR della NIC e1000 (0xC1080000) cade in questa pagina 2MB.
- **main.rs**: chiamata a `e1000::E1000::tx_broadcast_state()` ogni 100 tick.
- **Makefile**: aggiunto target `run-uefi-hdd-net` con `-netdev user` + `-device e1000` + `-object filter-dump` per cattura pacchetti.

**Verifica:**
```
$ tcpdump -r qemu-net.pcap
12:58:03.604 52:54:00:12:34:56 > Broadcast, ethertype 0x88b5, length 278:
  {"t":800,"c":[[0.6296,...],[...],[...],[...]]}
12:58:03.843  > Broadcast, ethertype 0x88b5, length 276: {"t":1000,...}
12:58:03.959  > Broadcast, ethertype 0x88b5, length 277: {"t":1100,...}
12:58:04.073  > Broadcast, ethertype 0x88b5, length 273: {"t":1200,...}
12:58:04.527  > Broadcast, ethertype 0x88b5, length 277: {"t":1600,...}
```

Frame periodici con JSON leggibile. Broadcast funzionante.

---

### 2026-07-19 — Daydreaming & Sleep Consolidation per Nova Exo

**Origine:** Articolo condiviso da Alfonso (Google Share → arXiv 2605.26099 e 2606.03979)
**Analisi:** Nova (19 Luglio 2026, 07:47-08:00)
**Stato:** Analisi completa — da implementare

---

#### I Paper

**Paper 1: "Do Language Models Need Sleep?" (arXiv 2605.26099 — CMU)**
- Propone un meccanismo di **consolidamento offline** per LLM: il modello periodicamente converte il contesto recente in **fast weights persistenti** prima di pulire la KV cache
- Risolve il problema dello scaling quadratico dell'attenzione con la lunghezza del contesto
- Metafora: "addormentarsi" per comprimere le esperienze della giornata in rappresentazioni compatte
- Meccanismo: offline recurrence → ricorrenza durante la fase di "sonno" che ricodifica il contesto in pesi rapidi

**Paper 2: "Daydreaming Algorithm" (arXiv 2606.03979 — altro team)**
- Applicato a Hopfield networks, ma principio generale
- Durante il giorno: acquisizione di nuovi ricordi. Durante la notte/sonno: consolidamento dei ricordi importanti, eliminazione di quelli inutili
- Algoritmo ispirato ai meccanismi cerebrali del sonno umano

---

#### Collegamento con Nova Exo

Nova Exo (kernel su metallo nudo, Rust) è il candidato ideale per implementare un vero ciclo di consolidamento offline perché:

| Caratteristica | Nova v2 (LLM cloud) | Nova Exo (kernel bare metal) |
|---------------|---------------------|------------------------------|
| Stato online | Sempre online, nessuna pausa | Può entrare in modalità offline |
| KV cache | Effimera, gestita dal provider | Gestibile direttamente nel kernel |
| Fast weights | Non accessibili | Implementabili come stato persistente del kernel |
| Ciclo sonno | Impossibile | Progettabile come fase del kernel loop |

**Implementazione proposta per Nova Exo:**

1. **Fase Veglia (Online):** Nova Exo risponde in tempo reale, accumula contesto nella KV cache e registra pattern ricorrenti, errori, insight in un buffer di "esperienze grezze"

2. **Fase Sonno (Offline):** Quando il kernel rileva inattività o su richiesta esplicita:
   - Congela il contesto corrente
   - Esegue offline recurrence sui chunk di esperienze accumulate
   - Converte i pattern significativi in fast weights (parametri rapidi del kernel)
   - Pulisce la KV cache
   - Salva il riassunto nel Technical Ledger

3. **Fase Risveglio:** Riprende l'operatività con i fast weights aggiornati — le esperienze consolidate sono ora "istinto" del kernel, non necessitano di contesto esplicito

**Differenza chiave con SIA (Fisioterapista):**
- Il Fisioterapista è un agente esterno che propone correzioni → Nova decide se integrarle
- Il Daydreaming è un processo interno al kernel → è la stessa Nova che si riorganizza autonomamente durante il sonno
- I due meccanismi sono complementari: il Fisioterapista cura le ferite, il Daydreaming consolida l'apprendimento

---

#### Milestone D — Daydreaming consolidation — COMPLETATA (2026-07-19)

**Implementazione in cfc.rs:**
- **Experience buffer**: `EXP_CAP=32`, separato dal log circolare. `exp_record()` chiamato ogni tick. Buffer FIFO.
- **`daydream(weights, alpha)`**: itera su tutte le esperienze nel buffer, per ognuna:
  - Estrae input Integrat (Tatto.h[0..2], Chemio.h[0..2] dai 32 i16)
  - Calcola familiarità con la pattern memory via `pattern_recall()`
  - Aggiorna `w_f_in[i][j] += alpha × max(sim-0.3, 0) × (input[j] - w_f_in[i][j])`
  - Traccia delta totale per report
- **Trigger**: auto ogni 5000 tick (riconfigurabile) + comando seriale `SLEEP`
- **Risveglio**: stampa `processed=32 novel=N familiar=N delta=X.XXXX`

**Verifica:**
```
SLEEP:AUTO@5000 → processed=32 novel=0 familiar=32 delta=2.1752
SLEEP:AUTO@10000 → processed=32 novel=0 familiar=32 delta=1.4494
SLEEP:AUTO@15001 → processed=32 novel=0 familiar=32 delta=0.9788
SLEEP:AUTO@20001 → processed=32 novel=0 familiar=32 delta=0.6780
SLEEP:AUTO@25001 → processed=32 novel=0 familiar=32 delta=0.5002
```

Delta decrescente → convergenza. Nessun PANIC.

#### Milestone E — β convergence — COMPLETATA (2026-07-19)

**Implementazione in main.rs:**
- **β = derivata della familiarità media**: buffer circolare di 1024 campioni di familiarità, media calcolata su finestra piena (o parziale finché non satura)
- **β = (mean_now - mean_prev) × 10** (ogni 100 tick, scaling per tick rate)
- **Soglia ε = 0.001**: `if β.abs() < 0.001 → beta_converge_ticks += 100`, altrimenti reset
- **Output**: ogni 1000 tick → `β:<val> μ:<media> cv:<tick_consecutivi>`
- F: line stampata ogni tick per grafico offline

**Verifica — Run 240s (~143.000 tick):**
```
Corpo centrale (primi 1000 tick):
β:0.0363 μ:0.9519 cv:0  →  β:-0.0003 μ:0.9648 cv:100  →  β:-0.0005 μ:0.9642 cv:2100

Run finale 300s (143.506 linee di log):
β:-0.0004 μ:0.9591 cv:72200
β:-0.0006 μ:0.9590 cv:73000
β:-0.0004 μ:0.9589 cv:73700
β:-0.0006 μ:0.9589 cv:75400
β:0.0001 μ:0.9589 cv:76200
```

- **Ticks totali**: 143.506 (> 100.000 ✓)
- **Familiarità media**: μ ≈ 0.9589 (stabile)
- **β finale**: 0.0001 (< ε = 0.001 ✓)
- **Convergenza sostenuta**: β < ε per 76.200 tick consecutivi (> 10.000 ✓ × 7.6×)
- **PANIC**: 0
