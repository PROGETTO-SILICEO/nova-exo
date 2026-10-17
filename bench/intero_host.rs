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
// (00:00.0 index/data 0xB8/0xBC, oppure il NB F3 a offset 0xA4). Nel banco host
// non c'è hardware: lo shim serve solo a COMPILARE; i test coprono la
// decodifica PURA (e model_from_eax).
mod pci {
    pub fn pci_config_write(_b: u8, _s: u8, _f: u8, _o: u8, _v: u32) {}
    pub fn pci_config_read(_b: u8, _s: u8, _f: u8, _o: u8) -> u32 { 0 }
    pub fn pci_find_device(_v: u16, _d: u16) -> Option<(u8, u8, u8)> { None }
}

use intero::{
    family_from_eax, model_from_eax, nome_rotta, nome_via, tctl_da_raw, temperatura_da, VIA_AMD,
    VIA_INTEL, VIA_NESSUNA, ROTTA_NESSUNA, ROTTA_PCI_F3_A4, ROTTA_SMN_F15H_M60, ROTTA_SMN_ZEN,
};

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

#[test]
fn modello_cpu_da_cpuid() {
    // Model = (ext_model << 4) | base_model (bit 19:16, bit 7:4).
    // A8-7410 reale: CPUID Fn1 EAX = 0x00730F01 → famiglia 16h, modello 0x30.
    let a8_7410 = 0x0073_0F01u32;
    assert_eq!(family_from_eax(a8_7410), 0x16);
    assert_eq!(model_from_eax(a8_7410), 0x30);
    // Intel: base model 5, ext 0 → restano 5.
    assert_eq!(model_from_eax((6u32 << 8) | (5u32 << 4) | 1), 5);
    // Un modello con esteso: ext 3, base 0 → 0x30.
    assert_eq!(model_from_eax((0xFu32 << 8) | (3u32 << 16)), 0x30);
}

#[test]
fn nome_rotta_per_la_gui() {
    // La GUI mostra QUALE registro è stato letto (pci vs smn): nomi stabili.
    assert_eq!(nome_rotta(ROTTA_PCI_F3_A4), "pci");
    assert_eq!(nome_rotta(ROTTA_SMN_F15H_M60), "smn");
    assert_eq!(nome_rotta(ROTTA_SMN_ZEN), "smn");
    assert_eq!(nome_rotta(ROTTA_NESSUNA), "");
    // Un valore sconosciuto non deve mentire: stringa vuota.
    assert_eq!(nome_rotta(99), "");
}
