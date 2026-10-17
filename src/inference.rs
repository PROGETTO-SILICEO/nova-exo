// ── Inference Engine (Il cervello nel metallo) — Sempre, 13 Ago 2026 ──
//
// VISIONE: "Niente cervello fuori. Il modello di inferenza vive nel metallo,
// come il CFC."
//
// Questo modulo definisce il CONTRATTO tra l'exokernel e il suo cervello:
//   - `InferenceEngine`: il trait che ogni runtime di inferenza deve
//     implementare per vivere nel metallo (oggi: stub; domani: LFM2.5
//     portato in no_std dentro il kernel).
//   - `StubBrain`: un runtime simulato che "pensa" come un LLM ma senza
//     runtime esterno — con latenza a tick e ragionamento interno,
//     così l'architettura si testa OGGI in QEMU e si scambia DOMANI
//     con il runtime vero (stessa interfaccia).
//
// Metodo Exo (skill): un passo per tick. Il cervello non si blocca:
// ha stati IDLE → THINKING → OUTPUT, avanza di un passo per battito.
//
// Collegamento anatomico: CFC (corpo) → interpreter (corteccia sensoriale)
// → InferenceEngine (corteccia associativa/prefrontale) → executive (volitivo).

/// Stato del cervello interno. Avanza di un passo per tick APIC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrainState {
    /// Idle: non sta pensando
    Idle,
    /// Sta "pensando" (simula latenza di un LLM, un passo per tick)
    Thinking { ticks_left: u32 },
    /// Ha prodotto un output pronto per l'esecutivo
    Output { ticks_left: u32 },
}

/// Il contratto: ogni runtime che vive nel metallo implementa questo.
pub trait InferenceEngine {
    /// Invia lo stato del corpo (interpretato) al cervello.
    /// Ritorna true se ha ACCETTATO l'input (inizia a pensare).
    fn submit(&mut self, input: &[f32; 4]) -> bool;
    /// Avanza di un tick. Ritorna true quando c'è un output pronto.
    fn tick(&mut self) -> bool;
    /// Legge l'output corrente (se pronto).
    fn output(&self) -> Option<[f32; 4]>;
    /// Stato corrente (per diagnosi su seriale).
    fn state(&self) -> BrainState;
    /// Nome del runtime (per log: "stub", "lfm25").
    fn name(&self) -> &'static str;
    /// Il corpo (M(t) e la salienza dell'errore somatico) parla al
    /// cervello: passo del neurotrasmettitore λ(t) per la plasticità
    /// (Passo 2, 17/10). Default no-op: i runtime senza Maestro
    /// (StubBrain, LFM futuro senza plasticità) lo ignorano.
    fn set_corpo(&mut self, _m_meta: f32, _s_som: f32) {}
}

/// Stub del cervello: simula il comportamento di LFM2.5-2.6B nel metallo.
///
/// Pensiero simulato:
///   - riceve il chemio interpretato (c,u,p,n)
///   - "ragiona" per T_THINK tick (simula il <think> del modello,
///     ~60-70% del tempo come misurato su LFM2.5 reale)
///   - produce un output = versione raffinata del chemio + un concetto
///     astratto, che il volitivo userà.
///
/// L'output è deterministico (per test): l'urgenza viene smussata,
/// la polarità amplificata — come se il cervello "riflettesse" prima
/// di decidere. Quando il runtime vero (LFM2.5) sarà nel metallo,
/// questo stub si sostituisce con lo stesso trait.
pub struct StubBrain {
    state: BrainState,
    input: [f32; 4],
    output: Option<[f32; 4]>,
    /// Numero di tick totali di "pensiero" (per misurare la latenza)
    total_think_ticks: u64,
}

/// Latenza di pensiero simulata in tick (~LFM2.5 su CPU: il <think>
/// occupa 60-70% dei token; qui simuliamo un "giro" di riflessione)
const T_THINK: u32 = 25;

