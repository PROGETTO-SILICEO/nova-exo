# Report — fix dei 3 difetti hardware (Lenovo AMD A8-7410, fam. 16h)

Branch: `feat/sensi-fix` · data: 17/10/2026
Metodo: implementazione dei fix diagnosticati il 14/10 (`diaries/2026-10-14.md`,
righe 46–62) e mappati in `KB-20261014-07`. Verifica sul **metallo = ri-deploy
successivo**; qui: `make test` (banchi host) + `make build` (kernel x86_64-unknown-none).

---

## (1) UDITO — la scoperta dei nodi non percorreva il MIXER

**Cosa ho cambiato**
- **Nuovo modulo PURO `src/hda_path.rs`**: tipi di widget (`WID_MIXER`,
  `WID_SELECTOR`, …), decodifica della connection list (`conn_node`), e due
  decisioni pure: `scegli_instradatore` (preferisce MIXER, poi SELECTOR) e
  `scegli_pin` (preferisce il jack `JACK_MIC_IN`, altrimenti il primo pin con
  `PINCAP_INPUT`). Nessun nodo hardcoded: sono gli stessi bit del codec.
- **`src/hda.rs`**: la vecchia `scopri_nodi` (ADC + pin "piatti") è sostituita da
  `scopri_catena`, che CHIEDE al codec la catena:
  1. trova il primo `WID_AUD_IN` (ADC);
  2. legge la `CONNECT_LIST` (verbo `0xF02`) dell'ADC, sceglie l'instradatore
     (`MIXER`/`SELECTOR`) e imposta la `CONNECT_SELECT` (verbo `0x701`) dell'ADC
     su di esso;
  3. legge la `CONNECT_LIST` dell'instradatore, sceglie il pin d'ingresso e
     imposta la **sua** `CONNECT_SELECT`;
  4. sblocca l'amp d'ingresso selezionato del mixer (`VERB_SET_AMP_GAIN_MUTE`,
     indice = sorgente scelta).
  Se l'ADC punta già a un pin (QEMU `hda-micro`) il percorso diretto è gestito
  senza instradatore. Aggiunto lo stato `via` (`hda::via()`) e stampato sulla
  riga diagnostica `HDA:t` insieme ad `adc=`/`pin=`.
  Atteso sul Lenovo (ALC233): `adc=8 via=23 pin=1b|19`.

**Test** — `bench/hda_path_host.rs` (8 test, codice REALE via `#[path]`):
impacchettamento a 4 nodi nella connection list; preferenza mixer>selector;
preferenza del jack mic sul pin; salto dei pin senza ingresso; catena ALC233
(`ADC→0x23→0x1b`); catena QEMU diretta.

**Dubbi (non verificabili senza metallo)**
- Che la `CONNECT_LIST` dell'ADC dell'ALC233 contenga *direttamente* il mixer
  0x23 (la mappa lo dice; l'algoritmo è generico e attraversa un solo anello —
  se il percorso reale avesse due anelli, servirebbe iterare).
- L'ordine dei byte nella risposta di `0xF02` per liste >4 nodi (uso l'indice
  logico come `snd_hda_get_connections`): su liste lunghe è il punto più fragile.
- Bias del microfono (`VREFEN` nei bit bassi di `PIN_CTL`): NON toccato (fuori
  scope). Se sul metallo la catena si scopre ma la cattura è muta, è il primo
  candidato.

---

## (2) TEMPERATURA AMD — solo MSR Intel, mai la via AMD

**Cosa ho cambiato** — `src/intero.rs`:
- **Vendor**: `cpuid(0)` → `vendor_intel()` / `vendor_amd()` (stringhe
  "GenuineIntel" / "AuthenticAMD"). `family_from_eax()` è PURA.
- **Via AMD**: `smn_read(addr)` scrive l'indirizzo nell'**index register**
  (`0xB8`) e legge il **data register** (`0xBC`) di `00:00.0` via `crate::pci`;
  registro `THM_TCON_CUR_TMP = 0x00059800` (famiglia 15h/16h). Decodifica PURA
  `tctl_da_raw = (raw >> 21) * 0.125`, con guardie (0 / tutti-1 / fuori range
  → `None`).
