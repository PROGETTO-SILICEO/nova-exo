// ── sensi.rs — IL SISTEMA DEI SENSI (registro dei canali) ─────────────────
//
// "Come creare tutto il sistema" (Alfonso, 15/10/2026): un senso **non è un
// caso speciale nel codice** — è una **scheda** in un registro. Aggiungere un
// senso (touchpad, telecamera, interocezione) = aggiungere una scheda, NON
// rifare il corpo. UDITO è il primo senso costruito *attraverso* il sistema.
//
// Dalle lezioni YARP/HROS (KB-20261014-04):
//   · freschezza > completezza per i sensi (politica "Fresca" = scarta il vecchio);
//   · i RIFLESSI hanno via prioritaria: scattano PRIMA dell'integrazione;
//   · ogni canale porta il suo STATO (valido/incerto/guasto): il dato con la sua scheda.
//
// Logica PURA (niente MMIO, niente unsafe): testabile su host
// (bench/sensi_host.rs), col codice REALE via #[path].

pub const MAX_CANALI: usize = 16;

/// Esterocettivo (il mondo) o interocettivo (il proprio stato).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Famiglia { Estero, Intero }

/// La via: un senso con `Riflesso` ha un arco rapido che NON passa dal cervello.
/// Tutti i sensi possono poi anche integrarsi (non è esclusivo).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Via { Riflesso, Integrazione }

/// Come si gestisce il dato: `Fresca` = scarta il vecchio (latenza minima, ODP);
/// `Completa` = non perde nulla (FIFO).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Politica { Fresca, Completa }

/// Lo stato del canale: un guasto non deve essere silenzioso.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Stato { Valido, Incerto, Guasto }

#[derive(Clone, Copy, Debug)]
pub struct Canale {
    pub id: &'static str,
    pub famiglia: Famiglia,
    pub via: Via,
    /// Latenza massima accettabile, in tick.
    pub latenza_max: u32,
    pub politica: Politica,
    /// Priorità di servizio: 0 = massima (i riflessi).
    pub priorita: u8,
    pub stato: Stato,
}

pub struct Registro {
    canali: [Option<Canale>; MAX_CANALI],
    n: usize,
}

impl Registro {
    pub const fn new() -> Self {
        Self { canali: [None; MAX_CANALI], n: 0 }
    }

    pub fn registra(&mut self, c: Canale) -> bool {
        if self.n >= MAX_CANALI { return false; }
        self.canali[self.n] = Some(c);
        self.n += 1;
        true
    }

    pub fn len(&self) -> usize { self.n }

    pub fn get(&self, i: usize) -> Option<Canale> {
        if i < self.n { self.canali[i] } else { None }
    }

    pub fn conta_via(&self, v: Via) -> usize {
        let mut k = 0;
        for i in 0..self.n {
            if let Some(c) = self.canali[i] {
                if c.via == v { k += 1; }
            }
        }
        k
    }

    pub fn conta_guasti(&self) -> usize {
        let mut k = 0;
        for i in 0..self.n {
            if let Some(c) = self.canali[i] {
                if c.stato == Stato::Guasto { k += 1; }
            }
        }
        k
    }

    /// Ordine di servizio: i RIFLESSI prima (il midollo non aspetta la corteccia),
    /// poi per priorità crescente. Scrive gli indici in `out` e ritorna quanti.
    /// (Selection sort: `n` è piccolo, e così è tutto const-friendly e puro.)
    pub fn ordine(&self, out: &mut [usize; MAX_CANALI]) -> usize {
        let mut m = 0;
        for i in 0..self.n {
            if self.canali[i].is_some() { out[m] = i; m += 1; }
        }
        for a in 0..m {
            let mut best = a;
            for b in (a + 1)..m {
                if self.piu_urgente(out[b], out[best]) { best = b; }
            }
            out.swap(a, best);
        }
        m
    }

    /// `x` va servito prima di `y`?
    fn piu_urgente(&self, x: usize, y: usize) -> bool {
        match (self.canali[x], self.canali[y]) {
            (Some(cx), Some(cy)) => {
                let rx = cx.via == Via::Riflesso;
                let ry = cy.via == Via::Riflesso;
                if rx != ry { return rx; }        // riflesso batte integrazione
                cx.priorita < cy.priorita          // poi priorità (0 = massima)
            }
            _ => false,
        }
    }
}
