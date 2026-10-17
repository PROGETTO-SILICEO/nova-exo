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
//  · AMD (famiglia 15h/16h, es. l'A8-7410 del Lenovo) — la **Tctl** si legge via
//    **SMN** (System Management Network), indiretto dal **root complex PCI
//    00:00.0**: si scrive l'indirizzo SMN nell'**index register** (0xB8) e si
//    legge il valore dal **data register** (0xBC). Registro THM_TCON_CUR_TMP =
//    0x00059800; decodifica `temp = (raw >> 21) * 0.125`. (KB-20261014-07.)
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
const AMD_SMN_THM_TCON_CUR_TMP: u32 = 0x0005_9800; // Tctl (famiglia 15h/16h)

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

/// La famiglia della CPU su cui gira Exo.
fn cpu_family() -> u32 {
    family_from_eax(cpuid(1).0)
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

/// Temperatura su AMD (famiglia 15h/16h) via SMN. Altre famiglie: non
/// implementate → None (onesto, non si indovina).
fn temperatura_amd() -> Option<f32> {
    match cpu_family() {
        0x15 | 0x16 => tctl_da_raw(smn_read(AMD_SMN_THM_TCON_CUR_TMP)),
        _ => None,
    }
}

/// La temperatura del corpo, se c'è un sensore. None = non c'è (onesto).
/// ⚠️ Legge gli MSR SOLO su Intel (altrove non esistono → #GP); su AMD usa SMN.
pub fn temperatura() -> Option<f32> {
    if vendor_intel() {
        return temperatura_da(rdmsr(MSR_THERM_TARGET), rdmsr(MSR_THERM_STATUS));
    }
    if vendor_amd() {
        return temperatura_amd();
    }
    None
}
