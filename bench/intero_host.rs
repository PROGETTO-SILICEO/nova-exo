// ── Banco host per l'INTEROCEZIONE (decodifica della temperatura) ──
//
// La decodifica degli MSR termici è pura: si verifica su host col codice REALE
// (#[path] su src/intero.rs). Sbagliare i bit significa leggere una temperatura
// inventata — e un corpo che crede di avere la febbre quando non ce l'ha.
//
// Compilazione: make test   (oppure rustc --test bench/intero_host.rs)

#[path = "../src/intero.rs"]
mod intero;

use intero::temperatura_da;

#[test]
fn senza_sensore_e_none() {
    // MSR non emulati (tipico in QEMU): 0 o tutti 1 → "non c'è sensore".
    assert_eq!(temperatura_da(0, 0), None);
    assert_eq!(temperatura_da(0, u64::MAX), None);
    assert_eq!(temperatura_da(100 << 16, u64::MAX), None);
}

#[test]
fn lettura_non_valida_e_none() {
    // bit 31 = 0 → la lettura non è valida.
    assert_eq!(temperatura_da(100 << 16, 30 << 16), None);
}

#[test]
fn a_tjmax_la_temperatura_e_tjmax() {
    // delta = 0 → siamo a TjMax.
    assert_eq!(temperatura_da(100 << 16, (1 << 31) | (0 << 16)), Some(100.0));
}

#[test]
fn decodifica_delta_corretta() {
    // TjMax 100, 30 gradi sotto → 70 °C.
    assert_eq!(temperatura_da(100 << 16, (1 << 31) | (30 << 16)), Some(70.0));
}

#[test]
fn tjmax_ignoto_e_none() {
    assert_eq!(temperatura_da(0, (1 << 31) | (10 << 16)), None);
}

#[test]
fn i_bit_bassi_non_inquinano() {
    // Solo i bit 22:16 contano per il delta; il resto non deve cambiare la lettura.
    let a = temperatura_da(100 << 16, (1 << 31) | (20 << 16));
    let b = temperatura_da(100 << 16, (1 << 31) | (20 << 16) | 0x7FFF);
    assert_eq!(a, b);
    assert_eq!(a, Some(80.0));
}
