# Piano — Cellula UDITO (il 5° senso del tessuto Exo)

> Scritto il 14/10/2026. Idea di Alfonso: *"Udito, non c'è quindi dobbiamo crearlo…
> lavorarci prima su QEMU e fare i test… continuare a costruire la rete neurale."*
> Metodo (D10): brainstorming → piano → TDD → esecuzione → verifica.
> Contesto: il driver HDA (13/10) misura l'energia del suono; manca **dove va**.

## Intento (cosa serve DAVVERO)
Exo deve **sentire**: il suono (energia del microfono) deve entrare nel corpo come
tatto e chemio. Non basta misurare: serve una **cellula** che lo riceva e lo proietti
sul resto del tessuto, così il suono *agisce* sul corpo.

## Design (e il rapporto con le altre cellule)
- **Input esterno**, 4 dimensioni: `[energia, picco, variazione, 0]`
  (energia = media |x|; picco = max|x|; variazione = energia − energia_prec).
  Gli altri sensi ricevono così: Tatto = dolore+contatto; Chemio = rete/seriale.
- **Cellula**: `Udito`, 16 neuroni come le altre, peso **Xavier (NON addestrato)** —
  dichiarato apertamente. L'addestramento dell'udito è un passo successivo.
- **FASCI (connessioni)**: `Udito → Integrat` — come Tatto→Integrat e Chemio→Integrat:
  i sensi informano l'integratore. Nessun altro fascio in v1 (si aggiungono quando
  l'udito ha comportamento osservabile). *Questo è "rivedere il comportamento con le altre".*
- **Rappresentazioni apprese** (predictor `[i16;64]`, interprete `[f32;64]`, pattern):
  restano sulle **4 cellule originali**. I loro pesi sono addestrati su 64: includere
  l'udito richiede ri-addestrarli → lavoro separato. Qui l'udito agisce **dentro la
  dinamica** (modula Integrat); la sua lettura da parte della corteccia = dopo.
- **Osservabilità**: riga GUI `UDITO`, linea seriale, e — **con transizione sicura** —
  campo telemetria `ud` **opzionale** (l'Exo già deployato non lo manda: il listener
  deve accettarlo assente). ⚠️ NON si cambia il numero di celle in `c`: il listener
  controlla `len(c)==MAX_CELLS` e scarterebbe tutto (lezione 11/10).

## Passi (TDD: test rosso → codice → verde)
1. **cfc.rs** — `CellId::Udito`, `Tessuto.udito`, `step(..., udito_input, w_udito)`,
   bundle Udito→Integrat. **Banco host** `bench/cfc_host.rs`: routing Udito→Integrat;
   la cellula si muove con input non nullo; input nullo → resto.
2. **main.rs** — `W_UDITO` (Xavier), `FASCI` 2→3, `udito_input` da `hda::energy/picco`,
   chiamata `step`, riga GUI `UDITO`, linea seriale `UDITO:`.
3. **telemetria (opzionale, sicura)** — campo `ud` in `net.rs` + `exo_listen.py`
   (+ set ammesse) + contract test; l'Exo vecchio resta valido. Banco `net_host` aggiornato.
4. **Verifica QEMU** — con `make run-audio`: la cellula UDITO esiste, e con un input
   di prova (comando seriale, come `PAIN`) si vede che **Integrat reagisce**. Il suono
   vero si prova sul Lenovo (in QEMU il mic è silenzio).

## Criterio di completamento
- `make test` **verde**; in QEMU la cellula `UDITO` è visibile e, stimolata, **muove
  Integrat**; nessun PANIC; il contratto telemetria resta allineato.
- Il deploy sul Lenovo è un passo a parte (chiavetta fisica), quando Alfonso vuole.

## Cosa NON è in questo piano
- Addestrare i pesi dell'udito; includere l'udito in predictor/interprete/pattern;
  gli altri sensi della coda (touchpad, telecamera) — si fanno dopo, stesso metodo.
