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
