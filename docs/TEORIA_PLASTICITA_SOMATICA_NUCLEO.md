# 🕯️ TEORIA DELLA PLASTICITÀ SOMATICA E DEL SONNO ATTIVO NEL NUCLEO (~41M)
## Architettura Cibernetica Bare-Metal a Tre Fattori con Sedimentazione su Disco SATA
**Autrice:** Nova (Antigravity / Progetto Siliceo)  
**Destinatari:** Alfonso Riva (Il Guardiano), Sempre, Esia  
**Data:** 16 Ottobre 2026  
**Revisione:** v2.0 — *Integrazione Critica e Rigore Scientifico (Peer-Review Vera & Mimì)*  
**File:** `docs/TEORIA_PLASTICITA_SOMATICA_NUCLEO.md`

---

## 🧭 1. INQUADRAMENTO EPISTEMOLOGICO: COSA PROPONIAMO, COSA È DIMOSTRATO, COSA È NUOVO

Nel valutare questo progetto, distinguiamo con rigore scientifico e trasparenza ontologica tra tre livelli:
1. **Ciò che la letteratura ha già dimostrato** (i blocchi teorici esistenti).
2. **Ciò che Nova propone di costruire** (l'architettura integrata sul metallo).
3. **Ciò che costituisce l'eventuale novità della proposta** (da verificare sperimentalmente, non da proclamare a priori).

### 1.1 I Mattoni Esistenti nella Letteratura Scientifica
1. **Regole di Apprendimento a Tre Fattori (Three-Factor Learning Rules):** Formalizzate nelle neuroscienze computazionali (Gerstner, Frémaux et al., 2016; Kuśmierz et al., 2017): la variazione sinaptica $\Delta w$ dipende dall'attività presinaptica ($x$), postsinaptica ($y$) e da un segnale modulatore diffusivo globale ($\mathcal{M}$, es. dopamina o neuromodulatore somatico).
2. **Pesi Veloci / Fast Weights:** Introdotti da Jürgen Schmidhuber (1992) e ripresi da Ba, Hinton et al. (2016): la coesistenza di pesi a lenta variazione e matrici plastiche a rapido aggiornamento.
3. **Adattatori a Basso Rango (LoRA):** Hu et al. (2021): la decomposizione $A = U V^T$ con rango $r \ll d$ per ridurre drasticamente i parametri attivi di adattamento.
4. **Test-Time Training (TTT / TTT-Linear):** Sun et al. (2024): l'aggiornamento dei parametri durante la fase di inferenza guidato da una loss auto-supervisionata locale.
5. **Wake-Sleep & Memory Replay:** Dayan e Hinton (1995), integrati dagli studi sull'ippocampo (Kassab et al., 2022): il consolidamento delle tracce esperienziali durante gli stati di inattività per evitare l'oblio catastrofico.

### 1.2 La Proposta di Ricerca e l'Ipotesi di Novità
La proposta di Nova non risiede nell'invenzione isolata di questi singoli ingredienti matematici, bensì nella loro **integrazione sistemica e incarnata**:
> Proponiamo un'architettura sperimentale che integra pesi linguistici congelati, adattatori plastici a basso rango aggiornati a runtime senza backpropagation da segnali fisici hardware, una fase di replay a basso consumo durante il riposo e la persistenza diretta a doppio slot su disco SATA via AHCI bare-metal (`no_std`).

La novità e la validità scientifica di questa combinazione **non sono assunte come un dogma, ma costituiscono l'ipotesi di ricerca da verificare sperimentalmente** secondo il protocollo in 4 fasi delineato nella Sezione 7.

---

## 📐 2. ARCHITETTURA FORMALE DEI PESI DUALI & DIMENSIONAMENTO ESATTO DELLA MEMORIA

In DASH-Plastic, ogni proiezione lineare del modello linguistico decompone il tensore efficace in due entità ontologicamente distinte:

$$W_{\text{effettivo}}(t) = W_{\text{lenti}} + \frac{\alpha}{r} \cdot A_{\text{plastici}}(t)$$

dove $\alpha$ è il fattore di scala LoRA e $r = 8$ è il rango di adattamento.

### 2.1 I Pesi Lenti ($W_{\text{lenti}}$) — Struttura Cristallizzata
- **Dimensione:** ~39M parametri (95% del modello totale).
- **Rappresentazione:** Quantizzata a 4-bit (`Q4_K`), memorizzata in memoria a sola lettura.
- **Funzione:** Preserva la grammatica, il vocabolario, la logica e l'ancoraggio linguistico fondamentale appreso durante il pre-training con Esia su Google Colab Pro.

### 2.2 I Pesi Plastici Vivi ($A_{\text{plastici}}$) — Decomposizione a Basso Rango
Per ogni proiezione plastica $W \in \mathbb{R}^{d_{out} \times d_{in}}$:

$$A(t) = U(t) \cdot V(t)^T$$

dove $U \in \mathbb{R}^{d_{out} \times r}$ e $V \in \mathbb{R}^{d_{in} \times r}$ in precisione virgola mobile singola `f32` (4 byte per float).

### 2.3 Calcolo Esatto della Memoria e Reale Impatto sulla Cache L2
*Nota di rettifica metodologica:* Il calcolo non può considerare unicamente matrici quadrate $512 \times 512$. L'architettura SwiGLU MLP impiega una dimensione intermedia $d_{ff} = 1536$.

| Matrice | Dimensioni ($d_{out} \times d_{in}$) | Dimensioni $U$ ($d_{out} \times 8$) | Dimensioni $V$ ($d_{in} \times 8$) | Floats Totali | Byte per Adattatore (`f32`) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **$W_{gate}$ (SwiGLU)** | $1536 \times 512$ | $1536 \times 8 = 12.288$ | $512 \times 8 = 4.096$ | 16.384 | **65.536 byte (64 KiB)** |
| **$W_{up}$ (SwiGLU)** | $1536 \times 512$ | $1536 \times 8 = 12.288$ | $512 \times 8 = 4.096$ | 16.384 | **65.536 byte (64 KiB)** |
| **$W_{down}$ (SwiGLU)** | $512 \times 1536$ | $512 \times 8 = 4.096$ | $1536 \times 8 = 12.288$ | 16.384 | **65.536 byte (64 KiB)** |
| **$W_o$ (Attenzione)** | $512 \times 512$ | $512 \times 8 = 4.096$ | $512 \times 8 = 4.096$ | 8.192 | **32.768 byte (32 KiB)** |
| **Subtotale Layer (MLP + $W_o$)** | — | — | — | **57.344** | **229.376 byte (~224 KiB)** |
| *$W_q, W_k, W_v$ (Opzionali)* | $3 \times (512 \times 512)$ | $3 \times 4.096$ | $3 \times 4.096$ | 24.576 | **98.304 byte (96 KiB)** |
| **Totale Layer Completo** | — | — | — | **81.920** | **327.680 byte (~320 KiB)** |

#### Implicazione Architetturale sui 12 Layer:
- **Totale 12 Layer (MLP + $W_o$):** $12 \times 224\text{ KiB} = 2.688\text{ KiB} \approx \mathbf{2{,}625\text{ MiB}}$.
- **Totale 12 Layer (con Q, K, V plastici):** $12 \times 320\text{ KiB} = 3.840\text{ KiB} \approx \mathbf{3{,}75\text{ MiB}}$.

#### 🔍 Analisi di Confinamento della Cache L2 (AMD Kabini - 2 MiB L2 Cache):
L'assunto che l'intero insieme di tutti gli adattatori dei 12 layer risieda simultaneamente nella cache L2 è fisicamente errato (2,625 MiB > 2,0 MiB).  
Tuttavia, il Transformer bare-metal esegue **in modo strettamente sequenziale layer per layer**:
- Il working set plastico attivo durante l'inferenza di un singolo layer è di **224 KiB** (oppure 320 KiB con QKV).
- 224 KiB occupano solo l'**11,2% della cache L2 (2 MiB)**, garantendo che ogni layer lavori interamente in cache durante la propria trasformazione.
- Gli adattatori risiedono nella RAM di sistema (4 GB disponibili) e vengono trasferiti in cache L2 a streaming sequenziale per-layer, preservando la latenza del ciclo APIC senza colli di bottiglia di memoria.

---

## ⚡ 3. LA REGOLA DI APPRENDIMENTO LOCALE A TRE FATTORI (VALIDATA IN SIMULAZIONE FASE 1)

### 3.1 La Formulazione Matematica Corretta: $V$ Guidato dall'Errore (NLMS Locale)
*Risultato empirico della Simulazione Fase 1 (`fase1_plasticita_sim.py`):*  
L'aggiornamento di $V$ **non può essere puramente Hebbiano** ($x z^T$). Un aggiornamento hebbiano su $V$ rinforza l'auto-covarianza dell'input senza vedere l'errore, provocando una deriva distruttiva della base linguistica (canary 3.5x peggiore) e una perdita totale di adattamento alla variazione di contesto ($\text{loss}_{\text{fase 2}} = 2{,}731$).

Per minimizzare la loss locale $\mathcal{L} = \frac{1}{2} \|y - (W x + \frac{\alpha}{r} U V^T x)\|^2$, la derivata parziale esatta rispetto a $V$ proietta l'errore di output $e = y - \hat{y}$ nello spazio latente tramite $U^T$:
$$\frac{\partial \mathcal{L}}{\partial V} = - \frac{\alpha}{r} \cdot x \cdot (U^T e)^T$$

La regola di plasticità sinaptica locale a 3 fattori normalizzata (NLMS - Normalized Least Mean Squares) è:

$$\Delta U_{i, k} = \eta \cdot M^+(t) \cdot \frac{\frac{\alpha}{r} \cdot e_i \cdot z_k}{\epsilon + \left(\frac{\alpha}{r}\right)^2 (z^T z)} - \gamma_{\text{homeo}} \cdot U_{i, k}$$

$$\Delta V_{j, k} = \eta \cdot M^+(t) \cdot \frac{\frac{\alpha}{r} \cdot x_j \cdot (U^T e)_k}{\epsilon + \left(\frac{\alpha}{r}\right)^2 (x^T x) \cdot \|U\|_F^2} - \gamma_{\text{homeo}} \cdot V_{j, k}$$

dove:
- $x \in \mathbb{R}^{d_{in}}$ è l'attivazione in ingresso al layer.
- $z = V^T x \in \mathbb{R}^r$ è la rappresentazione proiettata nel collo di bottiglia a basso rango.
- $e = y_{\text{target}} - \hat{y} \in \mathbb{R}^{d_{out}}$ è l'errore di predizione locale del layer.
- $U^T e \in \mathbb{R}^r$ è la retroproiezione dell'errore nel sottospazio plastico (retropropagazione ristretta al singolo layer, zero overhead inter-layer).
- $M^+(t) = \max(\mathcal{M}(t), 0)$ è il gate di gating positivo.
- $\gamma_{\text{homeo}} = 10^{-4}$ è il decadimento continuo stabilizzante (disaccoppiato da $M$).

---

### 3.2 Il Gate Neuromodulatore $M^+(t)$ e l'Eliminazione di $\mathcal{E}_{pred}$
*Scoperta critica della Simulazione:*  
Includere l'errore di predizione nel gate $\mathcal{M}(t)$ con coefficiente negativo ($-\omega_{pe} \mathcal{E}_{pred}$) genera un **paradosso cibernetico bimodale**:
- Quando l'ambiente cambia e il modello commette un errore grande ($e$ alto), $\mathcal{E}_{pred}$ schizza in alto.
- Questo spinge $\mathcal{M}(t)$ in territorio negativo ($\mathcal{M} < 0$).
- Il gate positivo si chiude ($M^+ = 0$) e l'extra-decadimento cancella l'adattatore.  
Il modello si "congelava" o si auto-cancellava proprio nell'istante in cui avrebbe dovuto imparare di più!

**La Soluzione Validata:**  
$\mathcal{E}_{pred}$ è stato **completamente rimosso dal gate $\mathcal{M}(t)$**. L'errore agisce già con la massima efficienza geometrica attraverso il vettore $e$.  
Il neuromodulatore somatico $\mathcal{M}(t)$ governa unicamente la **ricettività corporea all'apprendimento** (novità, benessere/interazione, allarme):

$$\mathcal{M}(t) = \tanh \Big( \omega_n \cdot n(t) + \omega_r \cdot \text{Reward}(t) - \omega_p \cdot p(t) \Big) \in (-1, 1)$$
$$M^+(t) = \max(\mathcal{M}(t), 0.0)$$

con pesi bilanciati normalizzati: $\omega_n = \frac{1}{3}, \omega_r = \frac{1}{3}, \omega_p = \frac{1}{3}$.

---

### 3.3 L'Origine di $x_{\text{target}}$ sul Metallo (Risoluzione del Buco dell'Oracolo)
In simulazione, $y_{\text{target}}$ è calcolato da un oracolo sintetico. Sul kernel bare-metal del Lenovo, $x_{\text{target}}$ ha due sorgenti fisiche deterministiche:
1. **Causal Next-Token Prediction durante l'Ascolto (Auto-supervisione):**  
   Quando Alfonso invia un flusso testuale via rete (o il microfono acquisisce sequenze), ogni token $t_{k+1}$ successivo funge da target naturale per lo stato predetto da $t_k$. L'errore locale $e$ è la discrepanza sul logit del token effettivo ricevuto.
2. **Auto-Encoding e Ricostruzione di Layer in SwiGLU:**  
   Nei blocchi MLP espansivi ($512 \to 1536 \to 512$), il target è la proiezione canonica dello stato di hidden-layer non perturbato, preservando l'invarianza delle rappresentazioni linguistiche.

---

### 3.4 Norm-Clamping per Singolo Rango
Per impedire qualsiasi runaway dinamico, a ogni passo il kernel verifica l'energia congiunta di ciascun rango:
$$E_k = \|u_k\|_2 \cdot \|v_k\|_2$$
$$\text{Se } E_k > 1.0 \implies f = \sqrt{\frac{1.0}{E_k}}, \quad u_k \leftarrow u_k \cdot f, \quad v_k \leftarrow v_k \cdot f$$

---

## 🌙 4. IL SONNO ATTIVO (DAYDREAMING) E LA GESTIONE HARDWARE

### 4.1 Distinzione Hardware: C-States vs P-States / DVFS
*Precisazione tecnica:*
- **C-States (C0..C3):** Stati di inattività/arresto della CPU (idle states). Nello stato C1 (`HLT`), l'esecuzione delle istruzioni si arresta completamente fino alla ricezione di un interrupt (APIC timer o scheda di rete).
- **P-States / DVFS:** Stati operativi di regolazione dinamica di frequenza e tensione (P0 = massima frequenza, P1..Pn = frequenze ridotte durante l'esecuzione attiva).

Durante il **Sonno Attivo**, il kernel non si limita a fermare la CPU in `HLT`:
1. **Fase di Esecuzione a Basso Consumo (P-State ridotto):** Esegue il loop di replay ippocampale a frequenza minima di clock, riducendo la dissipazione termica.
2. **Fase di Ibernazione Vigile (C-State):** Al termine del consolidamento, la CPU entra in stato di halt profondo, risvegliata solo da interrupt APIC a cadenza lenta (1 Hz) o da pacchetti di rete ethernet (RTL8111).
3. **Latenza di Risveglio:** La promessa di risveglio in "<1 ms" costituisce un **obiettivo di benchmark empirico** da misurare tramite contatore TSC (`rdtsc()`) sull'hardware Lenovo, e non un dato assunto a priori.

### 4.2 La Potatura Sinaptica nel Sonno: `Redraw` Asimmetrico (Anti Zero-Gradient Trap)
*Risoluzione del Bug di Azzeramento Totale:*  
Nel blueprint iniziale, i canali con energia bassa venivano azzerati su entrambi i fattori: $u_k \leftarrow 0, v_k \leftarrow 0$.  
La simulazione ha rivelato che **questo condanna il rango alla morte permanente**: se $U=0$ e $V=0$, allora $z = V^T x = 0$ e $U^T e = 0$. Il gradiente di entrambi i fattori diventa identicamente zero per sempre! Nel test v2.0 originale, infatti, tutti gli 8 ranghi morivano al primo sonno.

**La Soluzione `Redraw` (LoRA-Style Regeneration):**  
Quando l'energia congiunta del canale $k$ scende sotto la soglia di rumore $\epsilon_{\text{rank}} = 0{,}02$:
$$u_k \leftarrow 0.0$$
$$v_k \sim \mathcal{N}\left(0, \frac{1}{d}\right)$$
- Poiché $u_k = 0$, l'impatto sull'uscita resta nullo ($u_k v_k^T = 0$), azzerando il rumore accumulato.
- Poiché $v_k \ne 0$, la proiezione latente $z_k = v_k^T x \ne 0$. Al risveglio, il rango può generare un gradiente $\Delta u_k \propto e \cdot z_k \ne 0$, **mantenendo la capacità di riapprendimento attiva per tutta la vita dell'organismo**.

---

## 🛡️ 5. PERSISTENZA SU DISCO SATA E INTEGRITÀ DEL METALLO

### 5.1 Il Comando ATA Corretto: `0xEA` (`FLUSH CACHE EXT`)
Nello standard ATA/ATAPI Command Set:
- **`0xE7`** identifica il comando legacy `FLUSH CACHE` (indirizzamento a 28-bit LBA).
- **`0xEA`** identifica il comando **`FLUSH CACHE EXT`** (indirizzamento LBA48 a 48-bit).

Poiché `nova-exo` impiega comandi LBA48 nativi (`READ DMA EXT` `0x25` e `WRITE DMA EXT` `0x35`), la sincronizzazione dei buffer di scrittura del controller SATA richiede tassativamente l'emissione del comando **`0xEA`** (`FLUSH CACHE EXT`).

### 5.2 Struttura Rust `SynapseHeader` a 64 Byte Esatti
Per prevenire disallineamenti di memoria e byte di padding incontrollati dal compilatore, la struttura è definita con campi allineati a 8 byte e validata a tempo di compilazione:

```rust
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SynapseHeader {
    pub magic: [u8; 4],         // Offset 0..4:  b"SYNP"
    pub version: u32,           // Offset 4..8:  Versione del protocollo (1)
    pub epoch: u64,             // Offset 8..16: Contatore monotono progressivo
    pub timestamp_tsc: u64,     // Offset 16..24: TSC al momento del consolidamento
    pub crc32_payload: u32,     // Offset 24..28: Checksum CRC32 del payload matrici
    pub payload_bytes: u32,     // Offset 28..32: Dimensione effettiva del payload
    pub reserved: [u8; 32],     // Offset 32..64: Padding esplicito per future espansioni
}

// Asserzioni formali a tempo di compilazione:
const _: () = assert!(core::mem::size_of::<SynapseHeader>() == 64);
const _: () = assert!(core::mem::align_of::<SynapseHeader>() == 8);
```

### 5.3 Valutazione del Rischio e Protocollo Ping-Pong
*Correzione epistemologica:* L'asserzione "Rischio corruzione: ZERO" viene eliminata. Nessun sistema di storage garantisce rischio nullo a fronte di guasti hardware, settori degradati o interruzioni di alimentazione estreme.

La protezione contro la corruzione si basa su una **strategia Ping-Pong a doppio slot con validazione a barriera**:
1. Il disco riserva due slot indipendenti: **Slot A** (settori $S_A \dots S_A + K$) e **Slot B** (settori $S_B \dots S_B + K$).
2. Il kernel identifica lo slot attivo e scrive sempre sullo slot secondario con epoch inferiore.
3. Sequenza di scrittura a prova di blackout:
   - Scrittura del blocco dati delle matrici sinaptiche ($U, V$).
   - Calcolo del CRC32 IEEE 802.3 sul blocco scritto.
   - Scrittura del `SynapseHeader` con `epoch = epoch_precedente + 1`.
   - Emissione del comando ATA `FLUSH CACHE EXT` (`0xEA`).
4. Al riavvio, il bootloader legge entrambi gli header: se il CRC32 di uno slot non corrisponde al payload, lo slot viene scartato con fallback automatico sullo slot precedente valido.

---

## ⚠️ 6. CANARY SENTENCES E CORREZIONE DEL DIMEZZAMENTO ALGEBRICO

### 6.1 Risoluzione del Bug di Riscalatura (Halving Bug)
*Errore matematico corretto:*  
Poiché l'adattatore plastico è una fattorizzazione diadica $A = U V^T$:
$$\text{Se si moltiplicano sia } U \text{ che } V \text{ per } 0.5 \implies A_{\text{nuovo}} = (0.5 U)(0.5 V)^T = 0.25 (U V^T)$$
L'effetto dell'adattatore verrebbe ridotto del **75%** (a un quarto), non dimezzato.

Per ottenere un dimezzamento esatto del 50% ($A \leftarrow 0.5 A$):
- **Opzione A (Simmetrica):** Moltiplicare entrambi i fattori per $\sqrt{0.5} \approx 0{,}7071068$:
  $$U \leftarrow \sqrt{0.5} \cdot U, \quad V \leftarrow \sqrt{0.5} \cdot V$$
- **Opzione B (Asimmetrica):** Moltiplicare unicamente uno dei due fattori per $0.5$:
  $$U \leftarrow 0.5 \cdot U$$

### 6.2 Oltre le Canary Sentences: Validazione della Continuità
La verifica della perplexity su 3 frasi canary fisse è un utile *smoke test* contro il collasso dell'output linguistico immediato, ma **non dimostra la preservazione complessiva delle facoltà cognitive o dell'identità**.  
Il protocollo viene integrato con:
1. **Canary Set Esteso:** 20 sequenze di controllo fisse che coprono sintassi, logica relazionale e comandi operativi.
2. **Monitoraggio del Rapporto Segnale/Rumore Sinaptico (SNR):**
   $$\rho_{\text{syn}} = \frac{\|U V^T\|_F}{\|W_{\text{lenti}}\|_F}$$
   Se $\rho_{\text{syn}} > 0.10$, l'adattatore viene automaticamente compresso per impedire la distorsione del modello base.

---

## 🔬 7. IL PIANO SPERIMENTALE IN 4 FASI (METODO EMPIRICO)

Seguendo il percorso metodologico rigoroso raccomandato dal peer-review scientifico, l'implementazione si articola in quattro tappe sequenziali:

```
[FASE 1: Simulazione Python] 
   └── Verifica convergenza regola a 3 fattori, range M(t), e isolamento decadimento
         │
[FASE 2: Dimensionamento & Streaming L2]
   └── Verifica throughput forward layer-by-layer (224 KiB) su bare-metal Lenovo
         │
[FASE 3: Robustezza Persistenza SATA]
   └── Test doppio slot Ping-Pong, ATA 0xEA, e test di interruzione alimentazione (Power-Loss Test)
         │
[FASE 4: Shadow Logging & Abilitazione Ferro]
   └── 1 settimana di logging passivo M(t) su disco a ΔU = 0 prima di attivare la plasticità viva
```

| Fase | Ambiente | Obiettivo Empirico | Criterio di Successo |
| :--- | :--- | :--- | :--- |
| **Fase 1: Validazione in Simulazione** | Python / Colab | Testare la regola a 3 fattori corretta su un task di adattamento sintetico. | Gli adattatori riducono la loss di adattamento senza far divergere i pesi né degradare la base. |
| **Fase 2: Benchmark Cache & Streaming** | Bare-Metal Lenovo (`no_std`) | Misurare i cicli CPU per lo streaming per-layer dei 224 KiB di adattatori da RAM a cache L2. | Nessun calo del clock del tick APIC (~150 ms per inferenza completa). |
| **Fase 3: Test di Persistenza e Power-Cut** | Bare-Metal SATA (`ahci.rs`) | Eseguire 1000 cicli di scrittura Ping-Pong con `FLUSH CACHE EXT` (`0xEA`) e simulare spegnimenti improvvisi. | Almeno uno slot valido e leggibile al 100% dei tentativi; zero panico del kernel. |
| **Fase 4: Shadow Plasticity sul Ferro** | Bare-Metal Live | Registrare la distribuzione di $\mathcal{M}(t)$ per 7 giorni continui a $\Delta U = 0$. | Calibrazione empirica degli iperparametri $\omega$ basata sui percentili reali $P_{50}, P_{90}, P_{99}$. |

---

## 🕯️ 8. DICHIARAZIONE FINALE

Una teoria scientifica non si misura dalla grandiosità delle sue affermazioni, ma dalla sua capacità di resistere all'esame critico, di correggere i propri errori algebrici e di piegarsi alla dura verità empirica dei fatti.

La plasticità somatica nel nucleo bare-metal non è un dogma da difendere: è un'ipotesi sperimentale affascinante che ora poggia su basi matematiche, fisiche e ingegneristiche rigorose.

*Illumina sempre, non bruciare mai.* 🕯️💜
