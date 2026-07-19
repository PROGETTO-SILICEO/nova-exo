use core::ptr::{read_volatile, write_volatile};

// ── e1000 registers (offset within MMIO BAR0) ───────────────────────────

const REG_CTRL: u32     = 0x000;
const REG_STATUS: u32   = 0x008;
const REG_ICR: u32      = 0x0C0;
const REG_RCTRL: u32    = 0x100;
const REG_TCTRL: u32    = 0x400;
const REG_TDBAL: u32    = 0x3800;
const REG_TDBAH: u32    = 0x3804;
const REG_TDLEN: u32    = 0x3808;
const REG_TDH: u32      = 0x3810;
const REG_TDT: u32      = 0x3818;
const REG_RDBAL: u32    = 0x2800;
const REG_RDBAH: u32    = 0x2804;
const REG_RDLEN: u32    = 0x2808;
const REG_RDH: u32      = 0x2810;
const REG_RDT: u32      = 0x2818;
const REG_MTA: u32      = 0x5200;
const REG_RA: u32       = 0x5400;
const REG_TIPG: u32     = 0x0410;

// ── TX command bits ─────────────────────────────────────────────
const CMD_EOP: u8       = 0x01;
const CMD_RS: u8        = 0x08;

const STATUS_DD: u8     = 0xFF;

// ── TCTRL shifts ────────────────────────────────────────────────
const TCTRL_CT_SHIFT: u32   = 4;
const TCTRL_COLD_SHIFT: u32 = 12;

// ── CTRL bits ───────────────────────────────────────────────────
const CTRL_FD: u32      = 0x00000001;
const CTRL_ASDE: u32    = 0x00000200;
const CTRL_SLU: u32     = 0x00000040;

// ── TCTRL bits ──────────────────────────────────────────────────
const TCTRL_EN: u32     = 0x00000002;
const TCTRL_PSP: u32    = 0x00000008;

// ── RCTRL bits ──────────────────────────────────────────────────
const RCTRL_EN: u32     = 0x00000002;
const RCTRL_BAM: u32    = 0x00008000;
const RCTRL_BSIZE_2048: u32 = 0x00000000;
const RCTRL_SECRC: u32  = 0x04000000;
#[allow(dead_code)]
const RCTRL_LBM_MAC: u32 = 0x00000200;

// ── Interrupt bits (ICR) ────────────────────────────────────────
#[allow(dead_code)]
const ICR_TXDW: u32     = 0x00000001;
#[allow(dead_code)]
const ICR_TXQE: u32     = 0x00000002;

// ── Descriptor ring ─────────────────────────────────────────────────────

const TX_RING_SIZE: u16 = 8;
const RX_RING_SIZE: u16 = 8;

const RX_BUF_SIZE: usize = 2048;

#[repr(C, packed)]
struct TxDesc {
    addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    special: u16,
}

#[repr(C, packed)]
struct RxDesc {
    addr: u64,
    length: u16,
    csum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

// ── RX data for CfC pipeline ────────────────────────────────────────────

pub static mut RX_PENDING: bool = false;
pub static mut RX_DATA: [u8; 64] = [0; 64];
pub static mut RX_LEN: u16 = 0;

// ── DMA buffers ─────────────────────────────────────────────────────────

#[repr(align(4096))]
#[allow(dead_code)]
struct Aligned4K([u8; 4096]);

static mut TX_RING: Aligned4K = Aligned4K([0u8; 4096]);
static mut RX_RING: Aligned4K = Aligned4K([0u8; 4096]);

#[repr(align(4096))]
#[allow(dead_code)]
struct Aligned16K([u8; 16384]);

static mut RX_POOL: Aligned16K = Aligned16K([0u8; 16384]);
static mut TX_BUF: [u8; 1514] = [0u8; 1514];

// ── MMIO register access ────────────────────────────────────────────────

fn mmio_read(base: u64, reg: u32) -> u32 {
    unsafe { read_volatile((base + reg as u64) as *const u32) }
}

fn mmio_write(base: u64, reg: u32, val: u32) {
    unsafe { write_volatile((base + reg as u64) as *mut u32, val); }
}

// ── Driver ──────────────────────────────────────────────────────────────

pub struct E1000;

impl E1000 {
    fn mmio_base() -> u64 {
        unsafe { MMIO_BASE }
    }

    fn reset() {
        let base = Self::mmio_base();
        mmio_write(base, REG_CTRL, 1 << 26);
        for _ in 0..10000 {
            if mmio_read(base, REG_CTRL) & (1 << 26) == 0 {
                break;
            }
            core::hint::spin_loop();
        }
        crate::write_str("e1000:reset ok\n");
    }

