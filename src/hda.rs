// ── hda.rs — Intel High Definition Audio (il MICROFONO / l'udito) ─────────
//
// La coda dei sensi (ROADMAP_EXO, decisa con Alfonso 12/10/2026) parte
// dall'udito. Come per la tastiera: UN PEZZO alla volta, ognuno verificato
// sul metallo prima di passare al successivo.
//
// STADIO 1 (questo file, 13/10/2026) — far TROVARE l'udito al corpo:
//   1. trova il controller HDA su PCI (classe 04:03, non per vendor:device:
//      sul Lenovo reale l'HDA non è quello di QEMU);
//   2. mappa il BAR0 MMIO;
//   3. resetta il controller (GCTL.CRST 0→1);
//   4. enumera il codec (STATESTS) e ne legge l'IDENTITÀ (vendor:device, rev);
//   5. percorre l'albero dei widget e lo stampa (per scegliere, con i fatti,
//      il convertitore di ingresso allo stadio 2).
//
// COMANDI: si usa la via canonica CORB→RIRB (ring buffer in memoria), NON le
// "immediate command" IC/IR: verificato il 13/10 in QEMU che quella via non
// risponde (IR legge 0xFFFFFFFF). CORB/RIRB è ciò che usano tutti i driver
// veri (Linux compreso) ed è supportata ovunque.
//
// STADIO 2 (dopo la verifica di questo) — stream di cattura: BDL, buffer
// ciclico, formato, RUN; poi l'ENERGIA del suono → canale di senso.
//
// ONESTÀ (metodo Exo, anti-mock): in QEMU l'audio di cattura viene dal backend
// host; con `-audiodev none` il CONTENUTO è silenzio. Qui si verifica il
// DEVICE, non il suono. Il suono vero si misurerà sul Lenovo, che ha un
// microfono fisico. Non si inietta audio finto: mai.

use core::ptr::{read_volatile, write_volatile};

// ── Controller registers (offset da BAR0) ────────────────────────────────
const HDA_GCAP:     u32 = 0x00; // u16: capacità globali (n° stream in/out, 64-bit)
const HDA_VMIN:     u32 = 0x02; // u8
const HDA_VMAJ:     u32 = 0x03; // u8
const HDA_GCTL:     u32 = 0x08; // u32: CRST (bit0) = controller run/reset
const HDA_STATESTS: u32 = 0x0E; // u16: bit N = codec presente sulla linea SDI N
const HDA_CORBLBASE: u32 = 0x40; // u32
const HDA_CORBUBASE: u32 = 0x44; // u32
const HDA_CORBWP:    u32 = 0x48; // u16
const HDA_CORBRP:    u32 = 0x4A; // u16
const HDA_CORBCTL:   u32 = 0x4C; // u8
const HDA_CORBSTS:   u32 = 0x4D; // u8
const HDA_CORBSIZE:  u32 = 0x4E; // u8
const HDA_RIRBLBASE: u32 = 0x50; // u32
const HDA_RIRBUBASE: u32 = 0x54; // u32
const HDA_RIRBWP:    u32 = 0x58; // u16
const HDA_RINTCNT:   u32 = 0x5A; // u16
const HDA_RIRBCTL:   u32 = 0x5C; // u8
const HDA_RIRBSTS:   u32 = 0x5D; // u8
const HDA_RIRBSIZE:  u32 = 0x5E; // u8

const GCTL_CRST: u32 = 0x0000_0001;

// ── Verbi del codec (HDA spec) ───────────────────────────────────────────
const VERB_GET_PARAM: u16 = 0xF00;
const PAR_VENDOR_ID:  u16 = 0x00; // [31:16]=vendor [15:0]=device
const PAR_REVISION:   u16 = 0x02;
const PAR_NODE_COUNT: u16 = 0x04; // [7:0]=start [15:8]=count
const PAR_WIDGET_CAP: u16 = 0x09; // [23:20]=widget type
const PAR_PIN_CAP:    u16 = 0x0C; // [5]=INPUT capace
const PAR_CONNLIST_LEN: u16 = 0x0E; // quanti nodi nella lista di connessione
const VERB_GET_CONN_LIST:      u16 = 0xF02; // lista sorgenti di un widget
const VERB_GET_CONFIG_DEFAULT: u16 = 0xF1C; // tipo di jack di un pin ([23:20]=device)
const VERB_SET_POWER_STATE:    u16 = 0x705; // D0 = acceso
const VERB_SET_CONN_SELECT:    u16 = 0x701; // sceglie la sorgente di ingresso

