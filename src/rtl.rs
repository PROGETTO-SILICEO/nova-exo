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

// ── Registri (famiglia r8169) ──
const CFG_9346: u64 = 0x50; // unlock config (0xC0 = unlock, 0x00 = lock)
const TX_CONFIG: u64 = 0x40;
const TX_DESC_LO: u64 = 0x20;
const TX_DESC_HI: u64 = 0x24;
const CHIP_CMD: u64 = 0x37;
const TX_POLL: u64 = 0x38;
const PHYAR: u64 = 0x60; // PHY access register (bit31=busy, 16-20=reg, 0-15=data)

// ── RX (aggiunti 05/10/2026) ──
const RCR: u64 = 0x44; // Receive Configuration Register
const RDSAR_LO: u64 = 0xE4; // RX Descriptor Start Address (basso)
const RDSAR_HI: u64 = 0xE8; // RX Descriptor Start Address (alto)
const RMS: u64 = 0xDA; // RxMaxSize — la lunghezza massima che il chip accetta

// ── RxConfig (0x44): i valori sono presi LETTERALMENTE da r8169_main.c ──
// ERRORE CORRETTO il 05/10: credevo che i bit 8-10 fossero la lunghezza del
// buffer. Non lo sono: sono il DMA burst (RXCFG_DMA_SHIFT=8). La lunghezza
// sta in RxMaxSize (0xda), un registro separato che non avevo mai scritto.
//
// rtl_init_rxcfg, caso "default" (8105e/8106e/8102e): RX128_INT_EN | RX_DMA_BURST
const RX128_INT_EN: u32 = 1 << 15; // 0x8000
const RX_DMA_BURST: u32 = 7 << 8; // 0x0700 — burst massimo, non la lunghezza
// rtl_set_rx_mode: AcceptBroadcast | AcceptMyPhys | AcceptMulticast
// Si accetta SOLO fisico/multicast/broadcast: niente "tutto", così il corpo
// non sente il rumore della rete.
const RX_ACCEPT: u32 = 0x08 | 0x02 | 0x04; // 0x0E
const RX_CONFIG_VALUE: u32 = RX128_INT_EN | RX_DMA_BURST | RX_ACCEPT; // 0x870E

/// RxMaxSize: DEVE essere coerente con RX_BUF_LEN e con opts1, o il DMA
/// scrive fuori. Linux usa 16384 ("Low hurts"), ma con buffer da 2048 il
/// valore giusto è 2048: i frame Ethernet non superano 1518.
const RX_MAX_SIZE: u16 = 2048;

/// Descrittori RX (4) e dimensione di ciascun buffer.
/// I buffer DEVONO essere almeno `RX_DESC_BUF_SIZE+1`: se dicessimo al chip
/// 16383 e ne avessimo 2048, il DMA scriverebbe fuori dal buffer.
/// 64 descrittori (non 4): con 4 il FIFO del chip andava in OVERFLOW sulla
/// raffica di broadcast al boot e il ricevitore si fermava per sempre.
/// 64 × 2048 = 128 KB: tanto spazio per respirare, poca memoria.
const RX_DESC: usize = 64;
const RX_BUF_LEN: usize = 2048;

// PHY registers (standard MII)
const PHY_BMCR: u8 = 0; // Basic Mode Control: bit11=power down, bit9=restart autoneg
const PHY_BMSR: u8 = 1; // Basic Mode Status: bit2=Link Status
const PHY_ANAR: u8 = 4; // Auto-Neg Advertisement

// Descriptor flags (opts1)
const DESC_OWN: u32 = 1 << 31; // owned by NIC
const DESC_RING_END: u32 = 1 << 30;
const DESC_FIRST_FRAG: u32 = 1 << 29;
const DESC_LAST_FRAG: u32 = 1 << 28;

/// TX ring: 16 descriptor × 16 byte = 256 byte (allineato 256, requisito chip)
#[repr(align(256))]
struct Aligned256([u8; 256]);
static mut TX_RING: Aligned256 = Aligned256([0u8; 256]);

/// Buffer dati TX (il frame da spedire)
#[repr(align(64))]
struct Aligned2048([u8; 2048]);
static mut TX_BUF: Aligned2048 = Aligned2048([0u8; 2048]);

