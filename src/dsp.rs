// ── dsp.rs — segnali puri (una sola dipendenza: nessuna) ─────────────────
//
// Logica di segnale SENZA dipendenze dal kernel: così si testa su host
// (bench/hda_host.rs la include con #[path], come la lezione di hexutil:
// "un test che passa ma copre una copia non è un test, è una coincidenza").
//
// Serve all'udito (hda.rs): dall'onda alla sensazione — un numero 0..1.

/// Energia di un blocco di campioni i16: **media del valore assoluto**,
/// normalizzata a fondo scala 32768 → 0..1. Silenzio → 0.0.
/// (Media dell'assoluto, non RMS: basta e avanza per un canale di senso.
/// Su una sinusoide a fondo scala dà ~0.637; su un'onda quadra piena 1.0.)
pub fn energia(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sum: u64 = 0;
    for &s in samples {
        sum += (s as i64).unsigned_abs();
    }
    (sum as f32) / (samples.len() as f32) / 32768.0
}

/// Picco (valore assoluto massimo) normalizzato 0..1. Distingue "c'è un
/// suono forte" da "c'è un brusio diffuso": l'energia media può essere pari.
pub fn picco(samples: &[i16]) -> f32 {
    let mut m: u32 = 0;
    for &s in samples {
        let a = (s as i64).unsigned_abs() as u32;
        if a > m {
            m = a;
        }
    }
    (m as f32) / 32768.0
}