// Tipi di widget (HDA): 1 = Audio Input (ADC), 4 = Pin Complex
const WID_AUD_IN: u8 = 1;
const WID_PIN: u8 = 4;
// Tipo di jack (config default): 0xA = Mic In
const JACK_MIC_IN: u8 = 0xA;
// Pin cap: bit5 = ingresso
const PINCAP_INPUT: u32 = 0x20;

// CORB 256 entry × 4 B = 1 KB · RIRB 256 entry × 8 B = 2 KB (allineati)
const RING_ENTRIES: usize = 256;

#[repr(align(1024))]
struct CorbBuf([u32; RING_ENTRIES]);
#[repr(align(2048))]
struct RirbBuf([u32; RING_ENTRIES * 2]);

static mut CORB: CorbBuf = CorbBuf([0; RING_ENTRIES]);
static mut RIRB: RirbBuf = RirbBuf([0; RING_ENTRIES * 2]);

// ── Stato (letto una volta, esposto alla telemetria) ─────────────────────
static mut MMIO: u64 = 0;
static mut CODEC: u8 = 0;
static mut VENDOR: u16 = 0;
static mut DEVICE: u16 = 0;
static mut REVISION: u32 = 0xFFFF_FFFF;
static mut GCAP: u16 = 0;
static mut PRESENT: bool = false;
static mut CORB_OK: bool = false;
/// Maschera dei codec presenti (bit N = codec N su STATESTS). Il Lenovo ne ha
/// DUE: l'HDMI della scheda video (solo uscite) e il Realtek (col microfono):
/// bisogna SCEGLIERE quello con l'ingresso, non il primo.
static mut CODEC_MASK: u16 = 0;
/// Nodi SCOPERTI (ADC e pin del microfono): esposti per la GUI, così si vedono
/// senza seriale (il Lenovo non ha seriale) e si capisce se sono quelli giusti.
static mut ADC_NODO: u8 = 0xFF;
static mut PIN_NODO: u8 = 0xFF;
/// [0]=found [1]=reset_ok [2]=codec_ok [3]=codec_addr
static mut PROBE: [u8; 4] = [0, 0, 0, 0xFF];

// ── MMIO access ──────────────────────────────────────────────────────────
#[inline]
fn rd32(off: u32) -> u32 { unsafe { read_volatile((MMIO + off as u64) as *const u32) } }
#[inline]
fn wr32(off: u32, v: u32) { unsafe { write_volatile((MMIO + off as u64) as *mut u32, v) } }
#[inline]
fn rd16(off: u32) -> u16 { unsafe { read_volatile((MMIO + off as u64) as *const u16) } }
#[inline]
fn wr16(off: u32, v: u16) { unsafe { write_volatile((MMIO + off as u64) as *mut u16, v) } }
#[inline]
fn rd8(off: u32) -> u8 { unsafe { read_volatile((MMIO + off as u64) as *const u8) } }
#[inline]
fn wr8(off: u32, v: u8) { unsafe { write_volatile((MMIO + off as u64) as *mut u8, v) } }

fn delay(n: u32) {
    for _ in 0..n {
        unsafe { core::arch::asm!("pause", options(nomem, nostack, preserves_flags)); }
    }
}