// ── RX: ring e buffer (05/10/2026) ──
// Il ring deve stare su un confine da 256 byte (requisito del chip).
static mut RX_RING: Aligned256 = Aligned256([0u8; 256]);
/// Un buffer per descrittore: il chip ci scrive dentro via DMA.
/// 64 × 2 KB = 128 KB di BSS. Allineati a 256 (requisito del chip).
#[repr(align(256))]
struct RxBufs([[u8; RX_BUF_LEN]; RX_DESC]);
static mut RX_BUFS: RxBufs = RxBufs([[0u8; RX_BUF_LEN]; RX_DESC]);

/// Diagnostica RX: visibile dal metallo solo attraverso la telemetria UDP,
/// perché lì la seriale non esiste. Senza questa, un RX che non funziona
/// sarebbe indistinguibile da una rete silenziosa.
pub static mut RX_INIT_OK: bool = false;
pub static mut RX_CMD: u8 = 0; // CHIP_CMD dopo l'init
pub static mut RX_RCR: u32 = 0; // RxConfig (0x44) riletto dopo l'init
pub static mut RX_MAXSZ: u16 = 0; // RxMaxSize (0xda) riletto
/// opts1 del primo descrittore: se è 0x80003FFF (o 0xC0003FFF) il ring è armato.
pub static mut RX_DESC0: u32 = 0;
/// Quanti overflow RX sono stati visti e recuperati.
pub static mut RX_OVERFLOWS: u32 = 0;

/// Epoca di build. Va INCREMENTATA a mano ad ogni build che cambia il
/// protocollo o l'RX: senza, non si può sapere quale kernel sta parlando —
/// ed è esattamente la confusione che ha fatto perdere un giro il 05/10.
pub const NET_EPOCH: u32 = 7;

const INTR_STATUS: u64 = 0x3E;

/// Maschera degli OWN dei descrittori RX: bit i = descrittore i in mano al
/// chip. Se vale 0xF il ring è tutto armato; se è 0 il chip li ha consumati
/// e non riarmati. È il diagnostico che dice se il riarmo funziona DAVVERO,
/// letto dal vivo e non alla init.
pub unsafe fn rx_desc_own_mask() -> u32 {
    if !RTL_PRESENT || RTL_MMIO == 0 {
        return 0;
    }
    let d = (&raw const RX_RING.0) as *const u32;
    let mut m = 0u32;
    for i in 0..RX_DESC {
        let o = core::ptr::read_volatile(d.add(i * 4));
        if o & DESC_OWN != 0 {
            m |= 1 << i;
        }
    }
    m
}

/// IntrStatus: se c'è stato un errore RX (overflow/FIFO) lo dice.
pub unsafe fn intr_status() -> u16 {
    if !RTL_PRESENT || RTL_MMIO == 0 {
        return 0;
    }
    core::ptr::read_volatile((RTL_MMIO + INTR_STATUS) as *const u16)
}

/// Stato globale per il beacon periodico dal loop principale.
pub static mut RTL_MMIO: u64 = 0;
pub static mut RTL_PRESENT: bool = false;

/// MAC sorgente dei frame di Exo. È la firma "EXO-01" usata dal beacon dal
/// 04/10/2026, e ha funzionato da subito: la teniamo per continuità tra
/// beacon e telemetria. DEBITO NOTO: non è il MAC reale del chip
/// (`Rtl::probe().mac`), che sarebbe più corretto — da fare quando si
/// consoliderà l'init della NIC.
pub const EXO_MAC: [u8; 6] = [0xc8, 0x5b, 0x76, 0xe0, 0xe0, 0x01];

/// Il descrittore TX è libero? Serve a spedire **senza bloccare** il loop:
/// se è occupato si salta il giro invece di aspettare.
pub unsafe fn tx_ready() -> bool {
    if !RTL_PRESENT || RTL_MMIO == 0 {
        return false;
    }
    let d = (&raw const TX_RING.0) as *const u32;
    core::ptr::read_volatile(d) & DESC_OWN == 0
}

/// Spedisce un frame Ethernet già costruito (es. IPv4/UDP di `net.rs`).
/// Non rende pubblico `RTL_MMIO`: il TX passa sempre da qui.
pub unsafe fn tx_raw(frame: &[u8]) -> bool {
    if !RTL_PRESENT || RTL_MMIO == 0 {
        return false;
    }
    Rtl::tx_frame(RTL_MMIO, frame)
}

