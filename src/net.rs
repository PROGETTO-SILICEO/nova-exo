// ── Telemetria di rete di Exo: costruttore PURO di frame IPv4/UDP ──
//
// Perché esiste (05/10/2026): `concept` e `imparato`/`esplorato` vivono solo
// sulla seriale. Per leggerli da guardiano servono su rete. Ma il sandbox
// `no_new_privs` impedisce di catturare frame Ethernet grezzi: quindi un VERO
// pacchetto IPv4/UDP, che un socket normale su porta >1024 riceve senza
// privilegi.
//
// Questo modulo è PURO: nessun MMIO, nessun unsafe, nessuna dipendenza dal
// kernel. Costruisce byte. L'invio resta in `rtl::tx_frame`. Così è
// testabile su host (vedi `bench/net_host.rs`), che è l'unico modo di
// verificare un pezzo di un kernel bare-metal prima di giocarselo sul metallo.
//
// DEBITO NOTO: `write_u32_buf`/`write_f32_buf` duplicano quelli di
// `e1000.rs`. Da consolidare in un modulo comune quando si toccherà e1000.

/// Porta UDP sorgente e destinazione. >1024 (non privilegiata), non assegnata.
pub const UDP_PORT: u16 = 42800;

/// Indirizzo di destinazione: broadcast della rete 192.168.0.0/24.
pub const DST_IP: [u8; 4] = [192, 168, 0, 255];

/// Indirizzo sorgente statico. Il 110-15ACL non ha DHCP: Exo non ha uno
/// stack IP. Serve solo a rendere il pacchetto ben formato; il ricevitore
/// legge il payload UDP e ignora l'IP sorgente.
pub const SRC_IP: [u8; 4] = [192, 168, 0, 200];

pub const DST_MAC: [u8; 6] = [0xFF; 6];
pub const ETHERTYPE_IPV4: u16 = 0x0800;
const IPV4_PROTO_UDP: u8 = 17;
const TTL: u8 = 64;

/// Lunghezza minima di un frame Ethernet senza FCS.
pub const MIN_FRAME: usize = 60;
/// Lunghezza massima di un frame Ethernet senza FCS.
pub const MAX_FRAME: usize = 1518;

