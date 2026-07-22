// ── Predictive Forward Module (PFM) — Cognitive Upgrade ──
// Layer lineare appreso: 36 ingressi → 32 uscite
// Predice S(t+dt) da S(t) + I(t), apprende via delta rule
// L'errore MSE è il segnale di sorpresa — con buffer, trend,
// e memoria guidata: se l'errore sale persistentemente,
// force_store forza la memorizzazione dello stato corrente.
//
// Design teorico: TASK_predizione_esplicita.md (Nova, 20 Luglio 2026)
// Upgrade cognitivo: Sempre, 20 Luglio 2026

pub const PFM_IN: usize = 36;
pub const PFM_OUT: usize = 32;
const ERR_BUF: usize = 128;
const SHORT_WIN: usize = 16;
const LONG_WIN: usize = 64;
const ANOMALY_THRESH: f32 = 0.0005;
const ANOMALY_TRIGGER: u8 = 5;

#[derive(PartialEq)]
pub enum Trend {
    Stable,
    Rising,
    Falling,
}

pub struct PredictiveModule {
    weights: [[f32; PFM_IN]; PFM_OUT],
    biases: [f32; PFM_OUT],
    learning_rate: f32,
    last_error: f32,
    error_buf: [f32; ERR_BUF],
    error_idx: u8,
    error_count: u8,
    anomaly_ticks: u8,
}

impl PredictiveModule {
    pub fn new() -> Self {
        let mut w = [[0.0f32; PFM_IN]; PFM_OUT];
        let mut b = [0.0f32; PFM_OUT];
        let mut seed: u32 = 42;
        for i in 0..PFM_OUT {
            for j in 0..PFM_IN {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                w[i][j] = ((seed >> 16) as f32 / 65536.0 - 0.5) * 0.02;
            }
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            b[i] = ((seed >> 16) as f32 / 65536.0 - 0.5) * 0.01;
        }
        Self {
            weights: w, biases: b, learning_rate: 0.001, last_error: 0.0,
            error_buf: [0.0; ERR_BUF], error_idx: 0, error_count: 0,
            anomaly_ticks: 0,
        }
    }

    fn push_error(&mut self, err: f32) {
        self.error_buf[self.error_idx as usize] = err;
        self.error_idx = self.error_idx.wrapping_add(1) & 127;
        if self.error_count < 128 {
            self.error_count += 1;
        }
    }

    fn trend(&self) -> Trend {
        let n = self.error_count as usize;
        if n < SHORT_WIN {
            return Trend::Stable;
        }
        let short_n = SHORT_WIN;
        let long_n = LONG_WIN.min(n);
        let idx = self.error_idx as usize;
        let mask = ERR_BUF - 1;
        let mut long_sum = 0.0f32;
        let mut short_sum = 0.0f32;
        for i in 0..long_n {
            let pos = (idx + mask - i) & mask;
            let v = self.error_buf[pos];
            long_sum += v;
            if i < short_n {
                short_sum += v;
            }
        }
        let diff = short_sum / short_n as f32 - long_sum / long_n as f32;
        if diff > ANOMALY_THRESH { Trend::Rising }
        else if diff < -ANOMALY_THRESH { Trend::Falling }
        else { Trend::Stable }
    }

    pub fn predict(&self, state: &[i16; 32], input: &[f32; 4]) -> [f32; PFM_OUT] {
        let mut concat = [0.0f32; PFM_IN];
        for i in 0..32 {
            concat[i] = state[i] as f32 / 100.0;
        }
        concat[32] = input[0];
        concat[33] = input[1];
        concat[34] = input[2];
        concat[35] = input[3];

        let mut pred = [0.0f32; PFM_OUT];
        for i in 0..PFM_OUT {
            let mut sum = self.biases[i];
            for j in 0..PFM_IN {
                sum += self.weights[i][j] * concat[j];
            }
            pred[i] = sum;
        }
        pred
    }

