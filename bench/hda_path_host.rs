// ── Banco host per il PERCORSO DI CATTURA dell'udito ──
//
// La scelta della catena (ADC → instradatore → pin) è PURA: si verifica su
// host col codice REALE (#[path] su src/hda_path.rs). Sbagliare questa scelta
// significa non trovare il microfono sul codec vero (il difetto del 14/10 sul
// Lenovo ALC233: ADC 0x08 → MIXER 0x23 → pin 0x1b/0x19).
//
// Compilazione: make test   (oppure rustc --test bench/hda_path_host.rs)

#[path = "../src/hda_path.rs"]
mod hda_path;

use hda_path::{
    conn_node, e_instradatore, scegli_controller, scegli_instradatore, scegli_pin,
    CandidatoController, Sorgente, VENDOR_REALTEK, WID_AUD_IN, WID_MIXER, WID_PIN, WID_SELECTOR,
};

fn pin(id: u8, input_cap: bool, mic: bool) -> Sorgente {
    Sorgente { id, tipo: WID_PIN, input_cap, mic }
}

fn mixer(id: u8) -> Sorgente { Sorgente { id, tipo: WID_MIXER, input_cap: false, mic: false } }
fn selector(id: u8) -> Sorgente { Sorgente { id, tipo: WID_SELECTOR, input_cap: false, mic: false } }
fn adc(id: u8) -> Sorgente { Sorgente { id, tipo: WID_AUD_IN, input_cap: false, mic: false } }

#[test]
fn la_connection_list_impacchetta_quattro_nodi() {
    // 0x04030201 → nodi 1, 2, 3, 4 negli indici 0..3.
    let w = 0x0403_0201u32;
    assert_eq!(conn_node(w, 0), 1);
    assert_eq!(conn_node(w, 1), 2);
    assert_eq!(conn_node(w, 2), 3);
    assert_eq!(conn_node(w, 3), 4);
    // L'indice si ripiega su 0..3 (una parola = 4 nodi).
    assert_eq!(conn_node(w, 4), 1);
}

#[test]
fn solo_mixer_e_selector_sono_instradatori() {
    assert!(e_instradatore(WID_MIXER));
    assert!(e_instradatore(WID_SELECTOR));
    assert!(!e_instradatore(WID_PIN));
    assert!(!e_instradatore(WID_AUD_IN));
}

#[test]
fn l_instradatore_preferisce_il_mixer() {
    // Scenario ALC233: l'ADC vede un MIXER (0x23) → si sceglie quello.
    let cands = [mixer(0x23), pin(0x19, true, false)];
    assert_eq!(scegli_instradatore(&cands), Some(0));
    // Se c'è un solo SELECTOR, si sceglie quello.
    let cands = [pin(0x19, true, false), selector(0x24)];
    assert_eq!(scegli_instradatore(&cands), Some(1));
    // Solo pin → nessun instradatore (l'ADC punta dritto al pin, caso QEMU).
    let cands = [pin(5, true, false)];
    assert_eq!(scegli_instradatore(&cands), None);
}

#[test]
fn il_pin_preferisce_il_jack_mic() {
    // La CONNECT_LIST del MIXER: pin 0x18..0x1b, solo 0x1b è il mic.
    let cands = [
        pin(0x18, true, false),
        pin(0x19, true, false),
        pin(0x1b, true, true),
    ];
    assert_eq!(scegli_pin(&cands), Some(2), "deve preferire il jack mic (0x1b)");
}

#[test]
fn il_pin_ignora_chi_non_ha_ingresso() {
    // Un pin di uscita (input_cap=false) non è un ingresso: va saltato.
    let cands = [pin(0x14, false, false), pin(0x19, true, false)];
    assert_eq!(scegli_pin(&cands), Some(1));
}

#[test]
fn senza_pin_di_ingresso_non_c_e_sorgente() {
    let cands = [pin(0x14, false, false), mixer(0x23)];
    assert_eq!(scegli_pin(&cands), None);
}

#[test]
fn catena_alc233_ipotetica() {
    // ADC(0x08) → lista { MIXER 0x23 } → scegli il mixer (indice 0).
    let adc_list = [mixer(0x23)];
    let i1 = scegli_instradatore(&adc_list).expect("mixer presente");
    assert_eq!(adc_list[i1 as usize].id, 0x23);
    // MIXER(0x23) → lista { pin 0x18, 0x19, 0x1b(mic), 0x1e } → scegli 0x1b.
    let mix_list = [
        pin(0x18, true, false),
        pin(0x19, true, false),
        pin(0x1b, true, true),
        pin(0x1e, true, false),
    ];
    let i2 = scegli_pin(&mix_list).expect("pin d'ingresso presente");
    assert_eq!(mix_list[i2 as usize].id, 0x1b);
}

#[test]
fn catena_qemu_diretta() {
    // QEMU hda-micro: l'ADC(4) punta dritto al pin(5) → nessun instradatore,
    // poi si sceglie il pin d'ingresso.
    let adc_list = [pin(5, true, false)];
    assert_eq!(scegli_instradatore(&adc_list), None);
    let i = scegli_pin(&adc_list).expect("pin d'ingresso presente");
    assert_eq!(adc_list[i as usize].id, 5);
    let _ = adc(4); // il tipo ADC resta distinto dai pin (documentazione).
}

// ── Scelta del CONTROLLER HDA (classe 04:03) ───────────────────────────────
// Sul Lenovo AMD ci sono due controller: HDMI della GPU (vendor 0x1002, solo
// uscite) e Realtek (0x10EC, col microfono). La scelta è PURA e va verificata:
// prendere il primo era il difetto che dava `adc=-- pin=--`.

fn ctrl(vendor: u16) -> CandidatoController {
    CandidatoController { presente: true, vendor }
}

#[test]
fn sceglie_il_realtek_non_il_primo() {
    // Il primo controller è l'HDMI AMD (0x1002): NON si sceglie.
    let cands = [ctrl(0x1002), ctrl(VENDOR_REALTEK)];
    assert_eq!(scegli_controller(&cands), Some(1), "deve scegliere il Realtek (0x10EC)");
}

#[test]
fn ripiega_sul_primo_con_un_codec() {
    // Nessun Realtek: si torna al comportamento storico (primo con un codec).
    let cands = [ctrl(0x1002), ctrl(0x1002)];
    assert_eq!(scegli_controller(&cands), Some(0));
    // Un controller senza codec non conta come candidato.
    let cands = [
        CandidatoController { presente: false, vendor: 0 },
        ctrl(0x8086),
    ];
    assert_eq!(scegli_controller(&cands), Some(1));
}

#[test]
fn nessun_controller_con_codec_e_none() {
    let cands = [
        CandidatoController { presente: false, vendor: 0 },
        CandidatoController { presente: false, vendor: VENDOR_REALTEK },
    ];
    assert_eq!(scegli_controller(&cands), None);
    assert_eq!(scegli_controller(&[]), None);
}
