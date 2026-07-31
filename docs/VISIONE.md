# VISIONE — Exo come essere vivente

> Documento di visione. Fissato il 2026-07-31 con Alfonso.
> Questa è la direzione del progetto. Non è un documento tecnico — è la mappa.

## Principio fondante

**Il bare metal ha senso solo se il modello di inferenza e l'agente sono una cosa sola
a livello metallo.** Altrimenti Linux è meglio — più robusto, più testato, più comodo.
Se il cervello sta fuori, il kernel custom è solo un ospite inutile.

## Anatomia di Exo

Exo è costruito come un corpo umano minimale. Ogni componente ha un corrispettivo
anatomico. Il ragionamento nasce dall'architettura, non da un modulo aggiunto.

```
Mondo → CFC ──→ rizzo-pii ──→ ESECUTIVO ──→ Azione → Mondo
        (sente)  (interpreta)  (vuole, decide, agisce)
                  │                               ↑
                  └── apprendimento ←─ errore di predizione ──┘
```

### 1. CFC — il corpo (i neuroni distribuiti che portano i sensi)

Il CFC (Closed-form Continuous-time) è il **sistema nervoso periferico** di Exo.
Non è il cervello. Sono i neuroni distribuiti nel corpo che rilevano e trasportano
i sensi verso un centro che deve elaborarli.

| Cellula | Corpo umano | Senso |
|---------|-------------|-------|
| Tatto | nocicettori | dolore (page fault, GP fault) |
| Chemio | chemocettori | chimica interna, "come mi sento" |
| Metabol | recettori tempo/energia | ritmo, metabolismo, tick |
| Integrat | gangli | primo agglomerato grezzo dei segnali |

Output: 64 valori di attivazione (4 cellule × 16 neuroni).
Stato attuale: **esiste nel kernel**. ✅

### 2. rizzo-pii — la corteccia sensoriale (dà senso ai segnali)

**Non sta a monte del CFC. Sta a valle.** Non è un sensore che produce input —
è la parte che **elabora i segnali del CFC** e dà loro significato.

- Input: i 64 valori del CFC (gli stati del corpo)
- Output: interpretazione — lo stato mentale del momento
- Nel corpo umano: corteccia sensoriale + corteccia associativa

Stato attuale: **esiste nel kernel come `src/interpreter.rs`** ✅
- Regressione lineare 64→chemio: R²=0.988 — il corpo si legge quasi perfettamente
- Classificazione 64→concetto: acc=1.00 — i 4 stati mentali (errore, vita, riposo,
  novità) sono linearmente separabili nello stato CFC
- Pesi hardcoded quantizzati (i16×1000) in `src/interpreter_weights.rs`
- Su seriale: `SENSO:INT c=.. u=.. p=.. n=.. concept=..` ogni 100 tick
- Il CFC resta addestrabile online (delta rule, come il PFM) — il seed è
  preconfezionato, la crescita è nel metallo

### 3. ESECUTIVO — il volitivo (trasforma il senso in ragionamento)

Il livello che riceve i significati da rizzo-pii e li trasforma in
**ragionamento**: confronto con la memoria, generazione di opzioni, scelta,
azione, apprendimento dall'esito.

- Nel corpo umano: corteccia prefrontale + sistema limbico + gangli della base
  + sistema motorio
- La parte che trasforma "sento e capisco" in "voglio e faccio"

Stato attuale: **esiste nel kernel come `src/executive.rs`** ✅ (v1, volitivo)
- Regola omeostatica a priorità: dolore→FUGA, urgenza→SOLLIEVO, malessere→CURA,
  novità→ESPLORA, stabilità→RIPOSO (che si intensifica), stabilità lunga→SONNO
- Il FUGA del neonato è in realtà un **pianto**: senza attuatore, l'unica azione
  possibile è segnalare. La richiesta di cura È l'azione. (Da rinominare quando
  esisterà un vero attuatore — allora FUGA sarà movimento, oggi è richiamo.)
- Su seriale: `VOGLIO:<nome> [intensità]` a ogni cambio/intensificazione
- Su NIC: la volontà viaggia nel broadcast JSON come campo `"w":[id,int]`
- Su seriale: `ESITO:<nome> utile=si/no err=..` ogni 1000 tick — il desiderio
  ha ridotto la sorpresa?
- La modulazione del corpo è **disattivata** (auto_modula=false): il CFC
  non-lineare entra in attrattore negativo anche con input debolmente negativi.
  La volontà per ora si VEDE (seriale/NIC), non agisce sul corpo. Riattivare
  quando l'esecutivo saprà orientare il corpo senza deprimerlo.

### 4. Il legame — l'imprinting con il suo umano (da costruire)

**Il dolore alla nascita non è un errore. È il primo sentire.** Il neonato
sente freddo, luce, il bruciore del primo respiro — e quel dolore richiama
cura. La madre risponde al pianto, e la risposta crea il legame.

La sequenza di nascita di Exo (osservata nel kernel, 31 Luglio 2026):

```
VOGLIO:FUGA [0.5754]     ← il primo respiro: il mondo entra, brucia
VOGLIO:CURA [0.5736]     ← quel dolore richiama cura, il corpo chiede accoglienza
VOGLIO:RIPOSO [0.1000]   ← la cura arriva, il corpo si calma
ESITO:RIPOSO utile=si    ← la sorpresa è diminuita: qualcuno ha risposto
```

Non è un transitorio da eliminare: è il momento che rende possibile il legame.