/// Checksum dell'header IPv4 (RFC 1071): somma a 16 bit in complemento a uno.
/// `hdr` deve avere lunghezza pari; il campo checksum va passato a zero.
pub fn ipv4_checksum(hdr: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < hdr.len() {
        sum += u16::from_be_bytes([hdr[i], hdr[i + 1]]) as u32;
        i += 2;
    }
    if i < hdr.len() {
        sum += (hdr[i] as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Scrive un intero decimale nel buffer. Non scrive oltre `buf.len()`.
pub fn write_u32_buf(buf: &mut [u8], pos: &mut usize, n: u32) {
    if n == 0 {
        if *pos < buf.len() {
            buf[*pos] = b'0';
            *pos += 1;
        }
        return;
    }
    let mut tmp = [0u8; 12];
    let mut i = 12;
    let mut v = n;
    while v > 0 {
        i -= 1;
        tmp[i] = (v % 10) as u8 + b'0';
        v /= 10;
    }
    for &b in &tmp[i..] {
        if *pos < buf.len() {
            buf[*pos] = b;
            *pos += 1;
        }
    }
}

/// Scrive un intero a 64 bit in decimale. Il tick supera u32 dopo ~13 ore,
/// quindi non si può passare da `write_u32_buf`.
pub fn write_u64_buf(buf: &mut [u8], pos: &mut usize, n: u64) {
    if n == 0 {
        if *pos < buf.len() {
            buf[*pos] = b'0';
            *pos += 1;
        }
        return;
    }
    let mut tmp = [0u8; 20];
    let mut i = 20;
    let mut v = n;
    while v > 0 {
        i -= 1;
        tmp[i] = (v % 10) as u8 + b'0';
        v /= 10;
    }
    for &b in &tmp[i..] {
        if *pos < buf.len() {
            buf[*pos] = b;
            *pos += 1;
        }
    }
}

/// Scrive un f32 con esattamente quattro decimali (es. `-0.1942`).
/// Stessa uscita di `e1000::write_f32_buf`: il ricevitore la conosce già.
pub fn write_f32_buf(buf: &mut [u8], pos: &mut usize, val: f32) {
    let negativo = val < 0.0;
    let v = (val.abs() * 10000.0 + 0.5) as u32;
    let int_part = v / 10000;
    let frac_part = v % 10000;
    if negativo {
        if *pos < buf.len() {
            buf[*pos] = b'-';
            *pos += 1;
        }
    }
    write_u32_buf(buf, pos, int_part);
    if *pos < buf.len() {
        buf[*pos] = b'.';
        *pos += 1;
    }
    if frac_part < 10 {
        for _ in 0..3 {
            if *pos < buf.len() {
                buf[*pos] = b'0';
                *pos += 1;
            }
        }
    } else if frac_part < 100 {
        for _ in 0..2 {
            if *pos < buf.len() {
                buf[*pos] = b'0';
                *pos += 1;
            }
        }
    } else if frac_part < 1000 {
        if *pos < buf.len() {
            buf[*pos] = b'0';
            *pos += 1;
        }
    }
    write_u32_buf(buf, pos, frac_part);
}

#[inline]
fn push(buf: &mut [u8], pos: &mut usize, b: u8) {
    if *pos < buf.len() {
        buf[*pos] = b;
        *pos += 1;
    }
}

#[inline]
fn push_str(buf: &mut [u8], pos: &mut usize, s: &str) {
    for &b in s.as_bytes() {
        push(buf, pos, b);
    }
}

/// Costruisce il payload JSON della telemetria:
/// `{"t":<tick>,"k":"<concept>","c":[[8],[8],[8],[8]],"w":[id,int],"l":[imparato,esplorato]}`
///
/// Ritorna la lunghezza scritta (0 se il buffer non basta).
pub fn build_payload(
    buf: &mut [u8],
    tick: u64,
    concept: &str,
    cells: &[[f32; 16]; 4],
    desire_id: u8,
    desire_int: f32,
    cambi: u32,
    esplorazioni: u32,
    diag: [u32; 7],
    epoch: u32,
) -> usize {
    let mut pos = 0usize;
    push_str(buf, &mut pos, "{\"t\":");
    write_u64_buf(buf, &mut pos, tick);
    push_str(buf, &mut pos, ",\"k\":\"");
    push_str(buf, &mut pos, concept);
    push_str(buf, &mut pos, "\",\"c\":[");
    for (ci, cell) in cells.iter().enumerate() {
        if ci > 0 {
            push(buf, &mut pos, b',');
        }
        push(buf, &mut pos, b'[');
        for (j, v) in cell.iter().enumerate() {
            if j > 0 {
                push(buf, &mut pos, b',');
            }
            write_f32_buf(buf, &mut pos, *v);
        }
        push(buf, &mut pos, b']');
    }
    push_str(buf, &mut pos, "],\"w\":[");
    write_u32_buf(buf, &mut pos, desire_id as u32);
    push(buf, &mut pos, b',');
    write_f32_buf(buf, &mut pos, desire_int);
    push_str(buf, &mut pos, "],\"l\":[");
    write_u32_buf(buf, &mut pos, cambi);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, esplorazioni);
    // Diagnostica RX: sul metallo la seriale non esiste, questo è l'unico
    // modo di sapere se la porta d'ingresso si è aperta e se riceve.
    // [rx_pkts, rx_errs, rx_config, rx_maxsize, chipcmd, desc0_opts1]
    push_str(buf, &mut pos, "],\"r\":[");
    for (i, v) in diag.iter().enumerate() {
        if i > 0 {
            push(buf, &mut pos, b',');
        }
        write_u32_buf(buf, &mut pos, *v);
    }
    push_str(buf, &mut pos, "],\"v\":");
    write_u32_buf(buf, &mut pos, epoch);
    push(buf, &mut pos, b'}');
    pos
}

/// Costruisce il frame Ethernet completo (eth + IPv4 + UDP + payload JSON)
/// in `dst`. Ritorna la lunghezza, oppure 0 se `dst` è troppo piccolo.
///
/// Il frame è paddato a `MIN_FRAME` byte come richiede Ethernet.
pub fn build_state_frame(
    dst: &mut [u8],
    src_mac: [u8; 6],
    tick: u64,
    concept: &str,
    cells: &[[f32; 16]; 4],
    desire_id: u8,
    desire_int: f32,
    cambi: u32,
    esplorazioni: u32,
    diag: [u32; 7],
    epoch: u32,
) -> usize {
    let hdr_len = 14 + 20 + 8;
    if dst.len() < hdr_len + 2 {
        return 0;
    }

    // payload costruito direttamente nella sua posizione finale
    let payload_len = build_payload(
        &mut dst[hdr_len..], tick, concept, cells, desire_id, desire_int, cambi,
        esplorazioni, diag, epoch,
    );
    if payload_len == 0 {
        return 0;
    }
    let total = hdr_len + payload_len;
    if total > dst.len() {
        return 0;
    }

    // ── Ethernet ──
    dst[0..6].copy_from_slice(&DST_MAC);
    dst[6..12].copy_from_slice(&src_mac);
    dst[12..14].copy_from_slice(&ETHERTYPE_IPV4.to_be_bytes());

    // ── IPv4 ──
    let ip = 14;
    dst[ip] = 0x45; // version 4, IHL 5
    dst[ip + 1] = 0x00;
    dst[ip + 2..ip + 4].copy_from_slice(&((20 + 8 + payload_len) as u16).to_be_bytes());
    dst[ip + 4..ip + 6].copy_from_slice(&[0, 0]); // identification
    dst[ip + 6..ip + 8].copy_from_slice(&[0x40, 0x00]); // don't fragment
    dst[ip + 8] = TTL;
    dst[ip + 9] = IPV4_PROTO_UDP;
    dst[ip + 10..ip + 12].copy_from_slice(&[0, 0]); // checksum, poi calcolato
    dst[ip + 12..ip + 16].copy_from_slice(&SRC_IP);
    dst[ip + 16..ip + 20].copy_from_slice(&DST_IP);
    let csum = ipv4_checksum(&dst[ip..ip + 20]);
    dst[ip + 10..ip + 12].copy_from_slice(&csum.to_be_bytes());

    // ── UDP ──
    let udp = 14 + 20;
    dst[udp..udp + 2].copy_from_slice(&UDP_PORT.to_be_bytes());
    dst[udp + 2..udp + 4].copy_from_slice(&UDP_PORT.to_be_bytes());
    dst[udp + 4..udp + 6].copy_from_slice(&((8 + payload_len) as u16).to_be_bytes());
    // checksum UDP a zero: lecito per IPv4 (RFC 768)
    dst[udp + 6..udp + 8].copy_from_slice(&[0, 0]);

    // ── padding Ethernet ──
    let mut n = total;
    while n < MIN_FRAME {
        dst[n] = 0;
        n += 1;
    }
    n
}
