#!/usr/bin/env python3
"""
export_onnx.py — Esporta ExoChemio Encoder in formato ONNX

Uso: python3 export_onnx.py --model /path/to/model_dir

Output: model_dir/model.onnx
"""

import argparse
import json
import torch
import torch.nn as nn
from pathlib import Path
from transformers import AutoModel
from peft import LoraConfig, get_peft_model, TaskType


class ExoChemioEncoder(nn.Module):
    """mmBERT-base con LoRA + testa regressione — matcha l'architettura di training."""

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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=str, required=True)
    args = parser.parse_args()

    model_dir = Path(args.model)
    config = json.load(open(model_dir / "config.json"))

    print(f"Carico modello da {model_dir}...")
    model = ExoChemioEncoder(
        config["model_name"],
        config["hidden"], config["dropout"],
        config["lora_r"], config["lora_alpha"],
    )
    model.load_state_dict(torch.load(model_dir / "model.pt", map_location="cpu"))
    model.eval()

    # Export ONNX
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
    try:
        import onnx
        onnx_model = onnx.load(model_dir / "model.onnx")
        onnx.checker.check_model(onnx_model)
        print(f"✅ ONNX valido: {model_dir / 'model.onnx'}")

        import onnxruntime as ort
        session = ort.InferenceSession(str(model_dir / "model.onnx"))
        inputs = {
            "input_ids": dummy_input_ids.numpy(),
            "attention_mask": dummy_mask.numpy(),
        }
        outputs = session.run(None, inputs)
        print(f"Output ONNX: {outputs[0].tolist()}")
        print(f"✅ ONNX Runtime inference OK")
    except ImportError:
        print(f"⚠️  onnx/onnxruntime non installati. Salto verifica.")
        print(f"   File ONNX: {model_dir / 'model.onnx'}")

    print(f"\nFatto! Modello ONNX in: {model_dir / 'model.onnx'}")


if __name__ == "__main__":
    main()