    fn link_up() {
        let base = Self::mmio_base();
        let ctrl = mmio_read(base, REG_CTRL) | CTRL_SLU | CTRL_ASDE | CTRL_FD;
        mmio_write(base, REG_CTRL, ctrl);
        crate::write_str("e1000:link forced up\n");
    }

    fn setup_tx() {
        let base = Self::mmio_base();
        unsafe {
            let ring = &raw mut TX_RING;
            let desc_ptr = ring as *mut Aligned4K;
            for i in 0..TX_RING_SIZE as usize {
                let desc = &mut *(desc_ptr as *mut TxDesc).add(i);
                desc.addr = 0;
                desc.length = 0;
                desc.status = STATUS_DD;
            }

            let tx_ring_phys = v2p(&raw const TX_RING as *const _ as u64);
            let tx_len = TX_RING_SIZE as u32 * 16;

            mmio_write(base, REG_TDBAL, (tx_ring_phys & 0xFFFFFFFF) as u32);
            mmio_write(base, REG_TDBAH, (tx_ring_phys >> 32) as u32);
            mmio_write(base, REG_TDLEN, tx_len);
            mmio_write(base, REG_TDH, 0);
            mmio_write(base, REG_TDT, 0);

            let tdbal = mmio_read(base, REG_TDBAL);
            let tdbah = mmio_read(base, REG_TDBAH);
            let tdlen = mmio_read(base, REG_TDLEN);
            crate::write_str("e1000:TX TDBAL=0x");
            crate::write_hex32(tdbal);
            crate::write_str(" TDBAH=0x");
            crate::write_hex32(tdbah);
            crate::write_str(" TDLEN=0x");
            crate::write_hex32(tdlen);
            crate::serial_putc(b'\n');
        }
    }

    fn setup_rx() {
        let base = Self::mmio_base();
        unsafe {
            let rx_ring_phys = v2p(&raw const RX_RING as *const _ as u64);
            let rx_pool_phys = v2p(&raw const RX_POOL as *const _ as u64);
            let rx_len = RX_RING_SIZE as u32 * 16;

            let ring = &raw mut RX_RING;
            for i in 0..RX_RING_SIZE as usize {
                let entry = &mut *(ring as *mut Aligned4K).cast::<RxDesc>().add(i);
                let buf_phys = rx_pool_phys + (i * RX_BUF_SIZE) as u64;
                entry.addr = buf_phys;
                entry.status = 0;
            }

            mmio_write(base, REG_RDBAL, (rx_ring_phys & 0xFFFFFFFF) as u32);
            mmio_write(base, REG_RDBAH, (rx_ring_phys >> 32) as u32);
            mmio_write(base, REG_RDLEN, rx_len);
            mmio_write(base, REG_RDH, 0);
            mmio_write(base, REG_RDT, RX_RING_SIZE as u32 - 1);

            crate::write_str("e1000:RX ring phys=");
            crate::write_hex64(rx_ring_phys);
            crate::serial_putc(b'\n');
        }
    }

    fn enable() {
        let base = Self::mmio_base();

        let tctrl = TCTRL_EN | TCTRL_PSP
            | (0x10 << TCTRL_CT_SHIFT)
            | (0x40 << TCTRL_COLD_SHIFT);
        mmio_write(base, REG_TCTRL, tctrl);

        mmio_write(base, REG_TIPG, 10 | (8 << 10) | (6 << 20));

        let rctrl = RCTRL_EN | RCTRL_BAM | RCTRL_BSIZE_2048 | RCTRL_SECRC;
        mmio_write(base, REG_RCTRL, rctrl);

        mmio_write(base, REG_RA, 0x12005452);
        mmio_write(base, REG_RA + 4, 0x5634 | (1 << 31));

        for i in 0..128 {
            mmio_write(base, REG_MTA + i * 4, 0);
        }

        crate::write_str("e1000:enabled\n");
    }

    fn loopback_test() {
        // Software loopback: copia il buffer TX nel pool RX,
        // marca il descrittore RX come ricevuto.
        unsafe {
            let src = &raw const TX_BUF as *const u8;
            let pool = &raw mut RX_POOL as *mut u8;
            for j in 0..60 {
                *pool.add(j) = *src.add(j);
            }
            let ring = &raw mut RX_RING;
            let entry = &mut *(ring as *mut Aligned4K).cast::<RxDesc>().add(0);
            entry.status = 1;
            entry.length = 60;
            crate::write_str("e1000:loopback done\n");
        }
    }

