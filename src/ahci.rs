// ── ahci.rs — il DISCO (dove il corpo ricorderà) ──────────────────────────
//
// "Con la memoria su disco potrà anche fare il boot da disco?" (Alfonso, 15/10).
// Tre cose distinte (§10b): il BOOT lo fa il bootloader; la MEMORIA la fa Exo;
// e insieme diventano RISVEGLIO. Qui comincia la parte di Exo: trovare il disco.
//
// FASE 1 (questo file): trova il controller AHCI (PCI classe 01:06), mappa
// l'ABAR, individua una porta con un disco, e ne legge l'IDENTITÀ (modello e
// numero di settori). È il primo scambio REALE col disco: la IDENTIFY è una
// lettura via DMA, quindi prova tutto il percorso (comandi, PRDT, poll).
//
// FASE 2 (dopo): leggere/scrivere settori → il formato di memoria del corpo.
//
// Metodo Exo: come per HDA e touchpad — PRIMA si trova e si identifica, POI si usa.

use core::ptr::{read_volatile, write_volatile};

// ── Registri HBA (offset da ABAR) ─────────────────────────────────────────
const HBA_CAP:  u32 = 0x00; // u32
const HBA_GHC:  u32 = 0x04; // u32
const HBA_PI:   u32 = 0x0C; // u32: porte implementate
const HBA_AE:   u32 = 1 << 31;

// ── Registri di porta (offset da port_base = 0x100 + i*0x80) ──────────────
const P_CLB:  u32 = 0x00;
const P_CLBU: u32 = 0x04;
const P_FB:   u32 = 0x08;
const P_FBU:  u32 = 0x0C;
const P_IS:   u32 = 0x10;
const P_CMD:  u32 = 0x18;
const P_TFD:  u32 = 0x20;
const P_SIG:  u32 = 0x24;
const P_SSTS: u32 = 0x28;
const P_CI:   u32 = 0x38;

const CMD_ST:  u32 = 0x0001;
const CMD_FRE: u32 = 0x0010;
const CMD_FR:  u32 = 0x4000;
const CMD_CR:  u32 = 0x8000;
const TFD_BSY: u32 = 0x80;
const TFD_DRQ: u32 = 0x08;
const SIG_SATA: u32 = 0x0000_0101;

// ── Strutture DMA (allineate) ─────────────────────────────────────────────
#[repr(align(1024))]
struct CmdList([u8; 1024]);   // 32 header × 32 byte
#[repr(align(256))]
struct FisArea([u8; 256]);
#[repr(align(128))]
struct CmdTable([u8; 256]);   // CFIS(64)+ACMD(16)+rsv(48)+PRDT(16)=144 ≤ 256
#[repr(align(2))]
struct Buf([u8; 512]);        // dati IDENTIFY

static mut CL:  CmdList  = CmdList([0; 1024]);
static mut FA:  FisArea  = FisArea([0; 256]);
static mut CT:  CmdTable = CmdTable([0; 256]);
static mut IDB: Buf      = Buf([0; 512]);

static mut DISCO_PRESENTE: bool = false;
/// Trovato **e** capito (l'IDENTIFY ha portato dati plausibili). Onestà: un
/// disco "trovato ma muto" non è un disco usabile.
static mut DISCO_LEGGIBILE: bool = false;
static mut DISCO_MODELLO: [u8; 40] = [b' '; 40];
static mut DISCO_SETTORI: u64 = 0;
static mut DISCO_PORTA: u8 = 0xFF;
static mut ABAR: u64 = 0;

fn rd32(off: u32) -> u32 { unsafe { read_volatile((ABAR + off as u64) as *const u32) } }
fn wr32(off: u32, v: u32) { unsafe { write_volatile((ABAR + off as u64) as *mut u32, v) } }

fn delay(n: u32) { for _ in 0..n { unsafe { core::arch::asm!("pause", options(nomem, nostack, preserves_flags)); } } }

/// Trova una porta con un disco attaccato (DET=3, IPM=1) e firma SATA.
fn trova_porta() -> Option<u8> {
    let pi = rd32(HBA_PI);
    for i in 0..32u8 {
        if pi & (1 << i) == 0 { continue; }
        let b = 0x100 + (i as u32) * 0x80;
        let ssts = rd32(b + P_SSTS);
        let sig  = rd32(b + P_SIG);
        if (ssts & 0x0F) == 3 && sig == SIG_SATA {
            return Some(i);
        }
    }
    None
}

