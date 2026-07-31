// ── Interpreter (Interprete dello stato CFC) — Cognitive Upgrade ──
// Legge lo stato del corpo (64 valori CFC) e ne estrae il significato:
//   - 4 valori chemio interpretati (c=contesto, u=urgenza, p=polarità, n=novità)
//   - 1 concetto corrente (0=errore, 1=vita, 2=riposo, 3=novità)
//
// Addestrato fuori kernel (tools/train_interpreter.py):
//   R²=0.995 per chemio, acc=0.999 per concetto (lineare con bias).
// Pesi quantizzati i16×1000 in interpreter_weights.rs, layout [output][64|bias].
//
// Design: rizzo-pii adattato → non legge più testo, legge il corpo.
// È la corteccia che dà senso ai segnali periferici (CFC).
// Downgrade: Sempre, 31 Luglio 2026

use crate::interpreter_weights::{W_CHEMIO_Q, W_CONCEPT_Q};

/// Ingressi: 64 stati + 1 bias
pub const INTERP_IN: usize = 65;
pub const CONCEPTS: [&str; 4] = ["errore", "vita", "riposo", "novita"];

pub struct InterpretReport {
    /// Valori chemio interpretati (c, u, p, n) in [-1, 1]
    pub chemio: [f32; 4],
    /// Concetto corrente (argmax su 4 classi)
    pub concept: u8,
    /// Energia di attivazione (max score)
    pub energy: f32,
}

pub struct Interpreter {
    weights: [[i16; INTERP_IN]; 4],
    concept_weights: [[i16; INTERP_IN]; 4],
    learning_rate: f32,
    /// Traccia la qualità dell'interpretazione (errore medio)
    last_error: f32,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            weights: W_CHEMIO_Q,
            concept_weights: W_CONCEPT_Q,
            learning_rate: 0.001,
            last_error: 0.0,
        }
    }

    /// Interpreta lo stato CFC (64 f32) → chemio + concetto.
    /// state: stato f32 delle 4 cellule concatenate (T,C,M,I × 16).
    pub fn interpret(&self, state: &[f32; 64]) -> InterpretReport {
        // Chemio: 4 uscite, somma pesata con bias
        let mut chemio = [0.0f32; 4];
        for o in 0..4 {
            let mut sum = 0i64;
            for i in 0..64 {
                // state in [-1,1], pesi i16×1000
                let si = (state[i] * 1000.0) as i32;
                sum += self.weights[o][i] as i64 * si as i64;
            }
            // Bias (indice 64)
            sum += self.weights[o][64] as i64 * 1000i64;
            chemio[o] = (sum as f32 / 1_000_000.0).clamp(-1.0, 1.0);
        }

        // Concetto: argmax su 4 uscite
        let mut concept_scores = [0i64; 4];
        for o in 0..4 {
            let mut sum = 0i64;
            for i in 0..64 {
                let si = (state[i] * 1000.0) as i32;
                sum += self.concept_weights[o][i] as i64 * si as i64;
            }
            sum += self.concept_weights[o][64] as i64 * 1000i64;
            concept_scores[o] = sum;
        }
        let mut concept = 0u8;
        let mut max_score = concept_scores[0];
        for o in 1..4 {
            if concept_scores[o] > max_score {
                max_score = concept_scores[o];
                concept = o as u8;
            }
        }
        let energy = (max_score as f32 / 1_000_000.0).clamp(0.0, 1.0);

        InterpretReport { chemio, concept, energy }
    }

    /// Apprendimento online: dato lo stato CFC e il chemio osservato,
    /// aggiorna i pesi con delta rule (come PFM).
    pub fn learn(&mut self, state: &[f32; 64], observed: &[f32; 4]) -> f32 {
        let pred = self.interpret(state);
        let mut err_sum = 0.0f32;
        for o in 0..4 {
            let delta = pred.chemio[o] - observed[o];
            err_sum += delta * delta;
            for i in 0..64 {
                let w = self.weights[o][i] as f32 / 1000.0;
                let new_w = w - self.learning_rate * delta * state[i];
                self.weights[o][i] = (new_w * 1000.0) as i16;
            }
        }
        let err = err_sum / 4.0;
        self.last_error = err;
        err
    }

    pub fn last_error(&self) -> f32 {
        self.last_error
    }
}
