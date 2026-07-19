# Metodologia — Metriche di analisi Exo

## 1. Indice di Path-Dependency (PD)

L'impulso è un vettore seriale `[1.0, 0.0, 0.0, 0.0]` inviato a Chemio[0].
Rilevamento: picco in `|Chemio[0]|` > 0.1.

### Controllo — singolo impulso

```python
r_control(t) = Integrat[24..28](t) - baseline
```

con `baseline = media su 8 tick pre-impulso`.

### Test — doppio impulso (gap ~0.45 s ≡ ~20 tick)

```python
r_test(t) = Integrat[24..28](t) - baseline
```

### Indice PD

```
PD = RMS(r_test - r_control) / RMS(r_control)
```

dove `RMS(x) = sqrt(mean(x²))` su finestra di 48 tick post-impulso.

Un sistema senza memoria dà PD ≈ 1.0 (le due risposte coincidono).
PD > 1 indica che lo stato residuo del primo impulso altera la risposta al secondo.

---

## 2. Stretched Exponential (KWW)

Modello di Kohlrausch-Williams-Watts:

```
C(k) = C(0) · exp(-(k / τ₀)^β)
```

- `C(k)` = autocorrelazione al lag `k`
- `β` = esponente di stretching (`β=1` → esponenziale semplice, `β<1` → stretched)
- `τ₀` = costante di tempo caratteristica

### Procedura di fit

1. Media neuronale per cellula (es. Metabol: media su 8 neuroni)
2. Sottrazione media → normalizzazione a varianza unitaria
3. Autocorrelazione a lag `k` = 0..99
4. Grid search in log-space:
   - β ∈ [0.30, 1.00] step 0.05 (15 valori)
   - τ₀ ∈ [10, 1000] logspace (20 valori)
5. R² in log-space (coefficiente di determinazione su log|C(k)|)

### Bootstrap

Per ogni cellula:
1. 100 ricampionamenti con reinserimento delle traiettorie neuronali
2. Grid search per ogni campione
3. Media e deviazione standard di β e τ₀
4. Intervallo di confidenza 95% (percentili 2.5 e 97.5)

---

## 3. Tempo di autocorrelazione integrato τ_integ

```
τ_integ = Σ_{k=0}^{max_lag} |C(k)| / C(0)
```

Cattura l'inviluppo del decadimento anche in presenza di oscillazioni.

---

## 4. Baseline di confronto

### 4.1 Vanilla RNN (32 neuroni)

```python
h(t) = tanh(W · h(t-1) + W_in · input(t) + b)
```

- 4 cellule × 8 neuroni, stessa struttura di routing
- W ∼ N(0, 0.1), inizializzazione casuale fissa (seed=42)
- Nessun attrattore, nessuna sedimentazione
- Stessi dt (Tatto=0.001, altri=0.01)

### 4.2 AR(1) per cellula

```python
h(t) = α · h(t-1) + (1-α) · ε(t)
```

- `α = exp(-1/τ)` con τ differenziato per cellula
- Tatto τ=5, Chemio τ=10, Metabol τ=20, Integrat τ=30
- ε(t) ∼ N(0, 0.1), seed=42

### 4.3 Exo a pesi casuali

- Stessa architettura LTC/CfC del sistema reale
- Pesi fissi casuali (stessa inizializzazione, senza sedimentazione)
- Attrattore disabilitato

---

## 5. Stress test sedimentazione

Procedura:
1. Run sintetica di 100.000 tick con ciclo attrattore continuo
2. Campionamento norma Frobenius `||W||_F` ogni 1000 tick
3. Monitoraggio saturazione: `max|W_ij|` e `||W||_F / ||W_0||_F`
4. Soglia di saturazione: se `||W||_F` > 2× iniziale, peso in regime critico
