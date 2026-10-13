// ── Banco host per l'udito: dall'onda alla sensazione ─────────────────────
//
// L'energia del suono deve essere una misura che NON mente: silenzio = 0,
// fondo scala = 1, e il segno non conta (l'orecchio non sente "meno forte"
// una pressione negativa). Questo banco copre il codice REALE
// (`#[path]` su src/dsp.rs), non una copia.
//
// Compilazione: make test   (oppure rustc --test bench/hda_host.rs)

#[path = "../src/dsp.rs"]
mod dsp;

use dsp::{energia, picco};

#[test]
fn silenzio_e_zero() {
    assert_eq!(energia(&[0i16; 64]), 0.0);
    assert_eq!(picco(&[0i16; 64]), 0.0);
}

#[test]
fn blocco_vuoto_e_zero() {
    assert_eq!(energia(&[]), 0.0);
}

#[test]
fn fondo_scala_positivo_e_uno() {
    assert_eq!(energia(&[32767i16; 32]), (32767.0 / 32768.0));
    assert_eq!(picco(&[32767i16; 32]), (32767.0 / 32768.0));
}

#[test]
fn il_segno_non_conta() {
    // L'orecchio sente intensità, non polarità.
    let a = energia(&[20000i16; 16]);
    let b = energia(&[-20000i16; 16]);
    assert_eq!(a, b);
    assert_eq!(picco(&[-20000i16]), picco(&[20000i16]));
}

#[test]
fn meta_fondo_scala_e_meta() {
    let e = energia(&[16384i16; 10]);
    assert!((e - 0.5).abs() < 1e-6, "atteso 0.5, ottenuto {}", e);
}

#[test]
fn piu_forte_e_maggiore() {
    let piano = energia(&[1000i16; 100]);
    let forte = energia(&[20000i16; 100]);
    assert!(forte > piano);
}

#[test]
fn energia_e_picco_distinguono_diffuso_da_picco() {
    // Stessa energia media, ma il picco distingue i due segnali.
    let diffuso = [1000i16, -1000, 1000, -1000];
    let piccato = [0i16, 2000, 0, -2000];
    assert_eq!(energia(&diffuso), energia(&piccato));
    assert!(picco(&piccato) > picco(&diffuso));
}

#[test]
fn valore_massimo_negativo() {
    // i16::MIN: il valore assoluto non deve andare in overflow.
    let e = energia(&[i16::MIN]);
    assert!(e > 0.999, "atteso ~1.0, ottenuto {}", e);
}
