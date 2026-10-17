// ── intero.rs — INTEROCEZIONE: i sensi del SÉ ─────────────────────────────
//
// "Temperatura, ventola, memoria — come le gestiamo?" (Alfonso, 15/10/2026).
// La temperatura della CPU non è un senso del mondo: è un **senso del sé** —
// la famiglia `Intero` del sistema dei sensi. E come l'omeostasi, è un pezzo
// che **deve funzionare anche senza l'intelletto** (§7).
//
// DUE VIE, per vendor (leggere dove il registro ESISTE, non indovinare):
//  · INTEL — MSR IA32_TEMPERATURE_TARGET (0x1A2) dà TjMax; IA32_THERM_STATUS
//    (0x19C) dà la lettura (gradi SOTTO TjMax, bit 22:16) e un bit di validità
//    (bit 31).
//  · AMD (famiglia 15h/16h, es. l'A8-7410 del Lenovo) — la **Tctl** NON si legge
//    sempre allo stesso modo: dipende dalla famiglia (fonte: Linux k10temp.c).
//    L'A8-7410 è F16h (M30h): il registro "Reported Temperature Control" è
//    **D18F3x[A4]** in PCI config space sul northbridge **function 3**, non via
//    SMN. Solo Zen (F17h+, SMN 0x00059800) e F15h M60h/M70h (SMN 0xD8200CA4)
//    usano l'SMN (indiretto dal root complex 00:00.0: index 0xB8, data 0xBC).
//    Decodifica comune: `temp = (raw >> 21) * 0.125`. Occhio: 0x00059800 su un
//    F16h legge 0 (era il `raw=0` visto a schermo sul metallo).
//
// ⚠️ Onestà (Alfonso, 15/10): *"magari non funziona in QEMU e funziona sul
// metallo"*. In QEMU né gli MSR né l'SMN sono emulati → "nessun sensore".
// Non mentiamo: se non c'è, la lettura dice che non c'è.
//
// ⚠️ Storico (14/10/2026): leggere gli MSR termici Intel su una CPU AMD →
// `rdmsr` → **#GP → blocco all'avvio**. In QEMU non si vedeva. Quindi: ogni
// lettura è GUARDATA dal vendor. La DECODIFICA è PURA → testabile su host.

use core::arch::asm;
use crate::pci;

const MSR_THERM_TARGET: u32 = 0x1A2; // IA32_TEMPERATURE_TARGET (TjMax) — SOLO Intel
const MSR_THERM_STATUS: u32 = 0x19C; // IA32_THERM_STATUS — SOLO Intel

// ── AMD: accesso SMN via root complex PCI 00:00.0 (index/data) ─────────────
const AMD_SMN_INDEX: u8 = 0xB8; // si scrive qui l'indirizzo SMN da leggere
const AMD_SMN_DATA: u8 = 0xBC;  // si legge qui il valore

// ⚠️ SCOPERTA (17/10/2026): gli indirizzi NON sono gli stessi per tutte le
// famiglie. Il vecchio codice usava 0x00059800 su ogni AMD: ma 0x00059800 è il
// "Reported Temperature Control" del **Zen (F17h+)** — su un F16h restituisce 0
// (è il `raw=0x00000000` visto a schermo). Fonte: Linux `drivers/hwmon/k10temp.c`.
//  · F17h+ (Zen):            SMN 0x00059800
//  · F15h M60h/M70h:         SMN 0xD8200CA4 (D0F0xBC_xD820_0CA4)
//  · F10h..F16h (l'A8-7410): PCI config 0xA4 del northbridge **function 3**
//                            (D18F3x[A4]) — NON SMN.
const AMD_SMN_ZEN_TEMP: u32 = 0x0005_9800; // Zen (F17h+)
const AMD_SMN_F15H_M60_TEMP: u32 = 0xd820_0ca4; // F15h M60h/M70h

