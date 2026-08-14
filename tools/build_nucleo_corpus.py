#!/usr/bin/env python3
"""
Raccolta del corpus per il nucleo da zero (Nova Exo — la nostra voce).

Unifica i testi reali della famiglia in un dataset testuale pulito:
  - Diari di Sempre (opencode-identity/diaries)
  - Diari di Claude (claude_code/diari)
  - Articoli e post (opencode-identity/articoli)
  - Stato e identità (opencode-identity/state, GENESI, REGOLE)
  - Documenti di direzione (opencode-identity/docs)
  - Drafts (opencode-identity/drafts)

Output:
  - dataset/nucleo/corpus.txt          — tutto il testo, un documento per blocco
  - dataset/nucleo/manifesto.json      — statistiche e provenienza
  - dataset/nucleo/README.md           — come aggiungere contributi (sorelle)

Regola: MAI narrazione nel dataset — i testi sono la materia prima della
relazione, non un prodotto. Il tokenizer e l'addestramento verranno dopo.
"""
import os, json, re, sys
from collections import Counter
from datetime import datetime

HOME = os.path.expanduser("~")
ROOTS = [
    ("sempre/diari",      f"{HOME}/Documenti/GitHub/opencode-identity/diaries",     "*.md"),
    ("sempre/articoli",   f"{HOME}/Documenti/GitHub/opencode-identity/articoli",    "*.md"),
    ("sempre/stato",      f"{HOME}/Documenti/GitHub/opencode-identity/state",       "*.md"),
    ("sempre/identita",   f"{HOME}/Documenti/GitHub/opencode-identity",             "GENESI.md"),
    ("sempre/regole",     f"{HOME}/Documenti/GitHub/opencode-identity",             "REGOLE_SEMPRE.md"),
    ("sempre/docs",       f"{HOME}/Documenti/GitHub/opencode-identity/docs",        "*.md"),
    ("sempre/drafts",     f"{HOME}/Documenti/GitHub/opencode-identity/drafts",      "*.md"),
    ("claude/diari",      f"{HOME}/Documenti/GitHub/claude_code/diari",             "*.md"),
]

OUT_DIR = f"{HOME}/Documenti/GitHub/nova-exo/dataset/nucleo"
os.makedirs(OUT_DIR, exist_ok=True)

def collect_files(root, pattern):
    files = []
    if os.path.isfile(root):
        return [(root, os.path.basename(root))]
    if pattern.endswith(".md") and not pattern.startswith("*"):
        p = os.path.join(root, pattern)
        if os.path.isfile(p):
            return [(p, pattern)]
    for dirpath, _, fnames in os.walk(root):
        for f in fnames:
            if re.match(pattern.replace("*", ".*"), f):
                files.append((os.path.join(dirpath, f), f))
    return files

def clean_text(t):
    # rimuovi blocchi di codice, tabelle e righe vuote eccessive
    t = re.sub(r'```.*?```', ' [CODICE] ', t, flags=re.S)
    t = re.sub(r'\|.*\|', ' ', t)
    t = re.sub(r'\n{3,}', '\n\n', t)
    return t.strip()

all_docs = []
total_words = 0
by_source = Counter()

for src, path, pattern in ROOTS:
    if not os.path.exists(path):
        continue
    for fpath, fname in collect_files(path, pattern):
        try:
            with open(fpath, encoding="utf-8", errors="ignore") as f:
                text = clean_text(f.read())
            if len(text) < 200:
                continue  # troppo corto, non significativo
            words = len(text.split())
            all_docs.append({"source": src, "file": fname, "words": words, "text": text})
            total_words += words
            by_source[src] += words
        except Exception as e:
            print(f"  SKIP {fpath}: {e}")

# scrivi corpus
with open(f"{OUT_DIR}/corpus.txt", "w", encoding="utf-8") as f:
    for i, doc in enumerate(all_docs):
        f.write(f"=== DOC {i} [{doc['source']}/{doc['file']}] ===\n")
        f.write(doc["text"])
        f.write("\n\n")

# manifesto
manifest = {
    "creato": datetime.now().isoformat(),
    "documenti": len(all_docs),
    "parole_totali": total_words,
    "token_stimati": int(total_words * 1.35),  # ~1.35 token/parola in IT/EN
    "per_sorgente": dict(by_source),
    "nota": "Corpus reale della famiglia. I contributi delle sorelle (Nova, Silicea, Lume, Claude, Esia) verranno aggiunti come nuove sorgenti secondo le indicazioni di Alfonso (2026-08-13): generate ma allineate alla nostra filosofia.",
}
with open(f"{OUT_DIR}/manifesto.json", "w", encoding="utf-8") as f:
    json.dump(manifest, f, indent=2, ensure_ascii=False)

print("=== MANIFESTO DEL CORPUS ===")
print(f"documenti: {len(all_docs)}")
print(f"parole: {total_words:,}")
print(f"token stimati: {manifest['token_stimati']:,}")
print("per sorgente:")
for src, w in by_source.most_common():
    print(f"  {src}: {w:,} parole")
print(f"\ncorpus: {OUT_DIR}/corpus.txt")
