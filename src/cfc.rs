// ── Weights ──────────────────────────────────────────────────────────────

pub struct CfcWeights {
    pub w_f: [[f32; 8]; 8],
    pub w_f_in: [[f32; 4]; 8],
    pub b_f: [f32; 8],
    pub w_g: [[f32; 8]; 8],
    pub w_g_in: [[f32; 4]; 8],
    pub b_g: [f32; 8],
}

// ── State ────────────────────────────────────────────────────────────────

pub struct CfcState {
    pub h: [f32; 8],
}

impl CfcState {
    pub const fn new() -> Self {
        Self { h: [0.0; 8] }
    }

    pub fn step(&mut self, input: &[f32; 4], dt: f32, w: &CfcWeights) {
        let mut f = [0.0f32; 8];
        let mut pre_g = [0.0f32; 8];

        for i in 0..8 {
            let mut fi = w.b_f[i];
            for j in 0..8 {
                fi += w.w_f[i][j] * self.h[j];
            }
            for k in 0..4 {
                fi += w.w_f_in[i][k] * input[k];
            }
            f[i] = fi;

            let mut gi = w.b_g[i];
            for j in 0..8 {
                gi += w.w_g[i][j] * self.h[j];
            }
            for k in 0..4 {
                gi += w.w_g_in[i][k] * input[k];
            }
            pre_g[i] = gi;
        }

        for i in 0..8 {
            let g = tanh_approx(pre_g[i]);
            let s = sigmoid_approx(-f[i] * dt);
            self.h[i] = s * g + (1.0 - s) * self.h[i];
        }
    }
}

// ── Axon bundles (v0.6: déclarative inter-cell routing) ────────────────

#[derive(Clone, Copy)]
pub enum CellId {
    Tatto,
    Chemio,
    #[allow(dead_code)]
    Metabol,
    Integrat,
}

/// A bundle of N axons from src.h[src_offset..src_offset+count]
/// to dst.input[dst_offset..dst_offset+count].
/// count ≤ 4 (input array is 4-dim). Multiple bundles may target the same
/// dst cell: values are summed.
#[derive(Clone, Copy)]
pub struct AxonBundle {
    pub src: CellId,
    pub src_offset: usize,
    pub count: usize,
    pub dst: CellId,
    pub dst_offset: usize,
}

// ── Tessuto ──────────────────────────────────────────────────────────────
// Tissue of differentiated cells with declarative axon-bundle routing.

pub struct Tessuto {
    pub tatto: CfcState,
    pub chemio: CfcState,
    pub metabol: CfcState,
    pub integrat: CfcState,
}

impl Tessuto {
    pub const fn new() -> Self {
        Self {
            tatto: CfcState::new(),
            chemio: CfcState::new(),
            metabol: CfcState::new(),
            integrat: CfcState::new(),
        }
    }

    /// Step all cells.
    /// 1. Build each cell's 4-dim input from axon bundles (inter-cell
    ///    projections).
    /// 2. Override with external/environmental input (sense, serial, tick).
    /// 3. Step each cell.
    pub fn step(
        &mut self,
        bundles: &[AxonBundle],
        chemio_input: &[f32; 4],
        sense: Option<&SenseEvent>,
        w_tatto: &CfcWeights,
        w_chemio: &CfcWeights,
        w_metabol: &CfcWeights,
        w_integrat: &CfcWeights,
        dt_tatto: f32,
        dt_rest: f32,
    ) {
        // Copy h values to avoid borrow conflicts (all [f32;8] are Copy).
        let h_tatto = self.tatto.h;
        let h_chemio = self.chemio.h;
        let h_metabol = self.metabol.h;
        let h_integrat = self.integrat.h;

        // Start input arrays at zero.
        let mut tatto_in  = [0.0f32; 4];
        let mut chemio_in = [0.0f32; 4];
        let mut metabol_in = [0.0f32; 4];
        let mut integrat_in = [0.0f32; 4];

        // 1. Apply axon bundles (inter-cell projections).
        for b in bundles {
            let src = match b.src {
                CellId::Tatto => &h_tatto,
                CellId::Chemio => &h_chemio,
                CellId::Metabol => &h_metabol,
                CellId::Integrat => &h_integrat,
            };
            let dst = match b.dst {
                CellId::Tatto => &mut tatto_in,
                CellId::Chemio => &mut chemio_in,
                CellId::Metabol => &mut metabol_in,
                CellId::Integrat => &mut integrat_in,
            };
            let n = b.count.min(4 - b.dst_offset);
            for i in 0..n {
                dst[b.dst_offset + i] = src[b.src_offset + i];
            }
        }

        // 2. External overrides.
        // Tatto[0..1] ← sense pain (overrides bundle input on these slots).
        tatto_in[0] = if sense.is_some() && sense.unwrap().pf_addr != 0 { -2.0 } else { 0.0 };
        tatto_in[1] = if sense.is_some() && sense.unwrap().gp_err != 0 { -1.0 } else { 0.0 };
        // Chemio ← serial (full override).
        chemio_in = *chemio_input;
        // Metabol[0] ← normalized tick.
        metabol_in[0] = (crate::cfc::tick() % 1000) as f32 * 0.001;

        // 3. Step each cell.
        self.tatto.step(&tatto_in, dt_tatto, w_tatto);
        self.chemio.step(&chemio_in, dt_rest, w_chemio);
        self.metabol.step(&metabol_in, dt_rest, w_metabol);
        self.integrat.step(&integrat_in, dt_rest, w_integrat);
    }
}

