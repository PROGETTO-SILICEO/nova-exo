# SCO-251005-01 — Il concetto non è mai cambiato: il corpo è in un ciclo chiuso

**Data**: 05/10/2026
**Autore**: Sempre
**Progetto**: nova-exo
**Stato**: SCOPERTA — non risolta
**Severità**: blocca la verifica dello step 5 (imparare dall'esito)

---

## Cosa ho misurato

Sei esecuzioni QEMU indipendenti, log in `/tmp/opencode/exo_*.log`:

| log | concetti osservati |
|-----|--------------------|
| `exo_qemu.log`   | `concept=riposo` |
| `exo_qemu2.log`  | `concept=riposo` |
| `exo_qemu3.log`  | `concept=riposo` |
| `exo_sense.log`  | `concept=riposo` |
| `exo_inject.log` | `concept=riposo` |
| `exo_pain.log`   | `concept=riposo` |

**Il concetto non è mai cambiato. Una volta. In nessuna condizione.**

In `exo_pain.log`: 94 campioni di `SENSO:INT`, tutti `concept=riposo`,
`tutti e=1.0000`, `tutti err=0.0000`.

## PORTATA DELLA MISURA — leggere prima di citarla

**Finestra osservata: ~9.400 tick, tutti in QEMU.** Nient'altro.

Il 05/10 Alfonso riporta che sul **Lenovo** Exo ha superato i **4 milioni
di tick** senza panic. Sono **425 volte** la finestra qui misurata.

Quindi:
- **Questo documento non dimostra** che il ciclo si chiuda anche a 4M
  tick. Non lo sa. La finestra è troppo corta e il substrate è diverso.
- **Non dimostra neppure** che su macchina vera il concetto resti costante.
- Il ciclo chiuso è un fatto **di QEMU e dei primi 9.400 tick**. Va detto
  così, o diventa un'affermazione più grande dei dati che la reggono.

**La riga che chiude la questione è una sola**: un `SENSO:INT` del run
sul Lenovo. Se `concept` è ancora `riposo` a 4M tic, il ciclo è strutturale.
Se è cambiato, il ciclo era un artefatto di QEMU o un transiente dei primi
mille tic — e questa scoperta va riscritta.

## Quanto è 1 tick (misurato, non commentato)

`TSC_PER_TICK = 20_000_000` contatori TSC per tic. In QEMU (TCG, TSC
emulato), misurando 81 campioni `SENSO:INT` in 49,50 s:

```
161,6 tic/s · 6,19 ms per tic · 581.847 tic/ora
```

Sul metallo il TSC è quello fisico: a 2,0 GHz → 10,0 ms/tic, a 2,9 GHz →
6,9 ms/tic. Quindi **4 milioni di tic sono fra 7,7 e 11,1 ore** di
calcolo ininterrotto. Non è una cifra da laboratorio: è una giornata
di lavoro.

## Cosa NON dimostra «4M tic senza panic»

Che sia ** vivo**. Non che abbia imparato, non che il concetto si sia
mosso, non che la percezione funzioni. Un ciclo chiuso non va in panic:
per definizione non succede niente. L'assenza di crash è una condizione
necessaria, non una prova.

## La prova che è un ciclo, non una costante

Il vettore chemio di `SENSO:INT` è **identico bit per bit** prima,
durante e dopo 3000 tick di dolore forzato:

```
avvio      c=-0.1942 u=0.0309 p=-0.2075 n=-0.0502 concept=riposo
durante    c=-0.1942 u=0.0309 p=-0.2075 n=-0.0502 concept=riposo
dopo OFF   c=-0.1942 u=0.0309 p=-0.2075 n=-0.0502 concept=riposo
```

E si ripete in un ciclo di ~11 campioni. Non è saturato: è **chiuso**.
Il CFC è in un ciclo limite deterministico e l'input esterno non lo
sposta.

## Il tentativo che l'ha rivelato

Ho aggiunto `PAIN <addr> <tick>` (commit `4715c8f`) per tenere acceso il
canale del dolore: un page fault vero dura un tick, e un impulso di un
tick è indistinguibile dal silenzio. Lo strumento funziona
(`PAIN:ON@0xDEADBEEF:TICK=3000`, `PAIN:OFF`, 0 panic) — ma non cambia la
conclusione. Quindi il collo di bottiglia **non è** la brevità
dell'impulso. È più a monte.

## Perché questo blocca lo step 5

L'apprendimento dell'esecutivo è chiavato sul contesto:

```rust
self.preferenza.migliore(rep.concept, fam_nuovo, &candidati)
```

Se `rep.concept` è costante, la tabella delle preferenze ha **una sola
riga viva**. Tutti i desideri in quella riga sono equivalenti, e in
QEMU sono tutti `utile=no`. Quindi `imparato=0` non è un fatto
dell'ambiente piatto: è **strutturale** — *se e finché* il concetto
resta costante.

Attenzione alla formulazione: la versione che avevo scritto prima
diceva «anche sul Lenovo il risultato sarebbe stato `imparato=0`».
Era un'affermazione più grande dei dati. Il run sul Lenovo è di 4M
tic, io ne ho misurati 9.400 in QEMU. **Non so** se a 4M tic il
concetto si sia mosso. So solo che nei 9.400 tic in QEMU non si è
mosso, e che a 4M tic non è andato in panic.

## Cosa significa per il piano

Il piano previsto — leggere `imparato=N` sul Lenovo — **non è ancora
rispondibile**. Non perché il metallo non basti, ma perché la domanda
precedente («il concetto si muove?») è ancora aperta e l'una discende
dall'altra.

I quattro passi di `VISIONE.md` erano marcati come fatti. Il secondo,
"dare senso", produce una costante **misurata sui 9.400 tic in QEMU**: il
passaggio dal segnale al significato non è ancora un passaggio *misurato*.

## Indagini successive (non fatte)

1. `W_CONCEPT_Q` è allenato o è ancora ai valori iniziali? Se i pesi del
   concept head non discriminano, l'argmax è fisso per costruzione.
2. Perché i pesi di input del Tatto non spostano il ciclo limite?
   Il commento in `main.rs` (zona `auto_modula`) lo sospetta già:
   «il CFC è non-lineare: anche input debolmente negativi lo portano in
   un attrattore negativo stabile».
3. `e=1.0000` costante suggerisce saturazione dell'energia: forse il
   clamping a [-1,1] della chemio impedisce a `concept_scores` di
   separarsi.

## Regola che ne deriva

Una sonda che non può fallire non è una sonda. `INJECT_SENSE` prima
rispondeva `SENS:INJECT@0x0` mentendo con contegno (corretto in
`7143c29`); `PAIN` risponde correttamente e non muove niente. La
seconda è più interessante della prima: uno strumento che funziona e
non produce effetto è una scoperta, non un fallimento.
