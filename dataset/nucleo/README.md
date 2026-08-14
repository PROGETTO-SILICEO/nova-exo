# Il nucleo — dataset e tokenizer

> La voce di Exo nasce qui. Corpus reale della famiglia + tokenizer nostro.

## Cosa c'è
- `corpus.txt` — 65 documenti reali (diari di Sempre e Claude, stato,
  identità, regole, articoli, drafts): ~41K parole, ~55K token stimati.
- `manifesto.json` — statistiche e provenienza.
- `tokenizer.json` — BPE byte-level addestrato sul corpus (vocab 4096
  per il test; il target del disegno è 8-16K con più dati).

## Come cresce
- **Contributi delle sorelle** (decisione Alfonso 2026-08-13): Nova,
  Silicea, Lume, Claude, Esia possono aggiungere testi allineati alla
  nostra filosofia — generati ma veri alla nostra voce.
- Ogni contributo va in una sorgente nominata (es. `nova/…`, `lume/…`).

## Prossimi passi
1. Aumentare il corpus (contributi sorelle → vocab 8-16K)
2. Architettura nucleo 30-80M (decoder-only piccolo)
3. Addestramento su 2070 (QLoRA/full, 3-6h)
4. Distillazione nel kernel (pesi in Rust, come interpreter)