/// Allestisce i due ring buffer in memoria e avvia il trasporto comandi.
/// CORB = comandi SW→controller; RIRB = risposte controller→SW.
fn corb_init() {
    let corb_phys = crate::virt_to_phys(&raw const CORB as *const _ as u64);
    let rirb_phys = crate::virt_to_phys(&raw const RIRB as *const _ as u64);

    wr32(HDA_CORBLBASE, corb_phys as u32);
    wr32(HDA_CORBUBASE, (corb_phys >> 32) as u32);
    wr32(HDA_RIRBLBASE, rirb_phys as u32);
    wr32(HDA_RIRBUBASE, (rirb_phys >> 32) as u32);

    // Reset dei puntatori di lettura/scrittura (bit 15 = reset)
    wr16(HDA_CORBWP, 0);
    wr16(HDA_CORBRP, 0x8000);
    delay(50_000);
    wr16(HDA_CORBRP, 0x0000);
    wr16(HDA_RIRBWP, 0x8000);

    // Dimensione: 256 entry per entrambi (campo [1:0] = 0b10)
    let cs = rd8(HDA_CORBSIZE);
    wr8(HDA_CORBSIZE, (cs & 0xC0) | 0x02);
    let rs = rd8(HDA_RIRBSIZE);
    wr8(HDA_RIRBSIZE, (rs & 0xC0) | 0x02);

    // RIRB: una risposta per interrupt (RINTCNT=1) e IRQ abilitato.
    // PERCHÉ l'IRQ (lezione 13/10, verificata sul codice di QEMU): la CORB si
    // blocca quando `rirb_count == rirb_cnt`, e `rirb_count` si AZZERA SOLO se
    // l'IRQ è abilitato e lo sgombero di RIRBSTS abbassa il bit IRQ. Senza IRQ
    // la CORB si ferma dopo la prima risposta (sintomo: solo 1 verbo risponde).
    wr16(HDA_RINTCNT, 1);

    // Avvia i ring (bit 1 = run; RIRB anche bit 0 = interrupt enable)
    wr8(HDA_CORBCTL, 0x02);
    wr8(HDA_RIRBCTL, 0x03);
    delay(50_000);

    unsafe { CORB_OK = rd8(HDA_CORBSTS) & 0x02 != 0 || rd8(HDA_CORBCTL) & 0x02 != 0; }
}

/// Invia un verbo via CORB e attende la risposta su RIRB. 0xFFFF_FFFF = nulla.
fn codec_cmd(addr: u8, node: u8, verb: u16, payload: u16) -> u32 {
    let cmd = ((addr as u32) << 28)
        | ((node as u32) << 20)
        | ((verb as u32) << 8)
        | payload as u32;

    unsafe {
        let corb = &raw mut CORB as *mut u32;
        let rirb = &raw const RIRB as *const u32;

        let old_rp = (rd16(HDA_RIRBWP) & 0xFF) as usize;

        let wp = (rd16(HDA_CORBWP) + 1) & 0xFF;
        core::ptr::write_volatile(corb.add(wp as usize), cmd);
        wr16(HDA_CORBWP, wp);

        // Attende che il controller scriva una risposta (RIRBWP avanza)
        let mut got = false;
        for _ in 0..2_000_000 {
            if (rd16(HDA_RIRBWP) & 0xFF) as usize != old_rp { got = true; break; }
        }
        if !got { return 0xFFFF_FFFF; }

        let rp = (rd16(HDA_RIRBWP) & 0xFF) as usize;
        let resp = core::ptr::read_volatile(rirb.add(rp * 2)); // low word = risposta
        wr8(HDA_RIRBSTS, 0x5); // clear RIRBINT + RIRBOIS → azzera rirb_count (CORB riparte)
        resp
    }
}

fn wstr(s: &str) { crate::write_str(s); }
fn wh16(v: u16) { crate::write_hex16(v); }
fn wh32(v: u32) { crate::write_hex32(v); }

/// Percorre e stampa l'albero dei widget (diagnostica: serve a scegliere il
/// convertitore di ingresso con i fatti, non a indovinare).
fn walk(node: u8, depth: u8) {
    let cap = codec_cmd(unsafe { CODEC }, node, VERB_GET_PARAM, PAR_WIDGET_CAP);
    let wtype = ((cap >> 20) & 0xF) as u8;
    wstr("  HDA:w node=0x"); wh16(node as u16);
    wstr(" type="); wh16(wtype as u16);
    wstr(" cap=0x"); wh32(cap);
    crate::serial_putc(b'\n');

    if depth >= 3 || cap == 0xFFFF_FFFF { return; }
    let nc = codec_cmd(unsafe { CODEC }, node, VERB_GET_PARAM, PAR_NODE_COUNT);
    // Get Parameter 0x04 (Subordinate Node Count): [31:16]=start, [15:0]=count
    // (come Linux snd_hda_get_sub_nodes — non >>8: verificato 13/10 su QEMU).
    let start = ((nc >> 16) & 0xFF) as u8;
    let count = (nc & 0xFF) as u8;
    let count = if count > 16 { 16 } else { count };
    for i in 0..count {
        walk(start.wrapping_add(i), depth + 1);
    }
}

