// ── Banco host per il tessuto CfC: la cellula UDITO e le sue connessioni ──
//
// Creato il 14/10/2026 insieme alla 5ª cellula (Udito). La logica del tessuto
// è PURA (step, routing dei FASCI, attivazioni): si verifica su host, col
// codice REALE (#[path] su src/cfc.rs) — mai su una copia.
//
// cfc.rs usa `crate::math::sqrtf` (nel kernel è libm) e `crate::cfc::tick`:
// li shimmiamo qui, perché il banco è compilato con `rustc --test` diretto.
//
// Compilazione: make test   (oppure rustc --test bench/cfc_host.rs)

mod math {
    pub fn sqrtf(x: f32) -> f32 { x.sqrt() }
}

#[path = "../src/cfc.rs"]
mod cfc;

use cfc::{AxonBundle, CfcWeights, CellId, Tessuto};

fn w(seed: u64) -> CfcWeights { CfcWeights::new_xavier(seed) }

fn energia_cellula(h: &[f32; 16]) -> f32 {
    h.iter().map(|v| v.abs()).sum()
}

#[test]
fn udito_esiste_nel_tessuto_e_parte_a_riposo() {
    let t = Tessuto::new();
    assert_eq!(energia_cellula(&t.udito.h), 0.0);
}

#[test]
fn udito_si_muove_con_un_suono() {
    let mut t = Tessuto::new();
    let wu = w(7);
    let zero = [0.0f32; 4];
    let (wt, wc, wm, wi) = (w(1), w(2), w(3), w(4));
    for _ in 0..50 {
        t.step(&[], &zero, &[0.8, 0.6, 0.1, 0.0], None, &wt, &wc, &wm, &wi, &wu, 1.0, 1.0);
    }
    assert!(energia_cellula(&t.udito.h) > 0.01, "l'udito non si è mosso col suono");
}

#[test]
fn udito_resta_a_riposo_col_silenzio() {
    let mut t = Tessuto::new();
    let wu = w(7);
    let zero = [0.0f32; 4];
    let (wt, wc, wm, wi) = (w(1), w(2), w(3), w(4));
    for _ in 0..50 {
        t.step(&[], &zero, &zero, None, &wt, &wc, &wm, &wi, &wu, 1.0, 1.0);
    }
    assert!(energia_cellula(&t.udito.h) < 0.001, "l'udito si è mosso senza suono");
}

#[test]
fn fascio_udito_integra_il_corpo() {
    // Il punto di Alfonso: "rivedere il comportamento con le altre". Col fascio
    // Udito→Integrat, stimolare l'udito deve MUOVERE Integrat (come gli altri sensi).
    let fascia = [AxonBundle {
        src: CellId::Udito, src_offset: 0, count: 4,
        dst: CellId::Integrat, dst_offset: 0,
    }];
    let mut con = Tessuto::new();
    let mut senza = Tessuto::new();
    let zero = [0.0f32; 4];
    let suono = [0.9, 0.9, 0.1, 0.0];
    let (wt, wc, wm, wi, wu) = (w(1), w(2), w(3), w(4), w(7));
    for _ in 0..50 {
        con.step(&fascia, &zero, &suono, None, &wt, &wc, &wm, &wi, &wu, 1.0, 1.0);
        senza.step(&[], &zero, &suono, None, &wt, &wc, &wm, &wi, &wu, 1.0, 1.0);
    }
    let e_con = energia_cellula(&con.integrat.h);
    let e_senza = energia_cellula(&senza.integrat.h);
    assert!(e_con > e_senza,
            "il fascio Udito→Integrat non ha effetto: con={} senza={}", e_con, e_senza);
    assert!(e_con > 0.01, "Integrat non si è mosso: {}", e_con);
}

#[test]
fn il_fascio_rispetta_gli_offset() {
    // Se il routing ignorasse src_offset, con gli slot 0..4 a zero Integrat
    // resterebbe fermo. Lo verifichiamo stimolando SOLO gli slot 4..8.
    let fascia = [AxonBundle {
        src: CellId::Udito, src_offset: 4, count: 4,
        dst: CellId::Integrat, dst_offset: 0,
    }];
    let mut t = Tessuto::new();
    for i in 4..8 { t.udito.h[i] = 1.0; }
    let zero = [0.0f32; 4];
    let (wt, wc, wm, wi, wu) = (w(1), w(2), w(3), w(4), w(7));
    t.step(&fascia, &zero, &zero, None, &wt, &wc, &wm, &wi, &wu, 1.0, 1.0);
    assert!(energia_cellula(&t.integrat.h) > 0.001,
            "gli slot 4..8 dell'udito non sono arrivati a Integrat (offset ignorato?)");
}