- `temperatura()`: se Intel → MSR (come prima); se AMD → SMN **solo su famiglia
  15h/16h**; altrimenti `None`. **Nessuna lettura cieca**: il vendor è guardato
  prima di ogni accesso (lezione del 14/10: MSR Intel su AMD → #GP → blocco).

**Test** — `bench/intero_host.rs` (10 test): i 6 storici sulla decodifica Intel,
più `tctl_da_raw` (sensore assente, 50/100 °C, 1/8 di grado, fuori range,
bit bassi che non inquinano) e `family_from_eax` (Intel 6/15, AMD 16h/17h).

**Dubbi (non verificabili senza metallo)**
- **Punto più incerto.** La KB (`KB-20261014-07`) dice famiglia 15h/16h → SMN
  `0x00059800` su `00:00.0` con index/data `0xB8/0xBC`. Il **k10temp attuale**
  (verificato via webfetch su `torvalds/linux`, `drivers/hwmon/k10temp.c`) usa
  `0xB8/0xBC` solo per **15h M60h/M70h**; per **famiglia 16h** legge il registro
  `REG_REPORTED_TEMPERATURE = 0xA4` in config space del device NB F3 (con la
  *stessa* decodifica `(raw>>21)*0.125`). Ho implementato la via SMN come da KB
  e task; **se sul metallo dà `None`/valore implausibile**, il fallback da
  provare è config `0xA4` sul device NB F3 (BDF da confermare: D18F3).
- Il registro `0x00059800` su `00:00.0` potrebbe non essere emulato dal chipset
  Jaguar (SoC): la guardia di range evita di riportare una temperatura inventata,
  ma il "sensore c'è" dipende dal metallo.

---

## (3) CONTATTO / "fuga" col touch — smistamento PS/2 8042

**Cosa ho trovato e cambiato**
- Il ruscellamento by-AUX era **già presente** (`kbd::read_source` + il ramo
  `if aux { touchpad::feed }` in `main.rs`, commit `71a253b`): i byte del mouse
  NON entravano nella tastiera. Il difetto quindi non era la mancanza dello
  smistamento. Ho comunque reso la decisione **esplicita, pura e testata**:
  - `src/ps2.rs`: nuovo `Porta { Tastiera, Mouse }` e `destinazione(status)`
    (bit0 OBF, bit5 AUX) — PURA.
  - `src/kbd.rs`: `read_source` ora usa `ps2::destinazione` (l'IO resta qui;
    la decisione è verificata su host). Si legge lo STATUS **prima** del byte.
- Il ramo di `main.rs` continua a mandare gli AUX al touchpad (`feed`) e i byte
  tastiera alla tastiera, con lo stesso filtro ACK/self-test/`0xE0`.

**Test** — `bench/touchpad_host.rs` (12 test): i 8 storici sul protocollo +
`destinazione` (niente byte se OBF=0; tastiera con AUX=0; mouse con AUX=1).

**Dubbi (non verificabili senza metallo) — il difetto è probabilmente altrove**
- Lo smistamento c'era già: **il fix del "CONTATTO satura / fuga col touch" non è
  dimostrabile con questa sola modifica.** Ipotesi residue da verificare sul
  metallo, in ordine di probabilità:
  1. il touchpad reale (Synaptics/Elantech) può **non essere in modalità 3-byte
     standard** dopo `0xFF`/`0xF4` → l'`Assemblatore` (che richiede il bit3 sul
     primo byte) scarta/desincronizza pacchetti, producendo gesti spuri e
     contatto continuo. Da provare: leggere `ID` (già esposto in `TP-ID`) e, se
     non-standard, impostare il modo relativo corretto.
  2. `cfc::sense_touchpad(attivo)` con `attivo` vero a ogni movimento tiene il
     contatto esteso alto e può alimentare il riflesso di "fuga": da separare
     **movimento** (gesto) da **contatto** (dito giù), e dare al contatto una
     soglia/finestra.
  3. Il bit AUX del FCH AMD è affidabile? Lo smistamento dipende dal controller;
     se su questo chipset il bit5 non è popolato, serve una via diversa (es. due
     IRQ separati IRQ1/IRQ12) — l'IRQ è già previsto come passo successivo.

---

## Verifica automatica
- `make test` → **TUTTI I BANCHI VERDI** (hda_path_host 8, intero_host 10,
  touchpad_host 12; totale suite invariata verde).
- `make build` → **Finished release**, `check-elf` **Alignment OK**.
- Nessuna nuova dipendenza, `no_std` preservato, nessuna allocazione aggiunta.

## Cosa NON è verificabile senza il metallo (riepilogo)
1. UDITO: che `CONNECT_LIST` reale dell'ALC233 dia la catena `0x08→0x23→0x1b/19`.
2. TEMPERATURA: se la via SMN `0x00059800`/`0xB8-0xBC` è quella giusta per la
   famiglia 16h (la KB dice sì, il k10temp attuale indica `0xA4` come fallback).
3. CONTATTO: la causa vera della "fuga" col touch (probabile modalità-pacchetto
   del touchpad o semantica contatto-vs-gesto), non il routing.
