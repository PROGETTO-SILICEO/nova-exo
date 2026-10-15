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
}

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
