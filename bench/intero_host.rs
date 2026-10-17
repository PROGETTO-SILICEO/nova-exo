// ── Banco host per l'INTEROCEZIONE (decodifica della temperatura) ──
//
// La decodifica dei sensori termici (Intel via MSR, AMD via SMN) è pura: si
// verifica su host col codice REALE (#[path] su src/intero.rs). Sbagliare i bit
// significa leggere una temperatura inventata — e un corpo che crede di avere
// la febbre quando non ce l'ha.
//
// Compilazione: make test   (oppure rustc --test bench/intero_host.rs)

#[path = "../src/intero.rs"]
mod intero;

// Shim del modulo PCI: intero.rs legge la temperatura AMD via config space
// (00:00.0, index/data 0xB8/0xBC). Nel banco host non c'è hardware: lo shim
// serve solo a COMPILARE; i test coprono la decodifica PURA.
mod pci {
    pub fn pci_config_write(_b: u8, _s: u8, _f: u8, _o: u8, _v: u32) {}
    pub fn pci_config_read(_b: u8, _s: u8, _f: u8, _o: u8) -> u32 { 0 }
}

use intero::{family_from_eax, nome_via, tctl_da_raw, temperatura_da, VIA_AMD, VIA_INTEL, VIA_NESSUNA};

// ── Intel (MSR) ────────────────────────────────────────────────────────────

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

// ── AMD (SMN) ──────────────────────────────────────────────────────────────

#[test]
fn amd_sensore_assente_e_none() {
    // SMN non emulato / registro assente: 0 o tutti-1 → None (non si mente).
    assert_eq!(tctl_da_raw(0), None);
    assert_eq!(tctl_da_raw(u32::MAX), None);
}

#[test]
fn amd_decodifica_tctl() {
    // temp = (raw >> 21) * 0.125 → 50 °C = 400 << 21; 100 °C = 800 << 21.
    assert_eq!(tctl_da_raw(400 << 21), Some(50.0));
    assert_eq!(tctl_da_raw(800 << 21), Some(100.0));
    // Il valore minimo plausibile (1/8 di grado) non è zero.
    assert_eq!(tctl_da_raw(1 << 21), Some(0.125));
}

#[test]
fn amd_temperatura_fuori_range_e_none() {
    // 1201/8 = 150.125 °C > 150 → non credibile.
    assert_eq!(tctl_da_raw(1201 << 21), None);
    // I bit bassi (diversi dal campo temperatura) non devono inquinare.
    assert_eq!(tctl_da_raw((400 << 21) | 0x1F_FFFF), Some(50.0));
}

#[test]
fn famiglia_intel_e_amd() {
    // Intel family 6 (base = 6 nei bit 11:8), model 5, stepping 1.
    let intel = (6u32 << 8) | (5u32 << 4) | 1;
    assert_eq!(family_from_eax(intel), 6);
    // Base 0xF senza estensione resta 15.
    let intel_f = 0xFu32 << 8;
    assert_eq!(family_from_eax(intel_f), 0xF);
    // AMD Jaguar A8-7410: family 16h = base 0xF + ext 1.
    let jaguar = (0xFu32 << 8) | (1u32 << 20);
    assert_eq!(family_from_eax(jaguar), 0x10);
    // AMD Zen: family 17h = base 0xF + ext 2.
    let zen = (0xFu32 << 8) | (2u32 << 20);
    assert_eq!(family_from_eax(zen), 0x11);
}

#[test]
fn nome_via_per_la_gui() {
    // La GUI mostra quale via è stata presa: il nome deve essere stabile.
    assert_eq!(nome_via(VIA_INTEL), "intel");
    assert_eq!(nome_via(VIA_AMD), "amd");
    assert_eq!(nome_via(VIA_NESSUNA), "nessuno");
    // Un valore sconosciuto non deve mentire su una via: "nessuno".
    assert_eq!(nome_via(99), "nessuno");
}
