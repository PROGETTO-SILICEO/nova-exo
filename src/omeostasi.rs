// ── omeostasi.rs — il corpo che si tiene (organi autonomi) ────────────────
//
// "Temperatura, ventola, memoria — come le gestiamo?" (Alfonso, 15/10/2026).
// Oltre ai SENSI (chi entra) il corpo ha due cose:
//   · gli ATTUATORI — chi AGISCE (ventola, LED, un giorno i motori);
//   · l'OMEOSTASI  — chi REGOLA, legando un senso del sé a un attuatore.
//
// L'omeostasi è un ORGANO AUTONOMO: **non passa dall'intelletto**. Se la mente
// tace, la ventola continua a girare — è §7 portato in fondo (il respiro).
//
// ISTERESI: una ventola che va on/off a ogni grado è un **tremolio**. Si entra
// sopra `setpoint + banda`, si esce sotto `setpoint - banda`: in mezzo, si sta.
//
// Logica PURA → testabile su host (bench/omeostasi_host.rs).

pub const MAX_ATTUATORI: usize = 8;

/// Un attuatore = un effettore del corpo. Chi agisce.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Attuatore {
    pub id: &'static str,
    pub acceso: bool,
}

pub struct Attuatori {
    a: [Option<Attuatore>; MAX_ATTUATORI],
    n: usize,
}

impl Attuatori {
    pub const fn new() -> Self { Self { a: [None; MAX_ATTUATORI], n: 0 } }

    pub fn registra(&mut self, id: &'static str, acceso: bool) -> bool {
        if self.n >= MAX_ATTUATORI { return false; }
        self.a[self.n] = Some(Attuatore { id, acceso });
        self.n += 1;
        true
    }

    pub fn len(&self) -> usize { self.n }

    pub fn get(&self, i: usize) -> Option<Attuatore> {
        if i < self.n { self.a[i] } else { None }
    }

    /// Comanda un attuatore per nome. false se non esiste.
    pub fn imposta(&mut self, id: &str, acceso: bool) -> bool {
        for i in 0..self.n {
            if let Some(a) = self.a[i] {
                if a.id == id {
                    self.a[i] = Some(Attuatore { acceso, ..a });
                    return true;
                }
            }
        }
        false
    }

    pub fn acceso(&self, id: &str) -> bool {
        for i in 0..self.n {
            if let Some(a) = self.a[i] {
                if a.id == id { return a.acceso; }
            }
        }
        false
    }

    pub fn conta_accesi(&self) -> usize {
        let mut k = 0;
        for i in 0..self.n {
            if let Some(a) = self.a[i] { if a.acceso { k += 1; } }
        }
        k
    }
}

/// Il regolatore: lega un valore del sé a un attuatore, con **isteresi**.
#[derive(Clone, Copy)]
pub struct Regolatore {
    setpoint: f32,
    banda: f32,
    alto: bool,
}

impl Regolatore {
    pub const fn new(setpoint: f32, banda: f32) -> Self {
        Self { setpoint, banda, alto: false }
    }

    /// Aggiorna con la lettura attuale; ritorna lo stato deciso (true = agisci).
    pub fn aggiorna(&mut self, v: f32) -> bool {
        if self.alto {
            if v < self.setpoint - self.banda { self.alto = false; }
        } else if v > self.setpoint + self.banda {
            self.alto = true;
        }
        self.alto
    }

    pub fn alto(&self) -> bool { self.alto }
}