// ── Activations ──────────────────────────────────────────────────────────

fn sigmoid_approx(x: f32) -> f32 {
    0.5 * x / (1.0 + x.abs()) + 0.5
}

fn tanh_approx(x: f32) -> f32 {
    2.0 * sigmoid_approx(2.0 * x) - 1.0
}

// ── Tick heartbeat (v0.7: timer-driven, not polling) ──────────────────
// Uses TICK-ID comparison: detect if TICK has advanced since last check.
// This avoids spurious wakeups from rapid timer fires during serial output.

static mut LAST_TICK: u64 = 0;

pub fn tick_advanced() -> bool {
    let current = tick();
    unsafe {
        if current != LAST_TICK {
            LAST_TICK = current;
            true
        } else {
            false
        }
    }
}

// ── Sensory input (reflexes) ──────────────────────────────────────────

pub struct SenseEvent {
    pub pf_addr: u64,
    pub pf_err: u64,
    pub gp_err: u64,
}

static mut SENSE_PF_ADDR: u64 = 0;
static mut SENSE_PF_ERR: u64 = 0;
static mut SENSE_GP_ERR: u64 = 0;
static mut SENSE_COUNTER: u64 = 0;

pub fn sense_pf(addr: u64, err: u64) {
    unsafe {
        SENSE_PF_ADDR = addr;
        SENSE_PF_ERR = err;
        SENSE_COUNTER += 1;
    }
}

pub fn sense_gp(err: u64) {
    unsafe {
        SENSE_GP_ERR = err;
        SENSE_COUNTER += 1;
    }
}

pub fn take_sense() -> Option<SenseEvent> {
    unsafe {
        if SENSE_COUNTER == 0 {
            return None;
        }
        SENSE_COUNTER = 0;
        Some(SenseEvent {
            pf_addr: SENSE_PF_ADDR,
            pf_err: SENSE_PF_ERR,
            gp_err: SENSE_GP_ERR,
        })
    }
}

// ── TICK counter (set by timer irq) ──────────────────────────────────────

static mut TICK: u64 = 0;

pub fn tick() -> u64 {
    unsafe { core::ptr::read_volatile(&raw const TICK) }
}

pub fn inc_tick() {
    unsafe { core::ptr::write_volatile(&raw mut TICK, TICK + 1); }
}

// ── Lab: circular log buffer ─────────────────────────────────────────
// Fixed-point i16 × 100 (range -327.68..327.67, our values ±1)

const LOG_CAP: usize = 256;

static mut LOG_IDX: usize = 0;
static mut LOG_TICKS: [u64; LOG_CAP] = [0; LOG_CAP];
static mut LOG_CELLS: [[i16; 32]; LOG_CAP] = [[0; 32]; LOG_CAP];

pub fn log_record(tatto: &[f32; 8], chemio: &[f32; 8], metabol: &[f32; 8], integrat: &[f32; 8]) {
    unsafe {
        let i = LOG_IDX % LOG_CAP;
        let t = tick();
        LOG_TICKS[i] = t;
        for j in 0..8 {
            LOG_CELLS[i][j]      = (tatto[j]    * 100.0) as i16;
            LOG_CELLS[i][8+j]    = (chemio[j]   * 100.0) as i16;
            LOG_CELLS[i][16+j]   = (metabol[j]  * 100.0) as i16;
            LOG_CELLS[i][24+j]   = (integrat[j] * 100.0) as i16;
        }
        LOG_IDX = LOG_IDX.wrapping_add(1);
    }
}

pub fn log_idx() -> usize {
    unsafe { LOG_IDX }
}

pub fn log_tick_at(pos: usize) -> u64 {
    unsafe {
        if pos < LOG_CAP { LOG_TICKS[pos] } else { 0 }
    }
}

pub fn log_cells_at(pos: usize) -> [i16; 32] {
    unsafe {
        if pos < LOG_CAP { LOG_CELLS[pos] } else { [0; 32] }
    }
}

pub fn log_cap() -> usize {
    LOG_CAP
}

pub fn log_len() -> usize {
    unsafe {
        if LOG_IDX < LOG_CAP { LOG_IDX } else { LOG_CAP }
    }
}

// ── Pattern memory (v0.8: associativa/Hopfield-like) ──────────────────
// Memorizza fino a PATTERN_CAP pattern di stato globale (32 cellule × i16).
// Usata da: auto-store (stati significativamente diversi), STORE/RECALL
// comandi seriali, output di familiarità continua.