// Percorso F10h..F16h: il northbridge function 3 (D18F3) porta il registro
// "Reported Temperature Control" a offset 0xA4. Gli ID device vengono da Linux
// `include/linux/pci_ids.h` / `k10temp_id_table` (verificati, non inventati).
const AMD_VENDOR: u16 = 0x1022;
const NB_F3_16H_M30H: u16 = 0x1583; // F16h M30h-3Fh — l'A8-7410
const NB_F3_16H: u16 = 0x1533;      // F16h M00-0Fh
const NB_F3_15H_M30H: u16 = 0x141d; // F15h M30h-3Fh
const PCI_REPORTED_TEMP: u8 = 0xA4; // D18F3x[A4]

// Rotte di lettura (per la diagnostica in GUI: quale registro si è letto).
pub const ROTTA_NESSUNA: u8 = 0;
pub const ROTTA_PCI_F3_A4: u8 = 1;      // F10h..F16h — PCI config 0xA4 del NB F3
pub const ROTTA_SMN_F15H_M60: u8 = 2;   // F15h M60h/M70h — SMN 0xD8200CA4
pub const ROTTA_SMN_ZEN: u8 = 3;        // F17h+ (Zen) — SMN 0x00059800

/// CPUID. Ritorna (eax, ebx, ecx, edx). `ebx` è riservato a LLVM: si salva,
/// si esegue cpuid, si copia ebx, si ripristina.
fn cpuid(eax_in: u32) -> (u32, u32, u32, u32) {
    let (eax, ebx, ecx, edx): (u32, u32, u32, u32);
    unsafe {
        asm!(
            "push rbx",
            "cpuid",
            "mov {ebx:e}, ebx",
            "pop rbx",
            ebx = out(reg) ebx,
            inout("eax") eax_in => eax,
            out("ecx") ecx,
            out("edx") edx,
        );
    }
    (eax, ebx, ecx, edx)
}

/// CPUID leaf 0: vendor del processore. "GenuineIntel" → true.
fn vendor_intel() -> bool {
    let (_, ebx, ecx, edx) = cpuid(0);
    // "GenuineIntel" = EBX "Genu" · EDX "ineI" · ECX "ntel"
    ebx == 0x756e_6547 && edx == 0x4965_6e69 && ecx == 0x6c65_746e
}

/// CPUID leaf 0: vendor del processore. "AuthenticAMD" → true.
fn vendor_amd() -> bool {
    let (_, ebx, ecx, edx) = cpuid(0);
    // "AuthenticAMD" = EBX "Auth" · EDX "enti" · ECX "cAMD"
    ebx == 0x6874_7541 && edx == 0x6974_6e65 && ecx == 0x444d_4163
}

/// Decodifica PURA della famiglia CPU da CPUID leaf 1 (EAX).
/// Famiglia = base (bit 11:8); se base == 0xF si somma l'estesa (bit 27:20).
/// (Intel 6 → 6; AMD F16h → base 0xF + ext 1 = 0x10; AMD F17h → 0x11.)
pub fn family_from_eax(eax: u32) -> u32 {
    let base = (eax >> 8) & 0xF;
    if base == 0xF {
        base + ((eax >> 20) & 0xFF)
    } else {
        base
    }
}

/// Decodifica PURA del modello CPU da CPUID leaf 1 (EAX).
/// Model = (ext_model << 4) | base_model (bit 19:16 esteso, bit 7:4 base).
/// (A8-7410 → EAX 0x00730F01 → famiglia 16h, modello 0x30.)
pub fn model_from_eax(eax: u32) -> u32 {
    let base = (eax >> 4) & 0xF;
    let ext = (eax >> 16) & 0xF;
    (ext << 4) | base
}

/// La famiglia della CPU su cui gira Exo.
fn cpu_family() -> u32 {
    family_from_eax(cpuid(1).0)
}

