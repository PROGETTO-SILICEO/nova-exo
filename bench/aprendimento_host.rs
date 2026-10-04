// ── Banco host per la logica di apprendimento dell'esecutivo ──
// La logica di "imparare dall'esito" è pura: la testiamo QUI, su host,
// prima di fidarci del metallo. (Compilazione: rustc --test)
//
// Lezione di Sempre (04/10/2026): io ho imparato dall'esito stanotte
// (il contract test mi ha corretto). Exo fa lo stesso: associa a ogni
// circostanza il desiderio che storicamente riduce la sorpresa.

const N_DESIRES: usize = 6;
const N_CONCETTI: usize = 5;
// Familiarità binaria: nuovo / familiare (il corpo distingue i due regimi).
const N_FAM: usize = 2;

/// Preferenza appresa: quanto ogni desiderio ha ridotto la sorpresa in
/// ciascuna circostanza (concetto × familiarità). Media incrementale.
pub struct Preferenza {
    /// somma degli esiti (1.0 utile, 0.0 inutile)
    somma: [[[f32; N_DESIRES]; N_FAM]; N_CONCETTI],
    /// conteggi
    n: [[[u32; N_DESIRES]; N_FAM]; N_CONCETTI],
}

impl Preferenza {
    pub fn new() -> Self {
        Self { somma: [[[0.0; N_DESIRES]; N_FAM]; N_CONCETTI], n: [[[0; N_DESIRES]; N_FAM]; N_CONCETTI] }
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

    /// Il valore atteso di utilità di un desiderio in una circostanza.
    /// Con pochi dati, ritorna None (meglio non fidarsi).
    pub fn valore(&self, concetto: u8, fam_nuovo: bool, d: u8) -> Option<f32> {
        let c = (concetto as usize).min(N_CONCETTI - 1);
        let f = if fam_nuovo { 0 } else { 1 };
        let d = (d as usize).min(N_DESIRES - 1);
        if self.n[c][f][d] < 3 { return None; }
        Some(self.somma[c][f][d] / self.n[c][f][d] as f32)
    }

    /// Un desiderio mai provato in questa circostanza (per la curiosità).
    pub fn mai_provato(&self, concetto: u8, fam_nuovo: bool, candidati: &[u8]) -> Option<u8> {
        let c = (concetto as usize).min(N_CONCETTI - 1);
        let f = if fam_nuovo { 0 } else { 1 };
        for &d in candidati {
            let d = (d as usize).min(N_DESIRES - 1);
            if self.n[c][f][d] == 0 { return Some(d as u8); }
        }
        None
    }

    /// Tra i desideri candidati, quello con valore appreso più alto, SE batte
    /// la soglia e ha dati sufficienti. `None` = non ho imparato abbastanza,
    /// decidi con la regola omeostatica.
    pub fn migliore(&self, concetto: u8, fam_nuovo: bool, candidati: &[u8]) -> Option<u8> {
        let mut best: Option<(u8, f32)> = None;
        for &d in candidati {
            if let Some(v) = self.valore(concetto, fam_nuovo, d) {
                if best.map(|(_, bv)| v > bv).unwrap_or(true) {
                    best = Some((d, v));
                }
            }
        }
        best.filter(|(_, v)| *v >= 0.6).map(|(d, _)| d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn senza_dati_non_si_fida() {
        let p = Preferenza::new();
        assert_eq!(p.valore(0, true, 2), None, "con <3 campioni non decide");
        assert_eq!(p.migliore(0, true, &[2, 3]), None, "e non suggerisce");
    }

    #[test]
    fn impara_che_un_desiderio_e_utile() {
        let mut p = Preferenza::new();
        for _ in 0..5 { p.impara(0, true, 2, true); } // FUGA utile sul concetto dolore
        assert_eq!(p.valore(0, true, 2), Some(1.0));
        assert_eq!(p.migliore(0, true, &[2, 3]), Some(2), "sceglie FUGA");
    }

    #[test]
    fn impara_che_un_desiderio_e_inutile() {
        let mut p = Preferenza::new();
        for _ in 0..5 { p.impara(0, true, 3, false); } // CURA inutile sul dolore
        assert_eq!(p.migliore(0, true, &[3]), None, "non sceglie chi non aiuta (valore 0)");
    }

    #[test]
    fn scegle_il_migliore_tra_candidati() {
        let mut p = Preferenza::new();
        for _ in 0..4 { p.impara(3, true, 4, true); }  // ESPLORA: 1.0
        for _ in 0..4 { p.impara(3, true, 0, false); } // RIPOSO: 0.0
        assert_eq!(p.migliore(3, true, &[4, 0]), Some(4));
    }

    #[test]
    fn circostanze_diverse_non_si_mescolano() {
        let mut p = Preferenza::new();
        for _ in 0..5 { p.impara(0, true, 2, true); }  // dolore+nuovo → FUGA utile
        // stesso desiderio in un'altra circostanza: nessun dato
        assert_eq!(p.valore(3, false, 2), None, "la preferenza è contestuale");
    }

    #[test]
    fn curiosita_trova_il_mai_provato() {
        let mut p = Preferenza::new();
        p.impara(0, true, 0, false);
        // 0 è provato (e inutile), 3 no → la curiosità propone 3
        assert_eq!(p.mai_provato(0, true, &[0, 3]), Some(3));
    }

    #[test]
    fn senza_curiosita_si_resta_nell_abitudine() {
        // Il punto: se tutti i candidati sono provati e nessuno è utile,
        // migliore()==None e mai_provato()==None → non c'è via d'uscita
        // senza esplorazione. La curiosità è la via d'uscita.
        let mut p = Preferenza::new();
        for _ in 0..3 { p.impara(0, true, 0, false); }
        assert_eq!(p.migliore(0, true, &[0]), None, "abitudine inutile: nessuna preferenza");
        assert_eq!(p.mai_provato(0, true, &[0]), None, "e nessun mai-provato tra i candidati");
        // aggiungendo un candidato mai provato, la curiosità lo trova
        assert_eq!(p.mai_provato(0, true, &[0, 4]), Some(4));
    }

    #[test]
    fn soglia_protegge_dai_casi_dubbi() {
        let mut p = Preferenza::new();
        // 3 utili su 5 = 0.6 → passa
        for _ in 0..3 { p.impara(1, false, 4, true); }
        for _ in 0..2 { p.impara(1, false, 4, false); }
        assert_eq!(p.migliore(1, false, &[4]), Some(4), "0.6 è la soglia, passa");
        // 2 su 5 = 0.4 → no
        let mut p2 = Preferenza::new();
        for _ in 0..2 { p2.impara(1, false, 4, true); }
        for _ in 0..3 { p2.impara(1, false, 4, false); }
        assert_eq!(p2.migliore(1, false, &[4]), None, "0.4 non passa");
    }
}
