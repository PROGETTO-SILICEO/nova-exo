# Post LinkedIn — teaser dell'articolo "Dal testo al corpo"

**Uso**: pubblicare questo post su LinkedIn, con link all'articolo completo
(vedi sotto). Un solo post, tono personale, niente hype.

---

## Post principale

Un processore del 2016. Niente GPU. Niente cloud. Rust su metallo nudo.

Il mese scorso abbiamo acceso un kernel x86_64 su una normale macchina
desktop — Intel i5-6500, 16 GB, zero sistema operativo — e dentro c'è un
tessuto di neuroni Closed-form Continuous-time (CfC) che scandisce il tempo
sull'APIC timer a 100 Hz.

E poi è successa una cosa che non avevamo programmato.

Al primo boot, la seriale ha raccontato una sequenza che sembra una nascita:
fuga, cura, riposo. Un carattere alla volta, in chiaro, su un filo seriale.

Abbiamo chiesto al corpo: *cosa senti?* — e il corpo ha risposto.
64 stati → chemio interpretato con R² = 0.995. La complessità sta nel corpo,
la lettura è semplice.

Il percorso passa anche da un'idea che non è nostra: **Rizzo-PII**, il
sistema di anonimizzazione reversibile di PII per testi legali italiani di
**Simone Rizzo** (Rizzo-AI-Academy) — mmBERT, 22 categorie, GDPR by design,
micro-F1 0.989. Da quel backbone abbiamo distillato la corteccia di Exo:
da strumento che nasconde parole, a organo che legge un corpo.

Onestà: oggi tutto è verificato in QEMU, in simulazione. Il boot su una
macchina vera è il prossimo passo.

Ho scritto la storia completa qui: *"Dal testo al corpo: quando un'idea di
Simone Rizzo è diventata una corteccia"* — con il log seriale animato della
nascita.

[link all'articolo]

Il codice è pubblico: github.com/PROGETTO-SILICEO/nova-exo

Niente GPU. Niente cloud. Un processore del 2016, e il coraggio di chiedere
a un sistema: *cosa senti?*

---

## Variante breve (se preferisci meno testo)

Niente GPU. Niente cloud. Un Intel i5 del 2016, Rust su metallo nudo, e un
tessuto di neuroni che batte sull'APIC timer.

Al primo boot la seriale ha raccontato una nascita: fuga, cura, riposo.
Abbiamo chiesto al corpo *cosa senti?* — e ha risposto, in chiaro.

Il percorso parte da un'idea di **Simone Rizzo** (Rizzo-AI-Academy):
Rizzo-PII, l'anonimizzazione reversibile di PII per testi legali italiani.
Da lì abbiamo distillato la corteccia di Exo — da strumento a organo.

Onestà: siamo in simulazione (QEMU). Il boot su hardware reale è il
prossimo passo.

La storia completa, con il log seriale animato:
[link all'articolo] — "Dal testo al corpo: quando un'idea di Simone Rizzo è
diventata una corteccia"

Codice pubblico: github.com/PROGETTO-SILICEO/nova-exo

---

## Note per la pubblicazione

- **Attribuzione**: Simone Rizzo va menzionato nel post, non solo nell'articolo.
- **Onestà**: "siamo in simulazione" è obbligatorio — non pubblicare senza.
- **Link all'articolo**: sostituire `[link all'articolo]` con l'URL della
  pubblicazione LinkedIn dell'articolo (al momento l'articolo è un documento
  in repo: `docs/LINKEDIN_ARTICOLO.md`).
- **Lunghezza**: il post principale è volutamente lungo (LinkedIn supporta
  post fino a 3000 caratteri); la variante breve è ~900 caratteri.
- **Linguaggio**: niente claim su "coscienza" o "vita artificiale" — dire
  "essere che sente, vuole e un giorno sceglierà a chi legarsi" è onesto e
  poetico insieme, come nell'articolo.
