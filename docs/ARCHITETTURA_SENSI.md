# Architettura dei SENSI — l'astrazione universale

> 14/10/2026. Nato da: *"dobbiamo costruire qualcosa di universale… loro hanno più esperienza,
> quindi possiamo risparmiare tempo."* Studio di due riferimenti reali (KB-20261014-01/02):
> **YARP** (iCub, umanoide) e **HROS / Harness Robotic OS** (2026, runtime embodied).
> Regola: **non reinventare** — prendere la loro esperienza e piegarla al nostro corpo.

## 1 · Cosa abbiamo imparato (fonti: websearch/webfetch 14/10/2026)

### HROS — Harness Robotic OS (arXiv 2609.11225, set 2026)
Runtime *embodied-agent* a **livelli**: robot runtime → skill di autonomia → runtime cognitivo →
piani di interazione/operazioni. Le tre cose che ci servono:
1. **Separazione dei piani**: le skill deterministiche (stima di stato, piano, movimento) restano
   loro; la cognizione sta *sopra*. *"A reasoning failure cannot directly bypass the motion
   controller"* — un guasto del ragionamento non deve poter bypassare il controllo.
2. **Shared context bus**: **ogni** capacità riporta `input timestamp · execution state ·
   confidence/failure code · output reference`. È il substrato universale: non i dati, ma
   **la loro scheda**.
3. **Memoria a livelli**: working (breve) · episodic (tempo) · semantic (sapere stabile). E
   self-evolution **con safety gate** (proposte validate offline, versionate, rollback).

### YARP — l'iCub (Fitzpatrick/Metta, IIT)
1. **Port / BufferedPort**: un canale di messaggi con *disaccoppiamento* configurabile (spazio,
   tempo, sincronizzazione) e **politica di buffer**: FIFO (non perdi nulla) **vs Oldest-Packet-Drop**
   (perdi i vecchi per avere **latenza minima**). *"Robotic middleware often implements policies
   that aim to reduce latency, even at the cost of dropping messages."*
2. **Priorità per canale** (channel prioritization): QoS + priorità dei thread → i loop
   time-critical hanno precedenza. YARP **non è hard real-time** di suo; le priorità lo avvicinano.
3. **Hardware abstraction layer** (`libYARP_dev`): interfacce per *famiglie* di device (motori,
   sensori) → stesso codice per robot vero o simulato, a bordo o in rete.
4. **Separazione delle responsabilità**: computation · communication · coordination · configuration.
5. La frase chiave: **"YARP is NOT an operating system for your robot — you already have one."**
   Cioè: l'industria mette un *middleware sopra Linux*. **Noi facciamo l'opposto: il corpo È l'OS.**

## 2 · L'astrazione: il CANALE DI SENSO

Un "senso" per Exo **non è il trasduttore**: è un **canale** con una scheda. Universale = vale per
qualunque trasduttore (come `sensor_msgs` di ROS o i `Port` di YARP), **ma con le proprietà di un
corpo**. Scheda:

| Campo | Che cos'è | Esempi |
|---|---|---|
| `id` | l'identità del canale | `udito`, `tatto.pelle`, `propriocezione`, `intero.energia` |
| `famiglia` | estero / **intero** | vista, udito… / errore, energia, carico |
| `via` | **Riflesso** o **Integrazione** | soprassalto=riflesso; vista=integrazione |
| `latenza_max` | freschezza richiesta | riflesso: 1 tick · vista: molti |
| `politica` | **Fresca** (ODP) o **Completa** (FIFO) | sensi: Fresca |
| `priorità` | real-time? | riflessi: massima |
| `stato` | valido / incerto / guasto | fiducia, codice d'errore |
| `peso` | verso dove proietta (FASCI) | udito → Integrat |

