#!/usr/bin/env python3
"""
ExoChemio Encoder Training
==========================
Fine-tune mmBERT-base con LoRA per produrre i 4 valori di attivazione di Chemio.

Architettura:
  mmBERT-base (encoder, LoRA r=16) → hidden[CLS] (768) → Dense(768→256, ReLU, Dropout)
  → Dense(256→4) → tanh (output in [-1, 1])

Training:
  - LoRA sul backbone (r=16, alpha=32) — ~2M parametri addestrabili invece di 305M
  - Testa di regressione full
  - SmoothL1Loss
  - LR: 1e-4 (testa) / 5e-5 (LoRA)
  - 20 epoche, batch effettivo 32, grad accum

Output:
  models/exo-chemio-encoder-v1/    — modello + tokenizer
  experiments/encoder_v1/          — metriche + log
"""

import json
import os
import sys
import time
import random
from pathlib import Path

import numpy as np
import torch
import torch.nn as nn
from torch.utils.data import Dataset, DataLoader
from transformers import (
    AutoModel,
    AutoTokenizer,
    get_linear_schedule_with_warmup,
)
from peft import LoraConfig, get_peft_model, TaskType
from torch.optim import AdamW

# ── Paths ──────────────────────────────────────────────────────────────
ROOT = Path(__file__).resolve().parents[2]
DATASET_PATH = Path(__file__).parent / "exo_chemio_dataset.json"
MODEL_NAME = "jhu-clsp/mmBERT-base"
SAVE_DIR = ROOT / "models" / "exo-chemio-encoder-v1"
RUN_DIR = ROOT / "experiments" / "encoder_v1"
os.makedirs(SAVE_DIR, exist_ok=True)
os.makedirs(RUN_DIR, exist_ok=True)

# ── Hyperparameters ────────────────────────────────────────────────────
MAX_LEN = 128
BATCH = 4               # batch piccolo per CPU
GRAD_ACCUM = 8          # batch effettivo = 32
EPOCHS = 20
LR_HEAD = 1e-4          # learning rate testa di regressione
LR_LORA = 5e-5          # learning rate parametri LoRA
WARMUP_RATIO = 0.1
WEIGHT_DECAY = 0.01
DROPOUT = 0.2
HIDDEN = 256
CLIP_GRAD = 1.0

# LoRA
LORA_R = 16
LORA_ALPHA = 32
LORA_DROPOUT = 0.1

# ── Device ──────────────────────────────────────────────────────────────
device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
print(f"Device: {device} | {torch.cuda.get_device_name(0) if torch.cuda.is_available() else 'CPU'}")
if torch.cuda.is_available():
    torch.cuda.empty_cache()


# ── Modello con LoRA + testa di regressione ──────────────────────────
class ExoChemioEncoder(nn.Module):
    """mmBERT-base con LoRA + testa di regressione per i 4 valori Chemio."""

    def __init__(self, model_name: str, hidden: int = HIDDEN, dropout: float = DROPOUT):
        super().__init__()
        # Carica backbone
        backbone = AutoModel.from_pretrained(model_name)

        # Applica LoRA
        lora_config = LoraConfig(
            task_type=TaskType.FEATURE_EXTRACTION,
            r=LORA_R,
            lora_alpha=LORA_ALPHA,
            lora_dropout=LORA_DROPOUT,
            target_modules=["attn.Wqkv", "attn.Wo", "mlp.Wi", "mlp.Wo"],
            bias="none",
        )
        self.backbone = get_peft_model(backbone, lora_config)
        self.backbone.print_trainable_parameters()

        # Testa di regressione
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


# ── Dataset ────────────────────────────────────────────────────────────
class ExoChemioDataset(Dataset):
    def __init__(self, data: list, tokenizer, max_len: int = MAX_LEN, augment: bool = True):
        self.texts = [d["text"] for d in data]
        self.targets = [d["target"] for d in data]
        self.tokenizer = tokenizer
        self.max_len = max_len
        self.augment = augment

    def __len__(self):
        return len(self.texts)

    def __getitem__(self, idx):
        text = self.texts[idx]
        target = torch.tensor(self.targets[idx], dtype=torch.float32)

        # Data augmentation testuale (solo train)
        if self.augment:
            r = random.random()
            if r < 0.3:
                text = text.lower()
            elif r < 0.1:
                text = text.upper()
            # Aggiungi rumore (errori di battitura simulati)
            if random.random() < 0.05 and len(text) > 3:
                pos = random.randint(0, len(text) - 1)
                chars = list(text)
                chars[pos] = random.choice("abcdefghijklmnopqrstuvwxyz")
                text = "".join(chars)

        enc = self.tokenizer(
            text,
            truncation=True,
            padding="max_length",
            max_length=self.max_len,
            return_tensors="pt",
        )
        return {
            "input_ids": enc["input_ids"].squeeze(0),
            "attention_mask": enc["attention_mask"].squeeze(0),
            "target": target,
        }


