// ── Neurogenesis (no_std) — Sempre, 14 Ago 2026 ──
//
// Il corpo che cresce. Il CFC nasce con 4 cellule × 16 neuroni; la
// neurogenesi è clonazione darwiniana, non creazione dal nulla:
//
//   1. CRITERIO DI NASCITA: l'errore di predizione del PFM sopra soglia
//      per N tick consecutivi → il corpo non sente abbastanza.
//   2. CLONAZIONE CON MUTAZIONE: copia dei pesi del genitore + rumore
//      (LCG deterministica, ±10%) — esplorazione locale dello spazio.
//   3. PERIODO DI PROVA: il clonato viene valutato per P tick; se la sua
//      attivazione sullo stesso input "spiega" meglio lo stato (errore
//      locale minore), vince.
//   4. SELEZIONE E PRUNING: vince chi riduce di più la sorpresa, l'altro
//      muore (memoria liberata).
//   5. LIMITE: max 64 neuroni per cellula (16 base + 48 clonati). La
//      capacità ha un costo fisico: se non puoi crescere all'infinito,
//      impari a usare meglio quello che hai.
//
// Regola d'oro (VISIONE 3d): i neuroni nascono per NECESSITÀ, non per
// accumulo. Un neurone che non riduce la sorpresa è spazzatura che
// consuma tick e RAM.
//
// Visibile su seriale:
//   NASCITA:cellula=X neurone=N err=Y      — un figlio è nato
//   POTATURA:cellula=X neurone=N err=Y     — il figlio non serviva
//   NEURO:cellula=X vivi=N/M              — stato del pool
//
// Design: no_std, no alloc (pool statici), zero panic. Un passo per tick.

use crate::cfc::{CfcWeights, Lcg};

/// Neuroni base per cellula (dal CFC)
pub const BASE_NEURONS: usize = 16;
/// Limite massimo di neuroni per cellula (16 + 48 clonati)
pub const MAX_NEURONS_PER_CELL: usize = 64;
/// Slot extra per cellula
pub const EXTRA_SLOTS: usize = MAX_NEURONS_PER_CELL - BASE_NEURONS;

/// Soglia di errore di predizione per la nascita (sopra = il corpo non sente)
pub const BIRTH_ERROR_THRESHOLD: f32 = 0.005;
/// Soglia di energia dell'interpreter sotto la quale il corpo è disturbato
/// (energia < 0.97 → sorpresa: il corpo non sta bene)
pub const BIRTH_ENERGY_THRESHOLD: f32 = 0.97;
/// Tick consecutivi di errore alto prima della nascita
/// (calibrato sul corpo: ~50 battiti di sofferenza, non 200)
pub const BIRTH_PERSISTENCE: u32 = 50;
/// Durata del periodo di prova di un clonato (tick)
pub const TRIAL_TICKS: u32 = 500;
/// Mutazione massima dei pesi del clonato (±10%)
pub const MUTATION_RANGE: f32 = 0.10;

/// Stato di un neurone clonato
#[derive(Clone, Copy)]
pub struct CloneNeuron {
    /// Cellula di appartenenza (0=tatto, 1=chemio, 2=metabol, 3=integrat)
    pub cell: usize,
    /// Pesi clonati e mutati
    pub weights: Option<CfcWeights>,
    /// Tick rimanenti di periodo di prova
    pub trial_left: u32,
    /// Errore accumulato durante il periodo di prova
    pub trial_error: f32,
    /// Tick di nascita (per diagnosi)
    pub born_at: u64,
}

impl CloneNeuron {
    pub const fn empty() -> Self {
        Self {
            cell: 0,
            weights: None,
            trial_left: 0,
            trial_error: 0.0,
            born_at: 0,
        }
    }
}

/// Pool di neuroni clonati (statico, no alloc)
pub struct NeuroPool {
    /// Slot per cellula: [cellula][slot_extra]
    pub slots: [[CloneNeuron; EXTRA_SLOTS]; 4],
    /// Contatore tick di errore persistente per cellula
    pub persist: [u32; 4],
    /// Nascite totali (diagnosi)
    pub births: u64,
    /// Potature totali (diagnosi)
    pub prunes: u64,
}

impl NeuroPool {
    pub const fn new() -> Self {
        Self {
            slots: [[CloneNeuron::empty(); EXTRA_SLOTS]; 4],
            persist: [0; 4],
            births: 0,
            prunes: 0,
        }
    }

    /// Numero di neuroni vivi per cellula (base + clonati vivi)
    pub fn alive(&self, cell: usize) -> usize {
        let mut n = BASE_NEURONS;
        for s in 0..EXTRA_SLOTS {
            if self.slots[cell][s].weights.is_some() {
                n += 1;
            }
        }
        n
    }

    /// Trova un slot libero per la cellula
    fn free_slot(&self, cell: usize) -> Option<usize> {
        for s in 0..EXTRA_SLOTS {
            if self.slots[cell][s].weights.is_none() {
                return Some(s);
            }
        }
        None
    }