/// STADIO 1 — trova, resetta, enumera. Ritorna true se il codec risponde.
pub fn init() -> bool {
    // 1. Trova il controller (classe 04:03; fallback QEMU 8086:2668)
    let dev = crate::pci::find_class(0x04, 0x03)
        .or_else(|| crate::pci::pci_find_device(0x8086, 0x2668));
    let (b, s, f) = match dev {
        Some(t) => t,
        None => { wstr("HDA: assente\n"); return false; }
    };
    unsafe { PROBE[0] = 1; }
    crate::pci::enable_bus_master(b, s, f);
    let (_, _, base) = crate::pci::read_bars(b, s, f);

    wstr("HDA: pci="); wh16(b as u16); crate::serial_putc(b':');
    wh16(s as u16); crate::serial_putc(b'.'); wh16(f as u16);
    wstr(" bar0=0x"); wh32(base as u32);
    crate::serial_putc(b'\n');

    if !crate::paging::map_mmio_page(base) {
        wstr("HDA: mmio non mappabile (fuori 3GB..4GB?)\n");
        return false;
    }
    unsafe { MMIO = crate::paging::mmio_virt_addr(base); }

    // 2. Capacità globali
    let gcap = rd16(HDA_GCAP);
    let vmin = rd8(HDA_VMIN);
    let vmaj = rd8(HDA_VMAJ);
    unsafe { GCAP = gcap; }
    wstr("HDA: gcap=0x"); wh16(gcap);
    wstr(" ver="); wh16(vmaj as u16); crate::serial_putc(b'.'); wh16(vmin as u16);
    wstr(" outs="); wh16(((gcap >> 12) & 0xF));
    wstr(" ins="); wh16(((gcap >> 8) & 0xF));
    crate::serial_putc(b'\n');

    // 3. Reset: CRST 0 → attesa → CRST 1
    wr32(HDA_GCTL, 0);
    delay(200_000);
    wr32(HDA_GCTL, GCTL_CRST);
    delay(500_000);
    let gctl = rd32(HDA_GCTL);
    unsafe { PROBE[1] = if gctl & GCTL_CRST != 0 { 1 } else { 0 }; }

    // 4. Attendi che il codec venga enumerato
    let mut st = 0u16;
    for _ in 0..200_000 {
        st = rd16(HDA_STATESTS);
        if st != 0 { break; }
    }
    if st == 0 {
        wstr("HDA: nessun codec su STATESTS\n");
        return false;
    }
    let mut codec = 0u8;
    for i in 0..16u8 {
        if st & (1 << i) != 0 { codec = i; break; }
    }
    wr16(HDA_STATESTS, st); // pulizia
    unsafe { CODEC = codec; CODEC_MASK = st; PROBE[2] = 1; PROBE[3] = codec; }

    // 4b. CORB/RIRB e primo comando
    corb_init();
    wstr("HDA: corb_sts=0x"); let csts = rd8(HDA_CORBSTS); wh16(csts as u16);
    wstr(" corb_ctl=0x"); let cctl = rd8(HDA_CORBCTL); wh16(cctl as u16);
    crate::serial_putc(b'\n');

    // 5. Identità del codec
    let vid = codec_cmd(codec, 0, VERB_GET_PARAM, PAR_VENDOR_ID);
    let rev = codec_cmd(codec, 0, VERB_GET_PARAM, PAR_REVISION);
    unsafe {
        VENDOR = ((vid >> 16) & 0xFFFF) as u16;
        DEVICE = (vid & 0xFFFF) as u16;
        REVISION = rev;
        PRESENT = vid != 0xFFFF_FFFF;
    }
    wstr("HDA: codec="); wh16(codec as u16);
    let (vend, devi) = unsafe { (VENDOR, DEVICE) };
    wstr(" id="); wh16(vend); crate::serial_putc(b':'); wh16(devi);
    wstr(" rev=0x"); wh32(rev);
    crate::serial_putc(b'\n');

    // 6. Albero dei widget (diagnostica → stadio 2)
    if unsafe { PRESENT } {
        wstr("HDA: widget tree\n");
        walk(0, 0);
        wstr("HDA: UDITO trovato e identificato\n");
    } else {
        wstr("HDA: codec muto (RIRB senza risposta)\n");
    }
    unsafe { PRESENT }
}

