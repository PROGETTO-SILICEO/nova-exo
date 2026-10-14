// ── intero.rs — INTEROCEZIONE: i sensi del SÉ ─────────────────────────────
//
// "Temperatura, ventola, memoria — come le gestiamo?" (Alfonso, 15/10/2026).
// La temperatura della CPU non è un senso del mondo: è un **senso del sé** —
// la famiglia `Intero` del sistema dei sensi. E come l'omeostasi, è un pezzo
// che **deve funzionare anche senza l'intelletto** (§7).
//
// Su Intel: MSR IA32_TEMPERATURE_TARGET (0x1A2) dà TjMax; IA32_THERM_STATUS
// (0x19C) dà la lettura digitale (gradi SOTTO TjMax, bit 22:16) e un bit di
// validità (bit 31). La DECODIFICA è PURA → testabile su host.
//
// ⚠️ Onestà (Alfonso, 15/10): *"magari non funziona in QEMU e funziona sul
// metallo"*. In QEMU questi MSR spesso non sono emulati → "nessun sensore".
// Non mentiamo: se non c'è, la lettura dice che non c'è.

use core::arch::asm;

const MSR_THERM_TARGET: u32 = 0x1A2; // IA32_TEMPERATURE_TARGET (TjMax)
const MSR_THERM_STATUS: u32 = 0x19C; // IA32_THERM_STATUS (lettura digitale)

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

/// Legge un MSR (Model-Specific Register).
pub fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi,
             options(nomem, nostack, preserves_flags));
    }
    ((hi as u64) << 32) | lo as u64
}

/// La temperatura del corpo, se c'è un sensore. None = non c'è (onesto).
pub fn temperatura() -> Option<f32> {
    temperatura_da(rdmsr(MSR_THERM_TARGET), rdmsr(MSR_THERM_STATUS))
}
