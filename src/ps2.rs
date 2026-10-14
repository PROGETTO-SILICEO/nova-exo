// ── ps2.rs — pacchetti PS/2 (PURI) ────────────────────────────────────────
//
// La decodifica di un pacchetto a 3 byte del mouse è PURA: nessun IO, nessun
// unsafe → testabile su host (bench/touchpad_host.rs la include con #[path]).
// Il driver (touchpad.rs) ha l'IO; QUI c'è la logica del protocollo.
//
// Convenzione PS/2: +x = destra, +y = BASSO. I segni stanno nei bit 4 e 5 del
// primo byte; il bit 3 del primo byte è sempre 1 (serve a ritrovare la fase).

/// Un pacchetto del mouse PS/2 (3 byte).
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Pacchetto {
    pub dx: i16,
    pub dy: i16,
    /// bit0 = sinistro, bit1 = destro, bit2 = centrale.
    pub bottoni: u8,
    pub overflow: bool,
}

/// DECODIFICA PURA di un pacchetto a 3 byte.
pub fn decodifica(b: [u8; 3]) -> Pacchetto {
    let f = b[0];
    let dx = if f & 0x10 != 0 { b[1] as i16 - 256 } else { b[1] as i16 };
    let dy = if f & 0x20 != 0 { b[2] as i16 - 256 } else { b[2] as i16 };
    Pacchetto { dx, dy, bottoni: f & 0x07, overflow: f & 0xC0 != 0 }
}

/// Assemblatore di pacchetti a 3 byte: consuma i byte che arrivano dal mouse e
/// restituisce il pacchetto quando è completo. Rifiuta i gruppi fuori fase
/// (primo byte senza il bit 3) invece di produrre gesti inventati.
pub struct Assemblatore {
    buf: [u8; 3],
    n: usize,
}

impl Assemblatore {
    pub const fn new() -> Self {
        Self { buf: [0; 3], n: 0 }
    }

    pub fn byte(&mut self, b: u8) -> Option<Pacchetto> {
        self.buf[self.n] = b;
        self.n += 1;
        if self.n == 3 {
            self.n = 0;
            if self.buf[0] & 0x08 == 0 {
                return None; // fuori fase
            }
            return Some(decodifica(self.buf));
        }
        None
    }
}
