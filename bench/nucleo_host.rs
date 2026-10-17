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

// ── Retroproiezione somatica (§3.5): W_proj = W_head^T ────────────────────

#[test]
fn retroproiezione_errore_soma_e_trasposta_corretta() {
    // W_head: [3 latente × 2 chemio] — miniaturizzato ma stesso layout riga-major
    // del nucleo [512 × 4].
    let head = [
        1.0, 2.0,   // latente 0
        3.0, 4.0,   // latente 1
        5.0, 6.0,   // latente 2
    ];
    let chemio_dim = 2;
    let latente_dim = 3;

    // Forward: out[i] = Σ_j x[j] · head[j*chemio + i]  (come nucleo.rs:381-384)
    let x = [0.5, -1.0, 2.0];
    let mut out = [0.0f32; 2];
    for i in 0..chemio_dim {
        let mut s = 0.0f32;
        for j in 0..latente_dim {
            s += x[j] * head[j * chemio_dim + i];
        }
        out[i] = s;
    }

    // Retroproiezione: e_hidden[j] = Σ_i e_som[i] · head[j*chemio + i]
    let e_som = [0.3, -0.7];
    let mut e_hidden = [0.0f32; 3];
    plasticita::retroproietta_errore_soma(&head, &e_som, &mut e_hidden);

    // Proprietà della trasposta: e_som · out == e_hidden · x
    let lhs: f32 = (0..chemio_dim).map(|i| e_som[i] * out[i]).sum();
    let rhs: f32 = (0..latente_dim).map(|j| e_hidden[j] * x[j]).sum();
    assert!(
        (lhs - rhs).abs() < 1e-6,
        "trasposta non corretta: e_som·out={} ≠ e_hidden·x={}",
        lhs, rhs
    );
}

#[test]
fn retroproiezione_sul_head_reale_del_nucleo() {
    // Stessa dimensione del nucleo reale: [512 × 4]
    let dim = nucleo::DIM;
    let chemio = nucleo::CHEMIO_DIM;
    assert_eq!(dim, 512);
    assert_eq!(chemio, 4);

    // Head sintetica deterministica
    let mut head = vec![0.0f32; dim * chemio];
    let mut s: u32 = 42;
    for v in head.iter_mut() {
        s = s.wrapping_mul(1103515245).wrapping_add(12345);
        *v = ((s >> 16) as f32 / 65536.0) - 0.5;
    }

    // Forward del nucleo (identico a nucleo.rs:381-384)
    let x_norm = vec![0.1f32; dim];
    let mut out = [0.0f32; 4];
    for i in 0..chemio {
        let mut sum = 0.0f32;
        for j in 0..dim {
            sum += x_norm[j] * head[j * chemio + i];
        }
        out[i] = sum;
    }

    // Retroproiezione
    let e_som = [0.1, -0.2, 0.3, -0.4];
    let mut e_hidden = vec![0.0f32; dim];
    plasticita::retroproietta_errore_soma(&head, &e_som, &mut e_hidden);

    // Proprietà trasposta: e_som · out == e_hidden · x_norm
    let lhs: f32 = (0..chemio).map(|i| e_som[i] * out[i]).sum();
    let rhs: f32 = (0..dim).map(|j| e_hidden[j] * x_norm[j]).sum();
    assert!(
        (lhs - rhs).abs() < 1e-4,
        "trasposta non corretta sulle 512×4: e_som·out={} ≠ e_hidden·x={}",
        lhs, rhs
    );
}

// ══════════════════════════════════════════════════════════════════════════
// IL MAESTRO (Passo 2, 17/10/2026): ‖W‖_F, armatura, distribuzione (Opzione A)
// ══════════════════════════════════════════════════════════════════════════

/// Buffer-pesi sintetico valido (header corretto, pesi a zero) — come lo
/// stub `nucleo_41m.bin` generato da pack_nucleo_bin.py.
fn modello_sintetico() -> Vec<u8> {
    let mut weights = vec![0u8; TOTAL_MODEL_BYTES];
    weights[0..4].copy_from_slice(NUCLEO_MAGIC);
    weights[4..8].copy_from_slice(&NUCLEO_VERSION.to_le_bytes());
    weights
}

