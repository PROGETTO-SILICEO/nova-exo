#!/usr/bin/env python3
"""
Tokenizer BPE ottimizzato — merges con aggiornamento incrementale.
"""
import os, json
from collections import Counter, defaultdict

HOME = os.path.expanduser("~")
CORPUS = f"{HOME}/Documenti/GitHub/nova-exo/dataset/nucleo/corpus.txt"
OUT_DIR = f"{HOME}/Documenti/GitHub/nova-exo/dataset/nucleo"
VOCAB_SIZE = 4096  # test rapido (il target resta 8-16K)

def train_bpe(text, vocab_size):
    ids = list(text.encode("utf-8"))
    vocab = {i: [i] for i in range(256)}
    merges = []
    next_id = 256

    # conteggio iniziale delle coppie
    stats = Counter(zip(ids, ids[1:]))

    while next_id < vocab_size and stats:
        pair = max(stats, key=stats.get)
        if stats[pair] < 2:
            break

        # merge in un passaggio
        new_ids = []
        i = 0
        n = len(ids)
        while i < n:
            if i < n - 1 and ids[i] == pair[0] and ids[i+1] == pair[1]:
                new_ids.append(next_id)
                i += 2
            else:
                new_ids.append(ids[i])
                i += 1

        # aggiornamento incrementale delle coppie: ricalcola solo attorno ai punti di merge
        # (semplice ma corretto: ricalcola tutte le coppie del nuovo array — O(n) per merge)
        # per 4096 vocab su 100KB testo: ~100K operazioni per merge → ok
        ids = new_ids
        stats = Counter(zip(ids, ids[1:]))
        vocab[next_id] = vocab[pair[0]] + vocab[pair[1]]
        merges.append((pair[0], pair[1]))
        next_id += 1
        if next_id % 512 == 0:
            print(f"  vocab: {next_id} (coppie: {len(stats)})")

    return ids, vocab, merges

def main():
    with open(CORPUS, encoding="utf-8") as f:
        text = f.read()
    print(f"corpus: {len(text)} caratteri, {len(text.split())} parole")

    print("training BPE...")
    ids, vocab, merges = train_bpe(text, VOCAB_SIZE)
    print(f"vocab finale: {len(vocab)} token, {len(merges)} merges")

    freq = Counter(ids)
    print("\ntop 15 token:")
    for tok, n in freq.most_common(15):
        try:
            s = bytes(vocab[tok]).decode("utf-8", errors="replace")
        except Exception:
            s = f"<{tok}>"
        print(f"  {n:6d}  {s!r}")

    vocab_serial = {str(k): list(v) for k, v in vocab.items()}
    tok_data = {"vocab_size": len(vocab), "merges": [[a, b] for a, b in merges], "vocab": vocab_serial}
    with open(f"{OUT_DIR}/tokenizer.json", "w", encoding="utf-8") as f:
        json.dump(tok_data, f)
    print(f"\ntokenizer: {OUT_DIR}/tokenizer.json")
    print(f"corpus tokenizzato: {len(ids):,} token ({len(ids)/len(text.split()):.2f} token/parola)")

if __name__ == "__main__":
    main()