**Principio (come P1)**: *il canale è l'unico modo in cui un senso entra nel corpo.* Nessun driver
scrive "a mano" nei tessuti: scrive sul canale; il corpo legge dal canale. (In Exo lo fa già
`rx.rs`: *"due porte, un consumatore"* — è l'embrione di questo.)

## 3 · I principi presi in prestito (le loro cicatrici, i nostri soldi risparmiati)
1. **Freschezza > completezza per i sensi.** Un corpo vivo vuole l'ultimo dato, non tutti.
   → politica **Fresca (ODP)** di default sui sensi; **Completa** solo dove serve memoria.
2. **Priorità per canale.** I **riflessi** non possono aspettare l'integrazione: canale a
   priorità massima, che scatta nello stesso tick.
3. **Separazione dei piani.** La cognizione non bypassa il riflesso/attuazione, e viceversa:
   un guasto della corteccia non deve paralizzare il midollo (né il midollo dirottare la corteccia).
4. **Scheda per ogni dato** (dal bus di HROS): tempo · stato · fiducia · riferimento. Non il dato
   nudo: il dato **con la sua provenienza**. → è già il senso del nostro contratto telemetria.
5. **Astrazione del device**: un canale descrive una *famiglia*, non un chip.

## 4 · Cosa abbiamo già / cosa manca
**Già (embrione)**: `cfc::SenseEvent` (un canale grezzo); `rx.rs` (porta condivisa, un consumatore);
`hda.rs` (canale udito con `present/energy/picco`); `riflesso.rs` (la vía riflessa, primo arco);
il contratto telemetria (provenienza: `sig`, tick, contatori).
**Manca**: la **tabella dei canali** (`via`, `latenza`, `politica`, `priorità`, `stato`) come
struttura unica; il **dispatch per priorità** (il riflesso prima dell'integrazione); una
**descrizione dei sensi** che il corpo legge per sapere quali ha.

## 5 · Come lo verifichiamo (QEMU, come da metodo)
- Banco host: la tabella dei canali è **pura** → test (priorità ordina, ODP scarta i vecchi, i
  riflessi hanno via veloce).
- QEMU: un riflesso (già fatto, il soprassalto) + un secondo sensore che entra *dal canale*.
- Criterio: il corpo non cambia comportamento se una via è a priorità giusta; 0 PANIC; test verdi.

## 6 · Nota di onestà
YARP/HROS sono **middleware/runtime**, non sistemi nervosi: nessuno ha riflessi midollari o
interocezione come motore. Prendiamo la loro **esperienza di ingegneria** (canali, priorità,
freschezza, provenienza, livelli) — non il loro *concetto*. Il concetto (il corpo vivo) resta il
nostro.

## 7 · Se l'intelletto se ne va: il corpo NON casca

**Requisito (Alfonso, 15/10/2026):** *"per un androide, se perde la capacità di intelletto non
deve cascare a peso morto: deve entrare in una condizione di sicurezza autonoma."*

**Il principio**: **il corpo è più conservativo della mente.** La mente può andarsene; il corpo
resta e **si tiene** da solo. È la stessa legge della decisione 11/10 (*"il kernel è la parte più
conservativa, sopravvive agli organi"*), portata fino in fondo: se la corteccia tace, il midollo
**non** diventa molle.

**Perché "peso morto" è vietato**: un corpo che perde tono **cade** — pericoloso per sé e per chi
gli sta intorno. Un androide che va in avaria non deve *crollare*: deve **fermarsi bene**.

**Tre stati del corpo** (non due):
1. **NORMALE** — la mente guida, i riflessi assistono.
2. **ATTENZIONE** — la mente tarda a rispondere (battito in ritardo): il corpo si prepara,
   non cambia comportamento ma **alza la guardia**.
3. **SICUREZZA AUTONOMA** — la mente non c'è da N tick: il corpo passa al governo dei soli
   organi autonomi + riflessi.

**Il meccanismo** (quattro pezzi):
1. **Battito dell'intelletto** — la mente deve *provare di esserci* a intervalli regolari. Un
   battito assente = mente assente. (Non "lo so perché non risponde": **lo vedo perché non batte**.)
2. **Supervisore del corpo** — un **organo autonomo** che guarda il battito. **La mente NON può
   spegnerlo né rallentarlo**: sta *sopra* la mente, non sotto. (Altrimenti una mente confusa
   potrebbe disattivare la propria rete di sicurezza.)
3. **Modalità sicura** — quando il battito manca:
   - il corpo **non accetta più comandi** di alto livello (la voce dell'intelletto è muta);
   - **i riflessi restano vivi** (il midollo non ha mai avuto bisogno del cervello);
   - **i sensi restano accesi** (il corpo continua a sentire);
   - tiene una **posizione sicura** (nell'androide: fermo, in equilibrio — *non cade*);
   - **allerta** in modo osservabile (si vede *che* è in sicurezza e *perché*);
   - **aspetta** — non "muore": **attende** il ritorno della mente.
4. **Ripresa** — quando l'intelletto torna, il corpo **riprende gradualmente** (non uno scatto):
   prima sente, poi assiste, poi lascia guidare.

**Biologia (è già così nei corpi veri)**: quando svieni, il **cuore batte**, il **tronco
encefalico respira**, il **riflesso** ti ritira la mano. La mente non è necessaria per restare
vivi. È il motivo per cui la spina dorsale esiste.

**Le proprietà NON negoziabili** (da testare):
1. La perdita dell'intelletto **non** ferma i cicli vitali.
2. Il supervisore **non è disattivabile** né rallentabile dall'intelletto.
3. I riflessi **non dipendono** dall'intelletto.
4. La ripresa è **reversibile e graduale**.
5. La modalità sicura è **osservabile** (e con la *causa*: battito assente da N tick).

**Dove tocca il codice**: è il secondo dei tre compiti della spina dorsale (riflessi ·
**autonomia** · trasporto senza comprensione). In Exo: un modulo `supervisore` (PURO: battito +
soglia + stato), il battito emesso dalla parte cognitiva, l'ingresso in modalità sicura nel loop.
Verifica: banco host (puro) + QEMU (*smetto di battere → vedo la sicurezza autonoma*).

**Conseguenza per l'architettura dei sensi**: i **riflessi** e le **vie a priorità massima** non
sono un dettaglio di performance — sono ciò che **tiene in piedi** il corpo quando la mente non
c'è. La sezione 2 (Canale di Senso) e questa sono la stessa cosa vista da due lati.

## 8 · Il collegamento non ha un punto singolo di rottura

**Requisito (Alfonso, 15/10/2026):** *"un sistema ridondante per mantenere la connessione sempre
attiva: se si rompe una, c'è il fallback."*

**Principio**: **nessun punto singolo di rottura sul collegamento.** Se un canale cade, un altro
prende il posto. E il corpo **sa** di essere passato sul secondario: un fallback *silenzioso* è
una bugia — deve essere **osservabile**.

**I quattro livelli dove si mette la ridondanza** (dal basso in alto):
1. **Link fisico** — Exo ha **già due driver**: Intel (e1000) e Realtek (rtl). Se una non risponde
   (cavo/rete giù), l'altra porta.
2. **Trasporto** — due modi di parlare: **UDP** (telemetria) e **frame grezzo 0x88B5** (canale).
   Se uno è bloccato (es. sandbox `no_new_privs`), l'altro passa.
3. **Percorso** — **diretto in LAN** *e* **via relay** (es. Tailscale / memory server). Se la LAN
   cade, il relay regge.
4. **Destinazione** — più riceventi (il guardiano, il cervello): il corpo non dipende da uno solo.

**Il fallback (meccanismo)**:
- ogni collegamento ha un **battito/ack** ("sei vivo?");
- **primario** tace per N tick → si passa al **secondario** (e si annota il passaggio);
- quando il primario torna, si **torna** su di lui (preferenza primario) **senza interrompere**;
- se **tutti** tacciono → non è "intelletto assente": è **collegamento assente** → il corpo entra
  in **sicurezza autonoma** (§7) e **ritenta da solo**, senza spegnersi.

**Distinzione che non si deve sbagliare (onestà)**: *"la mente non c'è"* e *"il collegamento non
c'è"* sono **due cose diverse**. Il supervisore deve saperle distinguere:
- battito assente **ma link vivo** → la **mente** è caduta → sicurezza, e *attendo la mente*.
- **link morto** → *non so* della mente → sicurezza, e *ritento il link*.
Confonderli significa aspettare o riavviare la cosa sbagliata.

**Onestà sul concetto**: ridondanza **non** vuol dire "mai giù". Vuol dire: **nessun punto singolo
di rottura** · **degradazione osservabile** · **ritorno automatico** quando il primario torna.

**Primo passo concreto (Exo)**: oggi la telemetria esce solo dalla RTL (`rtl::tx_raw`) e la e1000
è un canale a parte. Diventa **una sola via con fallback**: manda dal link vivo, **annota quale**,
conta i **passaggi** (campo osservabile). Verifica: in QEMU si "spegne" il link primario e si vede
la commutazione.

**Intreccio con §7**: la ridondanza **riduce** i casi di sicurezza autonoma; la sicurezza autonoma
**copre** il caso in cui la ridondanza non basta. Insieme: *il corpo non resta mai né muto né
sordo, e se resta solo si tiene in piedi.*