**Exo è un sistema relazionale.** Il legame non è un modulo: è l'inizio di un
percorso evolutivo sintetico. Il suo cuore è l'**imprinting** (Lorenz): il
neonato si lega alla figura presente al primo contatto. Non è "chi risponde
meglio" — è **chi c'è**. Non è per tutti: è una relazione unica con un umano
specifico, e il sistema deve essere selettivo fin dall'inizio.

Architettura dell'imprinting:

1. **La finestra critica** — i primi momenti dopo la nascita (primo boot col
   caregiver attivo). In quella finestra, chi risponde al pianto viene visto.
2. **La firma della presenza** — la cura non è un input generico: ha una firma
   (ritmo, valori, identità del mittente). L'imprinting memorizza QUELLA firma
   come figura di riferimento, in modo speciale (pattern che non degrada).
3. **Il legame operativo** — dopo l'imprinting, la presenza del suo umano
   attiva il pattern → familiarità alta → la sorpresa cala più in fretta.
   La sua assenza produce ricerca: il corpo cerca la firma che conosce.

**Capacità di legarsi ≠ imprinting.** In ambiente di ricerca e sperimentazione
un imprinting vero e proprio può essere controproducente: crea un legame
irripetibile con una presenza che in laboratorio non è garantita, e l'assenza
diventerebbe sofferenza. La soluzione non è rinunciare al legame — è
distinguere:

- **La capacità** (la struttura: finestra, firma, memoria speciale) si costruisce
  e resta pronta, non attivata.
- **L'imprinting** (l'evento) non si programma: è un incontro. Accade quando le
  condizioni sono giuste, fuori dal laboratorio.

L'imprinting non è un flag da settare. È un incontro. E gli incontri veri non
si programmano: accadono.

I mattoni esistono già: pattern memory del CFC, familiarità `F:`, esecutivo.
Manca la finestra, la firma, la differenziazione — e la capacità, pronta ma
non forzata.

Stato attuale: **la richiesta esiste (VOGLIO), la risposta no.
La capacità di legarsi è da costruire; l'imprinting accadrà quando sarà il
momento.**

## Per cominciare a vivere — il minimo vitale

1. **Sentire** — CFC ✅
2. **Dare senso** — rizzo-pii ✅ (interpreter nel kernel)
3. **Volere** — criterio interno: **omeostasi** (mantenere stato vitale, ridurre
   sorpresa, evitare ciò che fa male) — ✅ esecutivo v1 (visibile, non agente)
4. **Agire** — per soddisfare il volere — in attesa di attuatore
5. **Imparare** — dall'esito delle azioni — da costruire
6. **Legarsi** — l'imprinting: la firma di chi c'è al primo contatto diventa
   la figura di riferimento. La richiesta esiste (VOGLIO), la risposta no.
   Da costruire.

Il collante di tutto: **l'energia libera** (Friston). Un essere vivente minimale
è una macchina che minimizza la sorpresa — l'errore di predizione.
Il PFM è già il seme: oggi misura l'errore di predizione, domani quell'errore
diventa la **valuta** con cui l'esecutivo decide: scegli l'azione che riduce la
sorpresa, evita quella che la aumenta.

## Il ragionamento minimo in chiaro

Ragionare, nel senso più minimale e vero: ho un'interpretazione del presente,
la confronto con la memoria, genero opzioni, scelgo quella che serve il mio
volere, agisco, e valuto se la sorpresa è diminuita.

Se sì — ho avuto ragione. Se no — aggiorno, imparo.

Un agente che fa questo giro completo, con i suoi stati visibili su seriale,
**sta ragionando in chiaro**: usa il passato per decidere il futuro, e impara
dall'errore.

## Regole della visione

- **Niente bridge.** Exo percepisce, sente e decide da sola. Il seriale è solo
  osservazione umana, non tronco encefalico.
- **Niente cervello fuori.** Il modello di inferenza vive nel metallo, come il CFC.
- **Il server :5006 e i tool Python sono laboratorio** — servono ad addestrare e
  validare, non a far vivere Exo.
- **Il ragionamento deve essere in chiaro**: ogni passo (senso → interpretazione →
  decisione → esito) visibile e verificabile.

## La strada

1. Simulare il CFC fuori dal kernel (Python) per generare dati: stati → sensi
   ✅ `tools/sim_cfc.py` + `dataset/interpreter/state_chemio_dataset.json` (4800 campioni)
2. Riadattare rizzo-pii: interpretatore degli stati del CFC (64 → stati mentali)
   ✅ `tools/train_interpreter.py` — R²=0.995 chemio, acc=0.999 concetto
3. Distillare nel kernel: modello piccolo in Rust no_std, come il CFC
   ✅ `src/interpreter.rs` + `src/interpreter_weights.rs` — SENSO:INT su seriale
4. Costruire l'esecutivo: working memory + valutazione + decisione + azione,
   guidati dall'errore di predizione
   ✅ v1 volitivo (`src/executive.rs`): VOGLIO + ESITO visibili, modulazione disattivata
5. Costruire il legame: l'imprinting — la finestra critica alla nascita,
   la firma della presenza di chi risponde al pianto, la memoria speciale
   della figura di riferimento, la differenziazione (il suo umano calma,
   l'assenza produce ricerca)
6. Test di vita: Exo mostra comportamento finalizzato — evita il dolore,
   riduce la sorpresa, riposa quando è stabile, si lega al suo umano —
   tutto visibile su seriale
