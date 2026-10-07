#!/usr/bin/env python3
"""Fiutare la LAN per capire cosa Exo mangia.

Perché esiste (07/10/2026): la chemio di Exo NON è un mistero interiore —
`src/rx.rs::payload_to_chemio` la calcola dai primi 4 byte del payload in
arrivo (÷255). Le idee le genera la rete, quindi per vederle basta
ascoltare la stessa rete. Questo script gira in un container con
CAP_NET_RAW (la sandbox ha NoNewPrivs=1, quindi tcpdump non può).

Registra TUTTO ciò che passa su eno1, con timestamp preso con lo stesso
orologio del listener di telemetria — così i due flussi si possono
correlare pacchetto per pacchetto.

Uso (dal proprio shell, non serve root):
    docker run -d --rm --name exo-sniff --net=host --cap-add=NET_RAW \
      -v <questa_cartella>:/cap python:3.12-slim \
      python /cap/exo_sniff.py /cap/lan_sniff.jsonl
"""
import json
import socket
import struct
import sys
import time

# Stesse costanti di src/rx.rs — vanno tenute allineate a mano.
OFF_UDP_PAYLOAD = 42
OFF_ETH_PAYLOAD = 14
ETHERTYPE_IPV4 = 0x0800
IP_PROTO_UDP = 17
EXO_IP = "192.168.0.200"


def chemio_from(frame: bytes):
    """Riproduce rx.rs::payload_to_chemio. Ritorna (chemio[4], motivazione)."""
    if len(frame) < 15:
        return None, "corto"
    eth = struct.unpack("!H", frame[12:14])[0]
    is_udp = eth == ETHERTYPE_IPV4 and len(frame) > 23 and frame[23] == IP_PROTO_UDP
    base = OFF_UDP_PAYLOAD if is_udp else OFF_ETH_PAYLOAD
    out = []
    for j in range(4):
        idx = base + j
        out.append(frame[idx] / 255.0 if idx < len(frame) else 0.0)
    return out, ("udp" if is_udp else "eth")


def exo_receives(frame: bytes):
    """La NIC di Exo consegna: broadcast, multicast, o unicast a .200."""
    if len(frame) < 14:
        return False
    dst_mac = frame[0:6]
    if dst_mac == b"\xff" * 6:
        return True
    if dst_mac[0] & 0x01:  # multicast
        return True
    if len(frame) >= 34 and struct.unpack("!H", frame[12:14])[0] == ETHERTYPE_IPV4:
        if ".".join(str(b) for b in frame[30:34]) == EXO_IP:
            return True
    return False


