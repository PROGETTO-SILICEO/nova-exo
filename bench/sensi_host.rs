// ── Banco host per IL SISTEMA DEI SENSI (il registro dei canali) ──
//
// Il sistema è la parte che deve reggere quando aggiungiamo touchpad, telecamera,
// interocezione: se il registro è sbagliato, ogni senso nuovo è un caso speciale.
// Logica PURA: si verifica su host, col codice REALE (#[path] su src/sensi.rs).
//
// Compilazione: make test   (oppure rustc --test bench/sensi_host.rs)

#[path = "../src/sensi.rs"]
mod sensi;

use sensi::{Canale, Famiglia, Politica, Registro, Stato, Via, MAX_CANALI};

fn c(id: &'static str, via: Via, prio: u8) -> Canale {
    Canale {
        id, famiglia: Famiglia::Estero, via,
        latenza_max: 1, politica: Politica::Fresca,
        priorita: prio, stato: Stato::Valido,
    }
}

#[test]
fn registra_e_legge() {
    let mut r = Registro::new();
    assert!(r.registra(c("udito", Via::Riflesso, 0)));
    assert_eq!(r.len(), 1);
    assert_eq!(r.get(0).unwrap().id, "udito");
    assert!(r.get(1).is_none());
}

#[test]
fn il_registro_e_limitato() {
    let mut r = Registro::new();
    for _ in 0..MAX_CANALI {
        assert!(r.registra(c("x", Via::Integrazione, 1)));
    }
    assert!(!r.registra(c("troppo", Via::Integrazione, 1)), "il 17° canale deve essere rifiutato");
    assert_eq!(r.len(), MAX_CANALI);
}

#[test]
fn i_riflessi_vengono_prima() {
    // Il midollo non aspetta la corteccia: anche con priorità peggiore, un
    // riflesso deve essere servito prima di un'integrazione.
    let mut r = Registro::new();
    r.registra(c("chemio", Via::Integrazione, 0)); // indice 0
    r.registra(c("udito", Via::Riflesso, 5));      // indice 1
    r.registra(c("vista", Via::Integrazione, 1));  // indice 2
    let mut out = [0usize; MAX_CANALI];
    let m = r.ordine(&mut out);
    assert_eq!(m, 3);
    assert_eq!(out[0], 1, "il riflesso (udito) deve essere il primo");
}

#[test]
fn tra_pari_via_vince_la_priorita() {
    let mut r = Registro::new();
    r.registra(c("a", Via::Integrazione, 3)); // 0
    r.registra(c("b", Via::Integrazione, 1)); // 1
    let mut out = [0usize; MAX_CANALI];
    r.ordine(&mut out);
    assert_eq!(out[0], 1, "b (priorità 1) prima di a (priorità 3)");
}

#[test]
fn i_guasti_non_sono_silenziosi() {
    let mut r = Registro::new();
    let mut rotto = c("udito", Via::Riflesso, 0);
    rotto.stato = Stato::Guasto;
    r.registra(rotto);
    r.registra(c("tatto", Via::Riflesso, 0));
    assert_eq!(r.conta_guasti(), 1);
    assert_eq!(r.conta_via(Via::Riflesso), 2);
}

#[test]
fn ordine_conta_solo_i_canali_presenti() {
    let mut r = Registro::new();
    r.registra(c("solo", Via::Integrazione, 2));
    let mut out = [99usize; MAX_CANALI];
    assert_eq!(r.ordine(&mut out), 1);
    assert_eq!(out[0], 0);
    assert_eq!(out[1], 99, "non deve toccare oltre l'ultimo canale");
}
