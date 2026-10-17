// ── math.rs — un indirizzo solo per la matematica no_std ─────────────────
//
// Perché esiste (14/10/2026): `cfc.rs` chiamava `libm::sqrtf` direttamente.
// In no_std serve libm (core non ha sqrt), ma un modulo che chiama libm non è
// testabile su host: il banco è compilato con `rustc --test` e non ha libm.
// Con questo wrapper il kernel usa libm e il banco (`bench/cfc_host.rs`)
// sostituisce `crate::math` con `f32::sqrt` — così si testa il codice REALE.

/// Radice quadrata (no_std). Nel kernel: libm. Nei banchi host: f32::sqrt.
pub fn sqrtf(x: f32) -> f32 {
    libm::sqrtf(x)
}

/// Esponenziale (no_std).
pub fn expf(x: f32) -> f32 {
    libm::expf(x)
}

/// Potenza (no_std).
pub fn powf(x: f32, y: f32) -> f32 {
    libm::powf(x, y)
}

/// Seno e coseno (no_std).
pub fn sincosf(x: f32) -> (f32, f32) {
    libm::sincosf(x)
}

/// Tangente iperbolica (no_std). Usata dal neuromodulatore (plasticita.rs).
pub fn tanhf(x: f32) -> f32 {
    libm::tanhf(x)
}
