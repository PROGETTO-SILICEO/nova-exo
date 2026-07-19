use core::arch::asm;

// ── PCI config space access (legacy I/O ports 0xCF8/0xCFC) ──────
// Works on QEMU without ACPI/MCFG. Supports bus 0 only (single bus).

const PCI_CONFIG_ADDR: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

#[derive(Clone, Copy)]
#[repr(C)]
#[allow(dead_code)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub header_type: u8,
}

pub fn enumerate() {
    for slot in 0..32 {
        let vendor = pci_config_read(0, slot, 0, 0);
        if (vendor & 0xFFFF) == 0xFFFF {
            continue;
        }
        let class = pci_config_read(0, slot, 0, 8);
        let header = pci_config_read(0, slot, 0, 0x0C);

        let vendor_id = (vendor & 0xFFFF) as u16;
        let device_id = ((vendor >> 16) & 0xFFFF) as u16;
        let dev_class = ((class >> 24) & 0xFF) as u8;
        let dev_subclass = ((class >> 16) & 0xFF) as u8;
        let header_type = ((header >> 16) & 0xFF) as u8;

        print_device("PCI", slot, 0, vendor_id, device_id, dev_class, dev_subclass);

        // Multi-function device — scan functions 1-7
        if header_type & 0x80 != 0 {
            for func in 1..8 {
                let fv = pci_config_read(0, slot, func, 0);
                if (fv & 0xFFFF) != 0xFFFF {
                    print_device("PCI", slot, func,
                        (fv & 0xFFFF) as u16, ((fv >> 16) & 0xFFFF) as u16,
                        ((fv >> 24) & 0xFF) as u8, // class from function's own register
                        0);
                }
            }
        }
    }
}

pub fn pci_config_read(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let addr = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | (offset as u32 & 0xFC);
    unsafe {
        asm!("out dx, eax", in("dx") PCI_CONFIG_ADDR, in("eax") addr);
        let val: u32;
        asm!("in eax, dx", out("eax") val, in("dx") PCI_CONFIG_DATA);
        val
    }
}

pub fn pci_config_write(bus: u8, slot: u8, func: u8, offset: u8, val: u32) {
    let addr = (1u32 << 31)
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | (offset as u32 & 0xFC);
    unsafe {
        asm!("out dx, eax", in("dx") PCI_CONFIG_ADDR, in("eax") addr);
        asm!("out dx, eax", in("dx") PCI_CONFIG_DATA, in("eax") val);
    }
}

pub fn enable_bus_master(bus: u8, slot: u8, func: u8) {
    let cmd = pci_config_read(bus, slot, func, 0x04);
    pci_config_write(bus, slot, func, 0x04, cmd | 0x7);
}

/// Find a device by vendor:device ID. Returns (bus, slot, func) or None.
pub fn pci_find_device(vendor: u16, device: u16) -> Option<(u8, u8, u8)> {
    for slot in 0..32 {
        let vd = pci_config_read(0, slot, 0, 0);
        if (vd & 0xFFFF) as u16 == vendor && ((vd >> 16) & 0xFFFF) as u16 == device {
            return Some((0, slot as u8, 0));
        }
    }
    None
}

/// Read BARs of a device. Returns (bar0, bar_type, base_address_as_u64).
pub fn read_bars(bus: u8, slot: u8, func: u8) -> (u32, u32, u64) {
    let bar0 = pci_config_read(bus, slot, func, 0x10);
    let bar_type = bar0 & 0xF;
    let base = if bar_type & 0x6 == 0x4 {
        // 64-bit MMIO — BAR0 + BAR1 form a 64-bit address
        let bar1 = pci_config_read(bus, slot, func, 0x14);
        (bar0 & 0xFFFFFFF0) as u64 | ((bar1 as u64) << 32)
    } else if bar_type & 1 == 0 {
        // 32-bit MMIO
        (bar0 & 0xFFFFFFF0) as u64
    } else {
        // I/O space
        (bar0 & 0xFFFFFFFC) as u64
    };
    (bar0, bar_type, base)
}

/// Print BARs of a device.
pub fn print_bar(bus: u8, slot: u8, func: u8) {
    let (_raw, bar_type, base) = read_bars(bus, slot, func);
    let cmd = pci_config_read(bus, slot, func, 0x04);
    crate::write_str("NIC:BAR0=");
    crate::write_hex64(base);
    crate::write_str(" CMD=");
    crate::write_hex16((cmd & 0xFFFF) as u16);
    if bar_type & 1 == 0 {
        let is_64 = (bar_type & 0x6) == 0x4;
        crate::write_str(if is_64 { " MMIO64" } else { " MMIO32" });
    } else {
        crate::write_str(" I/O");
    }
    crate::serial_putc(b'\n');
}

/// Volatile MMIO read (32-bit).
#[allow(dead_code)]
pub unsafe fn mmio_read32(base: u64, offset: u32) -> u32 {
    let ptr = (base + offset as u64) as *const u32;
    core::ptr::read_volatile(ptr)
}

/// Volatile MMIO write (32-bit).
#[allow(dead_code)]
pub unsafe fn mmio_write32(base: u64, offset: u32, val: u32) {
    let ptr = (base + offset as u64) as *mut u32;
    core::ptr::write_volatile(ptr, val);
}

fn print_device(prefix: &str, slot: u8, func: u8,
                vendor: u16, device: u16, class: u8, subclass: u8) {
    crate::write_str(prefix);
    crate::write_str(":");
    crate::write_hex_byte(slot >> 4);
    crate::write_hex_byte(slot & 0xF);
    crate::write_str(".");
    crate::write_hex_byte(func);
    crate::write_str(" ");
    crate::write_hex16(vendor);
    crate::write_str(":");
    crate::write_hex16(device);
    crate::write_str(" cls=");
    crate::write_hex_byte(class);
    crate::write_str(".");
    crate::write_hex_byte(subclass);
    crate::serial_putc(b'\n');
}
