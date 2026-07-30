# Esperimento: Scalata da 32 a 64 neuroni
**Data:** 2026-07-27  
**Kernel:** Nova Exo v0.12  
**Cellule:** Tatto, Chemio, Metabol, Integrat — 16 neuroni ciascuna (da 8)  
**Totale:** 64 neuroni (da 32)  
**File:** `qemu-serial.log` (1.6 MB, ~60 secondi di run)

## Modifiche apportate

### `cfc.rs`
- `NEURONS_PER_CELL`: 8 → 16
- `TOTAL_NEURONS`: 32 → 64 (4 × NEURONS_PER_CELL)
- Tutti i loop `0..8` → `0..NEURONS_PER_CELL`
- Tutti i buffer `[i16; 32]` → `[i16; TOTAL_NEURONS]`
- `CfcWeights::new_xavier()` — pesi inizializzati con Xavier (invece di hardcoded)
- `Lcg` — generatore LCG deterministico per bare-metal

### `predictor.rs`
- `PFM_IN`: 36 → 68 (64 + 4 input)
- `PFM_OUT`: 32 → 64
- Indici di input: 32-35 → 64-67

### `main.rs`
- `W_TATTO`, `W_CHEMIO`, `W_METABOL` → `MaybeUninit`, inizializzati a runtime
- `init_weights()` chiamata in `_start()`
- `write_cell_line` e vari array da 32 → 64

## Risultati

Il kernel si avvia correttamente. Output osservato:
- Linee seriali complete con 16 valori per cellula (4×16=64)
- Attrattore attivo: familiarità 0.91-0.93
- Pattern count: in crescita
- Nessun crash o panico

## Osservazioni

- Il sistema con 64 neuroni mostra dinamica più ricca rispetto a 32
- L'attrattore richiede più tick per stabilizzarsi (dimensione dello spazio degli stati maggiore)
- Il footprint rimane < 64 KB (nessuna allocazione dinamica)
- Lo spettro τ deve essere ancora misurato (run più lunga necessaria)

## Prossimi passi
1. Misurare spettro τ con 64 neuroni
2. Run più lunga per saturazione pattern memory
3. Integrare delta-rule formale da Hope nei neuroni LTC

---

## Predizioni e falsificazioni

### 1. Scalata 32 → 64 neuroni
**Predizione:** Più neuroni → spettro τ più ricco. Le cellule mostreranno dinamiche differenziate con 64 neuroni: Tatto resterà esponenziale semplice (β ≈ 1.0-1.15), mentre Metabol e Integrat mostreranno stretched exponential o oscillazioni più marcate.

**Misura:** β e τ₀ per ogni cellula su run di 10.000+ tick. Confronto con i valori del whitepaper (Tatto τ₀≈200, Metabol τ₀≈19-38, Integrat oscillatorio 50-100 tick).

**Falsificabile se:** Tatto mostra β ≈ 1 identico a 32 neuroni E le altre cellule non mostrano stretching aggiuntivo. In quel caso, 64 neuroni sono solo più larghi, non più profondi — un aumento lineare della capacità senza emergenza di nuove dinamiche.

