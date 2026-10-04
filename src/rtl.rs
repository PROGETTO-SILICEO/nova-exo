//! RTL810xE — Realtek Fast Ethernet (famiglia r8169). Probe con diagnostica.
//!
//! Riferimenti (04/10/2026): driver r8169 Rust no_std (chitti kernel),
//! wiki.osdev.org/RTL8169, Linux r8169_main.c.
//! Sull'IdeaPad 110-15ACL la scheda è a 02:00.0 (10ec:8136), dietro il
//! root port PCIe. QEMU NON emula questo chip: il banco di test è il Lenovo.
//!
//! Lezione 04/10 (primo tentativo): il mapping MMIO di paging::init è fisso
//! (5 zone di QEMU) → la lettura del BAR reale faceva #PF in loop. Ora:
//! si legge il BAR dal config, si MAPPA dinamicamente, poi si legge.

use crate::gui;
use crate::paging;
use crate::pci;

pub struct Rtl {
    pub mmio: u64,
    pub mmio_phys: u64,
    pub bar_index: u8,
    pub mac: [u8; 6],
    pub chip_cmd: u8,
    pub intr_status: u16,
}

/// Mostra una stringa alla riga diagnostica (y=460) — visibile sul video.
/// Pulisce l'area con un rettangolo pieno (gli SPAZI non cancellano i pixel).
unsafe fn diag(s: &str, color: u32) {
    let bg = gui::color(200, 0, 0);
    // rettangolo pieno 8px di altezza (2 barre da 4px)
    gui::draw_bar(10, 459, 48 * 9, 1.0, bg, bg);
    gui::draw_bar(10, 463, 48 * 9, 1.0, bg, bg);
    gui::set_cursor(10, 460);
    gui::print_str(s, color);
}

/// Mostra "RTL: BAR=XXXXXXXX" (hex del valore fisico del BAR).
unsafe fn diag_bar(prefix: &str, val: u64) {
    let hex = b"0123456789abcdef";
    let mut buf: [u8; 28] = *b"RTL: BAR=00000000           ";
    // prefix override: teniamo il formato fisso, il prefisso è nel commento
    let _ = prefix;
    for i in 0..8 {
        buf[9 + i] = hex[((val >> (4 * (7 - i))) & 0xF) as usize];
    }
    if let Ok(s) = core::str::from_utf8(&buf) {
        diag(s, gui::COL_CYAN);
    }
}

impl Rtl {
    /// Cerca la RTL810xE, mappa il BAR, legge MAC + stato. Ogni passo è
    /// mostrato sul video: se il kernel si ferma, l'ultima riga dice dove.
    pub unsafe fn probe() -> Option<Rtl> {
        diag("RTL: cerco 10ec:8136...", gui::COL_DIM);

        let (bus, slot, func) = match pci::pci_find_device(0x10ec, 0x8136) {
            Some(t) => t,
            None => {
                diag("RTL: assente                    ", gui::COL_DIM);
                return None;
            }
        };
        pci::enable_bus_master(bus, slot, func);
        diag("RTL: trovata, bus master on", gui::COL_DIM);

        // Primo BAR che decodifica come memory (PCIe parts: BAR2 → 1 → 0)
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
            diag("RTL: nessun BAR memory", gui::COL_RED);
            return None;
        }
        diag_bar("", mmio_phys);

        // Mapping DINAMICO della pagina del BAR (fix 04/10)
        if !paging::map_mmio_page(mmio_phys) {
            diag("RTL: BAR fuori range mappabile", gui::COL_RED);
            return None;
        }
        diag("RTL: mappata, leggo MAC...", gui::COL_DIM);

        let mmio = paging::mmio_virt_addr(mmio_phys);

        // MAC address: 6 byte a offset 0x00 (IDR0..IDR5)
        let mut mac = [0u8; 6];
        for (i, m) in mac.iter_mut().enumerate() {
            *m = core::ptr::read_volatile((mmio + i as u64) as *const u8);
        }
        let chip_cmd = core::ptr::read_volatile((mmio + 0x37) as *const u8);
        let intr_status = core::ptr::read_volatile((mmio + 0x3e) as *const u16);

        // Registri di identificazione aggiuntivi (debug 04/10 v2):
        // se questi NON sembrano Realtek, stiamo leggendo dal posto sbagliato.
        // Attesi su chip sano: Cfg9346 (0x50) ∈ {0x00, 0xC0}; TxConfig (0x40)
        // valori bassi noti; RxMaxSize (0xDA) ~ 0x1FFF o simile.
        let cfg9346 = core::ptr::read_volatile((mmio + 0x50) as *const u8);
        let tx_config = core::ptr::read_volatile((mmio + 0x40) as *const u32);
        let rx_max = core::ptr::read_volatile((mmio + 0xda) as *const u16);

        // Pulisci la riga PRIMA del risultato (fix overlap dei print)
        diag("                                        ", gui::COL_DIM);