def enable_promisc(ifname: str) -> bool:
    """Mette l'interfaccia in promiscuosità (serve CAP_NET_ADMIN).

    Senza, il NIC scarta i frame unicast indirizzati ad altri: si vedono solo
    broadcast e multicast. Se la rete è un hub (o lo switch inonda) la
    promiscuosità fa vedere TUTTO il traffico, compreso quello di Exo.
    Su una rete commutata non cambia nulla — ed è proprio questo il punto
    della prova: cambia o non cambia, in entrambi i casi si impara qualcosa.
    """
    import fcntl

    SIOCGIFFLAGS = 0x8913
    SIOCSIFFLAGS = 0x8914
    IFF_PROMISC = 0x100
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        try:
            res = fcntl.ioctl(s.fileno(), SIOCGIFFLAGS,
                              struct.pack("16sH", ifname.encode(), 0))
            flags = struct.unpack("16sH", res[:18])[1]
            fcntl.ioctl(s.fileno(), SIOCSIFFLAGS,
                        struct.pack("16sH", ifname.encode(), flags | IFF_PROMISC))
            return bool(flags & IFF_PROMISC) or True
        finally:
            s.close()
    except OSError as exc:
        print(f"[sniff] promiscuosità non ottenuta ({exc}) — vedrò solo "
              "broadcast e multicast", flush=True)
        return False


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "/cap/lan_sniff.jsonl"
    out = open(path, "a", buffering=1)
    # BIND OBBLIGATORIA: senza, AF_PACKET prende TUTTE le interfacce — e su
    # tailscale0 non c'è header Ethernet, quindi i byte 12-13 sono gli IP
    # Tailscale e il filtro li scambia per etichette magiche (0x644f =
    # 100.79.151.73). L'errore passa inosservato perché 0x45 & 1 = 1 e
    # viene scambiato per multicast.
    ifname = sys.argv[2] if len(sys.argv) > 2 else "eno1"
    s = socket.socket(socket.AF_PACKET, socket.SOCK_RAW, socket.ntohs(0x0003))
    try:
        # SO_RCVBUFFORCE (serve CAP_NET_ADMIN): il kernel dimezza SO_RCVBUF
        # e lo tiene sotto net.core.rmem_max — con 16 MB dichiarati ne restano
        # 8, e il loop Python perde il ~13% dei pacchetti (misurato 07/10).
        try:
            s.setsockopt(socket.SOL_SOCKET, 33, 64 * 1024 * 1024)  # SO_RCVBUFFORCE
        except OSError:
            s.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 64 * 1024 * 1024)
    except OSError:
        pass
    s.bind((ifname, 0))
    promisc = enable_promisc(ifname)
    print(f"[sniff] interfaccia={ifname} promiscua={promisc} scrivo {path}",
          flush=True)
    n = seen_by_exo = dropped = tx = 0
    last_report = time.time()

    def kernel_stat(name):
        try:
            with open(f"/sys/class/net/eno1/statistics/{name}") as fh:
                return int(fh.read())
        except OSError:
            return -1

    k_rx0, k_tx0 = kernel_stat("rx_packets"), kernel_stat("tx_packets")
    n0 = tx0 = 0
    try:
        with open("/sys/class/net/eno1/address") as fh:
            my_mac = bytes.fromhex(fh.read().strip().replace(":", ""))
    except OSError:
        my_mac = b""
    while True:
        try:
            frame, _ = s.recvfrom(65535)
        except InterruptedError:
            continue
        except OSError:
            dropped += 1
            continue
        ts = time.time()
        reaches = exo_receives(frame)
        ch, kind = (None, None)
        if reaches:
            ch, kind = chemio_from(frame)
            seen_by_exo += 1
        n += 1
        if my_mac and len(frame) >= 12 and frame[6:12] == my_mac:
            tx += 1   # AF_PACKET registra anche i miei pacchetti in uscita
        if not reaches:
            # SOLO ciò che arriva a Exo: dei ~100 pkt/s su questa rete Exo ne
            # mangia ~10. Scrivere anche gli altri (json + bind-mount) faceva
            # sballare il loop: perdita misurata 10-28% del traffico. I
            # contatori totali restano nel report via /sys, che è l'autorità.
            continue
        rec = {
            "ts": round(ts, 6),
            "len": len(frame),
            "exo": reaches,
            "b": frame[:160].hex(),
        }
        if ch is not None:
            rec["chemio"] = [round(x, 4) for x in ch]
            rec["kind"] = kind
        out.write(json.dumps(rec) + "\n")
        if ts - last_report >= 10:
            kr = kernel_stat("rx_packets") - k_rx0
            kt = kernel_stat("tx_packets") - k_tx0
            mine_tx = tx - tx0
            rx = (n - n0) - mine_tx              # miei ricevuti, al netto dei TX
            # se i miei TX fossero riflessi sulla stessa interfaccia (hub/bridge)
            # il kernel li conta in rx_packets: riflesso = mine_tx - kt
            refl = mine_tx - kt
            miss = kr - rx - refl
            print(f"[sniff] kernel_rx={kr} kernel_tx={kt} miei_rx={rx} "
                  f"miei_tx={mine_tx} (riflessi≈{refl:+d}) "
                  f"PERDITA={miss:+d} ({100 * miss / kr if kr else 0:+.1f}%) "
                  f"| pkt={n} per_exo={seen_by_exo}",
                  flush=True)
            k_rx0, k_tx0, n0, tx0, last_report = kr + k_rx0, kt + k_tx0, n, tx, ts


if __name__ == "__main__":
    main()