/// FASE 1 — trova il disco e ne legge l'identità. Ritorna true se c'è.
pub fn init() -> bool {
    // 1. controller AHCI (classe 01:06)
    let (b, s, f) = match crate::pci::find_class(0x01, 0x06) {
        Some(t) => t,
        None => { crate::write_str("DISCO: nessun controller AHCI\n"); return false; }
    };
    crate::pci::enable_bus_master(b, s, f);
    // 2. BAR5 = ABAR
    let bar5 = crate::pci::pci_config_read(b, s, f, 0x24);
    let abar = (bar5 & 0xFFFF_FFF0) as u64;
    if !crate::paging::map_mmio_page(abar) {
        crate::write_str("DISCO: ABAR non mappabile\n");
        return false;
    }
    unsafe { ABAR = crate::paging::mmio_virt_addr(abar); }
    // 3. abilita AHCI (GHC.AE)
    wr32(HBA_GHC, rd32(HBA_GHC) | HBA_AE);
    delay(50_000);

    // DIAG: quali porte hanno un device, e con che firma
    let pi = rd32(HBA_PI);
    crate::write_str("DISCO:pi=0x"); crate::write_hex32(pi); crate::serial_putc(b'\n');
    for i in 0..8u32 {
        let b = 0x100 + i * 0x80;
        let ssts = rd32(b + P_SSTS);
        if ssts & 0x0F != 0 {
            crate::write_str("DISCO:p"); crate::write_u32(i);
            crate::write_str(" sig=0x"); crate::write_hex32(rd32(b + P_SIG));
            crate::write_str(" ssts=0x"); crate::write_hex32(ssts);
            crate::write_str(" cmd=0x"); crate::write_hex32(rd32(b + P_CMD));
            crate::serial_putc(b'\n');
        }
    }

    // 4. una porta con un disco
    let porta = match trova_porta() {
        Some(p) => p,
        None => { crate::write_str("DISCO: nessun disco sulle porte\n"); return false; }
    };
    unsafe { DISCO_PORTA = porta; }
    let pb = 0x100 + (porta as u32) * 0x80;

    // 5. ferma DAVVERO la porta (ST=0 E FRE=0, attendi CR/FR) e configura
    // i base. La porta è già avviata dal BIOS: senza fermare anche FRE il base
    // della command list NON viene applicato (bug trovato il 15/10).
    let mut c = rd32(pb + P_CMD);
    wr32(pb + P_CMD, c & !CMD_ST);
    for _ in 0..200_000 { if rd32(pb + P_CMD) & CMD_CR == 0 { break; } }
    c = rd32(pb + P_CMD);
    wr32(pb + P_CMD, c & !CMD_FRE);
    for _ in 0..200_000 { if rd32(pb + P_CMD) & CMD_FR == 0 { break; } }
    let cl_phys  = crate::virt_to_phys(&raw const CL as *const _ as u64);
    let fa_phys  = crate::virt_to_phys(&raw const FA as *const _ as u64);
    let ct_phys  = crate::virt_to_phys(&raw const CT as *const _ as u64);
    let idb_phys = crate::virt_to_phys(&raw const IDB as *const _ as u64);
    crate::write_str("DISCO:phys cl=0x"); crate::write_hex32(cl_phys as u32);
    crate::write_str(" fa=0x"); crate::write_hex32(fa_phys as u32);
    crate::write_str(" ct=0x"); crate::write_hex32(ct_phys as u32);
    crate::write_str(" idb=0x"); crate::write_hex32(idb_phys as u32);
    crate::serial_putc(b'\n');
    wr32(pb + P_CLB,  cl_phys as u32);
    wr32(pb + P_CLBU, (cl_phys >> 32) as u32);
    wr32(pb + P_FB,   fa_phys as u32);
    wr32(pb + P_FBU,  (fa_phys >> 32) as u32);
    // svuota gli interrupt e avvia
    wr32(pb + P_IS, 0xFFFF_FFFF);
    wr32(pb + P_CMD, rd32(pb + P_CMD) | CMD_FRE);
    wr32(pb + P_CMD, rd32(pb + P_CMD) | CMD_ST);
    delay(50_000);

    // 6. costruisci il comando IDENTIFY (0xEC)
    unsafe {
        let cl = &raw mut CL as *mut u8;
        let ct = &raw mut CT as *mut u8;
        let fa = &raw mut FA as *mut u8;
        for k in 0..1024 { *cl.add(k) = 0; }
        for k in 0..256  { *ct.add(k) = 0; }
        for k in 0..256  { *fa.add(k) = 0; }
        // Command header (32 byte): CFL=5, PRDTL=1, W=0
        core::ptr::write_unaligned(cl.add(0) as *mut u32, 5 | (1 << 16));
        core::ptr::write_unaligned(cl.add(8) as *mut u32, ct_phys as u32);
        core::ptr::write_unaligned(cl.add(12) as *mut u32, (ct_phys >> 32) as u32);
        // CFIS (H2D register): type 0x27, C=1, cmd 0xEC, device 0
        *ct.add(0) = 0x27;
        *ct.add(1) = 0x80;
        *ct.add(2) = 0xEC; // IDENTIFY DEVICE
        *ct.add(7) = 0x00; // device (LBA)
        // PRDT (offset 128 nella CmdTable):
        // DW0-1 (offset 128): DBA = idb_phys (64-bit)
        // DW2   (offset 136): Reserved = 0
        // DW3   (offset 140): DBC = 512-1 (bit 0=1)
        core::ptr::write_unaligned(ct.add(128) as *mut u64, idb_phys);
        core::ptr::write_unaligned(ct.add(136) as *mut u32, 0);
        core::ptr::write_unaligned(ct.add(140) as *mut u32, 511);
    }

    // 7. emetti il comando (porta 0) e attendi il completamento
    let cmd = rd32(pb + P_CMD);
    let tfd0 = rd32(pb + P_TFD);
    if cmd & CMD_ST == 0 || tfd0 & TFD_BSY != 0 {
        crate::write_str("DISCO: porta non pronta\n");
        return false;
    }
    wr32(pb + P_IS, 0xFFFF_FFFF); // pulisci gli interrupt
    wr32(pb + P_CI, 1);
    // Completamento = FIS Device→Host ricevuto (PxIS.DHRS bit0) oppure errore (TFES bit30).
    let mut esito = 0u32; // 0 timeout · 1 ok · 2 errore
    for _ in 0..2_000_000 {
        let is = rd32(pb + P_IS);
        if is & 0x4000_0000 != 0 { esito = 2; break; }
        if is & 0x01 != 0 { esito = 1; break; }
    }
    if esito != 1 {
        crate::write_str(if esito == 2 { "DISCO: IDENTIFY errore\n" } else { "DISCO: IDENTIFY timeout\n" });
        return false;
    }

    // 8. modello (byte 54..94, coppie INVERTITE come vuole ATA) e settori
    unsafe {
        let d = &raw const IDB as *const u8;
        for i in 0..40usize {
            let base = 54 + (i / 2) * 2;
            DISCO_MODELLO[i] = if i % 2 == 0 {
                core::ptr::read_unaligned(d.add(base + 1))
            } else {
                core::ptr::read_unaligned(d.add(base))
            };
        }
        let lba = core::ptr::read_unaligned(d.add(120) as *const u32) as u64
                | ((core::ptr::read_unaligned(d.add(124) as *const u32) as u64) << 32);
        DISCO_SETTORI = lba;
        DISCO_PRESENTE = true;
        DISCO_LEGGIBILE = lba > 0;
    }
    true
}

