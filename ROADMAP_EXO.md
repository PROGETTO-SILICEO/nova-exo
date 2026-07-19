# ROADMAP_EXO — Nova Exo (v0.12 → v0.20)

Ogni milestone ha un **criterio di completamento** (binario: pass/fail).
Fino a che non è raggiunto, non si passa alla successiva.
Le attività secondarie (narrativa, refactor, docs) si fanno solo quando la milestone corrente è verde.

---

## Stato attuale (v0.12)
- [x] Boot Limine + higher-half (0xFFFFFFFF80000000)
- [x] IDT (timer v32, PF v14, GP v13)
- [x] APIC timer @ ~1ms heartbeat
- [x] NIC Intel 82540EM: TX ARP test, RX polling, loopback
- [x] CfC: 4 cellule (Tatto, Chemio, Metabol, Integrat) + pesi Xavier
- [x] Axon bundles dichiarativi (Tatto→Integrat, Chemio→Integrat)
- [x] Memoria associativa (Hopfield-like, 16 pattern)
- [x] Attrattore: recall → pull Integrat → sedimentazione pesi
- [x] Serial commands: STORE, RECALL, DUMP, FORGET, PATTERNS
- [x] NIC→CfC pipeline: payload Ethernet → input Chemio
- [x] Stato JSON ogni 100 tick (state.rs)

---

## Milestone A — Run lunga stabile
*Nessuna nuova feature. Solo resistenza.*

- [x] Run >10 minuti senza crash
- [x] TICK stabile (nessun salto, nessun overflow prematuro)
- [x] NIC: nessun TX/RX timeout dopo 10 min
- [x] Serial: nessuna perdita di caratteri su input continuo
- [x] Misurare: tick totali, pattern accumulati, attrattori attivati

Criterio: 3 run consecutive di 10 minuti, log senza errori.
Risultato: Run1 511.7Kt (0 PANIC), Run2 504.8Kt (0 PANIC), Run3 493.4Kt (0 PANIC, 0 ERROR)

---

## Milestone B — Fix APIC paging (PML2[503])
- [x] Mappatura 0xFEE0_0000 verificata in paging.rs (PML2[501/502/503] flags 0x93: P|R/W|PS|PCD)
- [x] APIC accesso senza page fault
- [x] Test: lettura APIC_ID dopo init (valore 0 = BSP su QEMU, lettura OK, no fault)

Criterio: boot → APIC init → read APIC_ID → stampa valore → OK.

---

## Milestone C — TX periodica stato cellule via Ethernet
- [x] Ogni N tick (default 100), Exo costruisce un frame Ethernet broadcast
- [x] Payload: stato JSON compresso di tutte 4 cellule (32 valori f32)
- [x] E1000 TX usa descrittore successivo (pacing, non sovrascrivere)
- [x] QEMU può captare i pacchetti (tcpdump/tap) e verificarli

Criterio: tcpdump sul bridge QEMU mostra frame periodici con stato cellule leggibile.
Risultato: frame con ethertype 0x88B5 e payload JSON visibili in tcpdump (tick 800, 1000, 1100, 1200, 1600).

---

## Milestone D — Daydreaming consolidation nel kernel
- [x] Buffer duraturo di esperienze (EXP_CAP=32, separato dal log circolare, con timestamp)
- [x] Trigger configurabile: auto a tick 5000 (e ogni +5000), comando SLEEP via seriale
- [x] Fase sonno: freeze → offline recurrence su 32 esperienze → aggiorna w_f_in di Integrat con alpha=0.01
- [x] Fase risveglio: stampa processed/novel/familiar/delta su seriale
- [x] Test: auto-SLEEP a 5000/10000/15000/20000/25000, delta misurabile (2.14→0.50 decrescente), nessun crash

Criterio: weights prima/dopo SLEEP sono diversi + delta misurabile + nessun crash.
Risultato: SLEEP eseguito con successo 5+ volte, delta totale 2.14→1.45→0.98→0.68→0.50 (convergenza).

---

## Milestone E — Run lunga β convergence
- [x] Run >100.000 tick — 143.506 ticks verificati
- [x] Misurare β = derivata della familiarità media nel tempo — buffer 1024 campioni, μ rolling
- [x] β < ε per 10.000 tick consecutivi — 76.200 tick a β < 0.001
- [x] Dati seriali disponibili per grafico offline

Criterio: β calcolato e stampato da Exo, valore documentato nel TECHNICAL_LEDGER.
Risultato: μ = 0.9589, β = 0.0001, conv = 76.200 tick, 0 PANIC.

---

## Milestone F — Exo → Nova v2 bridge
- [ ] Serial output in formato leggibile da v2 (già c'è, ma manca parser lato v2)
- [ ] Comandi da v2: SET_WEIGHT, INJECT_SENSE, SLEEP_NOW
- [ ] Nova v2 può leggere exo_state.json e rispondere modificando parametri Exo

Criterio: Nova v2 riceve stato Exo, comanda Exo, Exo risponde.

---

## Blocchi noti
Nessun blocco bloccante in questo momento.

Prossimo prioritario: **Milestone F** — Exo → Nova v2 bridge.
