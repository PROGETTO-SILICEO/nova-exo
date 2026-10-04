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

/// Mappa DINAMICAMENTE la pagina 2MB che contiene `phys` nella zona MMIO
/// virtuale (PML3[3] → copre i phys 0xC0000000..0xFFFFFFFF).
/// Fix 04/10/2026: il mapping di init() è fisso (5 zone di QEMU); i BAR
/// reali dei device stanno altrove → serviva il mapping a runtime.
/// Ritorna false se `phys` è fuori dal range coperto o le tabelle mancano.
pub fn map_mmio_page(phys: u64) -> bool {
    unsafe {
        if !(0xC000_0000..=0xFFFF_FFFF).contains(&phys) {
            return false;
        }
        let hhdm = crate::HHDM_OFFSET;
        if hhdm == 0 {
            return false;
        }
        let cr3: u64;
        asm!("mov {}, cr3", out(reg) cr3);
        let pml4_phys = cr3 & 0xFFFF_FFFF_FFFF_F000;
        let pml4_virt = hhdm + pml4_phys;

        // PML4[508] → PML3
        let pml3_entry =
            core::ptr::read_volatile((pml4_virt + MMIO_PML4_IDX * 8) as *const u64);
        if pml3_entry & 0x1 == 0 {
            return false;
        }
        let pml3_phys = pml3_entry & 0xFFFF_FFFF_FFFF_F000;

        // PML3[3] → PML2
        let pml2_entry = core::ptr::read_volatile((hhdm + pml3_phys + 3 * 8) as *const u64);
        if pml2_entry & 0x1 == 0 {
            return false;
        }
        let pml2_phys = pml2_entry & 0xFFFF_FFFF_FFFF_F000;

        // Entry per la pagina 2MB: allinea phys a 2MB, flags 0x9B (P|RW|UC)
        let page = phys & !0x1F_FFFF;
        let idx = (phys >> 21) & 0x1FF;
        core::ptr::write_volatile(
            (hhdm + pml2_phys + idx * 8) as *mut u64,
            page | 0x9B,
        );
        // Invalida la TLB per quell'indirizzo virtuale + fence
        let virt = mmio_virt_addr(page);
        asm!("invlpg [{}]", in(reg) virt);
        asm!("sfence");
        true
    }
}

pub fn init() {    unsafe {
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

        // PML2[8] -> 2MB MMIO page at 0xC1000000 (NIC BAR0, covers 0xC1080000)
        write_volatile((hhdm + pml2_page + 8 * 8) as *mut u64, 0xC100_0000u64 | 0x9B);
        // PML2[501] -> 2MB MMIO page at 0xFEA00000 (MSI)
        write_volatile((hhdm + pml2_page + 501 * 8) as *mut u64, 0xFEA0_0000u64 | 0x9B);
        // PML2[502] -> 2MB (IOAPIC at 0xFEC00000)
        write_volatile((hhdm + pml2_page + 502 * 8) as *mut u64, 0xFEC0_0000u64 | 0x9B);
        // PML2[503] -> 2MB (Local APIC at 0xFEE00000)
        write_volatile((hhdm + pml2_page + 503 * 8) as *mut u64, 0xFEE0_0000u64 | 0x9B);
        // PML2[488] -> 2MB (Framebuffer VBE at 0xFD000000)
        // 488*2MB = 0x3D000000 → 0xC0000000 + 0x3D000000 = 0xFD000000 ✓
        write_volatile((hhdm + pml2_page + 488 * 8) as *mut u64, 0xFD00_0000u64 | 0x9B);

        // Full TLB flush: reload CR3
        asm!("sfence");
        asm!("mov cr3, {}", in(reg) cr3);
    }
}