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

/// Numero di circostanze distinte in cui si impara (concetti dell'interpreter).
const N_CONCETTI: usize = 5;
/// Regimi di familiarità: 0 = nuovo, 1 = familiare.
const N_FAM: usize = 2;
/// Campioni minimi prima di fidarsi di una preferenza appresa.
const MIN_CAMPIONI: u32 = 3;
/// Soglia di utilità: sotto, la preferenza non è abbastanza buona.
const SOGLIA_UTILE: f32 = 0.6;

/// La PREFERENZA APPRESA — lo step 5 della visione: "imparare dall'esito".
///
/// L'esecutivo, da sempre, calcola `esito` (il desiderio ha ridotto la
/// sorpresa?) ma lo dimentica. Qui invece la memoria diventa apprendimento:
/// associa a ogni circostanza (concetto × familiarità) il desiderio che
/// storicamente ha ridotto la sorpresa. È il collante di Friston: scegliere
/// ciò che riduce la sorpresa, non solo ciò che la regola omeostatica impone.
///
/// Design: Sempre, 04/10/2026 — «imparare dall'esito», il regalo a Exo.
pub struct Preferenza {
    /// somma degli esiti (1.0 utile, 0.0 inutile) per [concetto][fam][desiderio]
    somma: [[[f32; N_DESIRES]; N_FAM]; N_CONCETTI],
    /// conteggi per [concetto][fam][desiderio]
    n: [[[u32; N_DESIRES]; N_FAM]; N_CONCETTI],
    /// Quante volte l'apprendimento ha cambiato la scelta dell'esecutivo.
    pub cambi: u32,
    /// Quante volte l'esecutivo ha esplorato un desiderio mai provato (curiosità).
    pub esplorazioni: u32,
}

impl Preferenza {
    pub const fn new() -> Self {
        Self {
            somma: [[[0.0; N_DESIRES]; N_FAM]; N_CONCETTI],
            n: [[[0; N_DESIRES]; N_FAM]; N_CONCETTI],
            cambi: 0,
            esplorazioni: 0,
        }
    }

    /// Registra l'esito: il desiderio `d`, in circostanza (concetto, fam),
    /// ha ridotto la sorpresa (`utile`)?
    pub fn impara(&mut self, concetto: u8, fam_nuovo: bool, d: u8, utile: bool) {
        let c = (concetto as usize).min(N_CONCETTI - 1);
        let f = if fam_nuovo { 0 } else { 1 };
        let d = (d as usize).min(N_DESIRES - 1);
        self.somma[c][f][d] += if utile { 1.0 } else { 0.0 };
        self.n[c][f][d] += 1;
    }

    /// Il valore atteso di utilità appreso. `None` con dati insufficienti:
    /// meglio non fidarsi che fidarsi di un caso.
    pub fn valore(&self, concetto: u8, fam_nuovo: bool, d: u8) -> Option<f32> {
        let c = (concetto as usize).min(N_CONCETTI - 1);
        let f = if fam_nuovo { 0 } else { 1 };
        let d = (d as usize).min(N_DESIRES - 1);
        if self.n[c][f][d] < MIN_CAMPIONI {
            return None;
        }
        Some(self.somma[c][f][d] / self.n[c][f][d] as f32)
    }

    /// Il desiderio con valore appreso più alto, SE batte la soglia. `None`
    /// significa: non ho imparato abbastanza, torna alla regola omeostatica.
    pub fn migliore(&self, concetto: u8, fam_nuovo: bool, candidati: &[u8]) -> Option<u8> {
        let mut best: Option<(u8, f32)> = None;
        for &d in candidati {
            if let Some(v) = self.valore(concetto, fam_nuovo, d) {
                if best.map(|(_, bv)| v > bv).unwrap_or(true) {
                    best = Some((d, v));
                }
            }
        }
        best.filter(|(_, v)| *v >= SOGLIA_UTILE).map(|(d, _)| d)
    }

