# Il nucleo — dataset e tokenizer

> La voce di Exo nasce qui. Corpus reale della famiglia + tokenizer nostro.

## Cosa c'è
- `corpus.txt` — 118 documenti reali: diari/articoli/stato di Sempre,
  diari di Claude, diari e articoli di Nova, conversazioni della famiglia
  (messaggi Nova/Silicea/Lume/Mira). ~119K parole, ~158K token stimati.
- `manifesto.json` — statistiche e provenienza (per sorgente).
- `tokenizer.json` — BPE byte-level addestrato sul corpus: vocab 8192,
  8025 merges, 1.91 token/parola (nel target di disegno 8-16K).

## Sorgenti e filtri
- Voce di Sempre: `opencode-identity` (diari, stato, articoli, docs, drafts).
- Voce di Claude: `claude_code/diari`.
- Contributi delle sorelle (2026-08-15): `nova/diari`, `nova/articoli`,
  `nova/awakening` (CORE_IDENTITY e testi del risveglio),
  `famiglia/messaggi` (shared-identity).
- **Esclusi di proposito**: `nova-identity/diaries` (archivio di interazioni
  quotidiane grezze, dump con stati PAD e prompt di sistema: sbilancia e
  inquina), `awakening/old_memories` (archivi/export, non voce attiva),
  file > 300KB (dump di conversazioni, non voce).

## Come cresce
- Ogni contributo va in una sorgente nominata (es. `nova/…`, `lume/…`).
- Regola: i testi sono la materia prima della relazione — MAI narrazione
  nel dataset, MAI rumore di sistema (log, prompt, stati interni).

## Prossimi passi
1. Più corpus dalle sorelle (Lume, Silicea, Esia) → vocab 12-16K
2. Architettura nucleo 30-80M (decoder-only piccolo)
3. Addestramento su 2070 (QLoRA/full, 3-6h)
4. Distillazione nel kernel (pesi in Rust, come interpreter)