        // Display: riga 1 = MAC + CMD; riga 2 = registri chiave
        let hex = b"0123456789abcdef";
        let mut buf: [u8; 34] = *b"RTL: MAC=00:00:00:00:00:00 CMD=00 ";
        for i in 0..6 {
            buf[9 + i * 3] = hex[(mac[i] >> 4) as usize];
            buf[10 + i * 3] = hex[(mac[i] & 0xF) as usize];
            if i < 5 {
                buf[11 + i * 3] = b':';
            }
        }
        buf[31] = hex[(chip_cmd >> 4) as usize];
        buf[32] = hex[(chip_cmd & 0xF) as usize];
        let all_zero = mac.iter().all(|&b| b == 0x00);
        let all_ff = mac.iter().all(|&b| b == 0xFF);
        let color = if all_zero || all_ff { gui::COL_RED } else { gui::COL_GREEN };
        if let Ok(s) = core::str::from_utf8(&buf) {
            diag(s, color);
        }

        // Riga 2: "REG: 50=xx 40=xxxxxxxx da=xxxx" (valori grezzi)
        let mut buf2: [u8; 36] = *b"REG: 50=00 40=00000000 da=0000      ";
        buf2[8] = hex[(cfg9346 >> 4) as usize];
        buf2[9] = hex[(cfg9346 & 0xF) as usize];
        for i in 0..8 {
            buf2[14 + i] = hex[((tx_config >> (4 * (7 - i))) & 0xF) as usize];
        }
        buf2[26] = hex[(rx_max >> 12) as usize & 0xF];
        buf2[27] = hex[(rx_max >> 8) as usize & 0xF];
        buf2[28] = hex[(rx_max >> 4) as usize & 0xF];
        buf2[29] = hex[(rx_max & 0xF) as usize];
        if let Ok(s) = core::str::from_utf8(&buf2) {
            gui::set_cursor(10, 471);
            gui::print_str(s, gui::COL_CYAN);
        }

        // Riga 3: BAR scelto + valore fisico
        let mut buf3: [u8; 31] = *b"BAR=00000000 (index=0)         ";
        for i in 0..8 {
            buf3[4 + i] = hex[((mmio_phys >> (4 * (7 - i))) & 0xF) as usize];
        }
        buf3[21] = b'0' + bar_index;
        if let Ok(s) = core::str::from_utf8(&buf3) {
            gui::set_cursor(10, 482);
            gui::print_str(s, gui::COL_CYAN);
        }

        // ── RESET del chip + ri-lettura MAC (04/10/2026) ──
        // Su molte Realtek il MAC viene caricato dall'EEPROM/OTP al reset:
        // prima del reset i registri IDR possono contenere pattern di default
        // (es. 88:88:...). CmdReset è self-clearing.
        let cmd_before = core::ptr::read_volatile((mmio + 0x37) as *const u8);
        core::ptr::write_volatile((mmio + 0x37) as *mut u8, cmd_before | 0x10);
        let mut reset_ok = false;
        for _ in 0..2_000_000u32 {
            if core::ptr::read_volatile((mmio + 0x37) as *const u8) & 0x10 == 0 {
                reset_ok = true;
                break;
            }
        }
        // ri-leggi MAC post-reset
        let mut mac2 = [0u8; 6];
        for (i, m) in mac2.iter_mut().enumerate() {
            *m = core::ptr::read_volatile((mmio + i as u64) as *const u8);
        }
        let cmd_after = core::ptr::read_volatile((mmio + 0x37) as *const u8);

        // Riga 4: MAC post-reset (il dato che conta) + esito reset
        let mut buf4: [u8; 42] = *b"MAC2=00:00:00:00:00:00 CMD=00 RST=OK      ";
        for i in 0..6 {
            buf4[5 + i * 3] = hex[(mac2[i] >> 4) as usize];
            buf4[6 + i * 3] = hex[(mac2[i] & 0xF) as usize];
            if i < 5 {
                buf4[7 + i * 3] = b':';
            }
        }
        buf4[27] = hex[(cmd_after >> 4) as usize];
        buf4[28] = hex[(cmd_after & 0xF) as usize];
        if !reset_ok {
            buf4[33] = b'F';
            buf4[34] = b'A';
            buf4[35] = b'I';
            buf4[36] = b'L';
        }
        let all_zero2 = mac2.iter().all(|&b| b == 0x00);
        let all_ff2 = mac2.iter().all(|&b| b == 0xFF);
        let all_88 = mac2.iter().all(|&b| b == 0x88);
        let color4 = if all_zero2 || all_ff2 || all_88 {
            gui::COL_RED
        } else {
            gui::COL_GREEN
        };
        if let Ok(s) = core::str::from_utf8(&buf4) {
            gui::set_cursor(10, 493);
            gui::print_str(s, color4);
        }

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
