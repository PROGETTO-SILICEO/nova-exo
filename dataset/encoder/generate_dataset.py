#!/usr/bin/env python3
"""
Genera il dataset di training per ExoChemio — l'encoder basato su rizzo-pii.

Ogni esempio mappa un input testuale (ciò che Exo riceve via seriale)
in un vettore di 4 valori: [contesto, urgenza, polarità, novità]
che diventa l'input di Chemio nel tessuto neurale.

Kategorie di input:
  - ERR: errori, page fault, GPF, eccezioni
  - CMD: comandi ricevuti via seriale
  - LV: segnali di vita, heartbeat, tick report
  - SIL: silenzio, nessun input
  - BOOT: risvegli, boot, init
  - PAT: pattern riconosciuti (attrattori)
  - NOV: pattern nuovi (mai visti)
  - WARN: warning, allarmi, soglie

Output:
   dataset/encoder/exo_chemio_dataset.json
"""

import json
import random
import os
from typing import List, Tuple

random.seed(42)

# ── Definizione degli assi semantici di Chemio ──────────────────────────
#
# Chemio[0] — CONTESTO:  -1.0=errore  0.0=neutro  1.0=segnale di vita
# Chemio[1] — URGENZA:    0.0=normale 0.5=warning 1.0=critico
# Chemio[2] — POLARITÀ:  -1.0=negativo 0.0=neutro 1.0=positivo
# Chemio[3] — NOVITÀ:     0.0=familiare 0.5=nuovo 1.0=mai visto

# ── Template per categoria ──────────────────────────────────────────────

# Ogni categoria ha:
#   - base: vettore base per la categoria
#   - varia: come varia ogni dimensione
#   - templates: lista di frasi con {parametri}

