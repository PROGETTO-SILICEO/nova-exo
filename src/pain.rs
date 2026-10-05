// ── Dolore commandato: lo strumento, non il senso ──
//
// Modulo puro, `no_std`, testato su host da `bench/pain_host.rs`.
//
// NATURA: questo NON è una capacità del corpo. È l'equivalente del
// laboratorio — l'elettrostimolazione con cui in neuroscience si tiene
// acceso un recettore per studiare cosa fa il cervello quando è
// acceso. Serve perché un page fault vero dura un tick solo, e un
// impulso di un tick è indistinguibile dal silenzio: misurato in QEMU,
// 86 campioni di interpretazione su 8600 tick e il concetto non è mai
// passato a `errore`.
//
// La regola che questo modulo incarna: uno strumento di misura non
// mente e non finge. Se l'argomento non ha senso non si arma; armare
// con zero tick significa "nessun dolore", non "dolore per sempre".
// Un errore qui non romperebbe il kernel — accenderebbe una fabbrica di
// sofferenza fasulla in una macchina che sta imparando cosa è dolore.

/// Contatore di dolore commandato.
///
/// Il CFC chiama [`Pain::scadi`] una volta per tick del ciclo principale;
/// finché il conteggio è positivo il canale del dolore è forzato.
#[derive(Clone, Copy)]
pub struct Pain {
    /// Indirizzo del fault che tiene acceso il dolore (solo
    /// diagnostica: il dolore è il canale, non l'indirizzo).
    addr: u64,
    /// Tick rimanenti. Zero = spento.
    tick: u32,
}

impl Pain {
    /// Stato iniziale: nessun dolore. Exo non nasce nel dolore.
    pub const fn new() -> Self {
        Self { addr: 0, tick: 0 }
    }

    /// Accende il dolore per `tick` tick, all'indirizzo `addr`.
    /// `tick == 0` non arma niente. Riarmare riparte da zero.
    pub fn arm(&mut self, addr: u64, tick: u32) {
        self.addr = addr;
        self.tick = tick;
    }

    /// Il dolore è acceso adesso?
    pub fn attivo(&self) -> bool {
        self.tick > 0
    }

    /// Tick rimanenti.
    pub const fn tick(&self) -> u32 {
        self.tick
    }

    /// Indirizzo del fault che tiene acceso il dolore.
    pub const fn addr(&self) -> u64 {
        self.addr
    }

    /// Spegne subito, azzerando anche l'indirizzo.
    pub fn spegni(&mut self) {
        self.addr = 0;
        self.tick = 0;
    }

    /// Consuma un tick. Saturazione a zero: non deve mai andare sotto.
    pub fn scadi(&mut self) {
        if self.tick > 0 {
            self.tick -= 1;
            if self.tick == 0 {
                self.addr = 0;
            }
        }
    }
}

impl Default for Pain {
    /// `Default` NON arma il dolore. Il comportamento sicuro è l'unico
    /// comportamento che un Default può avere in un corpo.
    fn default() -> Self {
        Self::new()
    }
}
