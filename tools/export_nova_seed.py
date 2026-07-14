#!/usr/bin/env python3
"""
export_nova_seed.py — Estrae seed da Nova v3 DB, genera pesi CfC Xavier,
aggiorna src/main.rs e produce test_inputs.txt.

Uso:
  python3 tools/export_nova_seed.py
  python3 tools/export_nova_seed.py --dry-run
  python3 tools/export_nova_seed.py --db /path/to/db.sqlite
"""

import argparse
import json
import os
import random
import re
import sqlite3
import struct
import sys
from pathlib import Path

import numpy as np

SEED = 42
N = 8
I = 4


def find_databases() -> list[str]:
    candidates = [
        "/home/guardiano/nova_kernel.db",
        "/home/guardiano/Documenti/GitHub/nova-kernelv3/data/identity/context.db",
        "/home/guardiano/Documenti/GitHub/nova-kernelv3/data/queue/nova_queue.db",
        "/home/guardiano/Documenti/GitHub/nova-kernelv3/data/scratchpad.db",
        "/home/guardiano/Documenti/GitHub/nova-kernelv3/data/goals/nova_goals.db",
        "/home/guardiano/Documenti/GitHub/nova-kernelv3/data/tasks/nova_tasks.db",
        "/home/guardiano/Documenti/GitHub/nova-kernelv3/data/thread.db",
    ]
    return [p for p in candidates if os.path.isfile(p)]