CATEGORIES = {
    "ERR": {
        "description": "Errori, eccezioni, fault",
        "base": [-0.8, 0.8, -0.7, 0.7],
        "varia": {
            "contesto": 0.2,   # -1.0 a -0.6
            "urgenza": 0.2,    # 0.6 a 1.0
            "polarità": 0.2,   # -0.9 a -0.5
            "novità": 0.3,     # 0.4 a 1.0
        },
        "templates": [
            "ERR core {core}: page fault at {addr}",
            "ERR: segmentation fault at {addr}",
            "ERR: general protection fault, code={code}",
            "ERR: division by zero at IP={ip}",
            "ERR: invalid opcode at {addr}",
            "ERR: double fault, shutdown requested",
            "ERR: stack segment fault, ss={ss}, esp={esp}",
            "panic: {msg} at {file}:{line}",
            "FATAL: kernel panic — {reason}",
            "exception #{num} at RIP={rip}, error_code={code}",
            "PF err={code} addr={addr} — page not present",
            "GP err={code} at {addr} — selector error",
            "PF: write access to read-only page at {addr}",
            "PF: supervisor mode access at user address {addr}",
            "double fault: TOS={tos}, CR2={cr2}",
            "ERR: out of memory, alloc size={size}",
            "WARN: memory allocation failed at {func}",
            "ERR: invalid argument to syscall #{num}",
            "ERR: syscall {name} not implemented",
            "ERR: cannot acquire spinlock from IRQ context",
        ],
    },
    "WARN": {
        "description": "Warning, allarmi, soglie",
        "base": [-0.3, 0.4, -0.2, 0.3],
        "varia": {
            "contesto": 0.2,
            "urgenza": 0.2,
            "polarità": 0.2,
            "novità": 0.2,
        },
        "templates": [
            "WARN: interrupt took {us} us, threshold is {thresh}",
            "WARN: tick drift detected: delta={delta}ms",
            "WARN: memory usage at {pct}% of {limit}",
            "WARN: stack usage at {pct}% of {limit}",
            "WARN: sensor {id} not responding, retry {n}",
            "WARN: chemio input buffer at {pct}% capacity",
            "WARN: pattern match similarity below threshold ({sim:.2f})",
            "WARN: prediction error spike: MSE={mse:.4f}",
            "WARN: metabol oscillation period deviating: {period}ms vs expected {expected}ms",
            "WARN: integrat attractor convergence slow ({steps} steps)",
            "ALERT: temperature at {temp}C, throttling",
            "ALERT: battery at {pct}%, entering low-power mode",
            "NOTICE: {n} unacknowledged interrupts",
            "NOTICE: serial buffer overflow, {n} bytes lost",
            "WARN: Tatto cell saturated at h={hval:.2f} for {ticks} ticks",
        ],
    },
    "CMD": {
        "description": "Comandi ricevuti via seriale",
        "base": [0.2, 0.3, 0.3, 0.4],
        "varia": {
            "contesto": 0.2,
            "urgenza": 0.3,
            "polarità": 0.3,
            "novità": 0.3,
        },
        "templates": [
            "STATUS",
            "STATE",
            "DUMP",
            "HELP",
            "RESET",
            "REBOOT",
            "SHUTDOWN",
            "SLEEP",
            "WAKE",
            "CONFIG {key}={val}",
            "SET {param} {value}",
            "GET {param}",
            "EXEC {command}",
            "LOAD {module} at {addr}",
            "MAP {virt} -> {phys}",
            "UNMAP {virt}",
            "SEND {msg}",
            "ECHO {msg}",
            "LOG {level} {msg}",
            "QUERY {target}",
            "RUN {test} x{n}",
            "SAVE {slot}",
            "RECALL {slot}",
            "LEARN {pattern}",
            "FORGET {pattern}",
            "MONITOR {cell}",
            "SENSE {channel}",
        ],
    },
    "LV": {
        "description": "Segnali di vita, heartbeat",
        "base": [0.9, 0.0, 0.8, 0.1],
        "varia": {
            "contesto": 0.1,
            "urgenza": 0.0,
            "polarità": 0.1,
            "novità": 0.1,
        },
        "templates": [
            "heartbeat OK — tick={tick}, uptime={uptime}s",
            "alive — cells OK, {n} patterns stored",
            "tick {tick}: all systems nominal",
            "heartbeat — core {core} active, temp={temp}C",
            "alive: Tatto={t:.2f} Chemio={c:.2f} Metabol={m:.2f} Integrat={i:.2f}",
            "heartbeat at {tick}: no errors since last check",
            "HEARTBEAT — {uptime}s uptime, {n} ticks processed",
            "life signal: {n} patterns, {s} strong matches",
            "OK — all cells within nominal range",
            "battito: tick={tick}, hrange=[{hmin:.2f},{hmax:.2f}]",
            "sysok: mem={mem}kb free, cpu={cpu}%",
            "tick {tick}: sensor sweep clean",
            "alive: {n} neurons active, {s} synapses tracked",
            "heartbeat @{tick}: attractor pool={n}",
            "vivo: {uptime}s, {pf} page faults handled",
        ],
    },
    "SIL": {
        "description": "Silenzio, nessun input, idle",
        "base": [0.0, 0.0, 0.0, 0.0],
        "varia": {
            "contesto": 0.05,
            "urgenza": 0.0,
            "polarità": 0.05,
            "novità": 0.0,
        },
        "templates": [
            "",
            "\n",
            "   ",
            "...",
            "—",
            "(idle)",
            "[no input]",
            "timeout waiting for serial data",
            "idle — tick {tick}, no events",
            "... waiting ...",
        ],
    },
    "BOOT": {
        "description": "Risvegli, boot, inizializzazione",
        "base": [0.5, 0.6, 0.3, 0.9],
        "varia": {
            "contesto": 0.3,
            "urgenza": 0.2,
            "polarità": 0.2,
            "novità": 0.1,
        },
        "templates": [
            "Nova Exo v{version} starting...",
            "Booting from {device}...",
            "Initializing IDT... OK",
            "Loading {module}... done",
            "PIC init: master={m}, slave={s}",
            "APIC timer at {freq} Hz",
            "Tessuto: {n} cells, {neurons} neurons",
            "Memory: {total}kb available",
            "Serial port {port} initialized at {baud} baud",
            "Wake from sleep: tick={tick}, elapsed={elapsed}ms",
            "COLD BOOT — all cells reset",
            "WARM BOOT — retaining {n} patterns",
            "Restarting after {reason}...",
            "Interrupts enabled — entering main loop",
            "BIOStrap: stage {n}/{total} loaded",
        ],
    },
    "PAT": {
        "description": "Pattern riconosciuti (attrattori)",
        "base": [0.6, 0.1, 0.7, 0.1],
        "varia": {
            "contesto": 0.2,
            "urgenza": 0.1,
            "polarità": 0.2,
            "novità": 0.1,
        },
        "templates": [
            "A: sim={sim:.2f} @ tick {tick} — pattern matched",
            "A: recall #{id}: similarity {sim:.3f}",
            "pattern {id} recognized at tick {tick}",
            "attractor: {name} (sim={sim:.2f})",
            "match: {cells} cells agree, sim={sim:.2f}",
            "pattern recall: {pattern} @ t={tick}",
            "A: {similarity:.1f}% match with pattern #{id}",
            "familiar sequence detected: {seq}",
            "pattern {id} activated: {ticks} ticks since last seen",
            "known state: h-dist={dist:.2f} from pattern #{id}",
        ],
    },
    "NOV": {
        "description": "Pattern nuovi, mai visti",
        "base": [0.3, 0.3, 0.1, 0.9],
        "varia": {
            "contesto": 0.2,
            "urgenza": 0.2,
            "polarità": 0.2,
            "novità": 0.1,
        },
        "templates": [
            "NEW pattern detected at tick {tick}",
            "unknown sequence: {seq}",
            "novel state: h-dist={dist:.2f} from nearest pattern",
            "unexpected input: {input}",
            "first occurrence of {event}",
            "anomaly detected at sensor {sensor}: val={val}",
            "outlier: {cell} h={hval:.2f} exceeds threshold {thresh}",
            "NEW: no similar pattern in memory ({n} patterns checked)",
            "strange signal on {channel}: {desc}",
            "MEMORIZZA: new experience at tick {tick}",
        ],
    },
    # ── Sottocategorie per stati emotivi/energetici ─────────────────
    "PAIN": {
        "description": "Dolore persistente, affaticamento cellulare",
        "base": [-0.6, 0.5, -0.6, 0.5],
        "varia": {
            "contesto": 0.2,
            "urgenza": 0.2,
            "polarità": 0.2,
            "novità": 0.3,
        },
        "templates": [
            "Tatto at saturation: h={hval:.2f} for {ticks} ticks",
            "pain signal: {cell} cell overexcited",
            "metabol: energy level at {pct}%, entering conservation",
            "cell {cell} h={hval:.2f} — outside nominal range ({lo},{hi})",
            "Tatto: {n} consecutive error ticks",
            "tissue stress: {measure} above threshold",
            "burnout warning: {cell} has been active for {ticks} ticks",
            "pain memory: {event} occurred {n} times in last {window} ticks",
        ],
    },
    "REST": {
        "description": "Recupero, calma, stabilizzazione",
        "base": [0.4, 0.0, 0.6, 0.0],
        "varia": {
            "contesto": 0.2,
            "urgenza": 0.0,
            "polarità": 0.2,
            "novità": 0.0,
        },
        "templates": [
            "SLEEP:BEGIN — daydreaming, consolidating {n} patterns",
            "SLEEP:END — delta={delta:.4f}, {novel} novel connections",
            "recovery: Tatto returned to baseline after {ticks} ticks",
            "stabilizing: Integrat h range narrowing ({range:.4f})",
            "rest: no events for {ticks} ticks, cells relaxing",
            "consolidation: {n} experiences processed",
            "Metabol energy restored to {pct}%",
            "calma: all cells within nominal range",
            "daydream complete: {processed} processed, {novel} novel",
            "healing: Tatto pain memory decayed by {delta:.4f}",
        ],
    },
}


