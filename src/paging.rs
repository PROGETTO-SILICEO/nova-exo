use core::arch::asm;
use core::ptr::write_volatile;

/// Map a 2 MB MMIO region at a fixed virtual address (PML4[508]).
/// Uses two static 4 KB pages from kernel BSS as PML3 and PML2 tables.
/// After calling this, MMIO at `phys_base` (2 MB aligned) is accessible at
/// `mmio_virt_addr(phys_base)`.

#[repr(align(4096))]
#[allow(dead_code)]
struct Page4K([u8; 4096]);

static mut PML3_MMIO: Page4K = Page4K([0; 4096]);
static mut PML2_MMIO: Page4K = Page4K([0; 4096]);

/// PML4 index for our dedicated MMIO mapping
const MMIO_PML4_IDX: u64 = 508;

/// Returns the virtual address for a physical MMIO address.
/// phys must be in the range 3GB..4GB (PML3 index 3 within PML4[508]).
pub fn mmio_virt_addr(phys: u64) -> u64 {
    let pml4_base_raw = MMIO_PML4_IDX << 39;
    // Sign-extend from 48-bit to 64-bit canonical
    let pml4_base = if pml4_base_raw & (1u64 << 47) != 0 {
        pml4_base_raw | 0xFFFF_0000_0000_0000
    } else {
        pml4_base_raw
    };
    let pml3_3_base = pml4_base + (3u64 << 30);
    let offset_in_1gb = phys & 0x3FFF_FFFF;
    pml3_3_base + offset_in_1gb
}

pub fn init() {
    unsafe {
        let cr3: u64;
        asm!("mov {}, cr3", out(reg) cr3);
        let pml4_phys = cr3 & 0xFFFF_FFFF_FFFF_F000;

        let hhdm = crate::HHDM_OFFSET;
        if hhdm == 0 {
            return;
        }
        let pml4_virt = hhdm + pml4_phys;

        let pml3_page = crate::virt_to_phys(&raw const PML3_MMIO as *const _ as u64);
        let pml2_page = crate::virt_to_phys(&raw const PML2_MMIO as *const _ as u64);

        // Zero PML3 table
        for i in 0..512 {
            *((hhdm + pml3_page + i * 8) as *mut u64) = 0;
            *((hhdm + pml2_page + i * 8) as *mut u64) = 0;
        }
        asm!("sfence");

        // PML4[508] -> PML3 table
        write_volatile((pml4_virt + MMIO_PML4_IDX * 8) as *mut u64, pml3_page | 0x3);

        // PML3[3] -> PML2 table (covers 3GB..4GB)
        write_volatile((hhdm + pml3_page + 3 * 8) as *mut u64, pml2_page | 0x3);

        // PML2[501] -> 2MB MMIO page at 0xFEA00000 (uncacheable)
        write_volatile((hhdm + pml2_page + 501 * 8) as *mut u64, 0xFEA0_0000u64 | 0x87);
        // Next 2MB page (IOAPIC at 0xFEC00000)
        write_volatile((hhdm + pml2_page + 502 * 8) as *mut u64, 0xFEC0_0000u64 | 0x87);
        // Local APIC at 0xFEE00000
        write_volatile((hhdm + pml2_page + 503 * 8) as *mut u64, 0xFEE0_0000u64 | 0x87);

        // Full TLB flush: reload CR3
        asm!("sfence");
        asm!("mov cr3, {}", in(reg) cr3);
    }
}