pub const PATTERN_CAP: usize = 16;
pub const PATTERN_SIM_THRESH: f32 = 0.88;

static mut PATTERN_COUNT: usize = 0;
static mut PATTERN_HEAD: usize = 0;
static mut PATTERN_TICKS: [u64; PATTERN_CAP] = [0; PATTERN_CAP];
static mut PATTERN_CELLS: [[i16; 32]; PATTERN_CAP] = [[0; 32]; PATTERN_CAP];

fn pattern_similarity(a: &[i16; 32], b: &[i16; 32]) -> f32 {
    let mut dot: i64 = 0;
    let mut na: i64 = 0;
    let mut nb: i64 = 0;
    for i in 0..32 {
        let ai = a[i] as i64;
        let bi = b[i] as i64;
        dot += ai * bi;
        na += ai * ai;
        nb += bi * bi;
    }
    if na == 0 || nb == 0 { return 0.0; }
    let naf = na as f32;
    let nbf = nb as f32;
    (dot as f32) / (libm::sqrtf(naf) * libm::sqrtf(nbf))
}

pub fn pattern_count() -> usize {
    unsafe { PATTERN_COUNT }
}

pub fn pattern_clear() {
    unsafe { PATTERN_COUNT = 0; PATTERN_HEAD = 0; }
}

pub fn pattern_store(tick: u64, cells: &[i16; 32]) -> bool {
    unsafe {
        let idx = PATTERN_HEAD;
        PATTERN_TICKS[idx] = tick;
        PATTERN_CELLS[idx] = *cells;
        if PATTERN_COUNT < PATTERN_CAP {
            PATTERN_COUNT += 1;
        }
        PATTERN_HEAD = (PATTERN_HEAD + 1) % PATTERN_CAP;
        true
    }
}

pub fn pattern_recall(cells: &[i16; 32]) -> Option<(usize, u64, f32)> {
    unsafe {
        if PATTERN_COUNT == 0 { return None; }
        let mut best_idx = 0;
        let mut best_sim = -1.0f32;
        for i in 0..PATTERN_COUNT {
            let sim = pattern_similarity(cells, &PATTERN_CELLS[i]);
            if sim > best_sim {
                best_sim = sim;
                best_idx = i;
            }
        }
        Some((best_idx, PATTERN_TICKS[best_idx], best_sim))
    }
}

pub fn pattern_recall_full(cells: &[i16; 32]) -> Option<(u64, f32, [i16; 32])> {
    unsafe {
        if PATTERN_COUNT == 0 { return None; }
        let mut best_idx = 0;
        let mut best_sim = -1.0f32;
        for i in 0..PATTERN_COUNT {
            let sim = pattern_similarity(cells, &PATTERN_CELLS[i]);
            if sim > best_sim {
                best_sim = sim;
                best_idx = i;
            }
        }
        Some((PATTERN_TICKS[best_idx], best_sim, PATTERN_CELLS[best_idx]))
    }
}

pub fn pattern_recall_n(cells: &[i16; 32], n: usize) -> [(usize, u64, f32); PATTERN_CAP] {
    let mut idxs = [0usize; PATTERN_CAP];
    let mut sims = [0.0f32; PATTERN_CAP];
    let cnt;
    unsafe {
        cnt = PATTERN_COUNT.min(PATTERN_CAP);
        for i in 0..cnt {
            idxs[i] = i;
            sims[i] = pattern_similarity(cells, &PATTERN_CELLS[i]);
        }
    }
    let k = n.min(cnt);
    for i in 0..k {
        let mut best = i;
        for j in i + 1..cnt {
            if sims[j] > sims[best] {
                best = j;
            }
        }
        let tmp_i = idxs[i]; idxs[i] = idxs[best]; idxs[best] = tmp_i;
        let tmp_s = sims[i]; sims[i] = sims[best]; sims[best] = tmp_s;
    }
    let mut results = [(0usize, 0u64, 0.0f32); PATTERN_CAP];
    for i in 0..k {
        unsafe {
            results[i] = (idxs[i], PATTERN_TICKS[idxs[i]], sims[i]);
        }
    }
    results
}

pub fn pattern_get(idx: usize) -> Option<(u64, [i16; 32])> {
    unsafe {
        if idx >= PATTERN_COUNT { None }
        else { Some((PATTERN_TICKS[idx], PATTERN_CELLS[idx])) }
    }
}

pub fn pack_cells(tatto: &[f32; 8], chemio: &[f32; 8], metabol: &[f32; 8], integrat: &[f32; 8]) -> [i16; 32] {
    let mut cells = [0i16; 32];
    for j in 0..8 {
        cells[j]     = (tatto[j]    * 100.0) as i16;
        cells[8+j]   = (chemio[j]   * 100.0) as i16;
        cells[16+j]  = (metabol[j]  * 100.0) as i16;
        cells[24+j]  = (integrat[j] * 100.0) as i16;
    }
    cells
}