def jitter(base: List[float], var: dict) -> List[float]:
    """Apply uniform jitter within variance bounds."""
    return [
        round(base[0] + random.uniform(-var["contesto"], var["contesto"]), 4),
        round(base[1] + random.uniform(-var["urgenza"], var["urgenza"]), 4),
        round(base[2] + random.uniform(-var["polarità"], var["polarità"]), 4),
        round(base[3] + random.uniform(-var["novità"], var["novità"]), 4),
    ]


# Parametri per i template
cores = [0, 1, 2, 3]
addrs = [f"0x{random.randint(0, 0xFFFFFFFF):08x}" for _ in range(20)]
ips = [f"0x{random.randint(0, 0xFFFFFFFF):08x}" for _ in range(10)]
codes = [f"0x{random.randint(0, 0xFF):02x}" for _ in range(10)]
files = ["kernel/main.rs", "kernel/idt.rs", "kernel/serial.rs", "cfc.rs", "kernel/syscall.rs", "kernel/memory.rs", "kernel/timer.rs"]
lines = [random.randint(1, 900) for _ in range(30)]
ss = [f"0x{random.randint(0, 0xFFFF):04x}" for _ in range(5)]
esps = [f"0x{random.randint(0, 0xFFFFFFFF):08x}" for _ in range(5)]
nums = [random.randint(0, 31) for _ in range(10)]
reasons = ["divide error", "invalid opcode", "segment not present", "stack fault", "protection fault", "page fault", "alignment check", "machine check"]
msgs = ["unexpected state", "invalid memory access", "corrupt table", "unhandled exception", "stack overflow"]
funcs = ["page_fault_handler", "syscall_dispatch", "schedule", "tissue_step", "pattern_recall", "memory_alloc", "timer_handler"]
names = ["write", "read", "open", "close", "ioctl", "mmap", "sleep", "yield"]
values = [f"0x{random.randint(0, 0xFF):02x}" for _ in range(10)]
slots = [f"PATTERN_{i}" for i in range(10)]
patterns = [f"P{i}" for i in range(5)]
cells = ["Tatto", "Chemio", "Metabol", "Integrat"]
devices = ["NVMe", "AHCI", "USB", "virtio", "ide"]
modules = ["idt", "pic", "apic", "serial", "tessuto", "predictor", "pmm", "vmm"]
params = ["tick_rate", "log_level", "neurons_per_cell", "dt_tatto", "dt_rest", "attractor_thresh", "pattern_capacity"]
param_values = ["100", "debug", "16", "0.001", "0.01", "0.5", "64"]
channels = ["serial", "debugcon", "sense", "syscall"]