// ── STADIO 2 — stream di cattura (il suono che entra) ────────────────────
// Verbi del codec per il flusso di ingresso
const VERB_SET_STREAM_FORMAT:  u16 = 0x002;
const VERB_SET_AMP_GAIN_MUTE:  u16 = 0x003; // all nodes (ADC: input amp)
const VERB_SET_CONV_STREAMID:  u16 = 0x706;
const VERB_SET_PIN_CTL:        u16 = 0x707;

// Nodi del codec hda-micro (dal layout verificato): 4 = ADC, 5 = pin microfono
const ADC_NODE: u8 = 4;
const PIN_NODE: u8 = 5;

// Stream descriptor di INGRESSO #0 (QEMU implementa gli ingressi da 0x80)
const SD0:     u32 = 0x80;
const SD_CTL:  u32 = 0x00; // u32: [4:0] SRST/RUN, [23:20] stream tag
const SD_STS:  u32 = 0x03; // u8
const SD_LPIB: u32 = 0x04; // u32: posizione link (avanza = la DMA gira)
const SD_CBL:  u32 = 0x08; // u32: cyclic buffer length
const SD_LVI:  u32 = 0x0C; // u16: last valid index
const SD_FIFOW: u32 = 0x0E;
const SD_FMT:  u32 = 0x12; // u16: formato
const SD_BDPL: u32 = 0x18; // u32
const SD_BDPU: u32 = 0x1C; // u32

const STREAM_TAG: u32 = 1;
// Formato: 16 bit, 2 canali, 48 kHz → 0x0011 (come hda_codec_parse_fmt di QEMU)
const FMT_16BIT_2CH_48K: u16 = 0x0011;

const CHUNK: usize = 4096;
const CHUNKS: usize = 2;
const BUF_BYTES: usize = CHUNK * CHUNKS;

#[repr(align(4096))]
struct AudioBuf([i16; BUF_BYTES / 2]);
#[repr(align(128))]
struct BdlBuf([u8; 16 * CHUNKS]);

static mut AUDIO: AudioBuf = AudioBuf([0; BUF_BYTES / 2]);
static mut BDL: BdlBuf = BdlBuf([0; 16 * CHUNKS]);
static mut AUDIO_ON: bool = false;

/// Allestisce lo stream di cattura: BDL, buffer ciclico, formato, poi il
/// codec (ADC nodo 4 + pin nodo 5) e infine RUN. Da chiamare dopo `init()`.
/// SCOPERTA DEI NODI (14/10/2026): trova da sé l'ADC (Audio Input) e il pin del
/// microfono percorrendo l'albero dei widget. PRIMA erano fissi ai nodi di QEMU
/// (4 e 5): sul codec VERO (Realtek) sono altri → l'udito leggeva 0. Ora il
/// driver **chiede al codec chi è**, invece di indovinare. (Lezione del 14/10:
/// QEMU mi rendeva cieca all'hardware vero.)
fn scopri_nodi(c: u8) -> (u8, u8) {
    let mut adc: u8 = 0xFF;
    let mut pin: u8 = 0xFF;
    let mut pin_mic: u8 = 0xFF;

    let nc0 = codec_cmd(c, 0, VERB_GET_PARAM, PAR_NODE_COUNT);
    let fs = ((nc0 >> 16) & 0xFF) as u8;
    let fcount = (nc0 & 0xFF) as u8;
    for fi in 0..fcount.min(4) {
        let func = fs.wrapping_add(fi);
        let nc1 = codec_cmd(c, func, VERB_GET_PARAM, PAR_NODE_COUNT);
        let ws = ((nc1 >> 16) & 0xFF) as u8;
        let wcount = (nc1 & 0xFF) as u8;
        for wi in 0..wcount.min(32) {
            let node = ws.wrapping_add(wi);
            let cap = codec_cmd(c, node, VERB_GET_PARAM, PAR_WIDGET_CAP);
            if cap == 0xFFFF_FFFF { continue; }
            let wtype = ((cap >> 20) & 0xF) as u8;
            if wtype == WID_AUD_IN && adc == 0xFF {
                adc = node;
            } else if wtype == WID_PIN {
                let pincap = codec_cmd(c, node, VERB_GET_PARAM, PAR_PIN_CAP);
                if pincap & PINCAP_INPUT != 0 {
                    if pin == 0xFF { pin = node; }
                    let cfg = codec_cmd(c, node, VERB_GET_CONFIG_DEFAULT, 0);
                    let dev = ((cfg >> 20) & 0xF) as u8;
                    if dev == JACK_MIC_IN && pin_mic == 0xFF { pin_mic = node; }
                }
            }
        }
    }
    if pin_mic != 0xFF { pin = pin_mic; }
    (adc, pin)
}

