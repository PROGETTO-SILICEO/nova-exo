// ── Utilità esadecimali per le comandi seriali ──
//
// Modulo puro, senza dipendenze, `no_std` compatibile. Vive in `src/`
// perché lo usa il kernel, ed è incluso direttamente dal banco host
// `bench/hex_host.rs` via `#[path]`: un solo codice, due consumatori.
//
// Nato dal bug del 04/10/2026: `INJECT_SENSE 0xDEADBEEF` rispondeva
// `SENS:INJECT@0x0000000000000000`. Il parser inline in `main.rs`
// accettava solo cifre nude, quindi sul byte `x` usciva in `break` e
// restituiva 0 — un indirizzo valido a schermo, ma falso. Peggio:
// nessun errore. La regola che nasce qui è che un comando di prova
// non deve mai mentire sul proprio argomento.

/// Valore esadecimale di un singolo byte ASCII, se è una cifra hex.
#[inline]
fn hex_val(b: u8) -> Option<u64> {
    match b {
        b'0'..=b'9' => Some((b - b'0') as u64),
        b'a'..=b'f' => Some((b - b'a' + 10) as u64),
        b'A'..=b'F' => Some((b - b'A' + 10) as u64),
        _ => None,
    }
}

/// Interpreta `args` come un intero esadecimale a 64 bit.
///
/// Accetta il prefisso `0x`/`0X` (o meno), salta gli spazi iniziali e
/// rifiuta esplicitamente tutto ciò che non è esadecimale puro.
///
/// Restituisce `None` — invece di un valore troncato — quando:
/// - la stringa è vuota o solo spazi;
/// - il prefisso `0x` non è seguito da alcuna cifra;
/// - compare un carattere non esadecimale;
/// - le cifre eccedono i 64 bit disponibili.
///
/// `None` significa "non capito": il chiamante deve rifiutare il comando
/// invece di iniettare una lettura arbitraria.
pub fn parse_hex64(args: &[u8]) -> Option<u64> {
    let mut i = 0;

    // Spazi iniziali.
    while i < args.len() && args[i].is_ascii_whitespace() {
        i += 1;
    }

    // Prefisso opzionale 0x / 0X.
    let prefissato = args.len() >= i + 2
        && args[i] == b'0'
        && (args[i + 1] == b'x' || args[i + 1] == b'X');
    if prefissato {
        i += 2;
    }

    let inizio = i;
    let mut acc: u64 = 0;
    while i < args.len() {
        let d = match hex_val(args[i]) {
            Some(d) => d,
            None => return None, // rumore o carattere illegale: rifiutiamo
        };
        acc = match acc.checked_mul(16).and_then(|v| v.checked_add(d)) {
            Some(v) => v,
            None => return None, // overflow: meglio nessuno che troncato
        };
        i += 1;
    }

    // Nessuna cifra consumata: stringa vuota, solo spazi, o `0x` da solo.
    if i == inizio {
        return None;
    }

    Some(acc)
}

/// Stampa `v` come 16 cifre esadecimali in maiuscolo, senza prefisso.
/// Restituisce il numero di cifre emesse.
pub fn write_hex64_to(v: u64, out: &mut [u8; 16]) -> usize {
    const CIFRE: &[u8; 16] = b"0123456789ABCDEF";
    for i in 0..16 {
        let shift = 60 - 4 * i;
        out[i] = CIFRE[((v >> shift) & 0xF) as usize];
    }
    16
}

/// Scrive le `digits` cifre esadecimali MINUSCOLE (senza prefisso) dei bit
/// bassi di `v` in `out`. Serve alla GUI, che usa lo stile `10ec:0233`.
/// `digits` viene limitato a 16 (64 bit) e a `out.len()`.
pub fn write_hex_digits(v: u64, digits: usize, out: &mut [u8]) -> usize {
    const CIFRE: &[u8; 16] = b"0123456789abcdef";
    let digits = digits.min(16);
    let n = digits.min(out.len());
    for i in 0..n {
        let shift = 4 * (digits - 1 - i);
        out[i] = CIFRE[((v >> shift) & 0xF) as usize];
    }
    n
}

/// Scrive `v` in esadecimale minuscolo SENZA zeri iniziali (`0` → "0").
/// È la forma compatta dei nodi del codec (`a=8`, `v=23`, `pin=1b`).
pub fn write_hex_min(v: u64, out: &mut [u8; 16]) -> usize {
    const CIFRE: &[u8; 16] = b"0123456789abcdef";
    let mut iniziato = false;
    let mut n = 0;
    for i in (0..16).rev() {
        let d = ((v >> (4 * i)) & 0xF) as usize;
        if d != 0 {
            iniziato = true;
        }
        if iniziato {
            out[n] = CIFRE[d];
            n += 1;
        }
    }
    if n == 0 {
        out[0] = b'0';
        n = 1;
    }
    n
}

/// Scrive `b` in decimale (1..3 cifre) e ritorna quante ne ha scritte.
fn write_dec_byte(b: u8, out: &mut [u8]) -> usize {
    let mut n = 0;
    if b >= 100 {
        out[n] = b'0' + b / 100;
        n += 1;
    }
    if b >= 10 {
        out[n] = b'0' + (b / 10) % 10;
        n += 1;
    }
    out[n] = b'0' + b % 10;
    n += 1;
    n
}

/// Le 4 cifre di PROBE (found/reset_ok/codec_ok/codec_addr) concatenate in
/// decimale — la forma compatta pensata per la GUI (`p=1110`). Ritorna i byte
/// scritti (max 12: quattro valori da 3 cifre).
pub fn write_probe_digits(probe: [u8; 4], out: &mut [u8; 12]) -> usize {
    let mut n = 0;
    for &b in &probe {
        n += write_dec_byte(b, &mut out[n..]);
    }
    n
}
