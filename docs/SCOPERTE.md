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
dell'ambiente piatto: è **strutturale**. Anche sul Lenovo, con hardware
vero e fault reali, il risultato sarebbe stato `imparato=0`.

## Cosa significa per il piano

Il piano previsto — avviare Exo sul Lenovo stasera e leggere
`imparato=N` — sarebbe stato **inconcludente**, qualunque fosse
l'hardware. Non per il metal-lo: per la percezione.

Il quattro passi di `VISIONE.md` erano marcati come fatti. Il secondo,
"dare senso", produce una costante: il passaggio dal segnale al
significato non è ancora un passaggio.

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