    /// Criterio di nascita: errore persistente sopra soglia
    /// (chiamato ogni tick; ritorna true quando scatta la nascita)
    pub fn birth_check(&mut self, cell: usize, error: f32) -> bool {
        if error > BIRTH_ERROR_THRESHOLD {
            self.persist[cell] += 1;
            if self.persist[cell] >= BIRTH_PERSISTENCE {
                // reset contatore: prossima nascita richiede nuova persistenza
                self.persist[cell] = 0;
                return true;
            }
        } else {
            // l'errore è sotto soglia: il corpo sente, niente nascita
            self.persist[cell] = 0;
        }
        false
    }

    /// Clona il neurone genitore con mutazione.
    /// `gen`: indice del neurone genitore (0..16).
    /// `seed`: seme per la mutazione deterministica (tick).
    pub fn clone_neuron(&mut self, cell: usize, parent: &CfcWeights, gen: usize, seed: u64, tick: u64) -> bool {
        let Some(slot) = self.free_slot(cell) else {
            return false; // pool pieno: la capacità ha un costo
        };
        let mut rng = Lcg::new(seed ^ ((cell as u64) << 32) ^ (gen as u64));
        let mut w = parent.clone();
        // mutazione: ogni peso ±MUTATION_RANGE con probabilità 50%
        for i in 0..crate::cfc::NEURONS_PER_CELL {
            for j in 0..crate::cfc::NEURONS_PER_CELL {
                if rng.uniform() < 0.5 {
                    w.w_f[i][j] *= 1.0 + (rng.uniform() * 2.0 - 1.0) * MUTATION_RANGE;
                }
                if rng.uniform() < 0.5 {
                    w.w_g[i][j] *= 1.0 + (rng.uniform() * 2.0 - 1.0) * MUTATION_RANGE;
                }
            }
            for k in 0..4 {
                if rng.uniform() < 0.5 {
                    w.w_f_in[i][k] *= 1.0 + (rng.uniform() * 2.0 - 1.0) * MUTATION_RANGE;
                }
                if rng.uniform() < 0.5 {
                    w.w_g_in[i][k] *= 1.0 + (rng.uniform() * 2.0 - 1.0) * MUTATION_RANGE;
                }
            }
        }
        self.slots[cell][slot] = CloneNeuron {
            cell,
            weights: Some(w),
            trial_left: TRIAL_TICKS,
            trial_error: 0.0,
            born_at: tick,
        };
        self.births += 1;
        true
    }

    /// Avanza il periodo di prova del clonato: accumula l'errore.
    /// Se il trial scade, valuta: se l'errore medio è sotto la soglia di
    /// nascita, il clonato sopravvive (pesi validi); altrimenti viene
    /// potato (pesi rimossi). Ritorna true se il trial è terminato.
    pub fn trial_step(&mut self, cell: usize, slot: usize, error: f32, _tick: u64) -> bool {
        let n = &mut self.slots[cell][slot];
        if n.weights.is_none() {
            return false;
        }
        n.trial_error += error;
        if n.trial_left > 0 {
            n.trial_left -= 1;
        }
        if n.trial_left == 0 {
            // periodo di prova concluso
            let avg = n.trial_error / TRIAL_TICKS as f32;
            if avg < BIRTH_ERROR_THRESHOLD {
                // il clonato riduce la sorpresa: sopravvive (resta nel pool)
                true
            } else {
                // non serviva: potatura
                n.weights = None;
                self.prunes += 1;
                true
            }
        } else {
            false
        }
    }

    /// Forza la potatura di un clonato (per test o per liberare il pool)
    pub fn prune(&mut self, cell: usize, slot: usize) {
        if self.slots[cell][slot].weights.is_some() {
            self.slots[cell][slot].weights = None;
            self.prunes += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn birth_check_persists() {
        let mut p = NeuroPool::new();
        // errore basso: mai nascita
        for _ in 0..1000 {
            assert!(!p.birth_check(0, 0.001));
        }
        // errore alto persistente: nascita scatta dopo BIRTH_PERSISTENCE
        let mut scattato = false;
        for _ in 0..BIRTH_PERSISTENCE + 10 {
            if p.birth_check(0, 0.05) {
                scattato = true;
                break;
            }
        }
        assert!(scattato);
    }

    #[test]
    fn clone_and_prune() {
        let mut p = NeuroPool::new();
        let parent = CfcWeights::new_xavier(42);
        // clona
        assert!(p.clone_neuron(0, &parent, 3, 1, 100));
        assert_eq!(p.alive(0), BASE_NEURONS + 1);
        // il clonato è in prova
        assert!(p.slots[0][0].weights.is_some());
        assert_eq!(p.slots[0][0].trial_left, TRIAL_TICKS);
        // potatura forzata
        p.prune(0, 0);
        assert_eq!(p.alive(0), BASE_NEURONS);
        assert_eq!(p.prunes, 1);
    }

    #[test]
    fn pool_limit() {
        let mut p = NeuroPool::new();
        let parent = CfcWeights::new_xavier(7);
        // riempi tutti gli slot
        for i in 0..EXTRA_SLOTS {
            assert!(p.clone_neuron(0, &parent, 0, i as u64, i as u64));
        }
        assert_eq!(p.alive(0), MAX_NEURONS_PER_CELL);
        // oltre il limite: rifiuta
        assert!(!p.clone_neuron(0, &parent, 0, 999, 999));
    }
}