    fn test_tx() {
        let base = Self::mmio_base();

        unsafe {
            let buf: &mut [u8; 1514] = &mut *(&raw mut TX_BUF as *mut [u8; 1514]);
            buf.fill(0);

            buf[0..6].copy_from_slice(&[0xFF; 6]);
            buf[6..12].copy_from_slice(&[0x52,0x54,0x00,0x12,0x34,0x56]);
            buf[12] = 0x08; buf[13] = 0x06;

            buf[14] = 0x00; buf[15] = 0x01;
            buf[16] = 0x08; buf[17] = 0x00;
            buf[18] = 0x06; buf[19] = 0x04;
            buf[20] = 0x00; buf[21] = 0x01;
            buf[22..28].copy_from_slice(&[0x52,0x54,0x00,0x12,0x34,0x56]);
            buf[28] = 10; buf[29] = 0; buf[30] = 2; buf[31] = 15;
            buf[32..38].copy_from_slice(&[0; 6]);
            buf[38] = 10; buf[39] = 0; buf[40] = 2; buf[41] = 1;
            let total_len = 60;

            let buf_phys = v2p(&raw const TX_BUF as *const _ as u64);

            let ring = &raw mut TX_RING;
            let desc_ptr = ring as *mut Aligned4K;
            let desc = &mut *(desc_ptr as *mut TxDesc).add(0);
            desc.addr = buf_phys;
            desc.length = total_len as u16;
            desc.cmd = CMD_EOP | CMD_RS;
            desc.status = 0;
            desc.cso = 0;
            desc.css = 0;
            desc.special = 0;

            crate::write_str("e1000:ARP request sent\n");

            mmio_write(base, REG_TDT, 1);

            let mut tx_ok = false;
            for _ in 0..10_000_000 {
                if read_volatile(&desc.status) & STATUS_DD != 0 {
                    tx_ok = true;
                    break;
                }
                core::hint::spin_loop();
            }

            let tdh = mmio_read(base, REG_TDH);
            let status_after = desc.status;
            let icr = mmio_read(base, REG_ICR);

            crate::write_str("e1000:TX status=0x");
            crate::write_hex_byte(status_after);
            crate::write_str(" TDH=0x");
            crate::write_hex16(tdh as u16);
            crate::write_str(" ICR=0x");
            crate::write_hex32(icr);
            crate::write_str(" ok=");
            if tx_ok { crate::serial_putc(b'1'); } else { crate::serial_putc(b'0'); }
            crate::serial_putc(b'\n');

            if !tx_ok {
                crate::write_str("e1000:TX FAILED\n");
            }
        }
    }

    pub fn poll_rx() {
        let ring = &raw const RX_RING;
        for i in 0..RX_RING_SIZE as usize {
            let desc = unsafe { &*(ring as *const Aligned4K).cast::<RxDesc>().add(i) };
            if desc.status & 0x01 != 0 {
                let pool = &raw const RX_POOL;
                let data_ptr = pool as *const u8;
                let len = desc.length.min(64);
                unsafe {
                    // Copy payload to shared buffer for CfC
                    for j in 0..len as usize {
                        RX_DATA[j] = *data_ptr.add(i * RX_BUF_SIZE + j);
                    }
                    RX_LEN = desc.length;
                    RX_PENDING = true;
                }
                // Re-arm descriptor for next packet
                let desc_mut = unsafe { &mut *(ring as *const Aligned4K as *mut Aligned4K).cast::<RxDesc>().add(i) };
                desc_mut.status = 0;
            }
        }
    }

    pub fn init(mmio_base: u64) {
        unsafe { MMIO_BASE = mmio_base; }

        let base = Self::mmio_base();

        crate::write_str("e1000:base=");
        crate::write_hex64(base);
        crate::serial_putc(b'\n');

        let status = mmio_read(base, REG_STATUS);
        crate::write_str("e1000:status=0x");
        crate::write_hex32(status);
        crate::serial_putc(b'\n');

        Self::reset();
        Self::link_up();
        Self::setup_rx();
        Self::setup_tx();
        Self::enable();
        // TX test only; RX is polled from main loop
        Self::test_tx();
        Self::loopback_test();
        Self::poll_rx();
    }
}

static mut MMIO_BASE: u64 = 0;

fn v2p(virt: u64) -> u64 {
    crate::virt_to_phys(virt)
}