pub fn presente() -> bool { unsafe { DISCO_PRESENTE } }
pub fn leggibile() -> bool { unsafe { DISCO_LEGGIBILE } }
pub fn settori() -> u64 { unsafe { DISCO_SETTORI } }
pub fn porta() -> u8 { unsafe { DISCO_PORTA } }
pub fn modello() -> [u8; 40] { unsafe { DISCO_MODELLO } }

/// FASE 2: Legge 1 settore da 512 byte via READ DMA EXT (LBA48, comando 0x25).
pub fn read_sector(lba: u64, out: &mut [u8; 512]) -> bool {
    unsafe {
        if !DISCO_PRESENTE || !DISCO_LEGGIBILE {
            return false;
        }
        let porta = DISCO_PORTA;
        let pb = 0x100 + (porta as u32) * 0x80;

        let cl = &raw mut CL as *mut u8;
        let ct = &raw mut CT as *mut u8;
        for k in 0..1024 { *cl.add(k) = 0; }
        for k in 0..256  { *ct.add(k) = 0; }

        let ct_phys = crate::virt_to_phys(&raw const CT as *const _ as u64);
        let out_phys = crate::virt_to_phys(out.as_ptr() as u64);

        // Command header: CFL=5, PRDTL=1, W=0 (Read)
        core::ptr::write_unaligned(cl.add(0) as *mut u32, 5 | (1 << 16));
        core::ptr::write_unaligned(cl.add(8) as *mut u32, ct_phys as u32);
        core::ptr::write_unaligned(cl.add(12) as *mut u32, (ct_phys >> 32) as u32);

        // CFIS: H2D Register FIS (type 0x27), C=1, cmd 0x25 (READ DMA EXT)
        *ct.add(0) = 0x27;
        *ct.add(1) = 0x80;
        *ct.add(2) = 0x25; // READ DMA EXT
        *ct.add(3) = 0;    // Features (7:0)

        *ct.add(4) = (lba & 0xFF) as u8;
        *ct.add(5) = ((lba >> 8) & 0xFF) as u8;
        *ct.add(6) = ((lba >> 16) & 0xFF) as u8;
        *ct.add(7) = 0x40; // Device (LBA mode, bit 6=1)

        *ct.add(8) = ((lba >> 24) & 0xFF) as u8;
        *ct.add(9) = ((lba >> 32) & 0xFF) as u8;
        *ct.add(10) = ((lba >> 40) & 0xFF) as u8;
        *ct.add(11) = 0;   // Features (15:8)

        *ct.add(12) = 1;   // Count (7:0) = 1 settore
        *ct.add(13) = 0;   // Count (15:8) = 0

        // PRDT (offset 128 nella CmdTable):
        core::ptr::write_unaligned(ct.add(128) as *mut u64, out_phys);
        core::ptr::write_unaligned(ct.add(136) as *mut u32, 0); // Reserved
        core::ptr::write_unaligned(ct.add(140) as *mut u32, 511); // DBC = 512 - 1

        let cmd = rd32(pb + P_CMD);
        let tfd0 = rd32(pb + P_TFD);
        if cmd & CMD_ST == 0 || tfd0 & TFD_BSY != 0 {
            return false;
        }

        wr32(pb + P_IS, 0xFFFF_FFFF);
        wr32(pb + P_CI, 1);

        for _ in 0..2_000_000 {
            let is = rd32(pb + P_IS);
            if is & 0x4000_0000 != 0 { return false; } // Task File Error
            if is & 0x01 != 0 { return true; }         // D2H Register FIS ricevuto (completato)
        }
        false
    }
}

