// ── Banco host per il dolore commandato ──
//
// Perché esiste (05/10/2026): un page fault è un evento di UN solo tick.
// Sul Lenovo e in QEMU l'impulso -2.0 sul Tatto[0] non è mai arrivato
// all'argmax dell'interprete: 86 campioni su 8600 tick, `concept=riposo`
// senza eccezione. Il dolore non entrava nella percezione, quindi la
// catena dolore → FUGA → imparare dall'esito non era esercitabile.
// Un impulso troppo breve è indistinguibile dal silenzio.
//
// Serve poter SOSTENERE lo stimolo per un numero di tick noto: è
// l'equivalente sperimentale dell'elettrostimolazione in neuroscience.
// Test sul codice REALE (#[path] su src/pain.rs), mai su una copia.

#[path = "../src/pain.rs"]
mod pain;

use pain::Pain;

#[test]
fn appena_nato_non_fa_dolore() {
    let p = Pain::new();
    assert!(!p.attivo());
    assert_eq!(p.tick(), 0);
}

#[test]
fn armato_fa_dolore() {
    let mut p = Pain::new();
    p.arm(0xDEADBEEF, 10);
    assert!(p.attivo());
    assert_eq!(p.tick(), 10);
    assert_eq!(p.addr(), 0xDEADBEEF);
}

#[test]
fn scade_tick_per_tick() {
    let mut p = Pain::new();
    p.arm(1, 3);
    for atteso in [2u32, 1, 0] {
        p.scadi();
        assert_eq!(p.tick(), atteso, "conteggio dopo lo scadere");
    }
    assert!(!p.attivo(), "esaurito il dolore deve spegnersi");
}

#[test]
fn resta_attivo_mentre_il_conteggio_e_positivo() {
    // Un solo scadere non deve spegnere un dolore armato per 100 tick:
    // e' la differenza tra impulso e stimolo.
    let mut p = Pain::new();
    p.arm(1, 100);
    p.scadi();
    assert!(p.attivo());
    assert_eq!(p.tick(), 99);
}

#[test]
fn saturazione_a_zero() {
    // Scadere quando è già spento non deve andare sotto zero (underflow).
    let mut p = Pain::new();
    for _ in 0..10 {
        p.scadi();
    }
    assert_eq!(p.tick(), 0);
    assert!(!p.attivo());
}

#[test]
fn spegni_ferma_immediatamente() {
    let mut p = Pain::new();
    p.arm(7, 500);
    assert!(p.attivo());
    p.spegni();
    assert!(!p.attivo());
    assert_eq!(p.tick(), 0);
}

#[test]
fn armare_zero_tick_non_fa_dolore() {
    // Un comando `PAIN ... 0` deve significare "nessun dolore", non
    // "dolore infinito": è la differenza tra uno strumento e un bug.
    let mut p = Pain::new();
    p.arm(0xDEADBEEF, 0);
    assert!(!p.attivo());
}

#[test]
fn riarmatura_azzera_il_conteggio_precistente() {
    let mut p = Pain::new();
    p.arm(1, 50);
    p.scadi();
    p.scadi();
    p.arm(2, 5);
    assert_eq!(p.tick(), 5, "riarmare riparte da zero, non prosegue");
    assert_eq!(p.addr(), 2, "l'indirizzo è quello nuovo");
}

#[test]
fn spegnere_uno_spento_non_fa_nulla() {
    let mut p = Pain::new();
    p.spegni();
    p.arm(1, 5);
    p.spegni();
    p.spegni();
    assert_eq!(p.tick(), 0);
}

#[test]
fn indirizzo_zero_e_lecito() {
    // Usiamo 0xDEADBEEF di solito, ma l'indirizzo non è il concetto:
    // il dolore è il canale, non l'indirizzo. Deve poter valere 0.
    let mut p = Pain::new();
    p.arm(0, 4);
    assert!(p.attivo());
    assert_eq!(p.addr(), 0);
}

#[test]
fn ciclo_completo_conta_esattamente_n_tick() {
    let n = 37u32;
    let mut p = Pain::new();
    p.arm(0x1000, n);
    let mut attivi = 0;
    for _ in 0..n {
        if p.attivo() {
            attivi += 1;
        }
        p.scadi();
    }
    assert_eq!(attivi, n as usize, "il dolore deve durare esattamente n tick");
    assert!(!p.attivo());
}

#[test]
fn default_non_fa_dolore() {
    // L'errore più pericoloso sarebbe un Default che arma il dolore:
    // ogni avvio di Exo sarebbe un avvio nel dolore.
    let p: Pain = Default::default();
    assert!(!p.attivo());
}
