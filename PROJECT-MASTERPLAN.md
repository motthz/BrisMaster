# BrisMaster + HumanSim — MASTERPLAN TECNICO, AUDIT E PASSAGGIO DI CONSEGNE
> **Documento principale del progetto — 23 novembre 2026.**
>
> **Repository:** [motthz/BrisMaster](https://github.com/motthz/BrisMaster).
> **Versione rilevata:** workspace `0.6.16` nel ramo `main` al momento dell'audit.
> **Lavoro HumanSim:** ramo `feature/humansim-benchmark-foundation`, [PR #66](https://github.com/motthz/BrisMaster/pull/66) in bozza.
> **Status:** audit statico dei sorgenti e documenti presenti su GitHub; non equivale a test locale di ogni eseguibile, ambiente o asset.
>
> **Per chi riprende il progetto:** leggere TUTTO questo documento prima di modificare codice. Il progetto si può sviluppare con qualunque IDE, assistente o programmatore: **non dipende da Codex**. Non interpretare numeri obsoleti, ipotesi sperimentali, test sintetici o la baseline HumanSim come prove di realismo umano.

---

## 0. Executive summary / criterio di successo

Il repository ospita **BrisMaster**, un assistente Windows per otto giochi di carte, con motori specifici, lettura opzionale dello schermo per Briscola/Scopa, simulazioni e benchmark. Il progetto che si aggiunge è **HumanSim**, una popolazione di avversari artificiali **statisticamente simile a giocatori umani reali** di Briscola 1 contro 1. HumanSim NON deve diventare un'altra modalità di Massima e NON deve alterare i risultati del motore competitivo.

### Due obiettivi separati e misurati indipendentemente

**A. Realismo HumanSim.** Per ogni mano/storia/contesto, assegnare a ogni mossa legale una probabilità che descriva la frequenza e le circostanze delle scelte umane. Riprodurre la diversità tra persone, la coerenza di ogni persona nel tempo, le loro conoscenze limitate, la memoria e gli errori *strutturati*. Misurare su giocatori, sessioni e situazioni mai usati nell'allenamento.

**B. Forza di Massima.** Migliorare la capacità di battere una popolazione congelata di avversari realistici con confronti replicabili (stesse distribuzioni di carte, semi, posti invertiti, stesso tempo per decisione). Vittorie su HumanSim indicano forza contro **quel modello**, non un tasso garantito contro esseri umani.

**Definizione operativa prudente di "indistinguibile":** classificatori indipendenti e valutatori ciechi che, con adeguata potenza statistica e su partite/persona fuori campione, non distinguano la distribuzione HumanSim da quella umana meglio di una soglia di equivalenza preregistrata; inoltre log-loss, calibrazione e distribuzioni condizionali devono risultare credibili. Un singolo AUC circa 0,50 non prova l'indistinguibilità universale. Riportare sempre intervalli di confidenza, dimensioni campionarie e condizioni del test.

### Regole inderogabili

1. Nessun uso della vera mano avversaria o dell'ordine futuro del mazzo nella politica HumanSim.
2. Nessun dato sintetico etichettato come umano reale.
3. Nessuna contaminazione tra dati di addestramento e test; split per PERSONA e SESSIONE.
4. La selezione della mossa di Massima resta invariata salvo PR, test e decisione esplicita.
5. Baseline, modello, dataset, configurazione e risultati hanno versione/seed/hash.
6. Non rilasciare una nuova versione pubblica soltanto perché una PR compila: completare i gate.
7. Vietato descrivere le prestazioni su avversari a regole come validazione contro umani reali.
8. L'obiettivo è simulazione e playtesting controllato, non aggirare controlli anti-bot o regole di piattaforme online.

---

## 1. Stato effettivo del repository

### 1.1 Stack e prodotto

- Linguaggio principale: Rust 2021, workspace Cargo.
- GUI desktop: `eframe` / `egui`; cattura Windows con `xcap` e API Windows per overlay.
- Reti neurali tradizionali: codice Rust in `crates/core/src/net.rs`; dati/allenamento in Rust/Python.
- Allenamento RL sperimentale: `crates/rl-py`, Python + PyTorch e binding Python/Rust (crate escluso dal workspace root).
- Parallelismo/benchmark: `rayon`, random `SmallRng`, simulatore `briscola-sim`.
- Artefatti: reti binarie versionate, mazzi di riferimento, dati di test, installer Inno Setup.
- Distribuzione: Windows `BrisMaster.exe`, installer e aggiornamento automatico tramite GitHub Releases.
- Licenza del codice: MIT. **Licenze e provenienza delle immagini/reti/dataset vanno comunque controllate separatamente** prima di redistribuirli.

### 1.2 Giochi supportati e responsabilità

| Gioco | Engine / moduli | Valutazione |
|---|---|---|
| **Briscola** | `crates/core`, `crates/sim` | PIMC, ISMCTS, deduzione, finale esatto, simulatori e stili |
| **Scopa** | `crates/scopa`, `crates/scopa-sim` | inferenza delle mani, ricerca, benchmark |
| **Scala 40** | `crates/scala40`, `crates/scala40-sim` | combinazioni, suggerimenti e benchmark |
| **Burraco** | `crates/burraco` | carte/melds/engine/banco di prova integrato |
| **Sette e mezzo** | `crates/settemezzo` | risolutore e simulatore |
| **Blackjack** | `crates/blackjack` | regole, valore atteso, simulatore |
| **Poker all'italiana** | `crates/poker-it`, `crates/poker-it-sim` | CFR, stima avversario, test |
| **Texas Hold'em** | `crates/poker`, `crates/poker-sim` | CFR, astrazione, equity, test |

La GUI di tutti i giochi vive in `crates/app`. HumanSim v1 riguarda **esclusivamente Briscola a due giocatori**. Non applicare i suoi criteri ad altri giochi senza definire specifici dataset/regole/split.

### 1.3 Mappa dei file fondamentali

**Core Briscola**
- `crates/core/src/cards.rs` — ID delle 40 carte, semi, punti, forza e comparazione delle prese.
- `crates/core/src/game.rs` — `Game` (stato completo per simulazione), `Observation` (vista lecita), pesca, turno, trick, finalizzazione, ricostruzione mondi.
- `crates/core/src/ai.rs` — giocatori, euristiche, PIMC, ISMCTS, `RootChild`, rollout, solver alfa-beta, `EngineConfig`.
- `crates/core/src/infer.rs` — `Log`, `Belief`, deduzione condizionata alla storia delle mosse avversarie.
- `crates/core/src/net.rs` — reti politica/valore, rappresentazione `Position`, canonizzazione semi.
- `crates/core/src/rl.rs` — codifica RL della storia e interfaccia verso la rete auto-allenata.
- `crates/core/src/style.rs` — 12 parametri `Knobs`, 20 stili `STYLES`, `Persona`, `Adapt`.
- **Branch HumanSim** `crates/core/src/humansim.rs` — prima policy umanizzata *non calibrata*, `HumanSimPlayer`, distribuzioni e test.
- `crates/core/src/stats.rs` — statistica.

**Simulatore**
- `crates/sim/src/main.rs` — CLI: confronti, panel, gate, calibrate, report, altri banchi.
- `crates/sim/src/arena.rs` — mazzi in duplicato, giocatori a posti invertiti, intervalli per mazzo.
- `crates/sim/src/agents.rs` — factory, agenti, spec dei motori e riferimento a release passate.
- `crates/sim/src/persona.rs` — wrapper di tempo/tilt/cambio stile.
- `crates/sim/src/rl_agent.rs` — integrazione RL con simulatore.
- `crates/sim/src/bin/reali.rs` — ingest dei registri esistenti e ricostruzione delle mani avversarie non osservate.
- `crates/sim/src/bin/train.rs`, `selfplay.rs`, `bench.rs` — allenamento, dati sintetici e prestazioni.
- `bench/RISULTATI.md`, `bench/RAPPORTO.md`, `bench/0.6.0/README.md` — numeri storici e configurazioni.

**App e riconoscimento**
- `crates/app/src/assistant.rs` — stato della partita, consigli, storico, correzioni e integrazione della vista.
- `crates/app/src/assistant/session_replay.rs` — replay deterministico di sequenze annotate.
- `crates/app/src/record.rs` — registrazione volontaria delle partite.
- `crates/app/src/vision/mod.rs` — pipeline visiva e `CardView`/`Scene`.
- `crates/app/src/vision/detect.rs` — ricerca e normalizzazione di quadrilateri/carte.
- `crates/app/src/vision/reference.rs`, `nn.rs`, `memory.rs`, `track.rs` — riferimenti grafici, rete, memoria e tracking.
- `crates/app/src/vision/capture.rs`, `screen.rs`, `live.rs` — cattura, monitor, lavoro live.
- `crates/app/src/vision/deck_import.rs` — importazione di nuovi mazzi.
- `crates/app/src/psyfly.rs` — overlay Psy Fly.
- `crates/app/src/update.rs` — updater; `installer/BrisMaster.iss` — installer.

**Documenti esistenti da NON perdere**
- `README.md`, `docs/HUMANSIM.md` (sul branch), `docs/PARTITE-VERE.md`
- `docs/RICONOSCIMENTO-CARTE.md`, `docs/replay/README.md`
- `docs/RELEASE-0.6.15.md`, `docs/RELEASE-0.6.16.md`
- `bench/BANCO.md`, `bench/RAPPORTO.md`, `bench/0.6.0/README.md`
- `.github/workflows/ci.yml`, `.github/workflows/release.yml`

### 1.4 Struttura logica (data flow)

```text
Modalità A: motore competitivo Massima
  stato pubblico della partita + propria mano
    -> Observation + storico
    -> rete di politica per belief sulle mani avversarie
    -> campionamento di stati compatibili
    -> ISMCTS potenziato (rollout e finale esatto)
    -> scelta strategica e stime
    -> suggerimento GUI

Modalità B: HumanSim (NUOVO, indipendente da Massima)
  stato pubblico + mano della persona simulata
    -> Observation + storia + identità persistente
    -> credenze/memoria *imperfette ma coerenti*
    -> politica umana condizionata e calibrata su registri reali
    -> distribuzione di probabilità sulle SOLE mosse legali
    -> campionamento riproducibile con RNG dedicato
    -> carta scelta; traccia dei motivi (solo diagnostica)

Modalità C: laboratorio di benchmark
  congelare Massima X + HumanSim versione Y + seed/mazzi
    -> arena A vs B, poi B vs A a posti invertiti
    -> registro di ogni presa, mosse e risultati
    -> aggregazione per MAZZO, persona, fase, difficoltà
    -> IC e confronto con versioni precedenti
    -> report immutabile con commit/hash/configurazione
```

### 1.5 Regole esatte Briscola (codifica attuale)

- Partita 1 contro 1; mazzo 40 carte con quattro semi, dieci valori.
- ID: `card = seme * 10 + rank_index`; semi `Denari=0`, `Coppe=1`, `Spade=2`, `Bastoni=3`.
- `rank_index`: Asso 0, Due 1, Tre 2, Quattro 3, Cinque 4, Sei 5, Sette 6, Fante 7, Cavallo 8, Re 9.
- Punti: Asso 11; Tre 10; Re 4; Cavallo 3; Fante 2; altri 0. Totale mazzo 120.
- Forza: Asso > Tre > Re > Cavallo > Fante > 7 > 6 > 5 > 4 > 2.
- Chi risponde prende con carta dello stesso seme più forte, oppure briscola contro carta di altro seme; assenza di obbligo di rispondere al seme.
- Tre carte per giocatore, venti prese, chi vince la presa pesca per primo (quando il mazzo non è vuoto).
- `Game` può conoscere tutto; `Observation` espone soltanto informazioni consentite. Questa separazione è il confine anti-cheating.

---

## 2. Audit del motore Massima (situazione verificata dai sorgenti e dai report)

### 2.1 Baseline competitiva

La documentazione del repository descrive, per la v0.5.0 e successive fino alla 0.6.16, una modalità Massima basata su **ISMCTS potenziato, 20.000 iterazioni, deduzione delle mani** con una rete di politica allenata originariamente su giochi fra motori.

L'implementazione dei motori storici è utilizzabile come riferimento per nuove prove tramite factory `make_agent`, ad esempio `infer:20000:1:1000:0.25`, `ismcts+:20000`, `engine:1000`, `stile:circolo`. Per esperimenti che richiedono rete su file, impostare `BRISCOLA_POLICY` sul file previsto e verificare le opzioni nel sorgente `crates/sim/src/agents.rs`.

**Risultati pubblicati, non ricalcolati in questo audit:**
- `bench/RAPPORTO.md`, esperimenti 27/10/2026: con deduzione 52,4% contro Massima senza deduzione, su 8.000 partite (4.000 mazzi duplicati), CI riportato 51,5–53,4%; 54,5% contro PIMC 3.000.
- `bench/0.6.0/README.md`: fino a circa 88 milioni di partite RL sperimentali riportate; la rete auto-allenata non ha mostrato un miglioramento convincente sulla Massima; aumentare indiscriminatamente le iterazioni non è risultato efficace.
- `bench/RISULTATI.md`: il PIMC e l'ISMCTS hanno storici di prove ampie con controlli sui mazzi accoppiati.

**Conclusione tecnica:** aumentare solo la forza di calcolo o re-addestrare una rete generica non garantisce progressi. Un benchmark HumanSim empiricamente fondato individua errori e opportunità che non emergono dagli avversari euristici.

### 2.2 Limiti/attenzioni del motore

1. **Determinizzazione e strategia fusion:** più mondi completi non equivalgono automaticamente a decisioni ottimali in informazione imperfetta. Mantenere benchmark PIMC vs ISMCTS vs avversari non identici.
2. **Belief modellato su motori:** una rete addestrata su bot può attribuire probabilità non realistiche a mosse umane. Evitare che un modello errato azzeri mondi plausibili; mantenere un prior robusto e calibrato.
3. **Calcolo del finale:** a mazzo terminato l'informazione può essere completa per chi ricorda tutte le carte, ma HumanSim esperto e umano medio non sono obbligati a ricordare tutto. Non trasferire in HumanSim il solver senza modellare la memoria.
4. **Evitare valutazione autoreferenziale:** battere HumanSim addestrato su mosse generate da Massima è un test circolare, NON una misura di capacità contro umani reali.
5. **Tempo equo:** per paragonare vecchia e nuova Massima indicare budget di iterazioni, tempo per decisione, hardware/thread e distribuzione della latenza.
6. **Significatività:** risultati vicini al 50% richiedono molti mazzi, con intervalli calcolati su coppie correlate, non su singole partite come se fossero indipendenti.

### 2.3 Formati attuali

- Carte come ID `0..39`.
- `Observation`: `me`, `hand`, `opp_hand_len`, `deck_len`, `trump_card`, `trump`, `leader`, `table`, `points`, `played`, `trump_card_owner`.
- Le reti esistenti rinumerano i semi rispetto al seme di briscola per migliorare la generalizzazione. Ogni nuova codifica deve preservare l'inversa per risalire alla carta originale.
- `Position::of_obs` è la codifica limitata alle osservazioni; **non usare `Position::of(game)` per allenare/valutare la politica che vuole sembrare umana**, perché il modello vedrebbe stato completo.

---

## 3. Audit HumanSim M0: cosa esiste davvero

### 3.1 Implementazione sul branch di PR #66

File `crates/core/src/humansim.rs`, collegato in `crates/core/src/lib.rs`; in `crates/sim/src/agents.rs` sono state aggiunte le factory:

```text
humansim:7
humansim:42
humansim-style:circolo
humansim-style:prudente
humansim-style:domenica
```

`HumanSimPlayer` ha:
- ID seme identità, manopole `Knobs`, insieme di carte ricordate costruito all'avvio;
- funzioni di stima del costo di conservazione, rischio dell'apertura, valore della presa;
- `distribution(&Observation) -> Vec<(Card, f64)>`;
- softmax della utilità con temperatura e lieve probabilità di lapse/errori;
- `choose` tramite RNG iniettato dall'arena;
- test per distribuzioni normalizzate, mosse legali, assenza della vera mano nascosta, 120 punti e caricamento profili.

**QUESTO È UN PROTOTIPO NON CALIBRATO.** I valori di temperatura, lapse e utilità sono convenzioni progettuali; non sono frequenze osservate su persone reali. Il file non possiede ancora memoria episodica aggiornabile, personalità longitudinali allenate, modelli supervisionati, dataset umano o un test cieco. Il ramo è separato da `main`; non indicare "release pronta".

### 3.2 Cosa esiste già in `style.rs`

I 12 parametri: `memoria`, `aggressivita`, `risparmio`, `caccia`, `apre_carichi`, `protegge`, `apre_briscola`, `tiene_assi`, `errori`, `finale`, `tilt`, `fretta`. I 20 profili predefiniti includono `circolo`, `prudente`, `nonno`, `impulsivo`, `memoria-perfetta`, `domenica`.

**Miglioramento concreto:** la `Persona::choose` esistente aggiunge rumore chiamando il generatore casuale nel comparatore di `max_by`; per un modello probabilistico scientifico è meglio calcolare una volta i punteggi delle tre carte e campionare una distribuzione stabile, senza confronti casuali che cambiano durante la stessa selezione.

### 3.3 Gap prioritari

| ID | Problema | Impatto | Soluzione |
|---|---|---|---|
| HS-01 | Mancano dati di persone con mano/alternative note | Critico | recorder strumentato o torneo locale autorizzato |
| HS-02 | Scelte reali influenzate dai consigli di BrisMaster | Critico | campo `advice_exposure`, dati "no hint" per train/test principale |
| HS-03 | Split dei file `reali.rs` basato su hash della partita, non sul giocatore | Alto | split per identità/sessione, test bloccato |
| HS-04 | Baseline attuale è softmax su parametri manuali | Alto | stima empirica e rete di imitazione |
| HS-05 | La personalità non evolve fra sessioni | Alto | parametri gerarchici, identità e memoria persistente |
| HS-06 | Errori prevalentemente random | Alto | errori condizionati a scelta/contesto/attenzione |
| HS-07 | Nessun test di realismo offline | Critico | log-loss, calibrazione, discriminatore, blind test |
| HS-08 | Nessuna popolazione congelata per release | Alto | versioning dei profili e manifest |
| HS-09 | Nessuna misura di credibilità sotto avversari forti/adattivi | Medio | stress suite per distribuzione di forza |
| HS-10 | Mancano benchmark temporali separati per HumanSim | Medio | p50/p95/p99 e partite/min |
| HS-11 | Il modello ereditato dalla ricerca può sapere troppo | Alto | test di non-interferenza informativa |
| HS-12 | Assenza di audit dataset/provenienza nel CI | Alto | validatore + provenienza immutabile |

---

## 4. Audit della registrazione: opportunità già presente e rischio da correggere

### 4.1 Cosa fa il prodotto oggi

`crates/app/src/record.rs` salva un file JSON quando l'utente attiva **Registra questa partita** (spento di default), con eventi `briscola`, `primo_di_mano`, `pescata`, `giocata`, versione app e risultato. Nell'evento `Giocata`, **solo quando gioca l'utente**, la struttura `prima: Seen` contiene mano, carta sul tavolo, punteggi, numero carte nel mazzo e quantità della mano avversaria. Può essere registrato `consiglio` e `seguito_consiglio`.

`docs/PARTITE-VERE.md` e `crates/sim/src/bin/reali.rs` descrivono già:
- import delle registrazioni tramite `--partite`;
- deduplicazione partite;
- ricostruzione per campionamento degli ordini di pescata avversari;
- peso delle ricostruzioni con la rete di politica;
- una quota di controllo basata su hash del contenuto;
- allenamento `train --kind policy` e confronto delle reti.

### 4.2 Problemi metodologici veri

1. La mano di un **avversario esterno** non è osservata direttamente e `reali.rs` ricostruisce scenari compatibili, non la sua reale osservazione a ogni turno. Per HumanSim serviranno partite dove il decisore e le sue opzioni sono noti con certezza.
2. **Contaminazione del consiglio:** le mosse dell'utente nel recorder potrebbero essere state prese seguendo Massima. Impostare una modalità di raccolta **senza consigli**, o per lo meno salvare `advice_visible`, `advice_generated`, `advice_followed` e separare i casi.
3. L'hash di singole partite non impedisce che lo stesso giocatore/seduta finisca contemporaneamente in train e test. Gli split per giocatore/sessione sono obbligatori.
4. La rete attuale può influenzare quali ricostruzioni sopravvivono durante l'EM: rischio di auto-conferma e di evidenza artificiosamente elevata. Usare pesatura robusta, analisi ESS, sensitivity analysis a prior diversi, e dati con mano realmente nota quando disponibili.
5. Il recorder salva un timestamp ma non uno **pseudonimo stabile con consenso**, una versione delle regole, durata delle decisioni o fonte/qualità dello stato. Aggiungere con migrazione di schema.
6. Registrazioni incomplete, correzioni di OCR e rewind non devono essere trattate come verità certa: campi di provenienza, verifiche e livelli di confidenza.

### 4.3 Data collection raccomandata

Priorità numero uno: una piccola interfaccia locale **Briscola Human Lab** che permetta a volontari consenzienti di giocare contro persona o bot non consigliato, conservando *il vero stato disponibile a chi muove* prima di ogni carta. L'app può conoscere l'intero stato per arbitrare, ma nel log del decisore conserva solo la sua osservazione. Il modello HumanSim non riceverà mai le informazioni nascoste. Contemporaneamente salvare:
- latenza della scelta (solo se misurata);
- identificativo pseudonimo persona e sessione;
- sorgente, versione regole, dispositivo e modalità (facoltativo; evitare dati non necessari);
- indicatore di turno assistito oppure autonomo;
- ripetizioni contro avversari diversi;
- decisione reale con tutte e sole le mosse legali.

Partire da pilotaggio di qualità (es. decine di persone e molte partite ripetute) e fare **analisi della potenza** per dimensionare il dataset conclusivo: nessun numero inventato di partite rende automaticamente il modello indistinguibile.

### 4.4 Esempio di schema JSONL V1 (proposto, NON ancora implementato)

Ogni riga rappresenta una **decisione reale**, non solo una carta osservata in video:

```json
{
  "schema_version": "human_decision_v1",
  "game_id": "g-000314",
  "player_id": "pseudonym-p17",
  "session_id": "session-2026-11-23-p17-01",
  "ruleset": "briscola-italiana-1v1-v1",
  "source": "consented-local-lab",
  "data_origin": "human",
  "turn_index": 19,
  "trick_index": 10,
  "seat": 1,
  "trump_card": 14,
  "my_hand": [0, 12, 37],
  "table_card": 26,
  "deck_remaining": 14,
  "points": [24, 32],
  "legal_cards": [0, 12, 37],
  "chosen_card": 37,
  "public_history": [{"player": 0, "card": 26}],
  "advice_visible": false,
  "advice_followed": null,
  "decision_time_ms": 3120,
  "observation_quality": "exact",
  "is_complete_game": true
}
```

**Valori soltanto illustrativi**, non un log realmente ottenuto. `points` e `seat` devono avere convenzioni fisse; `public_history` nell'implementazione reale conterrebbe tutte le giocate pubbliche precedenti, ordinate, NON soltanto quella mostrata nell'esempio. Lo schema di partita deve aggiungere `player_id` per entrambi, seed dell'arbitro, `ruleset_version`, timestamp e consenso/provenienza. Gli identificativi non devono contenere nome, email, IP o altri dati direttamente identificativi.

### 4.5 Invarianti del validatore dati

- Ogni `chosen_card` appartiene a `legal_cards` e alla mano osservata.
- `legal_cards` coincide con le carte effettivamente giocabili secondo il ruleset.
- Nessuna carta appare in due posti incompatibili della stessa posizione.
- Dopo le 40 giocate di una partita completa le carte sono tutte esaurite; totale punti = 120, 20 prese.
- Storico, numero prese, punteggi e ordine turni sono ricostruibili.
- Dopo la pesca deve esserci l'aggiornamento coerente della mano e della briscola scoperta.
- `advice_visible` non può essere ignorato; le sessioni assistite sono distinte dalle autonome.
- Ogni dato ha `source`, `schema_version`, `observation_quality`, una sessione e un giocatore.
- Dati incoerenti sono **scartati e motivati**; mai corretti silenziosamente.
- I dati di test sono bloccati tramite manifest/hash e non sono disponibili al tuning.

---

## 5. Architettura target HumanSim V1–V3

### 5.1 Moduli nuovi consigliati (non confondere con file già creati)

```text
crates/core/src/humansim/
    mod.rs               # API e gating delle versioni
    observation.rs       # viste canoniche, nessun dato segreto
    policy.rs            # distribuzione azioni
    identity.rs          # persona persistente e prior
    episodic.rs          # memoria, credenze, attenzione
    features.rs          # caratteristiche leggibili
    baseline.rs          # baseline stile/softmax
    learned.rs           # caricamento e inferenza modello umano
    calibrate.rs         # temperatura/calibrazione di inferenza
    diagnostics.rs       # ragioni delle scelte, per audit
crates/sim/src/human_bench.rs
crates/sim/src/human_audit.rs
crates/sim/src/human_data.rs
scripts/humansim/
    validate.py
    prepare_splits.py
    train_baselines.py
    train_policy.py
    evaluate.py
    compare_sequences.py
    report.py
data/humansim/README.md   # schemi e manifest; non committare registrazioni private
bench/humansim/            # baseline, manifest, valutazioni, CI
```

**ATTENZIONE:** oggi `humansim.rs` è un FILE; prima di trasformarlo in cartella, migrare il modulo senza lasciare contemporaneamente `humansim.rs` e `humansim/mod.rs` allo stesso livello. Prediligere piccoli commit revisibili.

### 5.2 API concettuale

```rust
// PSEUDOCODICE, non firma Rust già implementata
struct HumanObservation {
    hand: Vec<Card>,
    public_history: Vec<PublicEvent>,
    trump_card: Card,
    trump_suit: Suit,
    score: [u8; 2],
    deck_remaining: u8,
    table: Option<Card>,
    legal_actions: Vec<Card>
}
struct HumanIdentity {
    id: SyntheticId,
    skill: SkillLevel,
    stable_preferences: LatentStyle,
    memory_capacity: MemoryProfile
}
struct HumanEpisode {
    belief_about_opponent: BeliefState,
    remembered_cards: CardSet,
    fatigue_state: Option<FatigueState>
}
trait HumanPolicy {
    fn distribution(
        &self, observation: &HumanObservation,
        identity: &HumanIdentity,
        episode: &HumanEpisode
    ) -> Vec<(Card, Probability)>;
    fn on_public_event(&mut self, event: &PublicEvent);
}
```

**Regola architetturale:** ambiente e osservatore creano `HumanObservation`; il modulo HumanSim non deve ricevere un `&Game` completo. `Game` rimane consentito ad arbitro e banco di prova, non all'inferenza della policy. `on_public_event` deve applicare solo eventi realmente visibili. Mantieni contesto prima e dopo una presa senza anticipare la pescata.

### 5.3 Stato persistente e popolazione gerarchica

Campionare **una volta per persona sintetica**:
- abilità e qualità di ragionamento;
- probabilità di ricordare i carichi, le briscole e le carte di poco valore (differenziate);
- tendenza ad aprire carichi o giocare liscio;
- propensione al rischio condizionata a punteggio/stadio;
- tendenza a consumare o conservare le briscole;
- preferenze coerenti per scelte strategicamente simili;
- eventuale frequenza di errori da fretta solo se misurata.

La variazione **entro la persona** deve riflettere contesto e incertezza, non rigenerare i parametri casualmente a ogni mano. La variazione **tra persone** è campionata da una popolazione calibrata con pesi realistici. Evitare etichette rigide "giocatore medio" o "professionista" senza sufficiente rappresentatività dei dati.

### 5.4 Modelli, dal più semplice al più potente

**Baseline B0 — stili esistenti:** `Persona` / `HumanSimPlayer` come controllo sintetico. Non attribuire realismo empirico.

**B1 — modello probabilistico interpretabile:** feature vettoriali condizionate a carta/fase/briscola/storia, softmax multinomiale o modello gerarchico misto. Parametri diversi per persone e gruppi, con regolarizzazione. Vantaggio: campioni piccoli, spiegabilità e audit.

**B2 — imitazione sequenziale:** rete GRU/LSTM/Transformer piccola che codifica storia pubblica, mano, rischi e identità; maschera le azioni illegali. Addestramento supervisionato con cross-entropy/log-loss e validazione per persona. L'imitazione sequenziale ha precedenti nella letteratura sulla simulazione di giocatori, ma la superiorità rispetto a B1 deve essere dimostrata sul nostro dataset, non presupposta.

**B3 — credenze e limiti cognitivi:** sottosistema calibrato che approssima cosa la persona pensa delle carte ancora nascoste; attenzione e memoria fallibile; errori strutturati (sopravvaluta briscole, ignora una carta uscita, calcola male la risposta). Questo strato NON deve usare una vera determinizzazione del mondo come informazione.

**B4 — ragionamento inverso opzionale:** modelli di preferenze latenti o inverse reinforcement learning per spiegare *perché* la persona rischia punti e sceglie un'azione. Solo se B1/B2 sono insufficienti e i dati consentono identificabilità.

**B5 — population mixture:** una distribuzione gerarchica di identità con forte diversità, incluse code rare ma osservate, per testare la robustezza della Massima.

### 5.5 Procedura di scelta della carta target

1. Osservazione *lecita* e stato persona.
2. Canonizzazione delle carte/semi, maschera azioni legali.
3. Vettore features interpretative e/o embedding sequenziale.
4. Politica `P(carta | obs, storia, identità)`.
5. Controllo di integrità: probabilità finite, non negative e normalizzate.
6. Campionamento usando RNG seme noto, **senza alterare la distribuzione in modo nascosto**.
7. Aggiornamento della memoria dopo gli eventi pubblici.
8. Logging con ID modello, persona, seed, probabilità di tutte le mosse e scelta.
9. In partita sperimentale, confrontare le mosse con una *valutazione strategica separata*, non introdotta nella policy salvo esperimento esplicitamente definito.

### 5.6 Attenzione a tre fallimenti frequenti

- **Omogeneizzazione:** una rete che predice solo la mossa più frequente elimina i tratti individuali e rende i giocatori troppo simili.
- **Compounding error:** durante una partita autonoma il modello visita stati diversi da quelli delle partite reali; non basta alta accuracy su singole mosse offline. Necessari test di **partite complete**.
- **Strategy leakage:** includere nel target le stime di vittoria del motore Massima, o filtrarle sempre con soglia, può produrre un bot forte "umanizzato", non una persona realistica. La qualità strategica è metrica separata.

---

## 6. Progettazione scientifica dei test: HumanSim deve passare più di un benchmark

### 6.1 Split rigorosi

- `TRAIN`: giocatori/persona e sessioni usati per stimare modello.
- `VALIDATION`: giocatori/sedute indipendenti per iperparametri.
- `TEST-NEW-PEOPLE`: **persone mai viste** per misura della generalizzazione di popolazione.
- `TEST-LONGITUDINAL`: sessioni successive per persone presenti nei dati precedenti, ma tempo futuro rigorosamente isolato.
- `TEST-UNSEEN-CONTEXTS`: situazioni rare e mazzi/strategie non dominanti.
- `BLIND-TEST`: partite complete senza rivelare origine e senza suggerimenti sul tipo di modello.

Qualsiasi revisione del modello dopo aver letto risultati del test finale richiede nuova partizione/test o un protocollo esplicito con correzione per selezione. La parte per persone mai viste misura il realismo della popolazione; il test longitudinale misura la fedeltà a una persona specifica. Non confonderli.

### 6.2 Metriche predittive offline

| Metrica | Calcolo | Interpretazione |
|---|---|---|
| Negative log-likelihood | `-mean(log P(mossa_umano))` | più bassa = meglio |
| Accuracy top-1 | quota di mosse umane più probabili | utile, ma insufficiente |
| Brier multiclass | errore quadratico sui vettori di probabilità | calibrazione/precisione |
| Calibration / reliability | frequenza reale per bucket di probabilità | onestà probabilistica |
| Coverage per persona | persone con risultati non degradati | evita sovra-ottimizzare la media |
| Regret vs Massima | `Q(best)-Q(chosen)`, stima separata | distribuzione degli errori, non realismo da sola |
| Conditional action rates | tagli, aperture, carichi, lisci, gestione di briscole per fase/punteggio | pattern umani |
| Tempo di scelta | distribution p50/p95 se tempo vero registrato | realismo temporale, non artificio |

Confrontare ogni metrica con **baseline adeguate**: casuale vincolato, euristico, stile legacy, modello globale senza identità, B1, B2. Riportare CI per persona/sessione. Le mosse obbligate con una sola carta vanno trattate separatamente, perché gonfiano l'accuracy.

### 6.3 Metriche di sequenza e interazione

- Distribuzione della lunghezza dei pattern: conservazione briscola, aggressività per fase, risposte dopo prese perse.
- Autocorrelazione delle scelte: lo stesso umano mantiene una tendenza per più prese?
- Coerenza della memoria quando le carte uscite sono note, ignote o ricordate erroneamente.
- Distanze tra statistiche di partite complete condizionate per skill (es. Wasserstein, MMD con bootstrap).
- Prestazioni HumanSim contro una pluralità di avversari (algoritmici, umani registrati e agenti che cambiano stile).
- Discriminatore **indipendente** umano-vs-sintetico, allenato e valutato su persone/partite non viste. Riportare AUC/accuracy/CI; confrontare anche umano-vs-umano per evitare la falsa impressione che tutti i comportamenti umani siano uniformi.
- Esperimento cieco con valutatori indipendenti. Prima del test fissare differenza massima accettabile da probabilità casuale e numerosità necessaria.

### 6.4 Criteri di uscita consigliati

**Requisiti tecnici minimi (obbligatori):**
- 0 mosse illegali in suite di stress e benchmark;
- 0 accessi a campi nascosti;
- riproduzione identica data stessa configurazione/seed/commit;
- nessun effetto regressivo inatteso su Massima e altri giochi;
- report standard prodotti automaticamente e integri.

**Realismo da raggiungere (da confermare prima dei test):**
- migliore log-loss del modello `Persona` calibrato sul set bloccato;
- buona calibrazione su 1/2/3 mosse disponibili, per livello e fase;
- distribuzioni di errori e punti comparabili a persone reali;
- discriminatore non significativamente migliore di soglia di equivalenza preregistrata (con sufficiente potenza);
- test cieco su intere partite e contesti nuovi, non solo screenshot o singole prese.

**Nota epistemica:** anche superando queste prove, usare “indistinguibile **nel protocollo X, su popolazione Y**”, non “indistinguibile da ogni essere umano in qualsiasi situazione”.

---

## 7. Banco di prova permanente di Massima

### 7.1 Perché il banco attuale è buono ma insufficiente

`crates/sim/src/arena.rs` usa lo stesso mazzo in due partite a posti invertiti e aggrega per mazzo: è la scelta giusta per ridurre varianza. Invece i 20 stili sintetici del `style.rs` non costituiscono una popolazione umana dimostrata. HumanSim aggiunge un benchmark misurato, non sostituisce il banco tradizionale.

### 7.2 Matrice fissa di valutazione

Per ogni nuova candidata Massima eseguire almeno:
- test vs Massima della release precedente, a mazzi/seed identici;
- vs panel algoritmico (euristico, PIMC, ISMCTS, Counter);
- vs popolazione HumanSim congelata, bilanciata per abilità e profilo;
- vs human-real replay *offline* con condizioni di informazione ben definite (non simulare che il modello possa rigiocare persone assenti in stati controfattuali senza modellarne le risposte);
- test speciale contro stili di coda, cambi stile, gioco apparentemente irrazionale;
- test sensibilità a numero di iterazioni, latenza e modalità di deduzione.

### 7.3 Manifest del benchmark (proposto)

```json
{
  "bench_schema": "human_bench_v1",
  "project_commit": "<git-sha>",
  "massima_version": "<engine-sha-or-tag>",
  "human_population": "humansim-v1.0.0",
  "human_model_sha256": "<hash>",
  "human_dataset_manifest": "<dataset-id>",
  "rule_version": "briscola-italiana-1v1-v1",
  "seed": 20261123,
  "deals": 5000,
  "seat_swap": true,
  "profiles": [{"id": "p1", "weight": 0.1}],
  "time_budget_ms": null,
  "hardware": "<cpu and OS>",
  "created_at": "<timestamp>"
}
```

Questa è una **specifica esemplificativa**, non un output attuale. Le weights vanno stimate sul campione umano, non impostate arbitrariamente. Salvare conteggi grezzi e IC al 95%; considerare boostrap gerarchico per mazzo e persona (entrambi possono generare correlazioni).

### 7.4 Criterio di regressione

Se una versione “Massima nuova” migliora molto su un singolo stile ma peggiora su esperti, rischio e robustezza vanno riportati separatamente. Non fondere tutto in un unico “win rate universale”. Definire una funzione primaria preregistrata e guardrail:
- punteggio aggregato per distribuzione di popolazione reale;
- delta per ciascuna fascia e worst-group;
- stabilità su holdout di seed;
- tempo CPU/p95 e memoria;
- tasso di scelte su posizioni non corrette se l'input arriva dalla visione.

Un risultato marginale su pochi mazzi è **inconclusivo**, non un miglioramento.

---

## 8. Vision e interfaccia: audit laterale ma essenziale per il prodotto

HumanSim può essere addestrato e testato nel simulatore **senza** riconoscimento visivo. Non legarlo alla pipeline OCR. Tuttavia BrisMaster reale presenta una seconda fonte di errore: se la schermata viene letta male, il motore valuta lo stato sbagliato, indipendentemente dalla forza dell'algoritmo.

### 8.1 Pipeline oggi presente

```text
cattura monitor (capture/screen/live)
 -> ricerca regioni geometriche e normalizzazione (detect)
 -> match con carte di riferimento / rete / memoria (reference/nn/memory)
 -> posizioni e sequenza nel tempo (layout/track/mod)
 -> stato della partita e ricostruzione (assistant/session_replay)
 -> consiglio tramite motore
 -> overlay opzionale Psy Fly / UI
```

Questa è una descrizione ad alto livello: i dettagli delle soglie e delle transizioni sono in `docs/RICONOSCIMENTO-CARTE.md`. L'app permette scelta manuale dell'area, diagnosi e conferma delle carte incerte.

### 8.2 Risultati misurati riportati

- `docs/RELEASE-0.6.15.md`: su 44 partite **sintetiche appaiate**, giocate corrette/confermate da 82,3% a 89,2%; lettura corretta alla prima cattura su giocate visibili 96,0%, ma carta + lato non coincidono in tutti i casi.
- `docs/RELEASE-0.6.16.md`: su 8 mazzi mai visti, carte giuste dal 64,5% al 74,3% e giuste **senza conferma** dal 60,1% al 70,5%; dopo apprendimento 71,9%–80,3%. Su 20 partite dell'utente, partite perfette da 4 a 8, conferme 31→14, carte manuali 16→10.
- Limiti aperti dichiarati: partita `u13` con prima mano errata e otto inserimenti manuali; carte trevigiane e mazzi del nord senza riferimento.

**Non confrontare il 96,0% sintetico della prima cattura con il 70,5% di carte su mazzi mai visti come se fossero la stessa metrica.** Cambiano denominatore, condizioni e criterio.

### 8.3 Miglioramenti visivi da non confondere con HumanSim

1. Dataset reale annotato per tavolo/sito/mazzo, separato per sito non visto.
2. Gerarchia: individuazione carte → lato/slot → riconoscimento ID → tracking → aggiornamento stato; misurare ogni livello.
3. Tracciamento multi-frame con vincoli delle regole (nessuna carta duplicata, mano di 3, ordine presa/pescata).
4. Calibrazione delle probabilità e **astensione** quando il dato è incerto.
5. Misurare `first-frame`, `first-correct`, `confirmed`, falsi “certi”, duplicati, ritardi e disponibilità del consiglio corretto.
6. Regression suite con replay visivo riproducibile; test Windows separati per multi-monitor/DPI/minimizzazione.
7. Proteggere la privacy dei diagnostici (screen capture può contenere altri elementi del desktop).

Non tentare di ottenere il “90% umano” tramite miglioramenti OCR: sono due metriche indipendenti.

---

## 9. CI, release, dipendenze e rischi di manutenzione

### 9.1 CI attuale

`.github/workflows/ci.yml` contiene:
- test del workspace Rust in release su Windows;
- build dell'app Windows;
- creazione dell'installer Inno Setup;
- confronto di regressione del motore Briscola su Ubuntu;
- test simulativi di Sette e mezzo.

`.github/workflows/release.yml`: su nuova versione Cargo in `main`, compila/testa, crea installer, tag e release GitHub. Non alzare `version` per un semplice merge della documentazione/HumanSim M0; release solo dopo revisione.

### 9.2 Criticità da sorvegliare

- `crates/app/src/assistant.rs` è molto esteso (oltre 300 KB) e `vision/detect.rs` circa 153 KB: necessaria decomposizione progressiva per responsabilità, con test di regressione prima/dopo.
- Il workspace include numerosi giochi e asset pesanti: una modifica al core può rompere GUI, simulatore e regressioni; evitare refactoring massivi nella stessa PR del nuovo modello.
- Il crate Python RL è fuori dal workspace standard: il test `cargo test --workspace` NON copre automaticamente training Python, compatibilità PyTorch, formato reti o dati. Aggiungere CI mirata.
- Dati binari di rete e risultati hanno provenance da documentare, con checksum e rapporto di addestramento; verificare peso degli asset e uso di Git LFS o release asset se in crescita.
- I benchmark storici usano diverse versioni/configurazioni: non aggregarli senza manifest comune.
- `unwrap`/panic su input nelle CLI sperimentali possono essere accettabili nei prototipi ma non nell'import di dati reali non fidati: sostituire con errori strutturati e test di input malformato.
- Proteggere la regressione a performance: una grande rete umana in ogni rollout può aumentare drasticamente il costo di calcolo. HumanSim va chiamato nei turni del simulatore e benchmark, non indiscriminatamente dentro ogni ramo della ricerca strategica.
- La CI più importante per HumanSim sarà la **validazione statistica dei dati/test**; aggiungere job dedicato dopo aver definito un dataset piccolo pubblico o sintetico chiaramente etichettato per smoke test.

### 9.3 Comandi reali verificati nel progetto (non promesse di esecuzione)

Da root del clone:
```bash
cargo test --workspace --release
cargo build --release -p brismaster
cargo run --release -p briscola-sim -- --player engine:1000 --opponent heuristic --deals 1000 --seed 20261123
cargo run --release -p briscola-sim -- --player engine:1000 --opponent humansim:7 --deals 1000 --seed 20261123
cargo run --release -p briscola-sim -- --player engine:1000 --opponent humansim-style:circolo --deals 1000 --seed 20261123
cargo run --release -p briscola-sim -- --panel --deals 2000 --player engine:1000 --seed 20261022 --out tabella.md
cargo run --release -p briscola-sim -- gate --player engine:1000 --deals 5000 --seed 20261022
```

I comandi `humansim:...` richiedono il **ramo della PR #66 o un merge successivo**. La presenza della CLI non implica che il nuovo profilo sia stato validato su persone reali.

Per la pipeline attuale di partite registrate:
```bash
cargo run --release -p briscola-sim --bin reali -- --partite partite-brismaster.json --out dati-reali
cargo run --release -p briscola-sim --bin train -- --kind policy --data dati-reali --init crates/core/nets/politica.bin --out politica-reale.bin --epochs 10
```
Questo processo allena la rete di **deduzione** da ricostruzioni di gioco; **non** è ancora la pipeline statistica dedicata a HumanSim con vere mani/alternative etichettate.

**CLI FUTURA PROPOSTA (DA IMPLEMENTARE):**
```bash
cargo run --release -p briscola-sim -- human-bench --engine infer:20000:1:1000:0.25 --population humansim-v1 --deals 5000 --seed 20261123 --out report/
cargo run --release -p briscola-sim -- human-audit --dataset data/humans/test.jsonl --policy humansim-v1 --out report/
```

### 9.4 Come riprendere da un altro PC o con un altro assistente

```bash
git clone https://github.com/motthz/BrisMaster.git
cd BrisMaster
git fetch origin
git switch feature/humansim-benchmark-foundation
git pull --ff-only
cargo test --workspace --release
```

Se la PR è stata fusa, invece eseguire `git switch main && git pull --ff-only`. Su Windows usare PowerShell, installare Rust toolchain e gli strumenti di compilazione Windows richiesti; per installer serve Inno Setup. Controllare che `cargo` sia nel PATH. Se una rete/dipendenza non è presente, **non inventare file sostitutivi**: cercare nei tag, negli asset della release e nella cartella `bench`.

**Procedura agentica neutrale** (non dipende da Codex):
1. leggere questo file, `docs/HUMANSIM.md`, `docs/PARTITE-VERE.md`, `bench/0.6.0/README.md`;
2. eseguire test e baseline prima di cambiare codice;
3. aprire un branch per una milestone e cambiare il minimo indispensabile;
4. aggiungere test unitari, regressione e benchmark con seed fissato;
5. scrivere `docs/HUMANSIM-CHANGELOG.md` o PR con motivazioni, risultati e limiti;
6. non toccare `main` o pubblicare release senza gate verdi;
7. aggiornare **questo documento** a ogni decisione architetturale sostanziale.

---

## 10. Roadmap dettagliata a milestone, senza limite temporale

### M0 — Base di progetto e audit (**in corso**)

**Output previsto:** PR #66 con policy legale e indipendente, test, documentazione e compilazione verificata.

- [x] Ramo HumanSim separato da `main`.
- [x] API `distribution` e `choose` con seme e hand-only.
- [x] Entry point `humansim:ID` e `humansim-style:NAME` in simulatore.
- [x] Test unitari scritti nel branch; **esito CI da verificare**.
- [x] Specifica `docs/HUMANSIM.md`.
- [x] Masterplan di progetto (questo file).
- [ ] Verifica finale verde dei job GitHub e degli eventuali warnings.
- [ ] Benchmark riproducibile di 1000+ mazzi contro stili storici.
- [ ] Report baseline HumanSim chiaramente dichiarato *non reale*.
- [ ] Review, cleanup e merge della PR soltanto dopo controlli.

**Gate:** zero illegalità, completezza, riproducibilità, CI verde. Nessuna dichiarazione di realismo.

### M1 — Osservazioni umane affidabili

**Output previsto:** `Human Lab`, recorder V2, schema JSONL, manifest dati, validatore.

- [ ] Aggiungere `human_lab` o tab laboratorio nell'app con modalità senza consigli.
- [ ] Registrare mano e alternative vere di ciascun giocatore volontario, eventi e scelta.
- [ ] Aggiungere identificativi pseudonimi, sessione, consent, condizioni e ruleset.
- [ ] Provenienza/qualità per ogni osservazione; controllare anomalie e correzioni.
- [ ] Esportare JSONL compatibile con strumenti Python/Rust.
- [ ] Includere partite complete e ripetute, variabilità di abilità.
- [ ] Fissare split per giocatore e sessione.
- [ ] Pubblicare solo schemi/esempi sintetici e risultati aggregati, non dati personali.

**Gate:** validatore deterministico che accetta registri corretti e rifiuta casi impossibili spiegando il motivo. Dataset pilotato con consenso verificato.

### M2 — Baseline empirica e analisi descrittiva

**Output previsto:** `human-audit` funzionante su dataset con mano nota.

- [ ] Analisi condizionata per categoria carta, giocatore, fase, punteggio, briscole.
- [ ] Regole per escludere mosse obbligate dalle metriche primarie.
- [ ] Baseline B1 interpretabile, tuning su validation.
- [ ] Softmax/calibrazione con stima incertezza.
- [ ] Test holdout su persone mai viste e longitudinali.
- [ ] Report dettagliato per skill, fascia, persona.
- [ ] Figura delle distribuzioni sintetiche vs umane, intervalli.

**Gate:** B1 supera i profili manuali su NLL e calibrazione, con test bloccato.

### M3 — Identità persistente, memoria, errore strutturato

**Output previsto:** stesso profilo stabile su partite diverse e stato episodico coerente.

- [ ] Definire `HumanIdentity` con parametri individuali e prior stimato.
- [ ] Definire `HumanEpisode` e aggiornamento da soli eventi pubblici.
- [ ] Separare memoria dei carichi, semi, briscole e conteggio generale.
- [ ] Gestire errori dipendenti dalla difficoltà della scelta, non da coin flip costante.
- [ ] Confrontare stabilità e variabilità con giocatori veri ripetuti.
- [ ] Tests: stesso seme ⇒ stessa traiettoria; cambi mano segreta ⇒ stessa distribuzione.
- [ ] Non implementare psicologia/tilt non osservata come fatto sperimentale.

**Gate:** miglioramento su coerenza individuale senza peggiorare predizione e calibrazione.

### M4 — Imitazione appresa, modello sequenziale

**Output previsto:** modello B2 piccolo con pesi versionati e inferenza Rust.

- [ ] Allenamento offline GRU/LSTM/Transformer piccolo da decisioni reali.
- [ ] Input canonico e mask azioni; normalizzazioni stabili.
- [ ] Identità latente regolarizzata e valutazione new-person.
- [ ] Early stopping e CV raggruppata per sessione/persona.
- [ ] Export eseguibile su CPU, controllo bitwise/differenze numeriche PyTorch-Rust.
- [ ] Compressione solo se non altera NLL/calibrazione oltre guardrail.
- [ ] Distillazione eventuale dal modello umano grande al piccolo, **mai** da Massima etichettata come umana.
- [ ] Valutazione offline + in partite autonome.

**Gate:** miglioramento significativo vs B1 su dati/test non visti e prestazioni di CPU accettabili.

### M5 — Validazione di indistinguibilità in dominio definito

**Output previsto:** rapporto pubblico riproducibile con test statistici preregistrati.

- [ ] Registrare definizioni equivalenza, potenza e campioni richiesti.
- [ ] Discriminatori diversi dalla rete usata in allenamento.
- [ ] Blind test con layout uniforme e player indipendenti.
- [ ] Analisi per skill, condizioni rare, avversario, fase.
- [ ] Baseline umano-vs-umano, comparazione delle distribuzioni.
- [ ] Intervalli bootstrap per giocatore/sessione, reporting dei fallimenti.
- [ ] Sanzionare dimensioni insufficienti come "inconclusivo".

**Gate:** soddisfare contemporaneamente metriche predittive, dinamiche e classificatore entro limiti dichiarati.

### M6 — Popolazione benchmark congelata per Massima

**Output previsto:** `HumanSim Population v1`, runner CI, manifest e report.

- [ ] Distribuzione calibrata per classi di abilità e stili osservati.
- [ ] 2+ batterie: nominale, worst-case; congelare RNG e weights.
- [ ] Confronto a posti invertiti, paired seeds, campioni adeguati.
- [ ] Report confrontabile con storici Massima.
- [ ] Release candidate della popolazione indipendente dalle release del motore.
- [ ] Scenario test di adattamento dello stile per Massima.
- [ ] Monitorare regressioni guardrail non solo media.

**Gate:** la nuova versione di Massima può essere valutata contro **la stessa** HumanSim v1 e ottenere risultati replicabili.

### M7 — Robustezza, ricerca e mantenimento

- [ ] Raffinare cognizione e credenze con dati aggiuntivi.
- [ ] Aggiungere nuovi tipi di giocatori solo se osservati.
- [ ] Audit per drift di comportamento fra piattaforme, tornei, versioni delle regole.
- [ ] Evitare che Massima sovra-ottimizzi una popolazione fissa: valutare una seconda popolazione **tenuta fuori dallo sviluppo**.
- [ ] Monitorare performance e testare nuova popolazione `v2` senza sovrascrivere `v1`.
- [ ] Archiviare dataset manifest, check, report, hash, parametri per riproducibilità.

---

## 11. Esperimenti prioritari (ordine per valore informativo, non per facilità)

| Exp | Ipotesi | Modifica | Confronto | Esito utile |
|---|---|---|---|---|
| EXP-01 | i profili manuali non predicono le persone | NLL `Persona` e baseline B1 | dataset reale out-of-player | quantificare il gap |
| EXP-02 | bias da consigli di Massima | isolare `advice_visible` | mosse consigliate vs no-hint | separare contaminazione |
| EXP-03 | memoria dei carichi è diversa dalla memoria di carte minori | tre pesi memoria | held-out e partite intere | modello più credibile |
| EXP-04 | l'identità conta oltre la skill | gerarchico vs modello globale | sessioni longitudinali | variazione intra/inter |
| EXP-05 | rete sequenziale batte B1 | GRU/Transformer piccolo | split identico | miglioramento reale, non overfit |
| EXP-06 | softmax produce troppi errori impossibili | errori strutturati | curva regret condizionale | realismo degli sbagli |
| EXP-07 | HumanSim non ha leakage | cambi nascosti invarianti | 100k stati perturbati | distribuzioni identiche |
| EXP-08 | avversario realisticamente adattivo modifica il valore di Massima | Massima vX vs vY | stessa popolazione congelata | differenza di forza |
| EXP-09 | modelli grandi offrono poco oltre B1 | ablation di dimensione | p95 CPU + NLL | scegliere costo giusto |
| EXP-10 | confronto troppo facile vs stessi modelli | test OOD, human-vs-human | valutazione cieca | prova onesta di realismo |

Per ogni esperimento salvare: `experiment_id`, `claim`, dataset split manifest, commit, codice, args, seed, hardware, metriche primarie, IC, metriche negative, limite, decisione finale.

---

## 12. Registro dei rischi (da aggiornare a ogni milestone)

| Rischio | Gravità | Rilevazione | Mitigazione |
|---|---|---|---|
| Dataset non umano/assistito | Critico | provenance, `advice_visible` | esclusione/test separati |
| Leakage di carte nascoste | Critico | metamorphic tests | API a sole osservazioni |
| Split non indipendenti | Critico | group audit | per persona/sessione |
| Overfit a pochi giocatori | Alto | new-person holdout | modelli gerarchici + regolarizzazione |
| Dati pochi in situazioni rare | Alto | coverage map | raccolta mirata e IC |
| Profilo uniforme e prevedibile | Alto | test sequenziali | identità e stato persistenti |
| Troppe mosse casuali | Alto | regret distribution | errori contestuali |
| Confondere forza con realismo | Critico | report separati | doppia pipeline |
| Overfit di Massima su HumanSim v1 | Alto | holdout v2/humans | pannelli indipendenti |
| Regressioni a OCR/stato | Alto | replay per sito/mazzo | CI e gate di astensione |
| Costi di inferenza | Medio | benchmark CPU | modelli piccoli e cache lecita |
| Pubblicazione dati personali | Critico | privacy review | pseudonimizzazione, accesso limitato |
| Aggiornamento automatico non verificato | Alto | build+installer+smoke test | release gate, rollback |
| Modifiche a game rules | Alto | invariant tests | versionare ruleset |
| Sostituzione modello senza hash | Alto | manifest/hash check | artefatti immutabili |

---

## 13. Decisioni architetturali (ADR) iniziali

**ADR-001 — HumanSim non dipende da Massima.** Accettata. Una policy di imitazione usa solo osservazioni, non consigli strategici del motore. Massima può stimare post hoc la qualità delle mosse.

**ADR-002 — Prima validare dati, poi scalare la rete.** Accettata. Una grande rete senza dati umani non è un miglioramento verificabile.

**ADR-003 — Popolazione e persona sono concetti diversi.** Accettata. Il modello di popolazione serve per nuovi individui; il modello personalizzato serve per simulare ripetutamente lo stesso individuo.

**ADR-004 — Paired seeds e CI per mazzo.** Accettata. Mantenere la struttura dell'attuale `arena.rs`.

**ADR-005 — Baseline storiche conservate.** Accettata. Non riscrivere o eliminare i parametri esistenti prima di avere un confronto con dataset reale.

**ADR-006 — Stato umano e simulatore sono separati.** Accettata. L'ambiente può conoscere tutto per arbitrare; il giocatore no.

**ADR-007 — Vision e HumanSim restano disaccoppiati.** Accettata. HumanSim deve funzionare senza UI. Eventuale integrazione OCR usa solo eventi verificati.

**ADR-008 — I test di indistinguibilità sono contestuali.** Accettata. Nessun requisito impossibile di "identico a tutte le persone"; definire popolazione, contesti e intervalli.

---

## 14. Backlog operativo immediato (priorità assolute)

**P0 — fare adesso**
1. Confermare CI PR #66; correggere qualsiasi failure di compilazione/test e tracciare il commit.
2. Aggiungere baseline benchmark dei 20 profili con report riproducibile.
3. Definire e validare schema `human_decision_v1` e regole dati reali.
4. Aggiungere `advice_visible/advice_followed` e modalità di raccolta no-hint, senza cambiare il comportamento dell'assistente normale.
5. Definire i gruppi `player_id`, `session_id` e test congelato.
6. Preparare report della distribuzione delle azioni reali per fase/abilità/contesto prima di scegliere la rete.

**P1 — dopo dati affidabili**
7. Modello interpretabile gerarchico e calibrazione probabilistica.
8. Memoria episodica e errori strutturati.
9. Rete sequenziale e test contro B1.
10. Discriminatore / blind testing su partite complete.
11. Popolazione congelata e integrazione con release.

**P2 — qualità del repository**
12. Estrarre moduli più piccoli da `assistant.rs` e `vision/detect.rs`, con test invarianti.
13. CI Python/Rust per export e inferenza della rete.
14. Manifest dei pesi binari e documentazione di provenienza.
15. Smoke test installazione e aggiornamenti Windows.

---

## 15. Checklist per review e handoff

Per ogni nuova PR HumanSim chi la riceve deve poter rispondere SÌ a:

- [ ] Quale problema misurabile risolve?
- [ ] Quali file modifica e perché? È isolata da Massima?
- [ ] Mostra la definizione precisa delle osservazioni e delle azioni?
- [ ] È dimostrato che non vede informazioni nascoste?
- [ ] I dati sono veri, sintetici o ricostruiti? Sono etichettati?
- [ ] Sono registrati seed, commit, parametri, hash dei dati/modelli?
- [ ] Cosa migliora sul test *non usato* per sviluppo?
- [ ] Che cosa peggiora, anche se la media migliora?
- [ ] Quanto impiega su CPU e quanta memoria usa?
- [ ] I test CI esistenti e quelli nuovi sono verdi?
- [ ] I benchmark di Massima e degli altri giochi rimangono comparabili?
- [ ] La documentazione centrale e il changelog sono aggiornati?
- [ ] È specificato come tornare alla versione precedente?

### Regola finale di mantenimento

`PROJECT-MASTERPLAN.md` è la **mappa principale**; `docs/HUMANSIM.md` contiene la specifica in sintesi, i file nel core contengono la verità del codice, `bench/` conserva prove riproducibili. In caso di contraddizione: **codice e report con commit/seed verificati prevalgono sulle frasi narrative**. Annotare esplicitamente la contraddizione e correggerla, non cancellare dati storici.

---

## 16. Riferimenti puntuali

**Repository**
- [Codice sorgente generale](https://github.com/motthz/BrisMaster)
- [PR #66, fondazione HumanSim](https://github.com/motthz/BrisMaster/pull/66)
- [Specifica sintetica HumanSim](docs/HUMANSIM.md)
- [Partite vere e training già esistente](docs/PARTITE-VERE.md)
- [Algoritmo riconoscimento carte](docs/RICONOSCIMENTO-CARTE.md)
- [Replay e test visuali](docs/replay/README.md)
- [Risultati storici motore](bench/RAPPORTO.md)
- [Esperimenti massivi 0.6.0](bench/0.6.0/README.md)
- [Release 0.6.16](docs/RELEASE-0.6.16.md)
- [CI](.github/workflows/ci.yml)

**Riferimenti esterni (non specifici di Briscola; nessuno prova che HumanSim sia realistico):**
- *Synthetic User Generation in Games: Cloning Player Behavior with Transformer Models*, Information (2025): https://www.mdpi.com/2078-2489/16/4/329
- *A Turing Test of Whether AI Chatbots Are Behaviorally Similar to Humans*, PNAS (2024): https://pmc.ncbi.nlm.nih.gov/articles/PMC10907317/
- *Using Provenance Data and Imitation Learning to Train Human-Like Bots*, Entertainment Computing (2024): https://www.sciencedirect.com/science/article/pii/S1875952123000587

**Fine del documento.**