// ── Banco host per gli archi riflessi ──
//
// Il primo riflesso di Exo (soprassalto a un suono che appare). La logica è PURA:
// qui si verifica su host, col codice REALE (#[path] su src/riflesso.rs).
// Il riflesso è la cosa che distingue "vivo" da "routine": deve essere veloce,
// non deve tremare, e non deve scattare per un rumore di fondo.
//
// Compilazione: make test   (oppure rustc --test bench/riflesso_host.rs)

#[path = "../src/riflesso.rs"]
mod riflesso;

use riflesso::Riflesso;

fn arco() -> Riflesso { Riflesso::new(0.5, 0.2, 200) }

#[test]
fn scatta_su_un_suono_che_appare() {
    let mut r = arco();
    assert!(!r.percepisce(0.0));
    assert!(r.percepisce(0.9), "il suono è apparso: deve soprassaltare");
    assert_eq!(r.scatti(), 1);
}

#[test]
fn non_scatta_col_silenzio() {
    let mut r = arco();
    for _ in 0..50 { assert!(!r.percepisce(0.0)); }
    assert_eq!(r.scatti(), 0);
}

#[test]
fn la_prima_percezione_non_scatta() {
    // Senza un "prima" non c'è salto: non ci si soprassalta alla nascita.
    let mut r = arco();
    assert!(!r.percepisce(0.9));
    assert_eq!(r.scatti(), 0);
}

#[test]
fn un_suono_forte_costante_non_fa_soprassaltare_due_volte() {
    let mut r = arco();
    assert!(!r.percepisce(0.0));
    assert!(r.percepisce(0.9));                       // appare: scatta
    for _ in 0..50 { assert!(!r.percepisce(0.9)); }   // resta forte: non più
    assert_eq!(r.scatti(), 1);
}

#[test]
fn una_salita_lenta_non_scatta() {
    // Un brusio che cresce piano non è un soprassalto: il salto conta, non il livello.
    let mut r = arco();
    let mut e = 0.0f32;
    while e <= 0.9 { assert!(!r.percepisce(e)); e += 0.03; }
    assert_eq!(r.scatti(), 0);
}

#[test]
fn refrattario_impedisce_scatti_ravvicinati() {
    let mut r = arco();
    assert!(!r.percepisce(0.0));
    assert!(r.percepisce(0.9));   // scatta → entra in refrattario
    assert!(!r.percepisce(0.0));  // giù
    assert!(!r.percepisce(0.9));  // su subito: NON riscatta
    assert_eq!(r.scatti(), 1);
    assert!(r.in_refrattario());
}

#[test]
fn dopo_il_refrattario_si_riarma() {
    let mut r = arco();
    assert!(!r.percepisce(0.0));
    assert!(r.percepisce(0.9));
    for _ in 0..200 { r.percepisce(0.0); } // consuma il refrattario
    assert!(!r.in_refrattario());
    assert!(r.percepisce(0.9), "riarmato: un nuovo suono deve scattare");
    assert_eq!(r.scatti(), 2);
}