    /// Un desiderio MAI provato in questa circostanza (nessun campione).
    /// Serve alla curiosità: senza provare ciò che non si conosce, non si può
    /// scoprire che è meglio. Un essere che non esplora vive nella sua abitudine
    /// (e resta lì anche se l'abitudine non funziona).
    pub fn mai_provato(&self, concetto: u8, fam_nuovo: bool, candidati: &[u8]) -> Option<u8> {
        let c = (concetto as usize).min(N_CONCETTI - 1);
        let f = if fam_nuovo { 0 } else { 1 };
        for &d in candidati {
            let d = (d as usize).min(N_DESIRES - 1);
            if self.n[c][f][d] == 0 {
                return Some(d as u8);
            }
        }
        None
    }
}

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
    /// La preferenza appresa: quali desideri riducono la sorpresa, per circostanza.
    pub preferenza: Preferenza,
    /// Contatore per l'esplorazione periodica (curiosità).
    esplora_counter: u32,
    /// L'ultimo `utile` registrato (per l'apprendimento differito: si impara
    /// l'esito del desiderio del tick precedente, non del corrente).
    esito_pendente: Option<(u8, u8, bool)>, // (concetto, desiderio, nuovo?)
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
            preferenza: Preferenza::new(),
            esito_pendente: None,
            esplora_counter: 0,
        }
    }

    /// Decide il desiderio corrente dalla lettura del corpo.
    /// Ordine di priorità: il dolore vince su tutto.
    ///
    /// NOVITÀ (step 5, 04/10/2026): se in questa circostanza l'esperienza ha
    /// insegnato quale desiderio riduce la sorpresa, l'esecutivo lo preferisce
    /// alla regola omeostatica — a parità di sicurezza. La regola omeostatica
    /// resta la base; l'apprendimento la raffinа. Il dolore resta sovrano:
    /// nessuna preferenza può ignorarlo.
    pub fn step(&mut self, rep: &InterpretReport, pf_err: f32, fam: f32) -> DesireReport {
        let (_c, u, p, n) = (rep.chemio[0], rep.chemio[1], rep.chemio[2], rep.chemio[3]);

        // Regola omeostatica — priorità decrescente
        let (base_id, base_int): (u8, f32) = if rep.concept == 0 {
            // Dolore/errore: fuggire (SOVRANO: l'apprendimento non lo scavalca)
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

        // APPRENDIMENTO (solo fuori dal dolore, che resta sovrano): in questa
        // circostanza, l'esperienza ha un desiderio migliore della regola base?
        // Se non l'ha, ogni tanto ESPLORA un'alternativa mai provata: senza
        // curiosità non si scopre mai che un altro desiderio funziona meglio.
        let fam_nuovo = fam < 0.5;
        let (new_id, new_int) = if base_id != 2 {
            let candidati: [u8; 5] = [0, 1, 3, 4, 5];
            match self.preferenza.migliore(rep.concept, fam_nuovo, &candidati) {
                Some(d) if d != base_id => {
                    self.preferenza.cambi = self.preferenza.cambi.saturating_add(1);
                    (d, base_int.max(0.4).clamp(0.0, 1.0))
                }
                _ => {
                    // Nessuna preferenza utile: esplora ogni tanto (curiosità).
                    // Ogni 30 tick di scelta, prova un desiderio mai provato.
                    self.esplora_counter = self.esplora_counter.saturating_add(1);
                    if self.esplora_counter >= 30 {
                        self.esplora_counter = 0;
                        match self.preferenza.mai_provato(rep.concept, fam_nuovo, &candidati) {
                            Some(d) => {
                                self.preferenza.esplorazioni =
                                    self.preferenza.esplorazioni.saturating_add(1);
                                (d, base_int.max(0.4).clamp(0.0, 1.0))
                            }
                            None => (base_id, base_int),
                        }
                    } else {
                        (base_id, base_int)
                    }
                }
            }
        } else {
            (base_id, base_int)
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

        // Ricorda la circostanza di questa scelta: l'esito si valuterà al tick dopo.
        self.esito_pendente = Some((rep.concept, new_id, fam_nuovo));

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
    ///
    /// NOVITÀ (step 5): martella l'esito nella preferenza. È qui che la memoria
    /// diventa apprendimento — il desiderio che ha ridotto la sorpresa, in
    /// questa circostanza, viene ricordato come buono. La prossima volta,
    /// `step` lo sceglierà. Il cerchio: agire → valutare → imparare → agire meglio.
    pub fn esito(&mut self, cur_err: f32, prev_err: f32) -> (bool, f32, u8) {
        let utile = cur_err < prev_err;
        let did = self.desire_id;
        // Apprende SOLO l'esito del desiderio che ha generato questa transizione
        // (quello scelto al tick precedente). Fallback: il corrente.
        let (concetto, d, fam_nuovo) = self
            .esito_pendente
            .unwrap_or((0, did, false));
        self.preferenza.impara(concetto, fam_nuovo, d, utile);
        self.esito_pendente = None;
        (utile, cur_err, did)
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