    pub fn predict_f32(&self, state: &[f32; PFM_OUT], input: &[f32; 4]) -> [f32; PFM_OUT] {
        let mut concat = [0.0f32; PFM_IN];
        for i in 0..PFM_OUT {
            concat[i] = state[i];
        }
        concat[32] = input[0];
        concat[33] = input[1];
        concat[34] = input[2];
        concat[35] = input[3];

        let mut pred = [0.0f32; PFM_OUT];
        for i in 0..PFM_OUT {
            let mut sum = self.biases[i];
            for j in 0..PFM_IN {
                sum += self.weights[i][j] * concat[j];
            }
            pred[i] = sum;
        }
        pred
    }

    /// Chain prediction: N passi, input congelato
    /// Ogni passo usa la predizione del passo precedente come stato
    pub fn dream(&self, state: &[i16; 32], input: &[f32; 4], steps: usize) -> [[f32; PFM_OUT]; 16] {
        let mut chain = [[0.0f32; PFM_OUT]; 16];
        let n = steps.min(16);
        let mut s: [f32; PFM_OUT] = {
            let mut a = [0.0f32; PFM_OUT];
            for i in 0..PFM_OUT { a[i] = state[i] as f32 / 100.0; }
            a
        };
        for k in 0..n {
            let pred = self.predict_f32(&s, input);
            chain[k] = pred;
            s = pred;
        }
        chain
    }

    pub fn compute_error(&self, predicted: &[f32; PFM_OUT], actual: &[f32; PFM_OUT]) -> f32 {
        let mut sum_sq = 0.0f32;
        for i in 0..PFM_OUT {
            let diff = predicted[i] - actual[i];
            sum_sq += diff * diff;
        }
        sum_sq / PFM_OUT as f32
    }

    fn learn(&mut self, state: &[i16; 32], input: &[f32; 4], actual: &[f32; PFM_OUT]) {
        let mut concat = [0.0f32; PFM_IN];
        for i in 0..32 {
            concat[i] = state[i] as f32 / 100.0;
        }
        concat[32] = input[0];
        concat[33] = input[1];
        concat[34] = input[2];
        concat[35] = input[3];

        let predicted = self.predict(state, input);
        let alpha = self.learning_rate;

        for i in 0..PFM_OUT {
            let delta = predicted[i] - actual[i];
            for j in 0..PFM_IN {
                self.weights[i][j] -= alpha * delta * concat[j];
            }
            self.biases[i] -= alpha * delta;
        }
    }

    pub fn step(&mut self, state: &[i16; 32], input: &[f32; 4], actual: &[i16; 32]) -> PredictionReport {
        let actual_f32: [f32; PFM_OUT] = {
            let mut a = [0.0f32; PFM_OUT];
            for i in 0..PFM_OUT {
                a[i] = actual[i] as f32 / 100.0;
            }
            a
        };

        let predicted = self.predict(state, input);
        let error = self.compute_error(&predicted, &actual_f32);
        self.last_error = error;
        self.learn(state, input, &actual_f32);

        self.push_error(error);
        let trend = self.trend();
        if trend == Trend::Rising {
            self.anomaly_ticks = self.anomaly_ticks.saturating_add(1);
        } else {
            self.anomaly_ticks = 0;
        }

        let force_store = self.anomaly_ticks >= ANOMALY_TRIGGER;

        let alpha_mod = if error < 0.001 { 0.5 }
                       else if error < 0.01 { 1.0 }
                       else { 2.0 };

        PredictionReport { error, alpha_mod, trend, anomaly_ticks: self.anomaly_ticks, force_store }
    }

    pub fn last_error(&self) -> f32 { self.last_error }
}

pub struct PredictionReport {
    pub error: f32,
    pub alpha_mod: f32,
    pub trend: Trend,
    pub anomaly_ticks: u8,
    pub force_store: bool,
}