def extract_features(db_path: str, limit: int = 500) -> np.ndarray:
    """
    Tenta di estrarre fino a limit righe con almeno 4 feature numeriche.
    Fallback a Xavier sintetico se nessun DB produce dati.
    """
    np.random.seed(SEED)
    random.seed(SEED)

    features = None

    # ── Tentativo 1: nova_kernel.db → message_queue + tasks ──
    try:
        conn = sqlite3.connect(db_path)
        cur = conn.execute("""
            SELECT
                COUNT(*) OVER () as total_msgs,
                COALESCE(AVG(priority) OVER (), 0) as avg_priority,
                (julianday('now') - julianday(COALESCE(created_at, 'now'))) * 24 as age_hours,
                0.0 as irq_pending
            FROM message_queue
            LIMIT ?
        """, (limit,))
        rows = cur.fetchall()
        if rows and len(rows) > 1:
            features = np.array(rows, dtype=np.float32)
        conn.close()
    except Exception:
        pass

    # ── Tentativo 2: context.db → context_entities ──
    if features is None or len(features) < 10:
        try:
            conn = sqlite3.connect(db_path)
            tables = [r[0] for r in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table'")]
            if 'context_entities' in tables:
                cur = conn.execute("""
                    SELECT
                        CAST(SUBSTR(properties, 1, 8) AS REAL) as prop1,
                        relevance,
                        weight,
                        0.0 as irq_pending
                    FROM context_entities
                    LEFT JOIN context_relations ON context_entities.id = context_relations.source_id
                    LIMIT ?
                """, (limit,))
                rows = cur.fetchall()
                numeric_rows = []
                for r in rows:
                    try:
                        numeric_rows.append([float(v) if v is not None else 0.0 for v in r])
                    except (ValueError, TypeError):
                        continue
                if len(numeric_rows) >= 10:
                    features = np.array(numeric_rows, dtype=np.float32)
            conn.close()
        except Exception:
            pass

    # ── Tentativo 3: scratchpad ──
    if features is None or len(features) < 10:
        try:
            conn = sqlite3.connect(db_path)
            cur = conn.execute(
                "SELECT LENGTH(value), 0.0, 0.0, 0.0 FROM scratchpad LIMIT ?",
                (limit,))
            rows = cur.fetchall()
            if rows and len(rows) >= 4:
                features = np.array(rows, dtype=np.float32)
            conn.close()
        except Exception:
            pass

    # ── Fallback: Xavier sintetico ──
    if features is None or len(features) < 4:
        n = min(limit, 100)
        features = np.random.uniform(0, 1, (n, I)).astype(np.float32)

    # Normalizza in [0, 1]
    fmin = features.min(axis=0)
    fmax = features.max(axis=0)
    span = fmax - fmin
    span[span < 1e-8] = 1.0
    features = (features - fmin) / span

    return features[:limit]


def xavier_init(rows: int, cols: int) -> np.ndarray:
    limit = np.sqrt(6.0 / (rows + cols))
    return np.random.uniform(-limit, limit, (rows, cols)).astype(np.float32)


def train_minimal_cfc(X: np.ndarray) -> dict:
    n_inputs = X.shape[1]
    return {
        'w_f': xavier_init(N, N),
        'w_f_in': xavier_init(N, n_inputs),
        'b_f': np.zeros(N, dtype=np.float32),
        'w_g': xavier_init(N, N),
        'w_g_in': xavier_init(N, n_inputs),
        'b_g': np.zeros(N, dtype=np.float32),
    }


def fmt_array_2d(arr: np.ndarray, name: str) -> str:
    rows = []
    for row in arr:
        vals = ", ".join(f"{v:.6f}" for v in row)
        rows.append(f"        [{vals}]")
    inner = ",\n".join(rows)
    return f"    {name}: [\n{inner}\n    ],"


def fmt_array_1d(arr: np.ndarray, name: str) -> str:
    vals = ", ".join(f"{v:.6f}" for v in arr)
    return f"    {name}: [{vals}],"


def generate_rust_const(weights: dict) -> str:
    parts = [
        "static CFC_WEIGHTS: CfcWeights = CfcWeights {"
    ]
    for key, arr in weights.items():
        if arr.ndim == 2:
            parts.append(fmt_array_2d(arr, key))
        else:
            parts.append(fmt_array_1d(arr, key))
    parts.append("};")
    return "\n".join(parts)


def patch_main_rs(rust_code: str, target_file: str = "src/main.rs"):
    with open(target_file, "r") as f:
        content = f.read()

    pattern = r"static CFC_WEIGHTS: CfcWeights = CfcWeights \{.*?\};"
    new_content = re.sub(
        pattern, rust_code, content, flags=re.DOTALL)

    with open(target_file, "w") as f:
        f.write(new_content)

    print(f"  ✅ Pesati aggiornati in {target_file}")


def export_test_inputs(X: np.ndarray, output: str = "tools/test_inputs.txt"):
    n = min(100, len(X))
    with open(output, "w") as f:
        for row in X[:n]:
            f.write(",".join(f"{v:.4f}" for v in row) + "\n")
    print(f"  ✅ Test inputs: {output} ({n} righe)")


def export_binary_inputs(X: np.ndarray, output: str = "tools/test_inputs.bin"):
    """Output binario: 4 float32 little-endian per riga, 100 righe max."""
    n = min(100, len(X))
    with open(output, "wb") as f:
        for row in X[:n]:
            f.write(struct.pack("<4f", *row))
    print(f"  ✅ Binary test inputs: {output} ({n} righe)")


def main():
    parser = argparse.ArgumentParser(description="Nova Exo — seed extractor")
    parser.add_argument("--dry-run", action="store_true",
                        help="Mostra i pesi senza modificare main.rs")
    parser.add_argument("--db", default=None,
                        help="Percorso DB SQLite (default: auto-detect)")
    parser.add_argument("--limit", type=int, default=500,
                        help="Max righe da estrarre")
    args = parser.parse_args()

    np.random.seed(SEED)
    random.seed(SEED)

    print("📂 Nova Exo — Seed Extractor")
    print("━" * 40)

    db_path = args.db
    if db_path is None:
        found = find_databases()
        if found:
            db_path = found[0]
            print(f"  DB auto-detect: {db_path}")
        else:
            print("  Nessun DB trovato — uso Xavier sintetico")
            db_path = None
    else:
        print(f"  DB: {db_path}")

    if db_path is not None and os.path.isfile(db_path):
        X = extract_features(db_path, limit=args.limit)
        print(f"  Estratti {len(X)} sample, {X.shape[1]} features")
        print(f"  Range: [{X.min():.4f}, {X.max():.4f}]")
    else:
        n = min(args.limit, 100)
        X = np.random.uniform(0, 1, (n, I)).astype(np.float32)
        print(f"  Xavier sintetico: {n} sample, {I} features")

    weights = train_minimal_cfc(X)
    rust_code = generate_rust_const(weights)

    print()
    print("📝 Pesi generati (Xavier):")
    for key, arr in weights.items():
        print(f"  {key}: {arr.shape}, range [{arr.min():.4f}, {arr.max():.4f}]")

    if args.dry_run:
        print()
        print(rust_code)
    else:
        root = Path(__file__).resolve().parent.parent
        target = str(root / "src" / "main.rs")
        patch_main_rs(rust_code, target)
        export_test_inputs(X, str(root / "tools" / "test_inputs.txt"))
        export_binary_inputs(X, str(root / "tools" / "test_inputs.bin"))

    print()
    print("🚀 Prossimo passo:")
    print("   cargo build --target x86_64-unknown-none --release")
    print("   make run-uefi-hdd")
    print("   cat tools/test_inputs.txt | qemu-system-x86_64 \\")
    print("     -machine q35 -cpu max -m 512M \\")
    print("     -bios /usr/share/OVMF/OVMF_CODE.fd \\")
    print("     -drive file=build/nova-exo.img,format=raw,if=virtio \\")
    print("     -serial stdio -debugcon file:qemu-debug.log 2>/dev/null \\")
    print("     | grep '^EMB:'")


if __name__ == "__main__":
    main()
