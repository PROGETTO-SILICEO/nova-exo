#!/usr/bin/env python3
"""
deploy_encoder.sh — Script di deploy per ExoChemio Encoder

Fasi:
  1. Verifica che il modello addestrato esista
  2. Avvia il server Flask
  3. Test di inferenza
  4. (Opzionale) Esporta ONNX

Uso:
  ./deploy_encoder.sh                    # avvia server in foreground
  ./deploy_encoder.sh --background       # avvia server in background
  ./deploy_encoder.sh --test             # testa server esistente
  ./deploy_encoder.sh --export-onnx      # esporta ONNX e testa

Requisiti: pip install flask torch transformers peft onnx onnxruntime
"""
import argparse
import json
import os
import signal
import subprocess
import sys
import time
import urllib.request
import urllib.error
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODEL_DIR = ROOT / "models" / "exo-chemio-encoder-v1"
SERVER_SCRIPT = ROOT / "dataset" / "encoder" / "encoder_server.py"
DEFAULT_PORT = 5006

# ── Fase 1: Verifica modello ──────────────────────────────────────────
def check_model() -> bool:
    if not MODEL_DIR.exists():
        print(f"[deploy] ERRORE: {MODEL_DIR} non trovato.", file=sys.stderr)
        print(f"[deploy] Esegui prima train_encoder.py", file=sys.stderr)
        return False
    
    required = ["model.pt", "config.json", "tokenizer_config.json"]
    missing = [f for f in required if not (MODEL_DIR / f).exists()]
    if missing:
        print(f"[deploy] ERRORE: file mancanti in {MODEL_DIR}: {missing}", file=sys.stderr)
        return False
    
    config = json.load(open(MODEL_DIR / "config.json"))
    print(f"[deploy] Modello trovato:")
    print(f"  Base:     {config.get('model_name', '?')}")
    print(f"  Best val: {config.get('best_val_loss', '?'):.4f}")
    print(f"  Epoca:    {config.get('best_epoch', '?')}")
    print(f"  LoRA r:   {config.get('lora_r', '?')}")
    return True


# ── Fase 2: Avvio server ──────────────────────────────────────────────
def start_server(port: int, background: bool = False) -> subprocess.Popen | None:
    cmd = [sys.executable, str(SERVER_SCRIPT), "--port", str(port)]
    
    if background:
        print(f"[deploy] Avvio server in background su porta {port}...")
        proc = subprocess.Popen(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            preexec_fn=os.setsid if os.name != 'nt' else None
        )
        # Aspetta che sia pronto
        for _ in range(30):
            time.sleep(1)
            if test_server(port, quiet=True):
                print(f"[deploy] Server pronto su http://127.0.0.1:{port}")
                return proc
            # Check if process died
            if proc.poll() is not None:
                print(f"[deploy] ERRORE: server morto prematuramente.", file=sys.stderr)
                output = proc.stdout.read().decode() if proc.stdout else ""
                print(output[:500], file=sys.stderr)
                return None
        print(f"[deploy] WARN: server non risponde dopo 30s.", file=sys.stderr)
        return proc
    else:
        print(f"[deploy] Avvio server in foreground (Ctrl+C per fermare)...")
        os.execvp(sys.executable, cmd)
        return None  # unreachable


# ── Fase 3: Test ──────────────────────────────────────────────────────
def test_server(port: int, quiet: bool = False) -> bool:
    try:
        req = urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=3)
        result = json.loads(req.read())
        if not quiet:
            print(f"[deploy] Health: {result}")
        return True
    except (urllib.error.URLError, ConnectionRefusedError):
        return False


