// ── Banco host per il parser esadecimale delle comandi seriali ──
//
// Perché esiste (04/10/2026): sul Lenovo ho lanciato
// `INJECT_SENSE 0xDEADBEEF` per verificare l'apprendimento dall'esito.
// La risposta è stata `SENS:INJECT@0x0000000000000000` — l'indirizzo
// era ZERO. Il parser inline in main.rs accettava solo cifre nude:
// sul primo byte `0` era felice, sul secondo byte `x` andava in break,
// restituendo 0 senza segnalare errore.
//
// Il test è sul codice REALE (`#[path]` su src/hexutil.rs), non su una
// copia: il bench di apprendimento è una copia e l'abbiamo già sentito
// come rischio di deriva.
//
// Compilazione: make test   (oppure rustc --test bench/hex_host.rs)

#[path = "../src/hexutil.rs"]
mod hexutil;

use hexutil::{parse_hex64, write_hex64_to};

/// Rende `v` come le 16 cifre che il kernel stampa su seriale.
fn hex16(v: u64) -> String {
    let mut buf = [0u8; 16];
    let n = write_hex64_to(v, &mut buf);
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

#[test]
fn senza_prefisso_lettura_normale() {
    assert_eq!(parse_hex64(b"DEADBEEF"), Some(0xDEADBEEF));
    assert_eq!(parse_hex64(b"deadbeef"), Some(0xDEADBEEF));
}

#[test]
fn con_prefisso_0x_minuscolo() {
    // Il caso che ha rotto il test sul Lenovo.
    assert_eq!(parse_hex64(b"0xDEADBEEF"), Some(0xDEADBEEF));
    assert_eq!(parse_hex64(b"0xdeadbeef"), Some(0xDEADBEEF));
}

#[test]
fn con_prefisso_zero_x_maiuscolo() {
    assert_eq!(parse_hex64(b"0XDEADBEEF"), Some(0xDEADBEEF));
}

#[test]
fn spazi_guidati_tollerati() {
    assert_eq!(parse_hex64(b"  0xDEADBEEF"), Some(0xDEADBEEF));
    assert_eq!(parse_hex64(b"\t0x10"), Some(0x10));
}

#[test]
fn prefisso_senza_cifre_e_errore() {
    // `0x` da solo non è un indirizzo: deve fallire, non valere 0.
    assert_eq!(parse_hex64(b"0x"), None);
    assert_eq!(parse_hex64(b"0X"), None);
}

#[test]
fn stringa_vuota_e_errore() {
    assert_eq!(parse_hex64(b""), None);
    assert_eq!(parse_hex64(b"   "), None);
}

#[test]
fn carattere_non_hex_e_errore() {
    assert_eq!(parse_hex64(b"DEADBEEG"), None);
    assert_eq!(parse_hex64(b"0xZZ"), None);
    assert_eq!(parse_hex64(b"hello"), None);
}

#[test]
fn cifre_maiuscole_e_minuscole_miscelate() {
    assert_eq!(parse_hex64(b"0xDeAdBeEf"), Some(0xDEADBEEF));
    assert_eq!(parse_hex64(b"aAbB"), Some(0xAABB));
}

#[test]
fn indirizzo_pieno_64bit() {
    assert_eq!(parse_hex64(b"0xFFFFFFFFFFFFFFFF"), Some(u64::MAX));
    assert_eq!(parse_hex64(b"0x00000000DEADBEEF"), Some(0x00000000DEADBEEF));
}

#[test]
fn overflow_saturato_a_none() {
    // 17 cifre esadecimali = 68 bit: non entra in u64. Non deve
    // tornare silenziosamente un numero troncato.
    assert_eq!(parse_hex64(b"0x1FFFFFFFFFFFFFFFF"), None);
}

#[test]
fn prefisso_0x_dopo_spazi_e_il_bug_originale() {
    // Regressione esatta del sintomo osservato: prima restituiva
    // Some(0), cioè iniettava sensazione all'indirizzo fisico zero.
    assert_ne!(parse_hex64(b"0xDEADBEEF"), Some(0));
    assert_ne!(parse_hex64(b"0xDEADBEEF"), None);
}

#[test]
fn stampa_sedici_cifre_maiuscole() {
    assert_eq!(hex16(0xDEADBEEF), "00000000DEADBEEF");
    assert_eq!(hex16(0), "0000000000000000");
    assert_eq!(hex16(u64::MAX), "FFFFFFFFFFFFFFFF");
}

#[test]
fn stampa_arrotonda_e_verifica() {
    // Il valore stampato deve tornare indietro identico: la risposta
    // seriale non può mentire su quello che è stato iniettato.
    for v in [0xDEADBEEFu64, 0x1_0000_0000u64, 0xF0F0_F0F0u64] {
        assert_eq!(parse_hex64(hex16(v).as_bytes()), Some(v));
    }
}
