//! acpi.rs — IL CORPO: scoperta delle tabelle ACPI (RSDP→XSDT/RSDT→FADT→DSDT)
//! e primi dati per l'**Embedded Controller (EC)**.
//!
//! Perché: la tastiera del Lenovo NON è su USB — è dietro l'EC, che consegna i
//! tasti all'8042 solo dopo l'init ACPI (diagnosi 12/10/2026: i8042 presente e
//! configurato bene — cfg 0xEA — ma zero scancode).
//!
//! Come: l'RSDP lo fornisce **Limine** (base revision 6 → indirizzo *virtuale*).
//! Le tabelle interne hanno indirizzi **fisici** → si leggono via **HHDM**.
//!
//! Sommari esposti:
//! - `summary()`    → [rsdp_ok, rev, n_tabelle, facp, dsdt_ok]
//! - `ec_summary()` → [pnp0c09_nel_dsdt, status_0x66, dsdt_kb]

use crate::HHDM_OFFSET;

static mut RSDP: u64 = 0;
static mut SUMMARY: [u64; 5] = [0, 0, 0, 0, 0];
static mut EC_SUM: [u64; 3] = [0, 0, 0];

pub fn set_rsdp(a: u64) {
    unsafe { RSDP = a; }
}
pub fn summary() -> [u64; 5] {
    unsafe { SUMMARY }
}
pub fn ec_summary() -> [u64; 3] {
    unsafe { EC_SUM }
}

#[inline]
fn hhdm(phys: u64) -> u64 {
    unsafe { HHDM_OFFSET.wrapping_add(phys) }
}

fn inb(port: u16) -> u8 {
    unsafe {
        let v: u8;
        core::arch::asm!("in al, dx", out("al") v, in("dx") port,
                         options(nomem, nostack, preserves_flags));
        v
    }
}

unsafe fn rd8(p: u64) -> u8 {
    core::ptr::read_volatile(p as *const u8)
}
unsafe fn rd32(p: u64) -> u32 {
    core::ptr::read_unaligned(p as *const u32)
}
unsafe fn rd64(p: u64) -> u64 {
    core::ptr::read_unaligned(p as *const u64)
}

fn sig(p: u64, s: &[u8]) -> bool {
    for (i, b) in s.iter().enumerate() {
        if unsafe { rd8(p + i as u64) } != *b {
            return false;
        }
    }
    true
}

/// Sonda non intrusiva: RSDP → tabella radice → FACP → DSDT. Cerca "PNP0C09"
/// nel DSDT (l'EC) e legge lo status dell'EC (0x66). Non scrive nulla.
pub fn probe() -> [u64; 5] {
    let mut sum = [0u64; 5];
    let mut ec = [0u64; 3];
    let rsdp0 = unsafe { RSDP };
    if rsdp0 == 0 {
        unsafe { SUMMARY = sum; EC_SUM = ec; }
        return sum;
    }

    // RSDP: virtuale (base rev ≥ 4) o fisico (rev 3). Prova entrambi.
    let rsdp = if sig(rsdp0, b"RSD PTR ") {
        rsdp0
    } else {
        let alt = hhdm(rsdp0);
        if sig(alt, b"RSD PTR ") {
            alt
        } else {
            unsafe { SUMMARY = sum; EC_SUM = ec; }
            return sum;
        }
    };
    sum[0] = 1;
    let revision = unsafe { rd8(rsdp + 15) };
    sum[1] = revision as u64;

    // Tabella radice: XSDT (rev ≥ 2) o RSDT.
    let (root_phys, entry_size, ntables) = if revision >= 2 {
        let xsdt = unsafe { rd64(rsdp + 24) };
        if xsdt == 0 {
            unsafe { SUMMARY = sum; EC_SUM = ec; }
            return sum;
        }
        let v = hhdm(xsdt);
        if !sig(v, b"XSDT") {
            unsafe { SUMMARY = sum; EC_SUM = ec; }
            return sum;
        }
        let len = unsafe { rd32(v + 4) } as u64;
        let n = if len > 36 { (len - 36) / 8 } else { 0 };
        (xsdt, 8u64, n)
    } else {
        let rsdt = unsafe { rd32(rsdp + 16) } as u64;
        if rsdt == 0 {
            unsafe { SUMMARY = sum; EC_SUM = ec; }
            return sum;
        }
        let v = hhdm(rsdt);
        if !sig(v, b"RSDT") {
            unsafe { SUMMARY = sum; EC_SUM = ec; }
            return sum;
        }
        let len = unsafe { rd32(v + 4) } as u64;
        let n = if len > 36 { (len - 36) / 4 } else { 0 };
        (rsdt, 4u64, n)
    };
    sum[2] = ntables;

    // FACP (FADT) → DSDT (offset 40, u32, fisico).
    let root_v = hhdm(root_phys);
    for i in 0..ntables.min(64) {
        let e_phys = if entry_size == 8 {
            unsafe { rd64(root_v + 36 + i * 8) }
        } else {
            unsafe { rd32(root_v + 36 + i * 4) as u64 }
        };
        if e_phys == 0 {
            continue;
        }
        let e_v = hhdm(e_phys);
        if sig(e_v, b"FACP") {
            sum[3] = 1;
            let dsdt_phys = unsafe { rd32(e_v + 40) } as u64;
            if dsdt_phys != 0 && sig(hhdm(dsdt_phys), b"DSDT") {
                sum[4] = 1;
                let dsdt_v = hhdm(dsdt_phys);
                let dlen = unsafe { rd32(dsdt_v + 4) } as u64;
                ec[2] = (dlen / 1024).min(0xFFFF);
                // Cerca "PNP0C09" (EC) nel DSDT — blob, non AML interpretato.
                let lim = dlen.min(256 * 1024);
                let mut off = 0u64;
                while off + 7 <= lim {
                    if sig(dsdt_v + off, b"PNP0C09") {
                        ec[0] = 1;
                        break;
                    }
                    off += 1;
                }
            }
        }
    }

    // Status dell'EC (porta 0x66). 0xFF = non risponde.
    ec[1] = inb(0x66) as u64;

    unsafe { SUMMARY = sum; EC_SUM = ec; }
    sum
}
