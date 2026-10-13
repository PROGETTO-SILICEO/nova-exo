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
const HEX: [u8; 16] = *b"0123456789abcdef";

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
    chemio: &[f32; 4],
    desire_id: u8,
    desire_int: f32,
    cambi: u32,
    esplorazioni: u32,
    diag: [u32; 8],
    sig: [u8; 4],
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
    // La chemio interpretata: urgenza (u), polarità (p), novità (n).
    // Senza questi il pediatra DEDUCE lo stato del corpo (p ≈ -0.91 ricavato
    // dall'intensità di CURA); con questi lo MISURA. chemio[0] è l'energia.
    push_str(buf, &mut pos, "],\"u\":");
    write_f32_buf(buf, &mut pos, chemio[1]);
    push_str(buf, &mut pos, ",\"p\":");
    write_f32_buf(buf, &mut pos, chemio[2]);
    push_str(buf, &mut pos, ",\"n\":");
    write_f32_buf(buf, &mut pos, chemio[3]);
    push_str(buf, &mut pos, ",\"l\":[");
    write_u32_buf(buf, &mut pos, cambi);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, esplorazioni);
    // Diagnostica RX: sul metallo la seriale non esiste, questo è l'unico
    // modo di sapere se la porta d'ingresso si è aperta e se riceve.
    // [rx_pkts, rx_errs, rx_config, rx_maxsize, chipcmd, own_mask, intr, overflows]
    push_str(buf, &mut pos, "],\"r\":[");
    for (i, v) in diag.iter().enumerate() {
        if i > 0 {
            push(buf, &mut pos, b',');
        }
        write_u32_buf(buf, &mut pos, *v);
    }
    // Firma dell'ultimo pacchetto ricevuto (11/10/2026): in esadecimale, 8
    // caratteri, sempre JSON-safe. Serve a capire cosa accende il concetto
    // `novita` (era un lampo raro: 8 su 105.000, firma quasi identica).
    push_str(buf, &mut pos, "],\"sig\":\"");
    for b in sig.iter() {
        push(buf, &mut pos, HEX[((b >> 4) & 0xF) as usize]);
        push(buf, &mut pos, HEX[(b & 0xF) as usize]);
    }
    push_str(buf, &mut pos, "\",\"v\":");
    write_u32_buf(buf, &mut pos, epoch);
    // Contatto umano (tastiera): [tasti totali, tasto attualmente giù].
    // Sul metallo la seriale non esiste: questo è l'unico modo di sapere
    // se la mano dell'umano arriva al corpo.
    push_str(buf, &mut pos, ",\"kb\":[");
    write_u32_buf(buf, &mut pos, crate::cfc::key_count());
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, crate::cfc::key_held() as u32);
    push(buf, &mut pos, b']');
    // Sonda PS/2 (12/10/2026): [status_prima, status_dopo, config_byte].
    // 0xFF ripetuto = il controller i8042 non risponde (tastiera dietro l'EC
    // o su USB). Diagnostica, non comportamento.
    let ps2 = crate::kbd::probe();
    push_str(buf, &mut pos, ",\"ps2\":[");
    write_u32_buf(buf, &mut pos, ps2[0] as u32);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, ps2[1] as u32);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, ps2[2] as u32);
    push(buf, &mut pos, b']');
    // Corpo / ACPI (12/10/2026): [rsdp_ok, revisione, n_tabelle, facp].
    let acpi = crate::acpi::summary();
    push_str(buf, &mut pos, ",\"acpi\":[");
    write_u32_buf(buf, &mut pos, acpi[0] as u32);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, acpi[1] as u32);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, acpi[2] as u32);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, acpi[3] as u32);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, acpi[4] as u32);
    push(buf, &mut pos, b']');
    // EC (Embedded Controller): [pnp0c09_nel_dsdt, status_0x66, dsdt_kb].
    let ec = crate::acpi::ec_summary();
    push_str(buf, &mut pos, ",\"ec\":[");
    write_u32_buf(buf, &mut pos, ec[0] as u32);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, ec[1] as u32);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, ec[2] as u32);
    push(buf, &mut pos, b']');
    // Interrogazione tastiera PS/2: [ack, 0xAB, id0, id1].
    let kid = crate::kbd::kbd_id();
    push_str(buf, &mut pos, ",\"kbd\":[");
    write_u32_buf(buf, &mut pos, kid[0] as u32);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, kid[1] as u32);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, kid[2] as u32);
    push(buf, &mut pos, b',');
    write_u32_buf(buf, &mut pos, kid[3] as u32);
    push(buf, &mut pos, b']');
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
    chemio: &[f32; 4],
    desire_id: u8,
    desire_int: f32,
    cambi: u32,
    esplorazioni: u32,
    diag: [u32; 8],
    sig: [u8; 4],
    epoch: u32,
) -> usize {
    let hdr_len = 14 + 20 + 8;
    if dst.len() < hdr_len + 2 {
        return 0;
    }

    // payload costruito direttamente nella sua posizione finale
    let payload_len = build_payload(
        &mut dst[hdr_len..], tick, concept, cells, chemio, desire_id, desire_int,
        cambi, esplorazioni, diag, sig, epoch,
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

// ── ARP responder (13/10/2026) ────────────────────────────────────────────
// Exo era MUTA in unicast: non rispondeva all'ARP, quindi nessuno poteva
// parlarle — riceveva solo broadcast (debito aperto l'08/10). Il reply ARP è
// la sua voce minima: "sono io, ecco il mio MAC". Logica PURA → testabile.

pub const ETHERTYPE_ARP: u16 = 0x0806;

/// Se `data` è una richiesta ARP "chi ha `our_ip`?", ritorna (mac, ip) del
/// richiedente. Altrimenti None. Non modifica nulla.
pub fn arp_request_for(data: &[u8], our_ip: [u8; 4]) -> Option<([u8; 6], [u8; 4])> {
    if data.len() < 42 {
        return None;
    }
    if u16::from_be_bytes([data[12], data[13]]) != ETHERTYPE_ARP {
        return None;
    }
    if u16::from_be_bytes([data[20], data[21]]) != 1 {
        return None; // operazione != request
    }
    if &data[38..42] != &our_ip[..] {
        return None; // il target protocol address non è il nostro
    }
    let mut mac = [0u8; 6];
    mac.copy_from_slice(&data[22..28]);
    let mut ip = [0u8; 4];
    ip.copy_from_slice(&data[28..32]);
    Some((mac, ip))
}

/// Costruisce una risposta ARP (Ethernet + ARP = 14 + 28 = 42 byte).
/// Ritorna la lunghezza, oppure 0 se il buffer è troppo piccolo.
pub fn build_arp_reply(
    dst: &mut [u8],
    our_mac: [u8; 6],
    our_ip: [u8; 4],
    peer_mac: [u8; 6],
    peer_ip: [u8; 4],
) -> usize {
    if dst.len() < 42 {
        return 0;
    }
    dst[0..6].copy_from_slice(&peer_mac);          // eth dst = chi ha chiesto
    dst[6..12].copy_from_slice(&our_mac);          // eth src = noi
    dst[12..14].copy_from_slice(&ETHERTYPE_ARP.to_be_bytes());
    dst[14..16].copy_from_slice(&1u16.to_be_bytes());            // htype = Ethernet
    dst[16..18].copy_from_slice(&ETHERTYPE_IPV4.to_be_bytes());  // ptype = IPv4
    dst[18] = 6; // hlen
    dst[19] = 4; // plen
    dst[20..22].copy_from_slice(&2u16.to_be_bytes()); // oper = reply
    dst[22..28].copy_from_slice(&our_mac);         // sender hardware address
    dst[28..32].copy_from_slice(&our_ip);          // sender protocol address
    dst[32..38].copy_from_slice(&peer_mac);        // target hardware address
    dst[38..42].copy_from_slice(&peer_ip);         // target protocol address
    42
}