#[test]
fn norma_q4_k_riproduce_il_riferimento_gguf_py() {
    // Riferimenti indipendenti (gguf-py `dequantize`, parità 1:1, diff 0.0
    // su 1/8/64/1024 blocchi — verificati il 17/10/2026):
    //   blocco 0 di blk.0.ffn_gate.weight (144 B)      → 0.242118686…
    //   primi 1024 blocchi dello stesso tensore (147 KB) → 7.91371887507…
    let blk0 = include_bytes!("../testdata/lfm25_blk0_q4k.bin");
    let n0 = tensor::norma_q4_k(blk0).expect("blocco Q4_K valido (144 B)");
    assert!((n0 - 0.242118686).abs() < 1e-5, "norma blocco 0: {}", n0);

    let big = include_bytes!("../testdata/q4k_1024blocchi_lfm25.bin");
    assert_eq!(big.len(), 147_456, "1024 blocchi × 144 B");
    let nbig = tensor::norma_q4_k(big).expect("1024 blocchi Q4_K validi");
    assert!((nbig - 7.913718875).abs() < 1e-4, "norma 1024 blocchi: {}", nbig);

    // Input non valido → errore, non panico.
    assert!(tensor::norma_q4_k(&[]).is_err(), "vuoto → errore");
    assert!(tensor::norma_q4_k(&[0u8; 145]).is_err(), "non multiplo di 144 → errore");
}

#[test]
fn maestro_rifiuta_buffer_corto() {
    let weights = modello_sintetico();
    let mut corto = vec![0.0f32; 10];
    let model = NucleoModel::parse(&weights).expect("buffer sintetico valido");
    let mut scratch = NucleoScratch::new();
    let mut brain = NucleoBrain::new(model, &mut scratch);

    assert!(!brain.arma_maestro(&mut corto), "un buffer corto deve rifiutare l'armatura");
    assert_eq!(brain.maestro_lambda(), None, "senza armatura il maestro è dormiente");
}

#[test]
fn maestro_lambda_parte_da_zero_e_sale_col_corpo() {
    let weights = modello_sintetico();
    let mut adattatori = vec![0.0f32; N_LAYERS * nucleo::ADATTATORI_LAYER_FLOATS];
    let model = NucleoModel::parse(&weights).expect("buffer sintetico valido");
    let mut scratch = NucleoScratch::new();
    let mut brain = NucleoBrain::new(model, &mut scratch);

    assert!(brain.arma_maestro(&mut adattatori), "buffer alla misura della teoria");

    // λ(0) = 0: il corpo parte MUTO (nessuna plasticità inventata a vuoto).
    assert_eq!(brain.maestro_lambda(), Some(0.0), "λ deve partire da 0");

    // Una percezione con salienza → λ sale, ma NON scatta (passa-basso).
    brain.set_corpo(0.8, 0.9);
    let lam1 = brain.maestro_lambda().expect("maestro armato");
    assert!(lam1 > 0.0 && lam1 < 0.2, "il primo passo di λ deve essere piccolo: {}", lam1);

    // Ciclo cognitivo completo: e_hidden finito, λ finito (nessun NaN).
    assert!(brain.submit(&[0.3, -0.2, 0.5, 0.7]));
    for _ in 0..N_LAYERS {
        brain.tick();
    }
    let (_e_som, e_hid) = brain.errore_somatico();
    assert_eq!(e_hid.len(), nucleo::DIM);
    assert!(e_hid.iter().all(|v| v.is_finite()), "e_hidden deve essere finito");
    let lam2 = brain.maestro_lambda().unwrap();
    assert!(lam2.is_finite() && lam2 > 0.0 && lam2 <= 1.0, "λ finito e in (0,1]: {}", lam2);
}