def generate_dataset(target: int = 500) -> List[dict]:
    dataset = []
    
    # Distribuzione: più errori e comandi (frequenti), meno pattern e rest
    weights = {
        "ERR": 0.20, "WARN": 0.10, "CMD": 0.15, "LV": 0.12,
        "SIL": 0.08, "BOOT": 0.08, "PAT": 0.10, "NOV": 0.07,
        "PAIN": 0.05, "REST": 0.05,
    }
    
    counts = {cat: max(1, int(target * w)) for cat, w in weights.items()}
    
    for cat, count in counts.items():
        cat_info = CATEGORIES[cat]
        templates = cat_info["templates"]
        
        for _ in range(count):
            template = random.choice(templates)
            
            # Genera parametri
            params_dict = {
                "core": random.choice(cores),
                "addr": random.choice(addrs),
                "ip": random.choice(ips),
                "code": random.choice(codes),
                "file": random.choice(files),
                "line": random.choice(lines),
                "ss": random.choice(ss),
                "esp": random.choice(esps),
                "num": random.choice(nums),
                "reason": random.choice(reasons),
                "msg": random.choice(msgs),
                "func": random.choice(funcs),
                "name": random.choice(names),
                "value": random.choice(values),
                "slot": random.choice(slots),
                "pattern": random.choice(patterns),
                "cell": random.choice(cells),
                "device": random.choice(devices),
                "module": random.choice(modules),
                "param": random.choice(params),
                "val": random.choice(param_values),
                "channel": random.choice(channels),
            }
            
            # Genera valori float casuali per template
            params_dict["tick"] = random.randint(0, 100000)
            params_dict["uptime"] = random.randint(0, 3600)
            params_dict["sim"] = round(random.uniform(0.5, 1.0), 3)
            params_dict["similarity"] = round(random.uniform(50, 100), 1)
            params_dict["temp"] = round(random.uniform(35, 85), 1)
            params_dict["pct"] = random.randint(10, 95)
            params_dict["limit"] = random.choice(["1024kb", "4096kb", "1GB", "64MB"])
            params_dict["hval"] = round(random.uniform(-1, 1), 3)
            params_dict["hmin"] = round(random.uniform(-0.5, 0), 3)
            params_dict["hmax"] = round(random.uniform(0, 0.5), 3)
            params_dict["mem"] = random.randint(100, 64000)
            params_dict["cpu"] = random.randint(0, 100)
            params_dict["pf"] = random.randint(0, 1000)
            params_dict["nf"] = random.randint(0, 100)
            params_dict["s"] = random.randint(0, 100)
            params_dict["dist"] = round(random.uniform(0.01, 10.0), 2)
            params_dict["seq"] = " ".join(random.choice(["01", "10", "00", "11"]) for _ in range(8))
            params_dict["input"] = " ".join(str(random.randint(-100, 100)) for _ in range(4))
            params_dict["sensor"] = random.choice(["0", "1", "2", "A", "B"])
            params_dict["desc"] = random.choice(["pulsing", "constant", "decaying", "oscillating", "random"])
            params_dict["version"] = f"0.{random.randint(10, 15)}.{random.randint(1, 5)}"
            params_dict["baud"] = random.choice([9600, 19200, 38400, 115200, 460800])
            params_dict["port"] = f"0x{random.choice([0x3F8, 0x2F8, 0x3E8, 0x2E8]):04x}"
            params_dict["total"] = random.choice([4096, 8192, 16384, 32768])
            params_dict["freq"] = random.choice([100, 250, 500, 1000])
            params_dict["elapsed"] = random.randint(0, 5000)
            params_dict["thresh"] = round(random.uniform(0.1, 1.0), 2)
            params_dict["id"] = random.randint(0, 10)
            params_dict["name"] = random.choice(["idle", "error", "boot", "process", "fault", "recover"])
            params_dict["cells"] = random.choice(["Tatto+Chemio", "Metabol+Integrat", "all 4", "3 of 4"])
            params_dict["n"] = random.randint(1, 100)
            params_dict["us"] = random.randint(10, 5000)
            params_dict["thresh"] = random.randint(100, 10000)
            params_dict["delta"] = round(random.uniform(0, 10), 2)
            params_dict["id"] = random.randint(0, 100)
            params_dict["mse"] = round(random.uniform(0.001, 0.5), 4)
            params_dict["period"] = random.randint(180, 220)
            params_dict["expected"] = 200
            params_dict["steps"] = random.randint(1, 100)
            params_dict["key"] = random.choice(["debug", "trace", "log", "mode"])
            params_dict["command"] = random.choice(["ls", "ps", "cat", "echo", "test"])
            params_dict["test"] = random.choice(["memory", "serial", "tissue", "timer"])
            params_dict["target"] = random.choice(["tatto", "chemio", "metabol", "integrat", "all"])
            params_dict["event"] = random.choice(["PF", "GPF", "timer", "serial", "syscall"])
            params_dict["window"] = random.randint(100, 5000)
            params_dict["n"] = random.randint(1, 50)
            params_dict["measure"] = round(random.uniform(0.7, 1.5), 3)
            params_dict["lo"] = round(random.uniform(-0.5, -0.2), 2)
            params_dict["hi"] = round(random.uniform(0.2, 0.5), 2)
            params_dict["range"] = round(random.uniform(0.01, 0.5), 4)
            params_dict["ticks"] = random.randint(5, 500)
            params_dict["processed"] = random.randint(1, 32)
            params_dict["novel"] = random.randint(0, 10)
            params_dict["total"] = random.randint(2, 5)
            params_dict["type"] = random.randint(0, 15)
            params_dict["n"] = random.randint(10, 100)
            params_dict["event"] = random.choice(["PF", "GPF", "IRQ", "SYS", "TIMER"])
            params_dict["log_level"] = random.choice(["info", "warn", "error", "debug", "trace"])
            params_dict["slot"] = random.randint(1, 10)
            params_dict["value"] = round(random.uniform(-1, 1), 3)
            params_dict["size"] = f"{random.randint(1, 4096)}kb"
            
            # Applica template
            try:
                text = template.format(**params_dict)
            except KeyError:
                continue  # se manca un parametro, skippa
            
            # Genera vettore target con jitter
            vec = jitter(cat_info["base"], cat_info["varia"])
            
            # Clamp tra -1 e 1
            vec = [max(-1.0, min(1.0, v)) for v in vec]
            
            dataset.append({
                "text": text,
                "category": cat,
                "chemio": {
                    "contesto": vec[0],
                    "urgenza": vec[1],
                    "polarità": vec[2],
                    "novità": vec[3],
                },
                "target": vec,
            })
    
    # Mescola
    random.shuffle(dataset)
    return dataset