/// Beacon di rete (04/10/2026): frame broadcast con contatore, chiamabile
/// dal loop ogni N tick. Robusto: il TX non dipende dal timing del boot.
pub unsafe fn tx_beacon(counter: u64) -> bool {
    if !RTL_PRESENT || RTL_MMIO == 0 {
        return false;
    }
    let mut frame = [0u8; 60];
    for b in frame.iter_mut().take(6) {
        *b = 0xFF; // broadcast
    }
    frame[6..12].copy_from_slice(&EXO_MAC); // firma EXO-01
    frame[12] = 0x88;
    frame[13] = 0xB5;
    let msg = b"EXO TICK ";
    frame[14..23].copy_from_slice(msg);
    // contatore decimale in coda
    let mut n = counter;
    let mut digits = [0u8; 20];
    let mut len = 0;
    if n == 0 {
        digits[0] = b'0';
        len = 1;
    }
    while n > 0 && len < 20 {
        digits[len] = b'0' + (n % 10) as u8;
        n /= 10;
        len += 1;
    }
    for i in 0..len {
        if 23 + i < 60 {
            frame[23 + i] = digits[len - 1 - i];
        }
    }
    Rtl::tx_frame(RTL_MMIO, &frame)
}

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

        // ── PHY + PRIMO PACCHETTO (04/10/2026) ──
        // La PHY può essere in power-down dopo il reset → link assente →
        // il TX esce dal MAC ma non dal cavo. Prima: link check + wake.
        let mut tx_result: u8 = 2; // 0=ok, 1=timeout, 2=skip
        let mut link_state: u8 = 0; // 0=down, 1=up, 2=riattivata
        if reset_ok {
            let mut up = Rtl::link_status(mmio);
            if up {
                link_state = 1;
            } else {
                Rtl::phy_wake(mmio);
                // attesa autoneg (max ~600k giri ≈ qualche secondo)
                for _ in 0..600_000u32 {
                    if Rtl::link_status(mmio) {
                        up = true;
                        break;
                    }
                }
                link_state = if up { 2 } else { 0 };
            }

            Rtl::tx_init(mmio);
            // RX (05/10/2026): apre la porta d'ingresso che mancava sul metallo.
            // Se la config non torna, RX_INIT_OK resta false e poll_rx non fa
            // nulla: meglio nessun ingresso che un ingresso che mente.
            let rx_ok = Rtl::rx_init(mmio);
            RTL_MMIO = mmio;
            RTL_PRESENT = true; // abilita il beacon periodico dal loop
            crate::write_str("rtl:rx_init=");
            crate::write_str(if rx_ok { "ok" } else { "FALLITO" });
            crate::write_str(" cmd=0x");
            crate::write_hex64(RX_CMD as u64);
            crate::write_str(" rcr=0x");
            crate::write_hex64(RX_RCR as u64);
            crate::serial_putc(b'\n');
            let mut frame = [0u8; 60];
            for b in frame.iter_mut().take(6) {
                *b = 0xFF; // dest: broadcast
            }
            frame[6..12].copy_from_slice(&[0xc8, 0x5b, 0x76, 0xe0, 0xe0, 0x01]); // SRC: MAC FIRMA "EXO-01" — distinguibile dal BIOS!
            frame[12] = 0x88;
            frame[13] = 0xB5; // ethertype: experimental (locale)
            let msg = b"EXO: PRIMO PACCHETTO DAL METALLO";
            let n = msg.len().min(60 - 14);
            frame[14..14 + n].copy_from_slice(&msg[..n]);
            tx_result = if Rtl::tx_frame(mmio, &frame) { 0 } else { 1 };
        }
        gui::set_cursor(10, 504);
        match tx_result {
            0 => gui::print_str(
                "TX: inviato (60B) - guarda lo switch!",
                gui::COL_GREEN,
            ),
            1 => gui::print_str("TX: timeout (DescOwn non pulito)", gui::COL_RED),
            _ => gui::print_str("TX: skip (no reset)", gui::COL_DIM),
        }
        gui::set_cursor(10, 515);
        match link_state {
            1 => gui::print_str("LINK: up (già attivo)", gui::COL_GREEN),
            2 => gui::print_str("LINK: riattivato (autoneg)", gui::COL_GREEN),
            _ => gui::print_str("LINK: down", gui::COL_RED),
        }

        // ── Diagnostica TX profonda (04/10/2026 v3) ──
        // Il DescOwn pulito NON basta: il chip può fallire la trasmissione
        // (TU/underrun) pulendo comunque il bit. Qui leggiamo l'esito VERO:
        //  opts1 post-TX: bit15=TOK (ok), bit14=TU (underrun), bit13=TXERR
        //  + IntrStatus (TxOK/TxErr), BMSR, PHYstatus (0x6C: cosa vede il MAC)
        if reset_ok {
            let d = (&raw const TX_RING.0) as *const u32;
            let opts1_after = core::ptr::read_volatile(d);
            let isr = core::ptr::read_volatile((mmio + 0x3E) as *const u16);
            let bmsr = Rtl::phy_read(mmio, PHY_BMSR);
            let physt = core::ptr::read_volatile((mmio + 0x6C) as *const u16);
            let mut b: [u8; 48] = *b"TXS: d=00000000 isr=0000 bmsr=0000 physt=0000   ";
            for i in 0..8 {
                b[7 + i] = hex[((opts1_after >> (4 * (7 - i))) & 0xF) as usize];
            }
            for i in 0..4 {
                b[20 + i] = hex[((isr >> (4 * (3 - i))) & 0xF) as usize];
                b[30 + i] = hex[((bmsr >> (4 * (3 - i))) & 0xF) as usize];
                b[41 + i] = hex[((physt >> (4 * (3 - i))) & 0xF) as usize];
            }
            if let Ok(s) = core::str::from_utf8(&b) {
                gui::set_cursor(10, 526);
                gui::print_str(s, gui::COL_CYAN);
            }
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

    /// Legge un registro PHY via PHYAR (0x60). Ritorna 0xFFFF se timeout.
    pub unsafe fn phy_read(mmio: u64, reg: u8) -> u16 {
        core::ptr::write_volatile(
            (mmio + PHYAR) as *mut u32,
            0x8000_0000 | ((reg as u32) << 16),
        );
        for _ in 0..100_000u32 {
            let v = core::ptr::read_volatile((mmio + PHYAR) as *const u32);
            if v & 0x8000_0000 == 0 {
                return (v & 0xFFFF) as u16;
            }
        }
        0xFFFF
    }

    /// Scrive un registro PHY via PHYAR (0x60).
    pub unsafe fn phy_write(mmio: u64, reg: u8, val: u16) {
        core::ptr::write_volatile(
            (mmio + PHYAR) as *mut u32,
            0x8000_0000 | ((reg as u32) << 16) | (val as u32),
        );
        for _ in 0..100_000u32 {
            let v = core::ptr::read_volatile((mmio + PHYAR) as *const u32);
            if v & 0x8000_0000 == 0 {
                return;
            }
        }
    }

    /// Il link è su? (BMSR bit 2). E attende l'autoneg fino a ~3s se richiesto.
    pub unsafe fn link_status(mmio: u64) -> bool {
        let bmsr = Self::phy_read(mmio, PHY_BMSR);
        bmsr & 0x0004 != 0
    }

    /// Riattiva la PHY: power-up + autoneg su tutte le velocità 10/100.
    pub unsafe fn phy_wake(mmio: u64) {
        // BMCR: clear power-down (bit11), abilita autoneg (bit12)
        let bmcr = Self::phy_read(mmio, PHY_BMCR);
        Self::phy_write(mmio, PHY_BMCR, (bmcr & !0x0800) | 0x1000);
        // ANAR: advertise 100FD|100HD|10FD|10HD (0x01E1)
        Self::phy_write(mmio, PHY_ANAR, 0x01E1);
        // Restart autoneg (bit9) + autoneg enable
        let bmcr2 = Self::phy_read(mmio, PHY_BMCR);
        Self::phy_write(mmio, PHY_BMCR, bmcr2 | 0x0200 | 0x1000);
    }

    /// Prepara il TX: descriptor ring + config + CmdTxEnb.
    /// (statica: usabile prima di costruire la struct)
    pub unsafe fn tx_init(mmio: u64) -> bool {
        let ring_phys = crate::virt_to_phys(&raw const TX_RING as *const _ as u64);
        let buf_phys = crate::virt_to_phys(&raw const TX_BUF as *const _ as u64);

        // Descriptor 0: addr = buffer fisico; opts1=0 (non owned = libero)
        let d = (&raw mut TX_RING.0) as *mut u32;
        core::ptr::write_volatile(d, 0);
        core::ptr::write_volatile(d.add(1), 0);
        core::ptr::write_volatile(d.add(2) as *mut u64, buf_phys);

        // Cfg9346 unlock → scrivi config → lock
        core::ptr::write_volatile((mmio + CFG_9346) as *mut u8, 0xC0);
        // TxConfig: IFG standard + MXDMA unlimited (valore canonico)
        core::ptr::write_volatile((mmio + TX_CONFIG) as *mut u32, 0x0300_0700);
        // Indirizzo del ring
        core::ptr::write_volatile((mmio + TX_DESC_LO) as *mut u32, ring_phys as u32);
        core::ptr::write_volatile((mmio + TX_DESC_HI) as *mut u32, (ring_phys >> 32) as u32);
        // Abilita il trasmettitore (CmdTxEnb)
        let cmd = core::ptr::read_volatile((mmio + CHIP_CMD) as *const u8);
        core::ptr::write_volatile((mmio + CHIP_CMD) as *mut u8, cmd | 0x04);
        core::ptr::write_volatile((mmio + CFG_9346) as *mut u8, 0x00);
        true
    }

    // ── RX (05/10/2026) ─────────────────────────────────────────────────
    // Il 110-15ACL non ha seriale: l'unico modo di far entrare qualcosa nel
    // corpo è la rete. La e1000 (Intel/QEMU) aveva già l'RX; la RTL810xE no.
    // Questa è la porta che manca — alimenta `crate::rx`, la stessa che usa
    // la e1000, così il consumatore (chemio_input) resta unico.
    //
    // QEMU NON emula questo chip: qui non si può testare. Quindi si scrive
    // difensivo (se la config non torna, NON si abilita RE) e si espone la
    // diagnostica, che viaggia nella telemetria UDP.

    /// Inizializza l'RX: ring di descrittori + buffer, RCR, RDSAR, e RE.
    /// Ritorna true solo se il ricevitore risulta davvero abilitato.
    pub unsafe fn rx_init(mmio: u64) -> bool {
        let ring_phys = crate::virt_to_phys(&raw const RX_RING.0 as *const _ as u64);

        // opts1 = DescOwn | (RingEnd se ultimo) | DIMENSIONE BUFFER.
        // La dimensione nei bit bassi è quella che mancava: senza, il chip non
        // sa quanto può scrivere. Da rtl8169_mark_to_asic.
        let d = (&raw mut RX_RING.0) as *mut u32;
        for i in 0..RX_DESC {
            let buf_phys =
                crate::virt_to_phys((&raw const RX_BUFS.0[i][0]) as *const _ as u64);
            let e = d.add(i * 4);
            core::ptr::write_volatile(e, crate::rx::desc_opts1(i, RX_DESC));
            core::ptr::write_volatile(e.add(1), 0);
            core::ptr::write_volatile(e.add(2) as *mut u64, buf_phys);
        }

        // Ordine di rtl_hw_start: unlock -> RxMaxSize -> descrittori -> lock
        // -> ChipCmd -> RxConfig. Seguito alla lettera, non reinventato.
        core::ptr::write_volatile((mmio + CFG_9346) as *mut u8, 0xC0);

        // RxMaxSize: senza questo il chip filtra tutto ("Low hurts")
        core::ptr::write_volatile((mmio + RMS) as *mut u16, RX_MAX_SIZE);

        // Indirizzo del ring: PRIMA l'alto, poi il basso (come Linux)
        core::ptr::write_volatile((mmio + RDSAR_HI) as *mut u32, (ring_phys >> 32) as u32);
        core::ptr::write_volatile((mmio + RDSAR_LO) as *mut u32, ring_phys as u32);

        core::ptr::write_volatile((mmio + CFG_9346) as *mut u8, 0x00);

        // Abilita trasmettitore E ricevitore (CmdTxEnb | CmdRxEnb)
        let cmd = core::ptr::read_volatile((mmio + CHIP_CMD) as *const u8);
        core::ptr::write_volatile((mmio + CHIP_CMD) as *mut u8, cmd | 0x04 | 0x08);

        // RxConfig per ultimo (rtl_init_rxcfg + rtl_set_rx_mode)
        core::ptr::write_volatile((mmio + RCR) as *mut u32, RX_CONFIG_VALUE);

        // Verifica: il ricevitore è acceso, la config accetta il broadcast,
        // RxMaxSize ha preso. Se una delle tre non torna, non si mente.
        let cmd_after = core::ptr::read_volatile((mmio + CHIP_CMD) as *const u8);
        let cfg_after = core::ptr::read_volatile((mmio + RCR) as *const u32);
        let max_after = core::ptr::read_volatile((mmio + RMS) as *const u16);
        RX_CMD = cmd_after;
        RX_RCR = cfg_after;
        RX_MAXSZ = max_after;
        RX_DESC0 = core::ptr::read_volatile(d);
        RX_INIT_OK = (cmd_after & 0x08) != 0
            && (cfg_after & 0x3F) == RX_ACCEPT
            && max_after == RX_MAX_SIZE;
        RX_INIT_OK
    }

    /// Ritira i pacchetti ricevuti verso la porta condivisa. Non bloccante:
    /// se non c'è niente, esce subito. Riarma ogni descrittore consumato.
    pub unsafe fn poll_rx() {
        if !RX_INIT_OK {
            return;
        }
        // Se il chip è andato in overflow, si ferma. Va svegliato: si scrive
        // indietro il bit (si pulisce) e si riabilita il ricevitore. Senza
        // questo, un solo overflow al boot zittisce la porta per sempre.
        let st = core::ptr::read_volatile((RTL_MMIO + INTR_STATUS) as *const u16);
        if st & (0x0010 | 0x0040) != 0 {
            core::ptr::write_volatile((RTL_MMIO + INTR_STATUS) as *mut u16, st);
            let cmd = core::ptr::read_volatile((RTL_MMIO + CHIP_CMD) as *const u8);
            core::ptr::write_volatile((RTL_MMIO + CHIP_CMD) as *mut u8, cmd | 0x08);
            RX_OVERFLOWS = RX_OVERFLOWS.wrapping_add(1);
        }
        let d = (&raw mut RX_RING.0) as *mut u32;
        for i in 0..RX_DESC {
            let e = d.add(i * 4);
            let opts1 = core::ptr::read_volatile(e);
            if opts1 & DESC_OWN != 0 {
                continue; // ancora in mano al chip: niente da prendere
            }
            let len = (opts1 & 0xFFFF) as u16;
            if len > 0 {
                let buf = (&raw const RX_BUFS.0[i][0]) as *const u8;
                let n = (len as usize).min(crate::rx::CAP).min(RX_BUF_LEN);
                let slice = core::slice::from_raw_parts(buf, n);
                crate::rx::feed(len, slice);
            } else {
                crate::rx::err();
            }
            // Riarma: torna in mano al chip. USA LA STESSA FUNZIONE dell'init:
            // era qui che la dimensione del buffer andava persa.
            core::ptr::write_volatile(e, crate::rx::desc_opts1(i, RX_DESC));
        }
    }

    /// Trasmette un frame. Ritorna true se il NIC l'ha preso in carico
    /// (DescOwn pulito dal chip = trasmesso).
    pub unsafe fn tx_frame(mmio: u64, frame: &[u8]) -> bool {
        if frame.is_empty() || frame.len() > 1518 {
            return false;
        }
        // copia il frame nel buffer DMA
        let buf = (&raw mut TX_BUF.0) as *mut u8;
        for (i, &b) in frame.iter().enumerate() {
            core::ptr::write_volatile(buf.add(i), b);
        }
        // arma il descriptor 0: DescOwn|First|Last|len|RingEnd
        let d = (&raw mut TX_RING.0) as *mut u32;
        let opts1 =
            DESC_OWN | DESC_FIRST_FRAG | DESC_LAST_FRAG | DESC_RING_END | frame.len() as u32;
        core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
        core::ptr::write_volatile(d, opts1);
        // poke: TxPoll = NPQ (normal priority queue)
        core::ptr::write_volatile((mmio + TX_POLL) as *mut u8, 0x40);
        // attesa: il chip pulisce DescOwn quando ha finito
        for _ in 0..2_000_000u32 {
            if core::ptr::read_volatile(d as *const u32) & DESC_OWN == 0 {
                return true;
            }
        }
        false
    }
}
