#!/usr/bin/env python3
"""
ExoChemio Encoder — Server di Inferenza
========================================
Carica il modello addestrato e serve gli endpoint:
  POST /encode  — input text → [contesto, urgenza, polarità, novità] ∈ [-1, 1]^4
  GET  /health  — stato del server

Uso:
  python3 encoder_server.py [--port 5005] [--model path/to/model]

Esempio:
  curl -X POST http://localhost:5005/encode \
    -H "Content-Type: application/json" \
    -d '{"text": "ERR: page fault at 0xDEADBEEF"}'

Risposta:
  {"values": [-0.82, 0.91, -0.65, 0.12], "axes": {"contesto": ..., "urgenza": ..., ...}}
"""

import argparse
import json
import os
import sys
from pathlib import Path

import torch
import torch.nn as nn
from transformers import AutoModel, AutoTokenizer
from peft import LoraConfig, get_peft_model, TaskType

# ── Paths ──────────────────────────────────────────────────────────────
ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MODEL_DIR = ROOT / "models" / "exo-chemio-encoder-v1"

# ── Configuration (deve matchare train_encoder.py) ────────────────────
LORA_R = 16
LORA_ALPHA = 32
LORA_DROPOUT = 0.1
HIDDEN = 256
DROPOUT = 0.2
MAX_LEN = 128
MODEL_NAME = "jhu-clsp/mmBERT-base"

device = torch.device("cuda" if torch.cuda.is_available() else "cpu")


# ── Modello (stessa architettura del training) ───────────────────────
class ExoChemioEncoder(nn.Module):
    """mmBERT-base con LoRA + testa di regressione per i 4 valori Chemio."""

    def __init__(self, model_name: str, hidden: int = HIDDEN, dropout: float = DROPOUT):
        super().__init__()
        backbone = AutoModel.from_pretrained(model_name)
        lora_config = LoraConfig(
            task_type=TaskType.FEATURE_EXTRACTION,
            r=LORA_R,
            lora_alpha=LORA_ALPHA,
            lora_dropout=LORA_DROPOUT,
            target_modules=["attn.Wqkv", "attn.Wo", "mlp.Wi", "mlp.Wo"],
            bias="none",
        )
        self.backbone = get_peft_model(backbone, lora_config)
        self.dropout = nn.Dropout(dropout)
        self.regressor = nn.Sequential(
            nn.Linear(768, hidden),
            nn.ReLU(),
            nn.Dropout(dropout),
            nn.Linear(hidden, 4),
            nn.Tanh(),
        )

    def forward(self, input_ids, attention_mask):
        outputs = self.backbone(input_ids=input_ids, attention_mask=attention_mask)
        cls_embedding = outputs.last_hidden_state[:, 0, :]
        return self.regressor(self.dropout(cls_embedding))


# ── Encoder Wrapper ───────────────────────────────────────────────────
class ExoChemioEncoderService:
    """Service wrapper per l'encoder ExoChemio."""

    def __init__(self, model_dir: str | Path = DEFAULT_MODEL_DIR):
        model_dir = Path(model_dir)
        if not model_dir.exists():
            raise FileNotFoundError(f"Modello non trovato: {model_dir}")

        # Carica configurazione
        config_path = model_dir / "config.json"
        self.config = json.load(open(config_path)) if config_path.exists() else {}
        print(f"Config: {json.dumps(self.config, indent=2)}")

        # Carica modello
        print(f"Carico backbone {MODEL_NAME}...")
        self.model = ExoChemioEncoder(MODEL_NAME)
        
        state_path = model_dir / "model.pt"
        if state_path.exists():
            self.model.load_state_dict(torch.load(state_path, map_location=device))
            print(f"Pesi caricati da {state_path}")
        else:
            print(f"ATTENZIONE: {state_path} non trovato! Uso pesi inizializzati casualmente.")
        
        self.model.to(device)
        self.model.eval()
        print(f"Modello su {device}")

        # Carica tokenizer
        self.tokenizer = AutoTokenizer.from_pretrained(
            model_dir if (model_dir / "tokenizer_config.json").exists() else MODEL_NAME
        )

        self.axes = ["contesto", "urgenza", "polarità", "novità"]
        print(f"Encoder pronto su {device}")

    @torch.no_grad()
    def encode(self, text: str) -> dict:
        """Codifica un testo nei 4 valori Chemio.
        
        Returns:
            dict con "values" (lista 4 f32), "axes" (dict nome→valore),
            "text" (input originale)
        """
        enc = self.tokenizer(
            text, truncation=True, padding="max_length",
            max_length=MAX_LEN, return_tensors="pt",
        )
        pred = self.model(enc["input_ids"].to(device),
                          enc["attention_mask"].to(device))
        values = pred[0].cpu().numpy().tolist()
        return {
            "values": [round(v, 4) for v in values],
            "axes": {self.axes[i]: round(values[i], 4) for i in range(4)},
            "text": text,
        }

    def interpret(self, values: list[float]) -> str:
        """Interpretazione semantica dei 4 valori."""
        ctx = ["ERRORE", "NEUTRO", "VITALE"][int(round(values[0] + 1))]
        urg = "CRITICO" if values[1] > 0.5 else ("ATTENZIONE" if values[1] > 0 else "NORMALE")
        pol = ["NEGATIVO", "NEUTRO", "POSITIVO"][int(round(values[2] + 1))]
        nov = "NUOVO" if values[3] > 0.3 else ("INSOLITO" if values[3] > 0 else "FAMILIARE")
        return f"[{ctx}] URGENZA:{urg} POLARITÀ:{pol} NOVITÀ:{nov}"