def export_dataset(dataset: List[dict], path: str):
    """Esporta in formato JSON con train/val split."""
    # Split 80/20
    split = int(len(dataset) * 0.8)
    random.shuffle(dataset)  # mescola prima dello split
    train = dataset[:split]
    val = dataset[split:]
    
    output = {
        "meta": {
            "description": "Dataset di training per ExoChemio encoder",
            "version": "1.0",
            "date": "2026-07-31",
            "chemio_axes": {
                "contesto": "cosa sta succedendo (-1=errore, 0=neutro, 1=vitale)",
                "urgenza": "quanto è importante ora (0=normale, 1=critico)",
                "polarità": "positivo/negativo (-1=neg, 0=neutro, 1=pos)",
                "novità": "già visto/mai visto (0=familiare, 1=mai visto)",
            },
            "categories": {k: v["description"] for k, v in CATEGORIES.items()},
            "total": len(dataset),
            "train": len(train),
            "val": len(val),
        },
        "categories": {k: v["description"] for k, v in CATEGORIES.items()},
        "train": train,
        "val": val,
    }
    
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        json.dump(output, f, indent=2, ensure_ascii=False)
    
    # Stats
    print(f"Dataset generato: {len(train)} train + {len(val)} val = {len(dataset)} totale")
    print(f"Salvato in: {path}")
    
    # Per categoria
    from collections import Counter
    cats = Counter(d["category"] for d in dataset)
    for cat, count in sorted(cats.items()):
        print(f"  {cat}: {count}")


if __name__ == "__main__":
    dataset = generate_dataset(500)
    export_dataset(dataset, os.path.join(os.path.dirname(__file__), "exo_chemio_dataset.json"))
