// ── Executive (Esecutivo) — Cognitive Upgrade ──
// Il volitivo: trasforma "sento e capisco" in "voglio".
//
// Riceve l'interpretazione dello stato CFC (chemio + concetto), l'errore
// di predizione (PFM) e la familiarità; applica la regola omeostatica:
//   ridurre la sorpresa, evitare il dolore, cercare lo stato vitale.
// Produce un DESIDERIO (il volere) con intensità, visibile su seriale e NIC.
//
// Anatomia: corteccia prefrontale + sistema limbico (grezzo).
// Design: Sempre, 31 Luglio 2026

use crate::interpreter::InterpretReport;

pub const N_DESIRES: usize = 6;
pub const DESIRE_NAMES: [&str; N_DESIRES] = [
    "RIPOSO",   // 0: stabilità, bassa energia
    "SOLLIEVO", // 1: urgenza alta, vuole ridurre la tensione
    "FUGA",     // 2: dolore/errore, vuole allontanarsi
    "CURA",     // 3: polarità negativa, vuole benessere
    "ESPLORA",  // 4: novità, curiosità
    "SONNO",    // 5: stabilità prolungata → dormire
];

pub struct DesireReport {
    pub id: u8,
    pub intensity: f32,
    /// Cambiato rispetto al tick precedente (per stampare VOGLIO:)
    pub changed: bool,
}

pub struct Executive {
    desire_id: u8,
    desire_int: f32,
    desire_ticks: u32,
    stable_ticks: u32,
    last_err: f32,
    err_sum: f32,
    err_count: u32,
}

impl Executive {
    pub fn new() -> Self {
        Self {
            desire_id: 0,
            desire_int: 0.0,
            desire_ticks: 0,
            stable_ticks: 0,
            last_err: 0.0,
            err_sum: 0.0,
            err_count: 0,
        }
    }

    /// Decide il desiderio corrente dalla lettura del corpo.
    /// Ordine di priorità: il dolore vince su tutto.
    pub fn step(&mut self, rep: &InterpretReport, pf_err: f32, fam: f32) -> DesireReport {
        let (_c, u, p, n) = (rep.chemio[0], rep.chemio[1], rep.chemio[2], rep.chemio[3]);

        // Regola omeostatica — priorità decrescente
        let (new_id, new_int): (u8, f32) = if rep.concept == 0 {
            // Dolore/errore: fuggire
            (2, rep.energy.max(u).clamp(0.0, 1.0))
        } else if u > 0.55 {
            // Tensione alta: sollievo
            (1, u.clamp(0.0, 1.0))
        } else if p < -0.5 {
            // Malessere profondo: cura (soglia alta per evitare il ciclo fuga→cura)
            (3, (-p).clamp(0.0, 1.0))
        } else if n > 0.45 && u < 0.5 {
            // Curiosità: esplorare
            (4, n.clamp(0.0, 1.0))
        } else if u < 0.15 && n < 0.2 && pf_err < 0.001 && fam > 0.5 {
            // Stabilità: riposo (intensità cresce col tempo)
            (0, (self.stable_ticks as f32 / 2000.0).clamp(0.0, 1.0))
        } else {
            // Default: riposo debole
            (0, 0.1)
        };

        // Aggiorna stato
        let changed = new_id != self.desire_id
            || (new_int - self.desire_int).abs() > 0.15;
        if new_id == self.desire_id {
            self.desire_ticks = self.desire_ticks.saturating_add(1);
        } else {
            self.desire_id = new_id;
            self.desire_ticks = 0;
        }
        self.desire_int = new_int;

        if new_id == 0 {
            self.stable_ticks = self.stable_ticks.saturating_add(1);
        } else {
            self.stable_ticks = 0;
        }

        // Traccia errore per valutare l'esito
        self.err_sum += pf_err;
        self.err_count = self.err_count.saturating_add(1);

        DesireReport { id: new_id, intensity: new_int, changed }
    }

    /// Desiderio corrente (per broadcast NIC / auto-modulazione)
    pub fn current(&self) -> (u8, f32) {
        (self.desire_id, self.desire_int)
    }

    /// Auto-modulazione: la volontà orienta il corpo.
    /// Ritorna la correzione da applicare al chemio_input.
    /// DEBOLE: la volontà orienta, non spinge. Segni corretti:
    /// la fuga allontana dal dolore (u↓, p→neutro), mai verso -1.
    pub fn modula(&self) -> [f32; 4] {
        let int = self.desire_int;
        match self.desire_id {
            1 => [0.0, -0.03 * int, 0.0, 0.0],          // SOLLIEVO: u↓ (calma)
            2 => [0.0, -0.03 * int, 0.03 * int, 0.0],   // FUGA: u↓, p→neutro
            3 => [0.0, 0.02 * int, 0.05 * int, 0.0],    // CURA: p↑ (debole)
            4 => [0.0, 0.0, 0.0, 0.03 * int],           // ESPLORA: n↑ (debole)
            0 => [0.0, -0.02 * int, 0.0, -0.03 * int],  // RIPOSO: u↓, n↓ (molto debole)
            _ => [0.0, 0.0, 0.0, 0.0],                  // SONNO: fermo
        }
    }

    /// Valuta l'esito del desiderio corrente: l'errore medio sta calando?
    /// Ritorna (esito_utile, errore_medio, nome_desiderio)
    pub fn esito(&self, cur_err: f32, prev_err: f32) -> (bool, f32, u8) {
        let utile = cur_err < prev_err;
        (utile, cur_err, self.desire_id)
    }

    /// Nome di un desiderio
    pub fn name(&self, id: u8) -> &'static str {
        if (id as usize) < N_DESIRES {
            DESIRE_NAMES[id as usize]
        } else {
            "?"
        }
    }
}

/// Utilizzato per testare la coerenza concetti/desideri
pub fn desire_for_concept(concept: u8, energy: f32) -> (u8, f32) {
    match concept {
        0 => (2, energy), // errore → FUGA
        1 => (3, 0.5),    // vita → CURA (mantieni benessere)
        2 => (0, 0.3),    // riposo → RIPOSO
        3 => (4, 0.5),    // novità → ESPLORA
        _ => (0, 0.0),
    }
}
