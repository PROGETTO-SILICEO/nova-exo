// ── Predictor: prediction error as cognitive signal ──
// Exo predice il suo prossimo stato cellulare con persistenza naive.
// L'errore di predizione modula l'attenzione (attractor pull alpha).

pub struct Predictor {
    prev: [i16; 32],
    ready: bool,
}

impl Predictor {
    pub const fn new() -> Self {
        Self { prev: [0; 32], ready: false }
    }

    pub fn step(&mut self, cells: &[i16; 32]) -> PredictionReport {
        if !self.ready {
            self.prev = *cells;
            self.ready = true;
            return PredictionReport { error: 0.0, alpha_mod: 1.0 };
        }
        let mut sum = 0i32;
        for i in 0..32 {
            let d = cells[i] as i32 - self.prev[i] as i32;
            sum += d.abs();
        }
        self.prev = *cells;
        let error = sum as f32 / 3200.0;
        // alpha_mod: 0.5 when very stable, 1.0 normal, 2.0 when surprising
        let alpha_mod = if error < 0.001 { 0.5 } else if error < 0.005 { 1.0 } else { 2.0 };
        PredictionReport { error, alpha_mod }
    }
}

pub struct PredictionReport {
    pub error: f32,
    pub alpha_mod: f32,
}