pub fn stream_init() -> bool {
    if !present() { return false; }
    let codec = unsafe { CODEC };

    let buf_phys = crate::virt_to_phys(&raw const AUDIO as *const _ as u64);
    let bdl_phys = crate::virt_to_phys(&raw const BDL as *const _ as u64);

    // BDL: CHUNKS descrittori {addr u64, len u32, flags u32}; l'ultimo ha IOC
    unsafe {
        let b = &raw mut BDL as *mut u8;
        for i in 0..CHUNKS {
            let addr = buf_phys + (i * CHUNK) as u64;
            let len = CHUNK as u32;
            let flags: u32 = if i == CHUNKS - 1 { 0x1 } else { 0 };
            core::ptr::write_unaligned(b.add(i * 16) as *mut u64, addr);
            core::ptr::write_unaligned(b.add(i * 16 + 8) as *mut u32, len);
            core::ptr::write_unaligned(b.add(i * 16 + 12) as *mut u32, flags);
        }
    }

    // Registri dello stream: BDL, lunghezza ciclica, LVI, formato
    wr32(SD0 + SD_BDPL, bdl_phys as u32);
    wr32(SD0 + SD_BDPU, (bdl_phys >> 32) as u32);
    wr32(SD0 + SD_CBL, BUF_BYTES as u32);
    wr16(SD0 + SD_LVI, (CHUNKS - 1) as u16);
    wr16(SD0 + SD_FIFOW, 4);
    wr16(SD0 + SD_FMT, FMT_16BIT_2CH_48K);

    // Reset dello stream (SRST) poi pronto a girare
    wr32(SD0 + SD_CTL, 0x01);
    delay(50_000);
    wr32(SD0 + SD_CTL, 0x00);
    delay(50_000);

    // ── SCOPERTA: ADC e pin del microfono (non più fissi ai nodi di QEMU) ──
    // ── SCEGLI IL CODEC COL MICROFONO ──
    // Il Lenovo ha DUE codec (HDMI + Realtek): il primo (HDMI) NON ha ingressi.
    // Si prova ogni codec presente e si prende quello che HA un ADC.
    let mut adc = 0xFFu8;
    let mut pin = 0xFFu8;
    let mut codec = codec;
    let mask = unsafe { CODEC_MASK };
    for i in 0..16u8 {
        if mask & (1 << i) == 0 { continue; }
        let (a, p) = scopri_nodi(i);
        wstr("HDA: prova codec="); wh16(i as u16);
        wstr(" adc="); wh16(a as u16); wstr(" pin="); wh16(p as u16);
        crate::serial_putc(b'\n');
        if a != 0xFF && p != 0xFF {
            codec = i; adc = a; pin = p;
            break;
        }
    }
    unsafe { CODEC = codec; ADC_NODO = adc; PIN_NODO = pin; }
    if adc == 0xFF || pin == 0xFF {
        wstr("HDA: nessun codec con ADC/pin — udito assente\n");
        return false;
    }
    wstr("HDA: SCELTO codec="); wh16(codec as u16);
    wstr(" adc="); wh16(adc as u16); wstr(" pin="); wh16(pin as u16);
    crate::serial_putc(b'\n');
    codec_cmd(codec, adc, VERB_SET_POWER_STATE, 0x0000); // D0 (acceso)
    codec_cmd(codec, pin, VERB_SET_POWER_STATE, 0x0000);
    // l'ADC sceglie il pin come sorgente d'ingresso (indice nella sua lista)
    let clen = (codec_cmd(codec, adc, VERB_GET_PARAM, PAR_CONNLIST_LEN) & 0x7F) as u32;
    if clen > 0 {
        let list = codec_cmd(codec, adc, VERB_GET_CONN_LIST, 0);
        let mut idx: u16 = 0;
        for b in 0..clen.min(4) {
            if ((list >> (8 * b)) & 0xFF) as u8 == pin { idx = b as u16; }
        }
        codec_cmd(codec, adc, VERB_SET_CONN_SELECT, idx);
    }
    // formato, stream-id, amp d'ingresso (sull'ADC), pin in ingresso
    codec_cmd(codec, adc, VERB_SET_STREAM_FORMAT, FMT_16BIT_2CH_48K);
    codec_cmd(codec, adc, VERB_SET_CONV_STREAMID, ((STREAM_TAG as u16) << 4) | 0);
    codec_cmd(codec, adc, VERB_SET_AMP_GAIN_MUTE, 0x707F); // input amp, L+R, gain max
    codec_cmd(codec, pin, VERB_SET_PIN_CTL, 0x0020);      // AC_PINCTL_IN_EN

    // RUN: tag nei bit 20-23 + bit1 (run). Il controller avvisa il codec
    // (intel_hda_notify_codecs) e la cattura parte.
    wr32(SD0 + SD_CTL, (STREAM_TAG << 20) | 0x02);
    delay(50_000);

    let on = rd32(SD0 + SD_CTL) & 0x02 != 0;
    unsafe { AUDIO_ON = on; }
    wstr("HDA: stream ctl=0x"); wh32(rd32(SD0 + SD_CTL));
    wstr(" lpib="); wh32(rd32(SD0 + SD_LPIB));
    wstr(" on="); wh16(if on { 1 } else { 0 });
    crate::serial_putc(b'\n');
    on
}