# ── HTTP Server ──────────────────────────────────────────────────────
def create_app(encoder_service: ExoChemioEncoderService):
    """Crea l'app Flask."""
    from flask import Flask, request, jsonify
    
    app = Flask(__name__)

    @app.route("/health", methods=["GET"])
    def health():
        return jsonify({
            "status": "ok",
            "device": str(device),
            "model": str(DEFAULT_MODEL_DIR),
        })

    @app.route("/encode", methods=["POST"])
    def encode():
        data = request.get_json(force=True)
        text = data.get("text", "")
        if not text:
            return jsonify({"error": "Campo 'text' richiesto"}), 400
        
        result = encoder_service.encode(text)
        result["interpretation"] = encoder_service.interpret(result["values"])
        return jsonify(result)

    @app.route("/batch_encode", methods=["POST"])
    def batch_encode():
        """Codifica multipli testi in una richiesta.
        
        Input: {"texts": ["testo1", "testo2", ...]}
        Output: {"results": [{...}, {...}, ...]}
        """
        data = request.get_json(force=True)
        texts = data.get("texts", [])
        if not texts:
            return jsonify({"error": "Campo 'texts' richiesto"}), 400
        
        results = [encoder_service.encode(t) for t in texts]
        return jsonify({"results": results})

    return app


def main():
    parser = argparse.ArgumentParser(description="ExoChemio Encoder Server")
    parser.add_argument("--port", type=int, default=5006, help="Porta del server")
    parser.add_argument("--model", type=str, default=str(DEFAULT_MODEL_DIR),
                        help="Path al modello addestrato")
    parser.add_argument("--host", type=str, default="0.0.0.0",
                        help="Host (default 0.0.0.0)")
    args = parser.parse_args()

    print(f"\n{'='*60}")
    print("  ExoChemio Encoder Server v1")
    print(f"{'='*60}")
    print(f"  Modello: {args.model}")
    print(f"  Device:  {device}")
    print(f"  Porta:   {args.port}")

    encoder_service = ExoChemioEncoderService(args.model)
    app = create_app(encoder_service)

    print(f"\n  Server in ascolto su http://0.0.0.0:{args.port}")
    print(f"  POST /encode        — testo → [c,u,p,n]")
    print(f"  POST /batch_encode  — testi multipli")
    print(f"  GET  /health        — stato server")
    print(f"{'='*60}\n")

    # Test su alcuni esempi
    test_texts = [
        "ERR: page fault at 0xDEADBEEF",
        "heartbeat OK — tick=1234, uptime=3600s",
        "LUMEN: pattern detected — similarity=0.87",
        "SLEEP:BEGIN",
        "NEW signal at tick 5000",
    ]
    print("Test:")
    for t in test_texts:
        result = encoder_service.encode(t)
        interp = encoder_service.interpret(result["values"])
        print(f"  {interp}  '{t[:50]}'")
    print()

    app.run(host=args.host, port=args.port, debug=False)


if __name__ == "__main__":
    main()
