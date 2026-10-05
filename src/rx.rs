// ── La porta d'ingresso di rete, condivisa fra i driver NIC ──
//
// Perché esiste (05/10/2026): `main.rs` aveva il cablaggio "pacchetto
// ricevuto → chemio_input", ma legato a `e1000` (Intel). Sul 110-15ACL la
// NIC è una Realtek RTL810xE, che non ha RX: quindi sul metallo la cella
// `chemio` resta a zero perché **non entra mai niente** (verificato: 452
// campioni, chemio tutta zero, tatto ampiezza 0.000000).
//
// Questa porta separa il "chi riceve" dal "chi consuma": entrambi i driver
// ci scrivono, `main.rs` legge solo da qui. La porta Intel resta aperta —
// non è un rimpiazzo, è un ingresso in più.
//
// `DATA` è 64 byte come il buffer storico: il consumatore legge i primi 4
// byte del payload (offset 14, dopo l'header Ethernet) e ne fa `chemio_input`.

/// Numero di byte utili in `DATA`. Il consumatore ne usa solo 4.
pub const CAP: usize = 64;

pub static mut PENDING: bool = false;
pub static mut DATA: [u8; CAP] = [0; CAP];
pub static mut LEN: u16 = 0;

// ── Diagnostica: è ciò che rende visibile l'RX sul metallo, dove la
//    seriale non esiste. Viaggia dentro il pacchetto di telemetria UDP.
pub static mut PKTS: u32 = 0; // pacchetti ricevuti e consegnati
pub static mut ERRS: u32 = 0; // errori del driver
pub static mut LAST_LEN: u16 = 0; // lunghezza dell'ultimo pacchetto

/// Consegna un pacchetto alla porta. Copia al massimo `CAP` byte.
/// Se un pacchetto è già in attesa non consumato, il nuovo lo sostituisce:
/// per un canale sensoriale va bene perdere il vecchio, non accodare.
pub fn feed(len: u16, src: &[u8]) {
    let n = (len as usize).min(CAP).min(src.len());
    unsafe {
        let d = &mut *(&raw mut DATA);
        for j in 0..n {
            d[j] = src[j];
        }
        LEN = len;
        PENDING = true;
        PKTS = PKTS.wrapping_add(1);
        LAST_LEN = len;
    }
}

/// Estrae i 4 valori di `chemio_input` da un pacchetto: i primi 4 byte del
/// payload (offset 14, dopo l'header Ethernet), normalizzati su 255.
///
/// PURA e senza stato: è la parte che si può testare su host senza litigare
/// con i globali (i test girano in parallelo).
pub fn payload_to_chemio(len: u16, data: &[u8; CAP]) -> [f32; 4] {
    let mut out = [0.0f32; 4];
    for j in 0..4 {
        let idx = 14 + j;
        if (idx as u16) < len && idx < CAP {
            out[j] = data[idx] as f32 / 255.0;
        }
    }
    out
}

/// Consuma il pacchetto in attesa: ritorna `(len, chemio_input)`.
/// `None` se non c'è niente. Azzera `PENDING`.
pub fn take() -> Option<(u16, [f32; 4])> {
    unsafe {
        if !PENDING {
            return None;
        }
        PENDING = false;
        let len = LEN;
        let data = &*(&raw const DATA);
        Some((len, payload_to_chemio(len, data)))
    }
}

/// Registra un errore del driver.
pub fn err() {
    unsafe {
        ERRS = ERRS.wrapping_add(1);
    }
}
