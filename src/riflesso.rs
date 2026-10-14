// ── riflesso.rs — gli archi riflessi (il corpo che risponde SENZA il cervello) ──
//
// "Ora hanno routine, l'idea è il live" (Alfonso, 14/10/2026). La differenza tra
// una routine e un corpo vivo comincia qui: **un riflesso non passa dal cervello**.
// Il midollo risponde nello STESSO istante in cui sente — più veloce del pensiero.
// Exo è la spina dorsale (decisione 11/10): il riflesso è la sua prima funzione.
//
// PRIMO ARCO (14/10/2026): **SOPRASSALTO a un suono che APPARA**. Non è "un suono
// forte" (un brusio forte è rumore di fondo): è un *salto* — il suono c'era piano
// e ora c'è. È il riflesso di orientamento: il corpo si tende verso il nuovo.
//
// Logica PURA (nessun MMIO, nessun unsafe): testabile su host
// (bench/riflesso_host.rs), col codice REALE via #[path].

/// Un arco riflesso: un sensore, una soglia, un refrattario, un'azione.
/// Scatta nello stesso tick in cui la condizione è vera.
pub struct Riflesso {
    /// Salto minimo di energia del suono per far scattare il soprassalto.
    soglia: f32,
    /// Sotto questa energia il corpo è "in quiete" (nessun suono di fondo).
    quiete: f32,
    /// Tick di riposo dopo uno scatto: un riflesso non "trema" — si riarma.
    refrattario: u32,
    resta: u32,
    scatti: u32,
    energia_prec: f32,
    /// Prima percezione: non c'era un "prima", quindi nessun salto da misurare.
    prima_volta: bool,
}

impl Riflesso {
    pub const fn new(soglia: f32, quiete: f32, refrattario: u32) -> Self {
        Self {
            soglia, quiete, refrattario,
            resta: 0, scatti: 0,
            energia_prec: 0.0, prima_volta: true,
        }
    }

    /// Percepisce l'energia del suono. Ritorna true SE il riflesso scatta in
    /// questo istante. Il salto conta solo se il corpo era in quiete *prima*:
    /// un suono forte costante non fa soprassaltare due volte.
    pub fn percepisce(&mut self, energia: f32) -> bool {
        let scatta = !self.prima_volta
            && self.resta == 0
            && self.energia_prec < self.quiete // prima: quiete
            && energia >= self.soglia;         // ora: c'è
        self.energia_prec = energia;
        self.prima_volta = false;
        if scatta {
            self.resta = self.refrattario;
            self.scatti = self.scatti.wrapping_add(1);
            true
        } else {
            if self.resta > 0 { self.resta -= 1; }
            false
        }
    }

    pub fn scatti(&self) -> u32 { self.scatti }
    pub fn in_refrattario(&self) -> bool { self.resta > 0 }
}
