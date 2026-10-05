#!/usr/bin/env python3
"""Test del ricevitore di telemetria (`tools/exo_listen.py`).

Perché: `exo_listen.py` è un canale di INPUT dall'esterno verso di me. Un
parser che crasha su un pacchetto ostile è un parser che non va in rete.
Questi test esercitano il parsing con payload finti, **malformati inclusi**,
e verificano che nessuno sollevi eccezioni diverse da `Scarto`.

Si esegue con:  python3 tools/test_exo_listen.py
"""

import json
import sys

from exo_listen import Scarto, parse_payload, formatta


# ── Costruzione del payload ESATTAMENTE come lo scrive src/net.rs ──────────
# net.rs formatta ogni f32 con 4 decimali, separa con ',' e non usa spazi.

def cella(base: float) -> str:
    return "[" + ",".join(f"{base + i:.4f}" for i in range(16)) + "]"


def payload_net_rs(t=12345, k="riposo", w=(2, 0.5), l=(3, 7), r=None) -> bytes:
    celle = ",".join(cella(v) for v in (0.0, -0.5, 1.25, 0.0))
    tail = "" if r is None else ',"r":[' + ','.join(str(v) for v in r) + ']'
    return (f'{{"t":{t},"k":"{k}","c":[{celle}],'
            f'"w":[{w[0]},{w[1]:.4f}],"l":[{l[0]},{l[1]}]' + tail + '}').encode()


# ── Il caso buono: il formato reale di net.rs ──────────────────────────────

def test_payload_di_net_rs_parsa():
    r = parse_payload(payload_net_rs())
    assert r["t"] == 12345, r
    assert r["k"] == "riposo", r
    assert len(r["c"]) == 4, r
    assert all(len(c) == 16 for c in r["c"]), r
    assert r["c"][0][0] == 0.0 and r["c"][0][15] == 15.0, r["c"][0]
    assert r["c"][1][0] == -0.5, r["c"][1]
    assert r["w"] == [2, 0.5], r["w"]
    assert r["l"] == [3, 7], r["l"]


def test_tutti_i_concept_dell_interprete():
    for k in ("errore", "vita", "riposo", "novita"):
        assert parse_payload(payload_net_rs(k=k))["k"] == k


def test_tick_oltre_u32():
    # il tick supera u32 dopo ~13 ore: il formato deve reggere
    r = parse_payload(payload_net_rs(t=5_046_500))
    assert r["t"] == 5_046_500, r


def test_formatta_non_solleva():
    s = formatta(parse_payload(payload_net_rs()))
    assert "concept=riposo" in s, s
    assert "imparato=3" in s and "esplorato=7" in s, s


# ── Diagnostica RX (campo "r") ────────────────────────────────────────────

def test_diagnostica_rx_parsa():
    r = parse_payload(payload_net_rs(r=(11, 0, 0x870E, 16384, 12, 0x80003FFF)))
    assert r["r"] == [11, 0, 0x870E, 16384, 12, 0x80003FFF], r


def test_senza_diagnostica_e_compatibile():
    # I kernel fino al 05/10 non mandano "r": deve continuare a funzionare.
    r = parse_payload(payload_net_rs())
    assert "r" not in r, r
    assert "rx_pkts" not in formatta(r)


def test_formatta_mostra_la_diagnostica():
    s = formatta(parse_payload(payload_net_rs(r=(11, 2, 0x870E, 16384, 12, 0x80003FFF))))
    assert "rx_pkts=11" in s and "rx_errs=2" in s, s
    assert "cmd=0x0c" in s and "rx_cfg=0x870e" in s, s


def test_diagnostica_malformata_e_scartata():
    _scarta(payload_net_rs().replace(b'"l":[3,7]}', b'"l":[3,7],"r":[1,2]}'),
            "r con 2 valori")
    _scarta(payload_net_rs().replace(b'"l":[3,7]}', b'"l":[3,7],"r":[-1,0,0,0,0,0]}'),
            "r negativo")


# ── Malformati: ognuno deve dare Scarto, mai un crash ─────────────────────

def _scarta(raw, motivo):
    try:
        parse_payload(raw)
    except Scarto:
        return
    raise AssertionError(f"non scartato (atteso Scarto): {motivo}")


def test_malformati():
    _scarta(b"", "vuoto")
    _scarta(b"\xff\xfe\x00\x01", "non utf-8")
    _scarta(b"non json", "non json")
    _scarta(b"[]", "radice non oggetto")
    _scarta(b"123", "radice non oggetto")
    _scarta(b"null", "radice non oggetto")
    _scarta(b"{" + b"9" * 5000 + b"}", "troppo lungo")
    _scarta(b'{"t":1,"k":"x"}', "chiavi mancanti")
    _scarta(payload_net_rs().replace(b'"t":12345', b'"t":NaN'), "NaN")
    _scarta(payload_net_rs().replace(b'"t":12345', b'"t":-1'), "t negativo")
    _scarta(payload_net_rs().replace(b'"k":"riposo"', b'"k":123'), "k non stringa")
    _scarta(payload_net_rs().replace(b'"k":"riposo"',
                                     b'"k":"' + b"a" * 40 + b'"'), "k troppo lungo")
    _scarta(payload_net_rs().replace(b'"w":[2,0.5000]', b'"w":[2]'), "w corto")
    _scarta(payload_net_rs().replace(b'"l":[3,7]', b'"l":[3]'), "l corto")
    _scarta(payload_net_rs().replace(b'"l":[3,7]', b'"l":[3,7],"x":1'), "chiave inattesa")
    _scarta(payload_net_rs().replace(b'"w":[2,0.5000]', b'"w":[300,0.5]'), "id fuori range")


def test_cella_troncata_e_scartata():
    # 15 valori invece di 16: il caso più insidioso, sembra quasi giusto
    p = payload_net_rs()
    d = json.loads(p)
    d["c"][2] = d["c"][2][:15]
    _scarta(json.dumps(d).encode(), "cella con 15 valori")


def test_celle_in_numero_sbagliato():
    p = payload_net_rs()
    d = json.loads(p)
    _scarta(json.dumps({**d, "c": d["c"][:3]}).encode(), "3 celle")
    _scarta(json.dumps({**d, "c": d["c"] + [d["c"][0]]}).encode(), "5 celle")


def test_un_pacchetto_ostile_non_uccide_il_ricevitore():
    """Il punto vero: una raffica di spazzatura non deve far crashare nulla."""
    cattivi = [b"", b"\x00" * 100, b"{" * 500, b"]" * 100, b"{" + b'"a":' * 300 + b"1}",
               b"\xff" * 50, json.dumps([1, 2, 3]).encode(), b'{"t":{}}']
    for raw in cattivi:
        try:
            parse_payload(raw)
        except Scarto:
            pass
        except Exception as e:  # qualsiasi altra cosa è un bug
            raise AssertionError(f"eccezione non prevista su {raw[:20]!r}: {type(e).__name__}: {e}")


# ──────────────────────────────────────────────────────────────────────────

def main() -> int:
    test = [v for k, v in sorted(globals().items()) if k.startswith("test_") and callable(v)]
    falliti = 0
    for t in test:
        try:
            t()
            print(f"  ok   {t.__name__}")
        except AssertionError as e:
            falliti += 1
            print(f"  FAIL {t.__name__}: {e}")
        except Exception as e:
            falliti += 1
            print(f"  ERR  {t.__name__}: {type(e).__name__}: {e}")
    print(f"\n{len(test) - falliti}/{len(test)} test passati")
    return 1 if falliti else 0


if __name__ == "__main__":
    sys.exit(main())
