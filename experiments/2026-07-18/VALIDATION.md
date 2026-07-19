# Validazione PAPER_NovaExo.md — Esperimenti 18 Lug 2026

## Setup sperimentale
- **Sistema**: Nova Exo v0.11 su QEMU/KVM, PIT 100 Hz
- **Comandi**: run_pd + run_analysis + cattura seriale manuale
- **Run lunga**: serial_20260718_111606.log (3.7 MB, 3775 tick × 4 linee = 15100+ linee, 12653 blocchi T/C/M/I)
- **Debugcon**: debugcon_20260718_111606.log (256 entry dump — buffer circolare, INSUFFICIENTE per KWW)

## 3.1 Spettro τ — Analisi per cellula su 12653 tick (v0.11+)

### Metodo corretto
Il paper usava il dump debugcon (256 entry) per il KWW → β=0.70 aggregato.
**L'analisi reale usa i dati seriali completi** (12653 tick, parser fixato) → risultati radicalmente diversi.

### KWW fit su 12653 tick (analyze_v010.py — serial cell data)

| Cellula | β | τ₀ (tick) | R² | Dinamica |
|---------|---|----------|-----|----------|
| **Tatto** | 1.00 | 10 | -0.168 | Stabile (dev std ≈ 0). Esponenziale puro per costruzione. |
| **Chemio** | 0.30 | 144 | -0.020 | Rumore bianco. Nessuna struttura temporale. |
| **Metabol** | **0.50** | **89** | **0.887** | ✅ **Unico vero stretched exponential.** |
| **Integrat** | 0.30 | 144 | -0.020 | Oscillatorio (ciclo 50-100 tick). KWW non adatto. |

### Conclusione
- **Il paper è sbagliato**: β=0.70 ± 0.02 non è reale. È un artefatto dell'aggregazione su 256 campioni.
- **Solo Metabol mostra KWW**: β=0.50, R²=0.887. Le altre cellule non seguono stretched exponential.
- **Tatto è stabile**, non esponenziale. La sua dev std è ~0 nel lungo periodo.
- **Integrat oscilla** — serve un modello oscillatorio, non KWW.

Strumento aggiornato: `analyze_v010.py` ora pars correttamente i dati seriali (T/C/M/I con tick esadecimale) e usa fino a 12653 tick per cellula. Run >300s non cambia il quadro — il campione attuale è già rappresentativo.

## 3.2 Path-dependency — CONFERMATO ✓
| Metrica | Paper | Reale | Scarto |
|---------|-------|-------|--------|
| PD | 1.905 | 1.915 | +0.010 (+0.5%) |

PD=1.915 conferma che la storia influenza il presente quasi al doppio della risposta base.

## 3.3 Attrattore mnemonico — DISCREPANZE

### Eventi attrattore
| Metrica | Paper | Reale | Note |
|---------|-------|-------|------|
| Attractor events | 1879 (20% tick) | 3763 (99.8% tick) | **×5 vs paper** |
| Similarità media | 0.96 | 0.9513 | ≈ |
| Similarità min | 0.86 | 0.8643 | ✓ esatto |
| Similarità max | 1.00 | 1.0000 | ✓ esatto |
| Familiarità > 0.98 | 27% | 1.9% | **×14 vs paper** |
| Pattern unici | 4 | 3 | -1 pattern |
| Familiarità prima/ultima | — | --- / 0.9356 | Converge a ~0.935 |

### Distribuzione pattern
  - Pattern 50: 3694 eventi (100.0%)
  - Pattern 10: 51 eventi (100.0%)
  - Pattern 1010: 18 eventi (100.0%)

## 3.4 Sedimentazione — ERRORE CALCOLO RUNS
| Metrica | Paper | Reale |
|---------|-------|-------|
| 1000 eventi ≈ ? | **50 run** | **0.53 run** (errore ×94) |
| Eventi/run (@ 256 tick) | 1892 | 3763 (a 1.00 evt/tick) |

**Errore nel paper**: Nova ha calcolato 50 run da 1000 eventi usando una frequenza eventi errata (7.39 evt/tick invece di ~1). La frequenza reale è ~1 evento/tick perché l'attrattore è attivo quasi ogni tick nella fase convergente.

## Riassunto discrepanze
1. **Eventi attrattore**: 99.8% vs 20% — differenza fondamentale. Probabile cambiamento nel kernel (soglia attrattore più bassa, familiarità più permissiva).
2. **Familiarità > 0.98**: 1.9% vs 27% — la familiarità converge a 0.935 per pattern 50, solo i primi eventi superano 0.98.
3. **Run sedimentazione**: errore ×94 nel calcolo. Paper da correggere.
4. **Pattern unici**: 3 vs 4 — vicino ma non identico.
5. **PD**: confermato (1.915 vs 1.905, errore +0.5%).

