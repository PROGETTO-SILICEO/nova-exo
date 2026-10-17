// ── Banco host per Il Nucleo (nucleo.rs): verifica runtime 41M ────────────
//
// Verifica sul codice REALE (#[path = "../src/nucleo.rs"]):
//   - Parse header e parsing zero-copy dei pesi
//   - Ciclo cognitivo interrupt-driven (1 layer per tick APIC)
//   - Zero allocazioni dinamiche
//   - Fallback e validazione errori
//
// Compilazione ed esecuzione: make test (oppure rustc --test bench/nucleo_host.rs)

mod math {
    pub fn sqrtf(x: f32) -> f32 { x.sqrt() }
    pub fn expf(x: f32) -> f32 { x.exp() }
    pub fn powf(x: f32, y: f32) -> f32 { x.powf(y) }
    pub fn sincosf(x: f32) -> (f32, f32) { (x.sin(), x.cos()) }
    pub fn tanhf(x: f32) -> f32 { x.tanh() }
}

#[path = "../src/plasticita.rs"]
mod plasticita;

#[path = "../src/tensor.rs"]
mod tensor;

#[path = "../src/attention.rs"]
mod attention;

#[path = "../src/inference.rs"]
mod inference;

#[path = "../src/nucleo.rs"]
mod nucleo;

use inference::{BrainState, InferenceEngine};
use nucleo::{
    NucleoBrain, NucleoModel, NucleoScratch, TOTAL_MODEL_BYTES,
    NUCLEO_MAGIC, NUCLEO_VERSION, N_LAYERS,
};

#[test]
fn scratch_initialization_zero_alloc() {
    let s = NucleoScratch::new();
    assert_eq!(s.x[0], 0.0);
    assert_eq!(s.gate_buf[1535], 0.0);
    assert_eq!(s.q_buf[511], 0.0);
}

#[test]
fn parse_rejects_undersized_or_bad_magic() {
    let small = [0u8; 100];
    assert!(NucleoModel::parse(&small).is_none());

    let mut bad_magic = vec![0u8; TOTAL_MODEL_BYTES];
    bad_magic[0..4].copy_from_slice(b"WXYZ");
    assert!(NucleoModel::parse(&bad_magic).is_none());
}

#[test]
fn parse_and_tick_nucleo_brain() {
    // Creiamo un buffer sintetico valido per il Nucleo
    let mut weights = vec![0u8; TOTAL_MODEL_BYTES];
    weights[0..4].copy_from_slice(NUCLEO_MAGIC);
    weights[4..8].copy_from_slice(&NUCLEO_VERSION.to_le_bytes());

    let model = NucleoModel::parse(&weights).expect("Parsing buffer Nucleo fallito");
    assert_eq!(model.layers.len(), N_LAYERS);

    let mut scratch = NucleoScratch::new();
    let mut brain = NucleoBrain::new(model, &mut scratch);

    assert_eq!(brain.state(), BrainState::Idle);
    assert_eq!(brain.name(), "nucleo-41m");

    // Sottoponiamo un impulso chemio
    let chemio_in = [0.2f32, 0.8f32, -0.4f32, 0.1f32];
    let accepted = brain.submit(&chemio_in);
    assert!(accepted);
    assert_eq!(brain.state(), BrainState::Thinking { ticks_left: 12 });

    // Secondo submit mentre pensa deve essere rifiutato
    assert!(!brain.submit(&chemio_in));

    // Avanziamo tick per tick (1 layer per tick)
    for l in 0..11 {
        let ready = brain.tick();
        assert!(!ready, "Non deve essere pronto al layer {}", l);
        assert_eq!(brain.state(), BrainState::Thinking { ticks_left: (11 - l) as u32 });
    }

    // Al dodicesimo tick, l'ultimo layer finisce e l'output è pronto
    let ready = brain.tick();
    assert!(ready, "Al dodicesimo tick l'output deve essere pronto");
    assert_eq!(brain.state(), BrainState::Output { ticks_left: 2 });

    let out = brain.output().expect("Output deve essere presente");
    for &val in out.iter() {
        assert!(val.is_finite(), "Output non finito: {}", val);
        assert!(val >= -1.0 && val <= 1.0, "Output fuori range [-1, 1]: {}", val);
    }

    // Consumo output
    brain.tick(); // ticks_left = 1
    brain.tick(); // ticks_left = 0 -> torna Idle
    assert_eq!(brain.state(), BrainState::Idle);
    assert_eq!(brain.total_think_ticks(), 12);
}

