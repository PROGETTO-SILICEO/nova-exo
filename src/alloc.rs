// ── Bump Allocator (no_std) — Sempre, 13 Ago 2026 ──
#![no_std]
//
// Allocatore lineare (bump) per l'exokernel. Serve DOMANI quando il
// runtime di inferenza (LFM2.5 nel metallo) dovrà allocare i 2GB di pesi.
// Oggi il kernel usa solo array statici — questo modulo è pronto,
// testabile, e NON è ancora il global allocator (per non rischiare
// regressioni: l'alloc richiede un heap inizializzato che ora non c'è).
//
// Design: bump allocator minimale
//   - memoria fornita da una regione statica (kernel BSS)
//   - alloc: allinea, avanza il puntatore
//   - dealloc: no-op (il bump non libera singolarmente; si resetta)
//   - reset: azzera il puntatore (per "cicli di vita" del modello)
//
// Thread-safety: usato solo dal BSP (un solo core), lock-free per ora.
// Se Exo diventa multicore, serve un lock (spinlock) attorno all'alloc.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Heap statico: 1 MiB di riserva per gli oggetti del runtime di inferenza.
/// (I pesi veri del modello arriveranno da una regione mappata dedicata;
///  questo heap copre strutture, tensori di lavoro, buffer di I/O.)
#[repr(align(4096))]
struct HeapRegion([u8; HEAP_SIZE]);

const HEAP_SIZE: usize = 1024 * 1024;

static mut HEAP: HeapRegion = HeapRegion([0u8; HEAP_SIZE]);

/// Puntatore di bump corrente (offset dentro HEAP).
static BUMP: AtomicUsize = AtomicUsize::new(0);

/// Errore di allocazione
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AllocError {
    OutOfMemory,
}

/// Allinea `v` al multiplo di `align` (potenza di 2).
fn align_up(v: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (v + align - 1) & !(align - 1)
}

/// Alloca `size` byte allineati. Il bump allocator non libera: si resetta.
///
/// # Safety
/// Ritorna un puntatore a memoria valida per 'static lifetime (non verrà
/// riutilizzata finché non si chiama reset). Non thread-safe tra alloc e
/// uso se due core allocano contemporaneamente (oggi: BSP only).
pub unsafe fn alloc(size: usize, align: usize) -> Result<*mut u8, AllocError> {
    let base = HEAP.0.as_ptr() as usize;
    let start = align_up(base + BUMP.load(Ordering::Relaxed), align);
    let end = start + size;
    if end - base > HEAP_SIZE {
        return Err(AllocError::OutOfMemory);
    }
    BUMP.store(end - base, Ordering::Relaxed);
    Ok(start as *mut u8)
}

/// Azzera il puntatore di bump: riparte dall'inizio dell'heap.
/// (usato a fine "ciclo di vita" del modello, o su reset)
pub fn reset() {
    BUMP.store(0, Ordering::Relaxed);
}

/// Byte allocati finora (diagnosi)
pub fn allocated_bytes() -> usize {
    BUMP.load(Ordering::Relaxed)
}

/// Capacità totale dell'heap (diagnosi)
pub fn capacity_bytes() -> usize {
    HEAP_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_basic() {
        reset();
        unsafe {
            let p = alloc(16, 8).expect("alloc");
            assert_eq!(p as usize % 8, 0);
            // scrittura di prova
            core::ptr::write_volatile(p, 0xABu8);
            assert_eq!(core::ptr::read_volatile(p), 0xAB);
        }
    }

    #[test]
    fn alloc_alignment() {
        reset();
        unsafe {
            let p1 = alloc(1, 1).unwrap();
            let p2 = alloc(8, 64).unwrap();
            assert_eq!(p2 as usize % 64, 0, "p2 deve essere allineato a 64");
            assert!(p2 as usize > p1 as usize);
        }
    }

    #[test]
    fn alloc_exhaust() {
        reset();
        unsafe {
            let r = alloc(HEAP_SIZE + 1, 1);
            assert!(r.is_err());
        }
    }

    #[test]
    fn reset_reuses() {
        reset();
        unsafe {
            let p1 = alloc(100, 8).unwrap();
            reset();
            let p2 = alloc(100, 8).unwrap();
            assert_eq!(p1, p2, "dopo reset riparte dallo stesso indirizzo");
        }
    }
}
