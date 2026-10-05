# Piano — Telemetria UDP di Exo (110-15ACL → guardiano)

Data: 2026-10-05. Autore: Sempre. Stato: da eseguire.

## Intento (perché)

Chiudere la domanda aperta **"il concetto si muove a 5 milioni di tic?"** senza
che Alfonso debba trascrivere righe dallo schermo. Serve telemetria continua che
porti **`concept`** e **`imparato`/`esplorato`** — le due cose che oggi esistono
solo sulla seriale.

Vincolo che decide l'architettura: sono in sandbox `no_new_privs` → **non
posso catturare frame Ethernet grezzi**. Un pacchetto **IPv4/UDP** invece lo
ricevo con un socket normale su porta >1024, **senza privilegi**. Quindi UDP,
non il frame `0x88B5`.

## Requisiti espliciti

1. Il 110-15ACL spedisce un pacchetto **IPv4/UDP broadcast** con telemetria.
2. Payload: `t` (tick), `k` (concept), `c` (4×8 celle), `w` (volontà), `l` ([imparato, esplorato]).
3. **Non rompere** il beacon `0x88B5` esistente né il timing del loop.
4. L'invio è **non bloccante**: se il descrittore TX è occupato, si salta il giro.
5. Il costruttore di frame è **puro** (niente MMIO, niente unsafe) → testabile su host.
6. Il ricevitore è **senza privilegi** e **isolato**: niente eval, parsing a lunghezza limitata.
7. Test su host per ogni pezzo non banale, secondo il pattern già in `bench/`.

## Costanti

| cosa | valore | perché |
|---|---|---|
| porta UDP | **42800** (src = dst) | >1024, non privilegiata, non assegnata |
| IP sorgente | **192.168.0.200** | statico: il 110 non ha DHCP |
| IP destinazione | **192.168.0.255** | broadcast di rete |
| MAC destinazione | `FF:FF:FF:FF:FF:FF` | broadcast |
| MAC sorgente | parametro (default `c8:5b:76:e0:e0:01`, quello del beacon che già funziona) | |
| cadenza | ogni **100 tic** | come il percorso e1000 |

## Step atomici (uno alla volta, test prima)

### S1 — `src/net.rs`: checksum IPv4
- Test: vettori noti (header a zeri → 0xFFFF; vettore calcolato a mano).
- Codice: `pub fn ipv4_checksum(hdr: &[u8]) -> u16`.
- **Criterio**: test rosso → codice → verde.

### S2 — `src/net.rs`: formattazione numerica nel buffer
- Test: interi (0, 7, 100, 4294967295) e f32 (`1.0000`, `-0.1942`, `0.0`, `-0.0`).
- Codice: `write_u32_buf`, `write_f32_buf` (come quelli di `e1000.rs`; **debito noto**: duplicazione da consolidare).
- **Criterio**: stesse uscite del percorso e1000.

### S3 — `src/net.rs`: costruzione del frame completo
- Test: ethertype `0x0800`; dst MAC broadcast; IP v4/IHL 5/proto 17; total length; UDP ports e length; IP checksum verificato con `ipv4_checksum`; dst IP `192.168.0.255`; lunghezza ≥ 60 e ≤ 1518.
- Test: il JSON contiene `"k":"riposo"`, `"l":[`, i 4 array.
- Codice: `pub fn build_state_frame(dst: &mut [u8; 1518], src_mac, tick, concept_idx, cells, desire_id, desire_int, cambi, esplorazioni) -> usize`.
- **Criterio**: tutti i test verdi.

### S4 — cablaggio in `src/main.rs`
- Ogni 100 tic: costruisci il frame e `rtl::tx_frame`, **solo se il descrittore è libero**.
- **Criterio**: build release ok, `check-elf` ok, QEMU non panica, il beacon continua.

### S5 — `tools/exo_listen.py` (ricevitore)
- Bind UDP su `0.0.0.0:42800`, parsing JSON con limiti, stampa e append su file.
- **Criterio**: test di parsing con payload finti (malformati inclusi) non crasha.

### S6 — verifica end-to-end sul metallo
- Alfonso avvia la chiavetta; io leggo i pacchetti; confronto `concept` con la seriale.
- **Criterio**: `concept` via rete == `concept` sulla seriale, per N campioni.

## Checkpoint di review
Dopo **S3** e dopo **S6**: fermarsi e riportare. Non proseguire oltre senza verifica.

## Fuori scope (per ora)
- Rimpiazzare il beacon `0x88B5` (resta: è la prova "sono vivo").
- Consolidare il formattatore duplicato (debito registrato nel ledger).
- Togliere il sandbox (decisione separata, deliberata).