// ── Plasticità (Fase 1): collegamento con Il Nucleo ──────────────────────

#[test]
fn adattatori_layer_hanno_il_layout_della_teoria() {
    // §2.3: 57.344 float = 229.376 B per layer; 12 layer = 2.752.512 B ≈ 2,625 MiB
    assert_eq!(nucleo::ADATTATORI_LAYER_FLOATS, 57_344);
    assert_eq!(nucleo::ADATTATORI_LAYER_BYTES, 229_376);
    assert_eq!(nucleo::ADATTATORI_TOTALI_BYTES, 2_752_512);
    // (d_out, d_in) delle proiezioni plastiche
    assert_eq!(nucleo::DIM_PROIEZIONE[nucleo::Proiezione::Wo as usize], (512, 512));
    assert_eq!(nucleo::DIM_PROIEZIONE[nucleo::Proiezione::Down as usize], (512, 1536));
    assert_eq!(nucleo::DIM_PROIEZIONE[nucleo::Proiezione::Gate as usize], (1536, 512));
}

#[test]
fn buffer_corto_non_fa_adattatori() {
    let mut corto = vec![0.0f32; nucleo::ADATTATORI_LAYER_FLOATS - 1];
    assert!(nucleo::AdattatoriLayer::nuovi(&mut corto).is_none(), "un buffer corto deve essere rifiutato");
}

#[test]
fn passo_plastico_alcanza_gli_adattatori_di_nucleo() {
    // init deterministica del buffer (U e V non nulli: la struttura li deve toccare)
    let mut buf = vec![0.0f32; nucleo::ADATTATORI_LAYER_FLOATS];
    let mut s: u32 = 7;
    for i in 0..buf.len() {
        s = s.wrapping_mul(1103515245).wrapping_add(12345);
        buf[i] = ((s >> 16) as f32 / 65536.0 - 0.5) / (512.0f32).sqrt();
    }
    let prima: f32 = buf.iter().map(|v| v * v).sum();
    let mut z = [0.0f32; plasticita::RANGO];
    let mut ute = [0.0f32; plasticita::RANGO];
    let x = vec![0.5f32; 512];
    let e = vec![0.2f32; 512];

    let esito = {
        let mut ad = nucleo::AdattatoriLayer::nuovi(&mut buf).expect("buffer alla misura della teoria");
        // l'uscita plastica della proiezione Wo deve essere non nulla
        let mut y = [0.0f32; 512];
        ad.predici(nucleo::Proiezione::Wo, &x, &mut z, &mut y);
        assert!(y.iter().any(|v| *v != 0.0), "con V inizializzata la predizione plastica Wo non è nulla");
        // un passo della regola deve raggiungere davvero le matrici
        ad.passo(nucleo::Proiezione::Wo, &x, &e, 1.0, 0.3, &plasticita::V21, 1.0, 0.0, &mut z, &mut ute)
    };
    let dopo: f32 = buf.iter().map(|v| v * v).sum();
    assert!(dopo != prima, "il passo di plasticità deve aver modificato gli adattatori di W_o");
    assert!(esito.rho.is_finite(), "ρ finito");
    // con w_norm = 1.0 (sottovalutato rispetto al vero ‖W_o‖ ≈ 22,6) il cap ρ
    // deve scattare: `rho` restituito è quello PRE-cap (come nella sim).
    assert!(esito.cap_hit, "con un w_norm piccolo il cap ρ deve scattare");
}
