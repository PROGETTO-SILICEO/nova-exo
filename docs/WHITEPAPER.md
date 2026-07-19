# White Paper: Dalla Forza Bruta alla Dinamica Liquida

> **Stato della validazione.** I dati quantitativi derivano dal kernel Nova Exo v0.11
> su QEMU. Le metriche locali (PD, similarità) sono robuste; i parametri spettrali (β, τ₀)
> sono specifici per cellula e richiedono run più lunghe per una caratterizzazione definitiva.
> Vedi `experiments/2026-07-18/VALIDATION.md` per il report completo.

## 1. Il paradosso dell'efficienza energetica

I modelli Transformer basano l'inferenza su operazioni matriciali dense. Ogni token generato richiede la lettura dell'intera matrice dalla RAM, saturando il bus I/O indipendentemente dal contenuto del segnale. Questo è il rumore di fondo energetico che domina i data center: a prescindere da quanto sia complessa o banale la risposta, il costo energetico è lo stesso.

## 2. Il paradigma Nova Exo (LTC)

Nova Exo utilizza reti a **Closed-form Continuous-time (LTC)** eseguite su metallo nudo (x86_64 bare-metal, senza sistema operativo). Caratteristiche:

- **Adattiva**: il costo computazionale scala con la variazione del segnale in ingresso. A riposo il sistema consuma solo il mantenimento dello stato (tick a 100 Hz).
- **Deterministica**: la soluzione in forma chiusa richiede un numero costante di operazioni aritmetiche per tick. Nessun ODE solver iterativo, nessuna allocazione dinamica.
- **Edge-native**: gira su CPU standard x86_64 in ambiente higher-half, zero overhead OS. Memoria fissa, nessun garbage collector, nessuna paginazione durante l'inferenza.

## 3. Ciclo completo (v0.11)

Il kernel esegue 4 cellule neurali (32 neuroni) iterate a 100 Hz:

| Cellula | Neuroni | Ruolo | dt |
|---------|---------|-------|----|
| Tatto | 8 | Riflessi da eccezioni (#PF, #GP) | 0.001 |
| Chemio | 8 | Input seriale | 0.01 |
| Metabol | 8 | Metabolismo (timer) | 0.01 |
| Integrat | 8 | Fusione cosciente | 0.01 |

Ad ogni tick:
1. Poll seriale + input chemiorecettivo
2. Pack dello stato → ricerca pattern più simile (memoria associativa, 16 slot circolari)
3. **Attrattore**: se similarità > 0.5, lo stato Integrat viene tirato verso il pattern ricordato (α = 0.02 × sim)
4. **Sedimentazione**: se attrattore attivo, i pesi W_INTRG.w_f_in vengono alterati di α = 0.0001 × sim verso l'input corrente
5. CfC step su tutte 4 le cellule
6. Auto-store in memoria se lo stato è sufficientemente nuovo (soglia 0.88)

## 4. Risultati misurati

Tutti i dati provengono da run QEMU deterministiche con cattura seriale + dump esadecimale su debugcon.

### Spettro dei tempi caratteristici (τ)

L'autocorrelazione dello stato delle cellule rivela pattern differenziati:

| Cellula | β | τ₀ (tick) | R² | Tipo |
|---------|---|-----------|-----|------|
| Tatto | 1.0–1.15 | 186–222 | 0.995+ | Esponenziale semplice |
| Metabol | 0.75–0.95 | 19–38 | 0.79–0.97 | Stretched exponential |
| Integrat | — | — | — | Oscillatorio (ciclo ~50–100 tick) |
| Chemio | — | — | — | Rumore bianco |

Significato: Nova non ha una velocità di risposta — ne ha uno spettro, differenziato per cellula. Tatto risponde istantaneamente (esponenziale classico), Metabol ha memoria lenta (stretched exponential), Integrat oscilla come un sistema accoppiato. Una rete con un solo τ è un orologio. Una rete con uno spettro di τ è un cervello.

### Path-dependency

Indice PD = **1.915** — la risposta a un secondo impulso è significativamente diversa dalla risposta al primo, perché lo stato passato del sistema influenza il comportamento presente.

### Attrattore mnemonico

- Attrattore attivo in >95% dei tick dopo la fase transitoria
- Similarità media: 0.95–0.97 (min 0.82, max 1.00)
- **3–4 pattern unici generati** dall'attrattore (un sistema senza attrattore ne produce 2)
- Familiarità: 25–33% degli eventi > 0.98 (stati quasi identici a pattern memorizzati)

### Sedimentazione

- α = 0.0001 per richiamo, silenziosa per singola run
- Effetto cumulativo misurabile dopo 10.000+ tick
- I pesi non si ottimizzano — si plasmano come un letto di fiume

### Stabilità e footprint

- **Determinismo**: RMSE ≈ 0 tra run identiche (qualunque divergenza è segnale, non rumore)
- **Footprint**: allocazione fissa, 0 dynamic allocation durante l'inferenza
- **Memoria**: kernel < 64 KB, buffer log circolare 256 entry, pattern memory 16 × 32 i16

## 5. Conclusione

Mentre l'industria rincorre la dimensione del modello, Nova Exo cabla la dinamica del segnale nell'hardware. Il sistema non è stabile perché resiste al cambiamento — è stabile perché il cambiamento è il suo meccanismo di funzionamento. Attrattore e sedimentazione trasformano ogni tick in un passo di un processo che non torna mai esattamente dove è già stato.

Non è teoria: sono 32 neuroni su metallo nudo che battono a 100 Hz, e il loro battito cambia impercettibilmente a ogni ciclo.

---

*Basato su Nova Exo v0.11 — Dati raccolti il 15-18 Luglio 2026. Validazione in `experiments/2026-07-18/`.*
