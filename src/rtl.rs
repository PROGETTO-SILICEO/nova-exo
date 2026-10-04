//! RTL810xE — Realtek Fast Ethernet (famiglia r8169). Primo step: probe.
//!
//! Riferimenti (04/10/2026): driver r8169 Rust no_std (chitti kernel),
//! wiki.osdev.org/RTL8169, Linux r8169_main.c.
//! Sull'IdeaPad 110-15ACL la scheda è a 02:00.0 (10ec:8136), dietro il
//! root port PCIe. QEMU NON emula questo chip: il banco di test è il Lenovo.
//!
//! Step 1 (questo file): la scheda risponde? → lettura MAC (offset 0x00)
//! + stato ChipCmd (0x37). Nessuna scrittura: solo letture, zero rischio.

use crate::pci;

pub struct Rtl {
    pub mmio: u64,
    pub mmio_phys: u64,
    pub bar_index: u8,
    pub mac: [u8; 6],
    pub chip_cmd: u8,
    pub intr_status: u16,
}

impl Rtl {
    /// Cerca la RTL810xE (10ec:8136), abilita bus master, trova il BAR
    /// memory, legge MAC + stato. Ritorna None se la scheda non c'è o il
    /// BAR non decodifica.
    pub unsafe fn probe() -> Option<Rtl> {
        let (bus, slot, func) = pci::pci_find_device(0x10ec, 0x8136)?;
        pci::enable_bus_master(bus, slot, func);

        // Primo BAR che decodifica come memory (le PCIe parts usano BAR2,
        // ma proviamo 2 → 1 → 0 come fa chitti).
        let mut mmio_phys: u64 = 0;
        let mut bar_index: u8 = 0;
        for idx in [2u8, 1, 0] {
            let bar = pci::pci_config_read(bus, slot, func, 0x10 + idx * 4);
            if bar & 0x1 == 0 && (bar & !0xF) != 0 {
                mmio_phys = (bar & !0xF) as u64;
                bar_index = idx;
                break;
            }
        }
        if mmio_phys == 0 {
            return None;
        }

        let mmio = crate::paging::mmio_virt_addr(mmio_phys);

        // MAC address: 6 byte a offset 0x00 (IDR0..IDR5)
        let mut mac = [0u8; 6];
        for (i, m) in mac.iter_mut().enumerate() {
            *m = core::ptr::read_volatile((mmio + i as u64) as *const u8);
        }

        // Stato: ChipCmd (0x37, u8) e IntrStatus (0x3e, u16)
        let chip_cmd = core::ptr::read_volatile((mmio + 0x37) as *const u8);
        let intr_status = core::ptr::read_volatile((mmio + 0x3e) as *const u16);

        Some(Rtl {
            mmio,
            mmio_phys,
            bar_index,
            mac,
            chip_cmd,
            intr_status,
        })
    }

    /// Il MAC è plausibile? (non tutti 0x00, non tutti 0xFF)
    pub fn mac_plausibile(&self) -> bool {
        let all_zero = self.mac.iter().all(|&b| b == 0x00);
        let all_ff = self.mac.iter().all(|&b| b == 0xFF);
        !all_zero && !all_ff
    }
}