/// Posizione del puntatore DMA (avanza se la cattura gira).
/// 0 se l'udito non c'è: MAI leggere l'MMIO quando il device è assente
/// (MMIO=0 → si leggerebbe un indirizzo illegale — bug chiuso il 13/10).
pub fn lpib() -> u32 {
    if !present() { return 0; }
    rd32(SD0 + SD_LPIB)
}

/// ENERGIA del suono catturato: media del valore assoluto, normalizzata 0..1.
/// Con `-audiodev none` (QEMU) il contenuto è silenzio → ~0. Sul Lenovo il
/// microfono vero dà il valore reale. Nessun dato finto, mai.
/// La matematica vive in `dsp::energia`, testata su host (bench/hda_host.rs).
pub fn energy() -> f32 {
    if !present() { return 0.0; }
    let s: &[i16] = unsafe { &AUDIO.0 };
    crate::dsp::energia(s)
}

/// PICCO del suono catturato (max |x|, 0..1). Distingue un suono forte da un
/// brusio diffuso a parità di energia. Stessa guardia di `energy()`.
pub fn picco() -> f32 {
    if !present() { return 0.0; }
    let s: &[i16] = unsafe { &AUDIO.0 };
    crate::dsp::picco(s)
}

pub fn stream_on() -> bool { unsafe { AUDIO_ON } }

/// Riga diagnostica periodica (LPIB + energia) per il monitor.
pub fn report() {
    if !present() { return; } // senza udito non si tocca l'MMIO
    wstr("HDA:t lpib="); wh32(lpib());
    let e = energy();
    wstr(" energia="); wh32((e * 10000.0) as u32);
    crate::serial_putc(b'\n');
}

/// Serve al consumatore (telemetria/pediatra) per sapere se l'udito c'è.
pub fn present() -> bool { unsafe { PRESENT } }
/// (ADC, pin) scoperti per il microfono: 0xFF = non trovati. Per la GUI.
pub fn nodi() -> (u8, u8) { unsafe { (ADC_NODO, PIN_NODO) } }
/// [found, reset_ok, codec_ok, codec_addr]
pub fn probe() -> [u8; 4] { unsafe { PROBE } }
pub fn vendor_device() -> (u16, u16) { unsafe { (VENDOR, DEVICE) } }
pub fn gcap() -> u16 { unsafe { GCAP } }