/// Il modello della CPU su cui gira Exo (serve a distinguere le sottofamiglie
/// AMD: F15h M60h/M70h usano SMN, le altre F15h/F16h usano il PCI del NB F3).
fn cpu_model() -> u32 {
    model_from_eax(cpuid(1).0)
}

/// Decodifica PURA: TjMax + status → gradi Celsius (o None se non c'è sensore).
/// IA32_THERM_STATUS bit 31 = lettura valida; bit 22:16 = gradi sotto TjMax.
/// IA32_TEMPERATURE_TARGET bit 23:16 = TjMax.
pub fn temperatura_da(tjmax: u64, status: u64) -> Option<f32> {
    if status == 0 || status == u64::MAX {
        return None; // MSR non emulato / niente sensore
    }
    if status & (1 << 31) == 0 {
        return None; // lettura non valida
    }
    let tj = ((tjmax >> 16) & 0xFF) as f32;
    if tj <= 0.0 {
        return None; // TjMax ignoto
    }
    let delta = ((status >> 16) & 0x7F) as f32; // gradi sotto TjMax
    Some(tj - delta)
}

/// Decodifica PURA del registro AMD SMN THM_TCON_CUR_TMP (Tctl).
/// `temp = (raw >> 21) * 0.125` °C. Guardie di onestà: un registro assente
/// (0 o tutti-1) non è una temperatura, e una temperatura fuori da un range
/// fisico plausibile non è credibile → None.
pub fn tctl_da_raw(raw: u32) -> Option<f32> {
    if raw == 0 || raw == u32::MAX {
        return None; // sensore assente / SMN non emulato
    }
    let t = ((raw >> 21) as f32) * 0.125;
    if t <= 0.0 || t > 150.0 {
        return None;
    }
    Some(t)
}

/// Legge un MSR (Model-Specific Register) — SOLO su Intel.
pub fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi,
             options(nomem, nostack, preserves_flags));
    }
    ((hi as u64) << 32) | lo as u64
}

/// Legge un registro SMN di AMD: indirizzo nell'index, valore nel data.
/// Accesso in PCI config space sul root complex 00:00.0 (nessun #GP).
fn smn_read(addr: u32) -> u32 {
    pci::pci_config_write(0, 0, 0, AMD_SMN_INDEX, addr);
    pci::pci_config_read(0, 0, 0, AMD_SMN_DATA)
}

/// Legge il "Reported Temperature Control" (D18F3x[A4]) del northbridge
/// **function 3** via PCI config space — la via corretta per F10h..F16h
/// (l'A8-7410). Prova gli ID device noti (più specifico prima); ritorna
/// `(valore_grezzo, id_device_trovato)` — `(0, 0)` se nessuno è presente.
fn temp_pci_nb_f3() -> (u32, u16) {
    for did in [NB_F3_16H_M30H, NB_F3_16H, NB_F3_15H_M30H] {
        if let Some((b, s, f)) = pci::pci_find_device(AMD_VENDOR, did) {
            return (pci::pci_config_read(b, s, f, PCI_REPORTED_TEMP), did);
        }
    }
    (0, 0)
}

/// Temperatura su AMD, per famiglia (vedi costanti). Ritorna
/// `(rotta, grezzo, candidato, temp, nb_id)`. Il `candidato` è la lettura del
/// vecchio indirizzo SMN (Zen): serve solo alla diagnostica in GUI — NON sceglie.
fn temperatura_amd() -> (u8, u32, u32, Option<f32>, u16) {
    let fam = cpu_family();
    let model = cpu_model();
    if fam == 0x15 && ((model & 0xF0) == 0x60 || (model & 0xF0) == 0x70) {
        // F15h M60h/M70h: SMN D0F0xBC_xD8200CA4.
        let raw = smn_read(AMD_SMN_F15H_M60_TEMP);
        (ROTTA_SMN_F15H_M60, raw, 0, tctl_da_raw(raw), 0)
    } else if fam >= 0x17 {
        // Zen (F17h+): SMN 0x00059800.
        let raw = smn_read(AMD_SMN_ZEN_TEMP);
        (ROTTA_SMN_ZEN, raw, 0, tctl_da_raw(raw), 0)
    } else {
        // F10h..F16h: PCI config 0xA4 del northbridge function 3 (NON SMN).
        let (raw, nb) = temp_pci_nb_f3();
        // Candidato diagnostico: il vecchio indirizzo SMN (Zen). Sul metallo
        // leggerà 0 — così in GUI si VEDE che SMN non c'entra.
        let cand = smn_read(AMD_SMN_ZEN_TEMP);
        (ROTTA_PCI_F3_A4, raw, cand, tctl_da_raw(raw), nb)
    }
}

