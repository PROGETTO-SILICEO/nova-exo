// ── Banco host per l'OMEOSTASI (il corpo che si tiene) ──
//
// Regolatore con isteresi + registro attuatori: logica PURA, verificata su host
// col codice REALE (#[path] su src/omeostasi.rs). Se l'isteresi è sbagliata,
// la ventola trema (on/off a ogni grado) — e un corpo che trema non è vivo.
//
// Compilazione: make test   (oppure rustc --test bench/omeostasi_host.rs)

#[path = "../src/omeostasi.rs"]
mod omeostasi;

use omeostasi::{Attuatori, Regolatore};

#[test]
fn registra_e_comanda_un_attuatore() {
    let mut a = Attuatori::new();
    assert!(a.registra("ventola", false));
    assert_eq!(a.len(), 1);
    assert!(!a.acceso("ventola"));
    assert!(a.imposta("ventola", true));
    assert!(a.acceso("ventola"));
    assert_eq!(a.conta_accesi(), 1);
}

#[test]
fn comando_su_attuatore_inesistente_fallisce() {
    let mut a = Attuatori::new();
    a.registra("ventola", false);
    assert!(!a.imposta("motore", true), "un attuatore che non c'e' non si comanda");
    assert_eq!(a.conta_accesi(), 0);
}

#[test]
fn il_regolatore_non_scatta_sotto_soglia() {
    let mut r = Regolatore::new(0.75, 0.05);
    for v in [0.0, 0.3, 0.5, 0.79] {
        assert!(!r.aggiorna(v), "non deve scattare a {}", v);
    }
}

#[test]
fn scatta_sopra_la_banda_alta() {
    let mut r = Regolatore::new(0.75, 0.05);
    assert!(!r.aggiorna(0.79));
    assert!(r.aggiorna(0.85), "sopra 0.80 deve agire");
}

#[test]
fn l_isteresi_evita_il_tremolio() {
    let mut r = Regolatore::new(0.75, 0.05);
    assert!(r.aggiorna(0.90));           // sale: agisce
    assert!(r.aggiorna(0.72), "tra 0.70 e 0.80 resta acceso (niente tremolio)");
    assert!(r.aggiorna(0.71));
    assert!(!r.aggiorna(0.68), "sotto 0.70 si spegne");
    assert!(!r.aggiorna(0.76), "tra 0.70 e 0.80 resta spento (niente tremolio)");
}

#[test]
fn un_ciclo_lo_riporta_accanto_alla_soglia() {
    // Un corpo sano: sale, agisce, scende, smette. Nessuno stato appiccicoso.
    let mut r = Regolatore::new(0.75, 0.05);
    assert!(r.aggiorna(0.99));
    assert!(!r.aggiorna(0.10));
    assert!(!r.alto());
}