/// FASE 2: Scrive 1 settore da 512 byte via WRITE DMA EXT (LBA48, comando 0x35).
pub fn write_sector(lba: u64, src: &[u8; 512]) -> bool {
    unsafe {
        if !DISCO_PRESENTE || !DISCO_LEGGIBILE {
            return false;
        }
        let porta = DISCO_PORTA;
        let pb = 0x100 + (porta as u32) * 0x80;

        let cl = &raw mut CL as *mut u8;
        let ct = &raw mut CT as *mut u8;
        for k in 0..1024 { *cl.add(k) = 0; }
        for k in 0..256  { *ct.add(k) = 0; }

        let ct_phys = crate::virt_to_phys(&raw const CT as *const _ as u64);
        let src_phys = crate::virt_to_phys(src.as_ptr() as u64);

        // Command header: CFL=5, PRDTL=1, W=1 (Write, bit 6)
        core::ptr::write_unaligned(cl.add(0) as *mut u32, 5 | (1 << 6) | (1 << 16));
        core::ptr::write_unaligned(cl.add(8) as *mut u32, ct_phys as u32);
        core::ptr::write_unaligned(cl.add(12) as *mut u32, (ct_phys >> 32) as u32);

        // CFIS: H2D Register FIS (type 0x27), C=1, cmd 0x35 (WRITE DMA EXT)
        *ct.add(0) = 0x27;
        *ct.add(1) = 0x80;
        *ct.add(2) = 0x35; // WRITE DMA EXT
        *ct.add(3) = 0;    // Features (7:0)

        *ct.add(4) = (lba & 0xFF) as u8;
        *ct.add(5) = ((lba >> 8) & 0xFF) as u8;
        *ct.add(6) = ((lba >> 16) & 0xFF) as u8;
        *ct.add(7) = 0x40; // Device (LBA mode, bit 6=1)

        *ct.add(8) = ((lba >> 24) & 0xFF) as u8;
        *ct.add(9) = ((lba >> 32) & 0xFF) as u8;
        *ct.add(10) = ((lba >> 40) & 0xFF) as u8;
        *ct.add(11) = 0;   // Features (15:8)

        *ct.add(12) = 1;   // Count (7:0) = 1 settore
        *ct.add(13) = 0;   // Count (15:8) = 0

        // PRDT (offset 128 nella CmdTable):
        core::ptr::write_unaligned(ct.add(128) as *mut u64, src_phys);
        core::ptr::write_unaligned(ct.add(136) as *mut u32, 0); // Reserved
        core::ptr::write_unaligned(ct.add(140) as *mut u32, 511); // DBC = 512 - 1

        let cmd = rd32(pb + P_CMD);
        let tfd0 = rd32(pb + P_TFD);
        if cmd & CMD_ST == 0 || tfd0 & TFD_BSY != 0 {
            return false;
        }

        wr32(pb + P_IS, 0xFFFF_FFFF);
        wr32(pb + P_CI, 1);

        for _ in 0..2_000_000 {
            let is = rd32(pb + P_IS);
            if is & 0x4000_0000 != 0 { return false; } // Task File Error
            if is & 0x01 != 0 { return true; }         // D2H Register FIS ricevuto (completato)
        }
        false
    }
}