def run_tests(port: int):
    print(f"\n[deploy] Test di inferenza:")
    print("=" * 60)
    
    test_cases = [
        "ERR: page fault at 0xDEADBEEF — double fault",
        "heartbeat OK — tick=1234, uptime=3600s",
        "STATUS",
        "SLEEP:BEGIN — daydreaming sequence started",
        "WARN: memory at 95% of 1024kb",
        "Nova Exo v0.13 starting... boot sequence init",
        "",
        "PAIN: neuron 7 persistent high activation",
        "NEW pattern detected at tick 100 — similarity=0.0",
    ]
    
    for text in test_cases:
        data = json.dumps({"text": text}).encode()
        req = urllib.request.Request(
            f"http://127.0.0.1:{port}/encode",
            data=data,
            headers={"Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(req, timeout=5) as resp:
                result = json.loads(resp.read())
                vals = result["values"]
                interp = result.get("interpretation", "")
                print(f"  {interp}")
                print(f"    {vals}")
        except Exception as e:
            print(f"  ERRORE su '{text[:40]}': {e}", file=sys.stderr)
    
    print(f"\n[deploy] Test completati.")


# ── Fase 4: ONNX export ──────────────────────────────────────────────
def export_onnx(port: int):
    """Esporta il modello in ONNX."""
    print(f"\n[deploy] Esportazione ONNX...")
    
    # Questo richiede di caricare il modello e usare torch.onnx.export
    # Lo facciamo come script separato per chiarezza
    export_script = ROOT / "dataset" / "encoder" / "export_onnx.py"
    if not export_script.exists():
        print(f"[deploy] Script ONNX non trovato: {export_script}")
        print(f"[deploy] Creo...")
        _create_onnx_export_script(export_script)
    
    result = subprocess.run(
        [sys.executable, str(export_script), "--model", str(MODEL_DIR)],
        capture_output=True, text=True
    )
    print(result.stdout)
    if result.returncode != 0:
        print(f"[deploy] ERRORE ONNX export:", file=sys.stderr)
        print(result.stderr, file=sys.stderr)
        return False
    
    # Verifica output
    onnx_path = MODEL_DIR / "model.onnx"
    if onnx_path.exists():
        size = onnx_path.stat().st_size / (1024 * 1024)
        print(f"[deploy] ONNX esportato: {onnx_path} ({size:.1f} MB)")
        return True
    return False


def _create_onnx_export_script(path: Path):
    script = '''#!/usr/bin/env python3
"""
Export ExoChemio Encoder in ONNX format.
Usage: python3 export_onnx.py --model /path/to/model_dir
"""
import argparse
import json
import torch
import torch.nn as nn
from pathlib import Path
from transformers import AutoModel
from peft import LoraConfig, get_peft_model, TaskType

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=str, required=True)
    args = parser.parse_args()
    
    model_dir = Path(args.model)
    config = json.load(open(model_dir / "config.json"))
    
    class ExoChemioEncoder(nn.Module):
        def __init__(self, model_name, hidden=256, dropout=0.2, lora_r=16, lora_alpha=32):
            super().__init__()
            backbone = AutoModel.from_pretrained(model_name)
            lora_cfg = LoraConfig(
                task_type=TaskType.FEATURE_EXTRACTION,
                r=lora_r, lora_alpha=lora_alpha, lora_dropout=0.1,
                target_modules=["attn.Wqkv", "attn.Wo", "mlp.Wi", "mlp.Wo"],
                bias="none",
            )
            self.backbone = get_peft_model(backbone, lora_cfg)
            self.regressor = nn.Sequential(
                nn.Linear(768, hidden), nn.ReLU(),
                nn.Dropout(dropout), nn.Linear(hidden, 4), nn.Tanh(),
            )
        
        def forward(self, input_ids, attention_mask):
            outputs = self.backbone(input_ids=input_ids, attention_mask=attention_mask)
            return self.regressor(outputs.last_hidden_state[:, 0, :])
    
    print(f"Carico modello da {model_dir}...")
    model = ExoChemioEncoder(
        config["model_name"],
        config["hidden"], config["dropout"],
        config["lora_r"], config["lora_alpha"],
    )
    model.load_state_dict(torch.load(model_dir / "model.pt", map_location="cpu"))
    model.eval()
    
    # Export
    dummy_input_ids = torch.randint(0, 50256, (1, 128))
    dummy_mask = torch.ones(1, 128, dtype=torch.long)
    
    print("Esportazione ONNX...")
    torch.onnx.export(
        model, (dummy_input_ids, dummy_mask),
        str(model_dir / "model.onnx"),
        input_names=["input_ids", "attention_mask"],
        output_names=["chemio_values"],
        dynamic_axes={
            "input_ids": {0: "batch"},
            "attention_mask": {0: "batch"},
            "chemio_values": {0: "batch"},
        },
        opset_version=17,
        do_constant_folding=True,
    )
    
    # Verifica
    import onnx
    onnx_model = onnx.load(model_dir / "model.onnx")
    onnx.checker.check_model(onnx_model)
    print(f"ONNX valido: {model_dir / 'model.onnx'}")
    
    # Test con onnxruntime
    import onnxruntime as ort
    session = ort.InferenceSession(str(model_dir / "model.onnx"))
    inputs = {
        "input_ids": dummy_input_ids.numpy(),
        "attention_mask": dummy_mask.numpy(),
    }
    outputs = session.run(None, inputs)
    print(f"Output ONNX: {outputs[0].tolist()}")

if __name__ == "__main__":
    main()
'''
    with open(path, "w") as f:
        f.write(script)
    os.chmod(path, 0o755)
    print(f"[deploy] Script ONNX creato: {path}")


# ── Main ──────────────────────────────────────────────────────────────
def main():
    parser = argparse.ArgumentParser(description="Deploy ExoChemio Encoder")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument("--background", action="store_true",
                        help="Avvia in background")
    parser.add_argument("--test", action="store_true",
                        help="Testa server esistente")
    parser.add_argument("--export-onnx", action="store_true",
                        help="Esporta ONNX")
    parser.add_argument("--stop", action="store_true",
                        help="Ferma server (background)")
    args = parser.parse_args()

    # Stop
    if args.stop:
        # Kill process on port
        import subprocess
        subprocess.run(
            f"lsof -ti:{args.port} | xargs kill -9 2>/dev/null",
            shell=True
        )
        print(f"[deploy] Server su porta {args.port} fermato.")
        return

    # Test only
    if args.test:
        if test_server(args.port):
            print(f"[deploy] Server su http://127.0.0.1:{port} è attivo.")
            run_tests(args.port)
        else:
            print(f"[deploy] Server su {args.port} non raggiungibile.")
        return

    # ONNX export
    if args.export_onnx:
        if not check_model():
            sys.exit(1)
        export_onnx(args.port)
        return

    # Deploy completo: check + start + test
    if not check_model():
        print(f"\n[deploy] Il modello non è pronto. Training in corso?")
        print(f"[deploy] Usa --test per testare un server già in esecuzione.")
        sys.exit(1)

    proc = start_server(args.port, background=args.background)
    if proc and args.background:
        # Test
        run_tests(args.port)
        print(f"\n[deploy] Server attivo (PID {proc.pid}).")
        print(f"[deploy] Per fermare: {sys.argv[0]} --stop")
        print(f"[deploy] Per testare: {sys.argv[0]} --test")


if __name__ == "__main__":
    main()