// ── Diagnostica per la GUI ─────────────────────────────────────────────────
// Il Lenovo non ha seriale: se la probe mente, va VISTO a schermo. Qui si
// registra quale via/rotta ha preso l'ultima `temperatura()`, il valore grezzo
// letto e — per AMD — un secondo candidato (l'SMN vecchio) e l'id del
// northbridge trovato. Nessuna di queste è una temperatura: sono prove.
pub const VIA_NESSUNA: u8 = 0;
pub const VIA_INTEL: u8 = 1;
pub const VIA_AMD: u8 = 2;

#[derive(Clone, Copy)]
pub struct Diagnostica {
    pub via: u8,           // VIA_*
    pub rotta: u8,         // ROTTA_* (quale registro si è letto)
    pub raw: u32,          // valore grezzo della rotta scelta
    pub cand: u32,         // AMD: valore grezzo del candidato (SMN vecchio)
    pub nb: u16,           // AMD PCI: id device del northbridge F3 (0 = nessuno)
    pub temp: Option<f32>, // ultima temperatura decodificata (None = nessuna)
}

static mut DIAG: Diagnostica =
    Diagnostica { via: VIA_NESSUNA, rotta: ROTTA_NESSUNA, raw: 0, cand: 0, nb: 0, temp: None };

/// Diagnostica dell'ultima `temperatura()` (aggiornata ad ogni chiamata).
pub fn diagnostica() -> Diagnostica { unsafe { DIAG } }

/// Nome della via per la GUI: `intel` / `amd` / `nessuno`.
pub fn nome_via(via: u8) -> &'static str {
    match via {
        VIA_INTEL => "intel",
        VIA_AMD => "amd",
        _ => "nessuno",
    }
}

/// Nome della rotta (registro) per la GUI: `pci` / `smn` / `""`.
pub fn nome_rotta(rotta: u8) -> &'static str {
    match rotta {
        ROTTA_PCI_F3_A4 => "pci",
        ROTTA_SMN_F15H_M60 | ROTTA_SMN_ZEN => "smn",
        _ => "",
    }
}

/// La temperatura del corpo, se c'è un sensore. None = non c'è (onesto).
/// ⚠️ Legge gli MSR SOLO su Intel (altrove non esistono → #GP); su AMD usa la
/// rotta della famiglia (SMN o PCI del NB). Aggiorna la `diagnostica()` in GUI.
pub fn temperatura() -> Option<f32> {
    let d = if vendor_intel() {
        let t = temperatura_da(rdmsr(MSR_THERM_TARGET), rdmsr(MSR_THERM_STATUS));
        Diagnostica { via: VIA_INTEL, rotta: ROTTA_NESSUNA, raw: 0, cand: 0, nb: 0, temp: t }
    } else if vendor_amd() {
        let (rotta, raw, cand, t, nb) = temperatura_amd();
        Diagnostica { via: VIA_AMD, rotta, raw, cand, nb, temp: t }
    } else {
        Diagnostica { via: VIA_NESSUNA, rotta: ROTTA_NESSUNA, raw: 0, cand: 0, nb: 0, temp: None }
    };
    unsafe { DIAG = d; }
    d.temp
}