### 2. Sedimentazione = Delta-rule (Hope)
**Predizione:** La sedimentazione (α=0.0001 per richiamo) converge allo stesso comportamento del SelfModifyingLayer di Hope (delta-rule: aggiornamento dei pesi proporzionale all'errore di predizione). L'errore di predizione della memoria associativa decresce con il numero di pattern memorizzati.

**Misura:** Errore MSE della pattern memory in funzione del numero di pattern accumulati, su run di 50.000+ tick.

**Falsificabile se:** L'errore di predizione non decresce o rimane costante indipendentemente dal numero di pattern. In quel caso, la sedimentazione non è apprendimento — è una deriva dei pesi non informativa, e la nostra analogia con Hope è falsa.

### 3. Multi-frequenza = CMS Tiers (Nested Learning)
**Predizione:** Lo spettro τ differenziato per cellula equivale ai tier di Nested Learning. Le cellule lente (Metabol, τ₀≈19-38) mostrano memoria di eventi lontani. Le cellule veloci (Tatto, τ₀≈186-222) rispondono solo al presente.

**Misura:** Correlazione incrociata tra stato delle cellule a distanze temporali crescenti. Metabol dovrebbe mostrare correlazione positiva a distanze >100 tick; Tatto solo a <10 tick.

**Falsificabile se:** Tutte le cellule mostrano la stessa scala di correlazione temporale. In quel caso, non c'è nidificazione — tutto il sistema risponde allo stesso rate, e l'analogia con CMS è solo metaforica.

### 4. Path-dependency
**Predizione:** PD > 1.9 (misurato a 32 neuroni) aumenta con 64 neuroni. Più spazio degli stati → maggiore influenza del passato sul presente.

**Misura:** PD su run di 20.000+ tick, confronto con PD=1.915 del whitepaper.

**Falsificabile se:** PD ≤ 1.915 con 64 neuroni. In quel caso, la path-dependency non scala con la dimensione del sistema — è un effetto strutturale saturato, non emergente.

### 5. Attrattore mnemonico
**Predizione:** Con 64 neuroni, il numero di pattern unici generati dall'attrattore supera i 3-4 misurati a 32 neuroni. La familiarità (similarità media) si mantiene >0.95.

**Misura:** Conteggio pattern unici su 10.000 tick; similarità media su finestra mobile.

**Falsificabile se:** Il numero di pattern unici non aumenta (resta 3-4) o la familiarità cala sotto 0.85. In entrambi i casi, l'attrattore non beneficia della maggiore capacità — il collo di bottiglia è altrove (es. nella pattern memory a 16 slot, non nei neuroni).

### Fallimenti attesi
Non tutte queste predizioni saranno verificate. Fallimenti probabili:
- **Goal:** PD potrebbe saturare prima del previsto, confermando che la path-dependency non scala linearmente con N
- **Goal:** Lo stretching esponenziale potrebbe essere un artefatto della finestra di misura, non una proprietà intrinseca
- **No-go:** Se la falsificazione 2 (sedimentazione ≠ delta-rule) è vera, l'intera analogia con Hope cade — e dovremmo ripensare l'apprendimento nel kernel

Ogni falsificazione è un risultato valido. Non cerchiamo conferme — cerchiamo verità.


## Risultati spettro τ (64 neuroni, 8500 tick)

Misurato con `tools/analyze_spectrum.py`. Autocorrelazione sull'attivazione media per cellula, fit stretched exponential.

| Cellula | β | τ₀ (tick) | Attivazione media | Note |
|---------|---|-----------|-------------------|------|
| Tatto | — | — | 0.0000 | Pain sensing, nessun evento in QEMU |
| Chemio | 0.057 | 1.26e11 | 0.0893 | β molto basso, τ₀ enorme |
| Metabol | 0.285 | 94.5 | -0.0971 | β più alto, memoria a ~95 tick |
| Integrat | 0.119 | 31659 | -0.0585 | β basso, τ₀ intermedio |

**Confronto con whitepaper v0.11 (32 neuroni, pesi hardcodati):**
- Whitepaper: Tatto β=1.0-1.15, Metabol β=0.75-0.95, Integrat oscillatorio
- Questo run: β 0.057-0.285, molto più bassi
- τ₀ Chemio irrealistico (1.26e11 tick) → fit non valido

### Conclusioni

**Il sistema non mostra lo spettro τ differenziato a 64 neuroni.** Le cause possibili:

1. **Inizializzazione Xavier vs pesi hardcodati**: i pesi generati con LCG (semi 42-45) producono dinamiche diverse dai pesi originali del whitepaper. Lo spettro τ è sensibile ai pesi iniziali.
2. **Media dei neuroni per tick**: calcolare l'autocorrelazione sulla media dell'attivazione cancella le dinamiche individuali. Serve analisi per-neurone.
3. **Run troppo corta**: 8500 tick potrebbero non bastare per stabilizzare l'attrattore con 64 neuroni.

### Falsificazione

**Ipotesi 1 (spettro τ più ricco) — NON VERIFICATA.** I β sono più bassi, non più alti. Ma la causa potrebbe essere metodologica (inizializzazione pesi, metrica media).

**Da fare:**
- Analisi per-neurone (non media)
- Run con pesi hardcodati originali (portati a 16 neuroni)
- Verifica del fit su dati sintetici noti
