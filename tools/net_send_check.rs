// ── Prova di accordo fra le due metà ──
//
// Costruisce il frame con il codice REALE (src/net.rs), ne estrae il payload
// UDP e lo spedisce in locale al ricevitore (tools/exo_listen.py).
// Se il parser accetta quello che il costruttore produce, il contratto fra
// il kernel e il ricevitore è verificato SENZA hardware.
//
// Non sta in bench/ perché il Makefile compila tutto bench/*.rs come test:
// un main() lì dentro romperebbe `make test`.
//
// Uso: rustc -O tools/net_send_check.rs -o /tmp/opencode/net_send_check
//      /tmp/opencode/net_send_check [porta]

#[path = "../src/net.rs"]
mod net;

use std::net::UdpSocket;

fn main() {
    let porta: u16 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(net::UDP_PORT);

    let mut buf = [0u8; 1518];

    // Una cella con valori realistici (il chemio vero oscilla ~[-0.2, 0.1])
    let mut cells = [[0.0f32; 16]; 4];
    for (ci, cell) in cells.iter_mut().enumerate() {
        for (i, v) in cell.iter_mut().enumerate() {
            *v = (ci as f32 * 0.1) - 0.2 + (i as f32 * 0.01);
        }
    }

    let n = net::build_state_frame(
        &mut buf,
        [0xc8, 0x5b, 0x76, 0xe0, 0xe0, 0x01],
        5_046_500, // tick reale misurato sul 110 il 05/10
        "riposo",
        &cells,
        2,
        0.5,
        3,
        7,
    );
    assert!(n > 0, "build_state_frame non ha scritto nulla");

    let off = 14 + 20 + 8;
    let payload = &buf[off..n];
    let testo = std::str::from_utf8(payload).expect("payload utf8");

    println!("frame: {n} byte, payload {} byte", payload.len());
    println!("payload: {testo}");

    // controllo di forma: ethertype IPv4 e protocollo UDP
    assert_eq!(u16::from_be_bytes([buf[12], buf[13]]), 0x0800, "ethertype");
    assert_eq!(buf[23], 17, "protocollo IP");

    let sock = UdpSocket::bind("127.0.0.1:0").expect("bind mittente");
    sock.send_to(payload, ("127.0.0.1", porta))
        .expect("invio");
    println!("inviato a 127.0.0.1:{porta}");
}