# ── Metriche ───────────────────────────────────────────────────────────
def compute_metrics(preds: np.ndarray, targets: np.ndarray) -> dict:
    mse = ((preds - targets) ** 2).mean()
    mae = np.abs(preds - targets).mean()
    ss_res = ((preds - targets) ** 2).sum()
    ss_tot = ((targets - targets.mean(axis=0)) ** 2).sum()
    r2 = 1 - ss_res / (ss_tot + 1e-10)
    acc_per_axis = [(np.abs(preds[:, i] - targets[:, i]) < 0.25).mean() for i in range(4)]
    
    # Correlazione di Pearson per asse
    corrs = []
    for i in range(4):
        p, t = preds[:, i], targets[:, i]
        if np.std(p) > 1e-6 and np.std(t) > 1e-6:
            c = np.corrcoef(p, t)[0, 1]
        else:
            c = 0.0
        corrs.append(float(c))
    
    return {
        "mse": float(mse),
        "mae": float(mae),
        "r2": float(r2),
        "acc_contesto": float(acc_per_axis[0]),
        "acc_urgenza": float(acc_per_axis[1]),
        "acc_polarità": float(acc_per_axis[2]),
        "acc_novità": float(acc_per_axis[3]),
        "corr_contesto": corrs[0],
        "corr_urgenza": corrs[1],
        "corr_polarità": corrs[2],
        "corr_novità": corrs[3],
    }


# ── Training ──────────────────────────────────────────────────────────
def train_epoch(model, loader, optimizer, scheduler, scaler, epoch_n, total_epochs):
    model.train()
    total_loss = 0
    all_preds, all_targets = [], []
    optimizer.zero_grad()
    n_accum = 0

    for step, batch in enumerate(loader):
        input_ids = batch["input_ids"].to(device)
        attention_mask = batch["attention_mask"].to(device)
        targets = batch["target"].to(device)

        use_amp = device.type == "cuda"
        with torch.amp.autocast(device_type=device.type, enabled=use_amp):
            preds = model(input_ids, attention_mask)
            loss = nn.functional.smooth_l1_loss(preds, targets, beta=0.5)

        loss = loss / GRAD_ACCUM
        if use_amp:
            scaler.scale(loss).backward()
        else:
            loss.backward()

        n_accum += 1

        if n_accum >= GRAD_ACCUM:
            if use_amp:
                scaler.unscale_(optimizer)
            nn.utils.clip_grad_norm_(model.parameters(), CLIP_GRAD)
            if use_amp:
                scaler.step(optimizer)
                scaler.update()
            else:
                optimizer.step()
            scheduler.step()
            optimizer.zero_grad()
            n_accum = 0

        total_loss += loss.item() * GRAD_ACCUM
        all_preds.append(preds.detach().cpu().numpy())
        all_targets.append(targets.cpu().numpy())

        steps_done = step // GRAD_ACCUM + 1
        if steps_done % 5 == 0 and n_accum == 0:
            avg = total_loss / (step + 1)
            lr = scheduler.get_last_lr()[0]
            print(f"  [{epoch_n+1}/{total_epochs}] step {step+1}/{len(loader)} "
                  f"loss={avg:.4f} lr={lr:.2e}", flush=True)

    preds = np.concatenate(all_preds, axis=0)
    targets = np.concatenate(all_targets, axis=0)
    metrics = compute_metrics(preds, targets)
    metrics["loss"] = total_loss / len(loader)
    return metrics


@torch.no_grad()
def evaluate(model, loader):
    model.eval()
    all_preds, all_targets = [], []
    total_loss = 0

    for batch in loader:
        input_ids = batch["input_ids"].to(device)
        attention_mask = batch["attention_mask"].to(device)
        targets = batch["target"].to(device)

        with torch.amp.autocast(device_type=device.type, enabled=device.type == "cuda"):
            preds = model(input_ids, attention_mask)
            loss = nn.functional.smooth_l1_loss(preds, targets, beta=0.5)

        total_loss += loss.item()
        all_preds.append(preds.cpu().numpy())
        all_targets.append(targets.cpu().numpy())

    preds = np.concatenate(all_preds, axis=0)
    targets = np.concatenate(all_targets, axis=0)
    metrics = compute_metrics(preds, targets)
    metrics["loss"] = total_loss / len(loader)
    return metrics