#[test]
fn opzione_a_muove_solo_wo_e_down() {
    // Opzione A (17/10): ARMATI solo W_o e W_down; Gate/Up DORMIENTI.
    // Su un modello a pesi nulli il forward produce x=y=e=0, quindi l'unico
    // termine che agisce è la DECADENZA γ (§3.1) — che corre per Wo e Down
    // nel forward e MAI per Gate/Up. È l'invariante strutturale da provare.
    use nucleo::{DIM_PROIEZIONE, Proiezione, ADATTATORI_LAYER_FLOATS, DIM, N_FF, SEED_ADATTATORI};
    const R: usize = plasticita::RANGO;
    let n_adatt = N_LAYERS * ADATTATORI_LAYER_FLOATS;

    // Offset (in float) delle V dentro un layer — stessa sequenza di split di
    // AdattatoriLayer::nuovi: [uGate,vGate,uUp,vUp,uDown,vDown,uWo,vWo].
    let (f, d) = (N_FF * R, DIM * R);
    let off_v = [f, f + d + f, f + d + f + d + d, f + d + f + d + d + f + d];
    let proiezioni = [Proiezione::Gate, Proiezione::Up, Proiezione::Down, Proiezione::Wo];

    // "Atteso appena inizializzato": replica ESATTA di Maestro::inizializza_adattatori.
    let mut atteso = vec![0.0f32; n_adatt];
    let mut s = SEED_ADATTATORI | 1;
    for l in 0..N_LAYERS {
        let base = l * ADATTATORI_LAYER_FLOATS;
        for (idx, p) in proiezioni.iter().enumerate() {
            let k = *p as usize;
            let d_in = DIM_PROIEZIONE[k].1;
            let len = d_in * R;
            let o = base + off_v[idx];
            plasticita::inizializza_v(&mut atteso[o..o + len], d_in, &mut s);
        }
    }

    // Esecuzione: arma, una percezione, un ciclo completo (12 layer).
    let weights = modello_sintetico();
    let mut adattatori = vec![0.0f32; n_adatt];
    {
        let model = NucleoModel::parse(&weights).expect("buffer sintetico valido");
        let mut scratch = NucleoScratch::new();
        let mut brain = NucleoBrain::new(model, &mut scratch);
        assert!(brain.arma_maestro(&mut adattatori));
        brain.set_corpo(0.9, 0.9);
        assert!(brain.submit(&[0.3, -0.2, 0.5, 0.7]));
        for _ in 0..N_LAYERS {
            brain.tick();
        }
    } // il prestito `&mut adattatori` termina con il brain

    // Tutto finito: la base sintetica (‖W‖_F = 0) NON deve generare NaN
    // (il guard su w_norm in Adattatore::passo mette rho = 0.0).
    assert!(adattatori.iter().all(|v| v.is_finite()), "nessun NaN con base sintetica");

    // Invariante: Gate e Up restano IDENTICI all'iniziale (dormienti).
    for l in 0..N_LAYERS {
        let base = l * ADATTATORI_LAYER_FLOATS;
        for idx in [0usize, 1] {
            let k = proiezioni[idx] as usize;
            let len = DIM_PROIEZIONE[k].1 * R;
            let o = base + off_v[idx];
            assert_eq!(
                &adattatori[o..o + len],
                &atteso[o..o + len],
                "Gate/Up (l={}, proiezione {:?}) NON devono muoversi (Opzione A)",
                l, proiezioni[idx]
            );
        }
        // Le U di Gate e Up restano a zero (mai inizializzate né aggiornate).
        let u_gate = base; // uGate è in testa al layer
        assert!(adattatori[u_gate..u_gate + f].iter().all(|v| *v == 0.0));
    }

    // Invariante: Wo e Down SI sono mossi (decadenza γ nel forward).
    let mut mossi = 0usize;
    for l in 0..N_LAYERS {
        let base = l * ADATTATORI_LAYER_FLOATS;
        for idx in [2usize, 3] {
            let k = proiezioni[idx] as usize;
            let len = DIM_PROIEZIONE[k].1 * R;
            let o = base + off_v[idx];
            if adattatori[o..o + len] != atteso[o..o + len] {
                mossi += 1;
            }
        }
    }
    assert_eq!(mossi, 2 * N_LAYERS, "Wo+Down devono essersi mossi in TUTTI i layer");
}