impl StubBrain {
    pub fn new() -> Self {
        Self {
            state: BrainState::Idle,
            input: [0.0; 4],
            output: None,
            total_think_ticks: 0,
        }
    }

    pub fn total_think_ticks(&self) -> u64 {
        self.total_think_ticks
    }
}

impl InferenceEngine for StubBrain {
    fn submit(&mut self, input: &[f32; 4]) -> bool {
        if self.state != BrainState::Idle {
            return false; // occupato, ignora
        }
        self.input = *input;
        self.state = BrainState::Thinking { ticks_left: T_THINK };
        true
    }

    fn tick(&mut self) -> bool {
        match self.state {
            BrainState::Idle => false,
            BrainState::Thinking { ticks_left } => {
                self.total_think_ticks += 1;
                if ticks_left <= 1 {
                    // Pensiero concluso: produce output raffinato.
                    // Il "cervello" riflette: smussa l'urgenza, amplifica
                    // la polarità, conserva contesto/novità.
                    let c = self.input[0];
                    let u = self.input[1] * 0.8;          // urgenza smussata
                    let p = (self.input[2] * 1.2).clamp(-1.0, 1.0); // polarità amplificata
                    let n = self.input[3];
                    self.output = Some([c, u, p, n]);
                    self.state = BrainState::Output { ticks_left: 2 };
                    true
                } else {
                    self.state = BrainState::Thinking { ticks_left: ticks_left - 1 };
                    false
                }
            }
            BrainState::Output { ticks_left } => {
                if ticks_left <= 1 {
                    self.state = BrainState::Idle;
                    // l'output resta leggibile finché non c'è nuovo submit
                } else {
                    self.state = BrainState::Output { ticks_left: ticks_left - 1 };
                }
                true
            }
        }
    }

    fn output(&self) -> Option<[f32; 4]> {
        self.output
    }

    fn state(&self) -> BrainState {
        self.state
    }

    fn name(&self) -> &'static str {
        "stub-brain"
    }
}

/// Helper: stampa lo stato del cervello su seriale.
/// (usato da main.rs a intervalli, non a ogni tick)
pub fn describe_state(s: BrainState) -> &'static str {
    match s {
        BrainState::Idle => "IDLE",
        BrainState::Thinking { .. } => "THINK",
        BrainState::Output { .. } => "OUTPUT",
    }
}

/// Dispatcher unificato per il cervello (StubBrain o NucleoBrain).
pub enum AnyBrain<'a> {
    Stub(StubBrain),
    Nucleo(crate::nucleo::NucleoBrain<'a>),
}

impl<'a> AnyBrain<'a> {
    pub fn total_think_ticks(&self) -> u64 {
        match self {
            AnyBrain::Stub(b) => b.total_think_ticks(),
            AnyBrain::Nucleo(b) => b.total_think_ticks(),
        }
    }
}

impl<'a> InferenceEngine for AnyBrain<'a> {
    fn submit(&mut self, input: &[f32; 4]) -> bool {
        match self {
            AnyBrain::Stub(b) => b.submit(input),
            AnyBrain::Nucleo(b) => b.submit(input),
        }
    }

    fn tick(&mut self) -> bool {
        match self {
            AnyBrain::Stub(b) => b.tick(),
            AnyBrain::Nucleo(b) => b.tick(),
        }
    }

    fn output(&self) -> Option<[f32; 4]> {
        match self {
            AnyBrain::Stub(b) => b.output(),
            AnyBrain::Nucleo(b) => b.output(),
        }
    }

    fn state(&self) -> BrainState {
        match self {
            AnyBrain::Stub(b) => b.state(),
            AnyBrain::Nucleo(b) => b.state(),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            AnyBrain::Stub(b) => b.name(),
            AnyBrain::Nucleo(b) => b.name(),
        }
    }

    fn set_corpo(&mut self, m_meta: f32, s_som: f32) {
        match self {
            AnyBrain::Stub(b) => b.set_corpo(m_meta, s_som),
            AnyBrain::Nucleo(b) => b.set_corpo(m_meta, s_som),
        }
    }
}