# ── Inference test ────────────────────────────────────────────────────
def inference_test(model, tokenizer, texts):
    model.eval()
    print(f"\n{'='*60}")
    print("Test di inferenza:")
    print(f"{'='*60}")
    for text in texts:
        enc = tokenizer(text, return_tensors="pt", truncation=True,
                        padding="max_length", max_length=MAX_LEN)
        with torch.no_grad():
            pred = model(enc["input_ids"].to(device), enc["attention_mask"].to(device))
        vals = pred[0].cpu().numpy().tolist()
        ctx = ["ERR", "NEU", "VIT"][int(np.clip(round(vals[0] + 1), 0, 2))]
        urg = f"{vals[1]:.2f}"
        pol = ["NEG", "NEU", "POS"][int(np.clip(round(vals[2] + 1), 0, 2))]
        nov = f"{vals[3]:.2f}"
        print(f"  [{ctx} | U:{urg} | {pol} | N:{nov}]  '{text[:60]}'")


# ── Main ───────────────────────────────────────────────────────────────
def main():
    print(f"\n{'='*60}")
    print(f"  ExoChemio Encoder Training v1")
    print(f"  mmBERT-base + LoRA (r={LORA_R}) + regressione 768→{HIDDEN}→4")
    print(f"{'='*60}")

    # 1. Carica dataset
    with open(DATASET_PATH) as f:
        dataset = json.load(f)

    train_data = dataset["train"]
    val_data = dataset["val"]
    print(f"\nDataset: {len(train_data)} train + {len(val_data)} val = "
          f"{len(train_data) + len(val_data)} totale")
    print(f"Categorie: {json.dumps(dataset['meta']['categories'], indent=2)}")

    # 2. Tokenizer
    print(f"\nCarico tokenizer {MODEL_NAME}...")
    tokenizer = AutoTokenizer.from_pretrained(MODEL_NAME)

    # 3. Dataset + DataLoader
    train_ds = ExoChemioDataset(train_data, tokenizer, augment=True)
    val_ds = ExoChemioDataset(val_data, tokenizer, augment=False)

    train_loader = DataLoader(train_ds, batch_size=BATCH, shuffle=True, num_workers=0)
    val_loader = DataLoader(val_ds, batch_size=BATCH, shuffle=False, num_workers=0)

    # 4. Modello
    print(f"\nCostruisco modello con LoRA...")
    model = ExoChemioEncoder(MODEL_NAME)
    model.to(device)

    # 5. Ottimizzatore con gruppi di LR differenziati
    # Dividi parametri: LoRA vs testa
    lora_params = [p for n, p in model.backbone.named_parameters() if p.requires_grad]
    head_params = list(model.regressor.parameters())

    optimizer = AdamW([
        {"params": lora_params, "lr": LR_LORA, "weight_decay": WEIGHT_DECAY},
        {"params": head_params, "lr": LR_HEAD, "weight_decay": WEIGHT_DECAY},
    ])

    total_steps = len(train_loader) * EPOCHS // GRAD_ACCUM
    warmup_steps = int(total_steps * WARMUP_RATIO)
    scheduler = get_linear_schedule_with_warmup(
        optimizer, num_warmup_steps=warmup_steps, num_training_steps=total_steps
    )

    scaler = torch.amp.GradScaler(device=device.type, enabled=(device.type == "cuda"))

    print(f"\nConfigurazione:")
    print(f"  Batch effettivo: {BATCH * GRAD_ACCUM}")
    print(f"  Epoche: {EPOCHS}")
    print(f"  Total steps optimizer: {total_steps}")
    print(f"  Warmup: {warmup_steps} steps")
    print(f"  LR LoRA: {LR_LORA} | LR testa: {LR_HEAD}")
    print(f"  Parametri LoRA addestrabili: {sum(p.numel() for p in lora_params)}")
    print(f"  Parametri testa: {sum(p.numel() for p in head_params)}")

    # 6. Training loop
    print(f"\n{'='*60}")
    print("  Training...")
    print(f"{'='*60}\n")

    best_val_loss = float("inf")
    best_epoch = -1
    history = []

    for epoch in range(EPOCHS):
        t0 = time.time()
        train_metrics = train_epoch(model, train_loader, optimizer, scheduler, scaler,
                                    epoch, EPOCHS)
        val_metrics = evaluate(model, val_loader)
        elapsed = time.time() - t0

        entry = {
            "epoch": epoch + 1,
            "train": train_metrics,
            "val": val_metrics,
            "time_s": round(elapsed),
        }
        history.append(entry)

        # Log
        print(f"\n  Epoca {epoch+1}/{EPOCHS} ({elapsed:.0f}s):")
        print(f"    Train: loss={train_metrics['loss']:.4f} "
              f"MSE={train_metrics['mse']:.4f} R²={train_metrics['r2']:.3f}")
        print(f"    Val:   loss={val_metrics['loss']:.4f} "
              f"MSE={val_metrics['mse']:.4f} R²={val_metrics['r2']:.3f}")
        print(f"    Acc:   ctx={val_metrics['acc_contesto']:.3f} "
              f"urg={val_metrics['acc_urgenza']:.3f} "
              f"pol={val_metrics['acc_polarità']:.3f} "
              f"nov={val_metrics['acc_novità']:.3f}")
        print(f"    Corr:  ctx={val_metrics['corr_contesto']:.3f} "
              f"urg={val_metrics['corr_urgenza']:.3f} "
              f"pol={val_metrics['corr_polarità']:.3f} "
              f"nov={val_metrics['corr_novità']:.3f}")

        # Salva best model
        if val_metrics["loss"] < best_val_loss:
            best_val_loss = val_metrics["loss"]
            best_epoch = epoch + 1
            # Salva stato completo del modello
            torch.save({
                "model_state_dict": model.state_dict(),
                "optimizer_state_dict": optimizer.state_dict(),
                "epoch": epoch,
                "val_loss": best_val_loss,
            }, SAVE_DIR / "best_checkpoint.pt")
            print(f"    → BEST model (loss={best_val_loss:.4f})")

    # 7. Salva modello finale
    print(f"\n{'='*60}")
    print("  Salvataggio modello...")
    print(f"{'='*60}")

    tokenizer.save_pretrained(SAVE_DIR)
    
    # Salva il modello completo (backbone LoRA + testa)
    torch.save(model.state_dict(), SAVE_DIR / "model.pt")
    
    # Salva solo la testa (per ricarica su backbone fresco)
    torch.save(model.regressor.state_dict(), SAVE_DIR / "regressor_head.pt")
    
    # Salva la configurazione del modello
    config = {
        "model_name": MODEL_NAME,
        "lora_r": LORA_R,
        "lora_alpha": LORA_ALPHA,
        "hidden": HIDDEN,
        "dropout": DROPOUT,
        "max_len": MAX_LEN,
        "chemio_axes": {
            "0": "contesto: -1=errore, 0=neutro, 1=vitale",
            "1": "urgenza: 0=normale, 1=critico",
            "2": "polarità: -1=negativo, 0=neutro, 1=positivo",
            "3": "novità: 0=familiare, 1=mai visto",
        },
        "best_val_loss": best_val_loss,
        "best_epoch": best_epoch,
    }
    with open(SAVE_DIR / "config.json", "w") as f:
        json.dump(config, f, indent=2, ensure_ascii=False)

    # 8. Metriche finali
    final_results = {
        "model": "ExoChemio Encoder v1",
        "base": MODEL_NAME,
        "train_samples": len(train_data),
        "val_samples": len(val_data),
        "epochs": EPOCHS,
        "batch_effective": BATCH * GRAD_ACCUM,
        "lora_r": LORA_R,
        "best_val_loss": best_val_loss,
        "best_epoch": best_epoch,
        "history": history,
        "final_val": history[-1]["val"] if history else None,
    }
    with open(RUN_DIR / "metrics.json", "w") as f:
        json.dump(final_results, f, indent=2, ensure_ascii=False)

    print(f"\nModello salvato in: {SAVE_DIR}")
    print(f"Metriche salvate in: {RUN_DIR / 'metrics.json'}")
    print(f"Miglior epoca: {best_epoch} (val_loss={best_val_loss:.4f})")

    # 9. Inference test
    test_texts = [
        "ERR: page fault at 0xDEADBEEF",
        "heartbeat OK — tick=1234, uptime=3600s",
        "STATUS",
        "SLEEP:BEGIN — daydreaming",
        "Nova Exo v0.13 starting...",
        "WARN: memory at 95% of 1024kb",
        "A: sim=0.87 @ tick 5000 — pattern matched",
        "NEW pattern detected at tick 100",
        "",
    ]
    inference_test(model, tokenizer, test_texts)

    print(f"\n{'='*60}")
    print("  TRAINING COMPLETATO.")
    print(f"{'='*60}")


if __name__ == "__main__":
    main()
