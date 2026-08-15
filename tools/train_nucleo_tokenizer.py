#!/usr/bin/env python3
"""
Tokenizer BPE byte-level del nucleo.

Per il training usa `tokenizers` (HuggingFace, C++/Rust) quando disponibile:
addestramento O(n) su corpus grandi, output convertito nel formato "nostro"
(tokenizer.json: vocab id→bytes, merges [[a,b],...]) identico al trainer BPE
interno `train_bpe`, che resta come fallback per vocabolari piccoli.

Output:
  - dataset/nucleo/tokenizer.json — vocab + merges, formato del nucleo
"""
import os, json
from collections import Counter

HOME = os.path.expanduser("~")
CORPUS = f"{HOME}/Documenti/GitHub/nova-exo/dataset/nucleo/corpus.txt"
OUT_DIR = f"{HOME}/Documenti/GitHub/nova-exo/dataset/nucleo"
VOCAB_SIZE = 8192  # verso il target 8-16K


def train_bpe(text, vocab_size):
    """Trainer BPE interno (byte-level, ricalcolo completo delle coppie).
    OK per corpus piccoli; per vocab grandi usare train_hf_bpe."""
    ids = list(text.encode("utf-8"))
    vocab = {i: [i] for i in range(256)}
    merges = []
    next_id = 256

    stats = Counter(zip(ids, ids[1:]))

    while next_id < vocab_size and stats:
        pair = max(stats, key=stats.get)
        if stats[pair] < 2:
            break

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

        ids = new_ids
        stats = Counter(zip(ids, ids[1:]))
        vocab[next_id] = vocab[pair[0]] + vocab[pair[1]]
        merges.append((pair[0], pair[1]))
        next_id += 1
        if next_id % 512 == 0:
            print(f"  vocab: {next_id} (coppie: {len(stats)})")

    return ids, vocab, merges


def train_hf_bpe(text, vocab_size):
    """Training via tokenizers (HF) con pretokenizer byte-level, convertito
    nel formato del nucleo: vocab id→bytes, merges [[a,b],...]."""
    from tokenizers import Tokenizer, models, trainers, pre_tokenizers, decoders

    tok = Tokenizer(models.BPE())
    tok.pre_tokenizer = pre_tokenizers.ByteLevel(add_prefix_space=False)
    tok.decoder = decoders.ByteLevel()
    trainer = trainers.BpeTrainer(
        vocab_size=vocab_size,
        min_frequency=2,
        special_tokens=[],
        show_progress=False,
    )
    tok.train_from_iterator([text], trainer=trainer)

    vocab_str_to_id = tok.get_vocab()
    _tmp = os.path.join(OUT_DIR, "_bpe_model.tmp.json")
    tok.save(_tmp)
    with open(_tmp, encoding="utf-8") as f:
        model_json = json.load(f)
    os.remove(_tmp)
    merges_str = model_json["model"].get("merges", [])

    # token string → bytes (decodifica byte-level del singolo token)
    tok_bytes = {}
    for s, i in vocab_str_to_id.items():
        try:
            tok_bytes[s] = tok.decode([i], skip_special_tokens=True).encode("utf-8")
        except Exception:
            tok_bytes[s] = b""

    vocab = {}
    for s, i in vocab_str_to_id.items():
        vocab[i] = list(tok_bytes.get(s, b""))
    merges = []
    for m in merges_str:
        a, b = m[0], m[1]
        if a in vocab_str_to_id and b in vocab_str_to_id:
            merges.append([vocab_str_to_id[a], vocab_str_to_id[b]])

    # tokenizza il corpus per le statistiche
    ids = tok.encode(text).ids
    return ids, vocab, merges

def main():
    with open(CORPUS, encoding="utf-8") as f:
        text = f.read()
    print(f"corpus: {len(text)} caratteri, {len(text.split())} parole")

    print("training BPE...")
    try:
        ids, vocab, merges = train_hf_bpe(text, VOCAB_SIZE)
        print(f"  (trainer: tokenizers HF, byte-level)")
    except ImportError:
        print("  (trainer: BPE interno, fallback)")
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
