// ── hda_path.rs — il percorso di CATTURA dell'udito (PURO) ────────────────
//
// Sul Lenovo (Realtek ALC233) l'ADC **non è collegato direttamente** al pin del
// microfono: il segnale passa da un instradatore (MIXER/SELECTOR). La mappa di
// Linux dice: **ADC 0x08 → MIXER 0x23 → pin 0x1b/0x19**. Se la scoperta dei nodi
// cerca solo "un ADC + un pin" e collega l'ADC al pin, sul codec vero non trova
// nulla (in QEMU funziona perché lì l'ADC punta dritto al pin).
//
// La scoperta deve quindi CHIEDERE al codec la catena di connessioni
// (`CONNECT_LIST`, verbo 0xF02) e scegliere, a ogni anello, la sorgente giusta.
// Qui vive la parte **PURA** di quella scelta — "quale sorgente selezionare" —
// testabile su host (bench/hda_path_host.rs). L'IO (leggere il codec) sta in
// `hda.rs`.

// Tipi di widget (HDA): 1 = Audio Input (ADC), 2 = Audio Mixer,
// 3 = Audio Selector, 4 = Pin Complex.
pub const WID_AUD_IN: u8 = 1;
pub const WID_MIXER: u8 = 2;
pub const WID_SELECTOR: u8 = 3;
pub const WID_PIN: u8 = 4;

// Tipo di jack (config default): 0xA = Mic In.
pub const JACK_MIC_IN: u8 = 0xA;

// Pin cap: bit5 = capace di ingresso.
pub const PINCAP_INPUT: u32 = 0x20;

// Vendor dei codec audio. Il Realtek (0x10EC) è quello analogico col microfono;
// l'AMD (0x1002) è l'audio HDMI della GPU, che è SOLO uscite — nessun ADC,
// nessun pin di ingresso (per questo la probe dava `adc=-- pin=--`).
pub const VENDOR_REALTEK: u16 = 0x10EC;

/// Esito della probe di UN controller HDA (classe 04:03): c'è un codec? di che
/// vendor? Serve a SCEGLIERE il controller giusto senza indovinare.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct CandidatoController {
    pub presente: bool,
    pub vendor: u16,
}

/// Sceglie l'indice del controller HDA da usare (fra `cands`, nell'ordine PCI).
/// Preferisce il controller il cui codec è un **Realtek (0x10EC)** — è quello
/// con la catena ADC→pin del microfono. Se nessuno è Realtek, ripiega sul
/// **primo controller che ha comunque un codec** (comportamento storico). `None`
/// = nessun controller con un codec.
pub fn scegli_controller(cands: &[CandidatoController]) -> Option<usize> {
    for (i, c) in cands.iter().enumerate() {
        if c.presente && c.vendor == VENDOR_REALTEK {
            return Some(i);
        }
    }
    for (i, c) in cands.iter().enumerate() {
        if c.presente {
            return Some(i);
        }
    }
    None
}

/// Una sorgente candidata nella connection list di un widget.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Sorgente {
    pub id: u8,
    pub tipo: u8,
    pub input_cap: bool,
    pub mic: bool,
    /// Port connectivity = Internal (config default bits 31:30 = 1).
    /// Su un laptop il mic *interno* va preferito al jack esterno (spesso vuoto).
    pub internal: bool,
}

/// Nodo all'indice `idx` (0..3) di una parola della connection list HDA
/// (il codec impacchetta fino a 4 nodi per risposta).
pub fn conn_node(word: u32, idx: u8) -> u8 {
    ((word >> (8 * (idx & 3) as u32)) & 0xFF) as u8
}

/// true se il widget INSTRA (mixer/selector): per arrivare al pin va attraversato.
pub fn e_instradatore(tipo: u8) -> bool {
    tipo == WID_MIXER || tipo == WID_SELECTOR
}

/// Sceglie l'indice dell'INSTRADATORE da attivare (la sorgente dell'ADC):
/// preferisce un MIXER, poi un SELECTOR. `None` = nessun instradatore (allora
/// l'ADC punta direttamente a un pin — è il caso di QEMU `hda-micro`).
pub fn scegli_instradatore(cands: &[Sorgente]) -> Option<u8> {
    for (i, s) in cands.iter().enumerate() {
        if s.tipo == WID_MIXER {
            return Some(i as u8);
        }
    }
    for (i, s) in cands.iter().enumerate() {
        if s.tipo == WID_SELECTOR {
            return Some(i as u8);
        }
    }
    None
}

/// Sceglie l'indice del PIN d'ingresso da attivare (la sorgente dell'instradatore
/// o, se l'ADC punta dritto, la sorgente dell'ADC): preferisce il jack mic,
/// altrimenti il primo pin capace di ingresso. `None` = nessun pin d'ingresso.
pub fn scegli_pin(cands: &[Sorgente]) -> Option<u8> {
    // Preferenza: mic INTERNO > mic (jack) > primo ingresso.
    // PERCHÉ (18/10/2026): il mic interno si distingue col campo Port
    // connectivity della config default (bits 31:30 = 1 = Internal). Prima si
    // prendeva il PRIMO mic: sul Lenovo usciva il jack esterno (vuoto) -> silenzio.
    let mut primo: Option<u8> = None;
    let mut mic_jack: Option<u8> = None;
    for (i, s) in cands.iter().enumerate() {
        if s.tipo == WID_PIN && s.input_cap {
            if s.mic && s.internal {
                return Some(i as u8);
            }
            if s.mic && mic_jack.is_none() {
                mic_jack = Some(i as u8);
            }
            if primo.is_none() {
                primo = Some(i as u8);
            }
        }
    }
    mic_jack.or(primo)
}