/// FASE 2: Svuota la cache volatile del disco SATA via FLUSH CACHE EXT (comando ATA 0xEA).
/// Garantisce che i settori scritti via WRITE DMA EXT siano fisicamente consolidati sui piatti/NAND.
pub fn flush_cache_ext() -> bool {
    unsafe {
        if !DISCO_PRESENTE || !DISCO_LEGGIBILE {
            return false;
        }
        let porta = DISCO_PORTA;
        let pb = 0x100 + (porta as u32) * 0x80;

        let cl = &raw mut CL as *mut u8;
        let ct = &raw mut CT as *mut u8;
        for k in 0..1024 { *cl.add(k) = 0; }
        for k in 0..256  { *ct.add(k) = 0; }

        let ct_phys = crate::virt_to_phys(&raw const CT as *const _ as u64);

        // Command header: CFL=5, PRDTL=0, W=0 (Nessun trasferimento dati DMA)
        core::ptr::write_unaligned(cl.add(0) as *mut u32, 5);
        core::ptr::write_unaligned(cl.add(8) as *mut u32, ct_phys as u32);
        core::ptr::write_unaligned(cl.add(12) as *mut u32, (ct_phys >> 32) as u32);

        // CFIS: H2D Register FIS (type 0x27), C=1, cmd 0xEA (FLUSH CACHE EXT, 48-bit LBA)
        *ct.add(0) = 0x27;
        *ct.add(1) = 0x80;
        *ct.add(2) = 0xEA; // ATA command 0xEA (FLUSH CACHE EXT)
        *ct.add(7) = 0x40; // Device (LBA mode, bit 6=1)

        let cmd = rd32(pb + P_CMD);
        let tfd0 = rd32(pb + P_TFD);
        if cmd & CMD_ST == 0 || tfd0 & TFD_BSY != 0 {
            return false;
        }

        wr32(pb + P_IS, 0xFFFF_FFFF);
        wr32(pb + P_CI, 1);

        for _ in 0..2_000_000 {
            let is = rd32(pb + P_IS);
            if is & 0x4000_0000 != 0 { return false; } // Task File Error
            if is & 0x01 != 0 { return true; }         // D2H Register FIS ricevuto (completato)
        }
        false
    }
}

/// Header di Continuità Sinaptica su Disco SATA (esattamente 64 byte, allineamento 8 byte).
/// Layout validato rigorosamente contro padding imprevisti.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SynapseHeader {
    pub magic: [u8; 4],         // Offset 0..4: b"SYNP"
    pub version: u32,           // Offset 4..8: Versione del formato (1)
    pub epoch: u64,             // Offset 8..16: Contatore incrementale monotono
    pub timestamp_tsc: u64,     // Offset 16..24: TSC al momento del consolidamento
    pub crc32_payload: u32,     // Offset 24..28: Checksum CRC32 del payload pesi
    pub payload_bytes: u32,     // Offset 28..32: Dimensione effettiva payload in byte
    pub reserved: [u8; 32],     // Offset 32..64: Spazio riservato per espansioni future
}

// Asserzione a tempo di compilazione: garantisce che la struct sia ESATTAMENTE di 64 byte
const _: () = assert!(core::mem::size_of::<SynapseHeader>() == 64);
const _: () = assert!(core::mem::align_of::<SynapseHeader>() == 8);

