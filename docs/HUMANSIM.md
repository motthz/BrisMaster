# HumanSim — benchmark di avversari umani simulati per BrisMaster

**Stato: specifica e protocollo sperimentale (non un modello già validato).**

## 1. Obiettivo

Costruire un sistema indipendente dal motore competitivo ("Massima") che generi
giocatori di Briscola 1 contro 1 le cui scelte siano statisticamente difficili da
distinguere da quelle di persone reali, **a parità di informazioni e contesto**.

HumanSim è un **benchmark**, non una versione depotenziata del motore Massima.
La somiglianza umana non è sinonimo di forza: l'avversario artificiale deve
riprodurre la distribuzione di abilità, errori sistematici, memoria e stili
dei giocatori osservati.

Non è possibile certificare "indistinguibile" in assoluto. La dichiarazione
ammissibile è sempre relativa a un campione, una popolazione, un protocollo
sperimentale e intervalli di confidenza.

## 2. Stato del repository (baseline)

- `crates/core/src/style.rs`: 12 parametri (`Knobs`), 20 profili `STYLES`,
  `Persona` e `Adapt`. Sono euristiche impostate manualmente, **non** parametri
  calibrati su un corpus rappresentativo di partite umane.
- `crates/core/src/ai.rs`: modelli forti (PIMC, ISMCTS, solver) utilizzabili
  come **misura di qualità della mossa**, non come fonte di etichette "umane".
- `crates/sim/src/agents.rs`: parsing giocatori `stile:`, `persona:`,
  `umano-*`, `engine:`, `ismcts+:` e versioni precedenti.
- `crates/sim/src/arena.rs`: partite duplicate su stessi mazzi, alternanza
  dei posti, semi riproducibili, statistiche di esito.
- `bench/RISULTATI.md`: risultati storici della **forza**; nessun risultato
  va interpretato come prova di realismo umano.

Queste parti saranno riutilizzate. HumanSim avrà propri modelli, dati e test.
La modalità Massima non dovrà essere alterata accidentalmente.

## 3. Contratto del giocatore sintetico

### Osservazioni ammesse

Solo quanto una persona al tavolo potrebbe sapere:
- carte della propria mano;
- carta aperta sul tavolo, briscola (quando visibile) e storia pubblica
  delle prese/pescate;
- punteggi, numero di carte nel mazzo, ordine di gioco;
- informazioni pubbliche sull'avversario, se presenti;
- tempo disponibile / latenza solo **quando rilevati nei dati**.

**Divieti per la politica HumanSim:** accedere alla mano avversaria vera,
all'ordine delle pescate future o ai campi nascosti di `Game`.
Le informazioni complete del simulatore possono servire soltanto per audit,
generazione delle partite e calcolo *separato* della qualità strategica.

### Risposta

`HumanSimPolicy::distribution(observation, history, persona_state) -> {legal_card: p}`

Proprietà:
1. tutte e sole le carte legali hanno probabilità positiva o nulla;
2. le probabilità sommano a 1 (entro tolleranza numerica);
3. `choose` campiona dalla distribuzione usando un seme riproducibile;
4. lo stato della persona conserva identità e preferenze per l'intera partita;
5. eventuale variabilità tra partite si campiona da una distribuzione esplicita,
   con seme e versione salvati;
6. nessun accesso a campi nascosti; nessuna dipendenza da Massima in inferenza.

## 4. Modello a livelli

**Livello A — popolazione:** campionare gruppi (principianti, intermedi,
esperti) e stili; le frequenze reali si stimano dai dati. Niente quote
arbitrarie presentate come realistiche.

**Livello B — persona persistente:** ciascuna identità conserva memoria,
preferenze, regole favorite, qualità di ragionamento e calibrazione del rischio.
L'eterogeneità tra giocatori deve essere maggiore della semplice variazione
casuale all'interno di un individuo.

**Livello C — stato episodico:** memoria della mano, sequenza delle prese,
punteggio, briscole rimaste stimate, attenzione / eventuale stanchezza.
Le dinamiche episodiche sono abilitate solo quando validate su dati adeguati.

**Livello D — politica della mossa:** distribuzione condizionata all'insieme di
informazioni. Baseline interpretabile con `Knobs`, successivamente modello
supervisionato che apprende le preferenze di scelta dalle partite umane.
Le azioni non valide sono sempre mascherate.

**Livello E — ragionamento limitato:** modello umano delle conseguenze
probabili; evitare di chiamare direttamente il solver a informazione perfetta
dalla politica, salvo in condizioni in cui l'informazione sia pubblica.

Una rete di politica può essere allenata offline (Python) ed eseguita tramite
infrastruttura esistente in Rust, previa misura di accuratezza e latenza.
Non serve introdurre un LLM durante ogni mossa.

## 5. Dati reali necessari

Per ogni decisione, conservare almeno:
- `game_id`, `anonymous_player_id`, `session_id`, `ruleset_version`;
- `turn_index`, `seat`, `hand_before`, `table_card`, `trump`;
- `public_history`, `score_before`, `deck_remaining`;
- `legal_moves`, `chosen_card`;
- eventuale `decision_time_ms` se effettivamente misurato;
- `origin` e indicatore di attendibilità della ricostruzione.

Registrare partite con persone consenzienti o usare dati la cui raccolta sia
lecita e autorizzata. Non usare indistintamente registrazioni senza mani note:
senza `hand_before` non sappiamo quali alternative erano disponibili.
Non utilizzare mosse prodotte da bot per etichettare dati "umani".

Controlli: carte duplicate, mosse impossibili, regole coerenti,
ricostruzione delle 20 prese, totale di 120 punti, vincoli temporali,
identificativi pseudonimizzati, qualità della sorgente.

Il dataset deve distinguere competenza, persone, sessioni, varianti regionali
e regole: le versioni non comparabili **non** vanno aggregate.

## 6. Train/validation/test anti-leakage

Separare per **giocatore e sessione**, non soltanto per singola mossa.
Mantenere un test bloccato e mai utilizzato per modificare parametri o
scegliere iperparametri. In un test dedicato ai *digital twin* individuali,
separare cronologicamente le sessioni della stessa persona.

Conservare versioni immutabili del dataset, seed, hash del modello,
commit Git e configurazione per riprodurre ogni risultato.
Non pubblicare dati personali o partite private senza autorizzazione.

## 7. Metriche di realismo (primarie)

1. **NLL / log-loss** sulla vera carta umana, e **top-1 accuracy**,
   rispetto a `Persona`, `HeuristicPlayer` e modelli di frequenza.
2. **Calibrazione:** quando un modello assegna una probabilità alla scelta,
   verificare che gli eventi accadano con frequenza corrispondente.
3. **Distribuzioni di comportamento:** differenze sulle frequenze di
   apertura di carico, taglio di briscola, conservazione di Asso/Tre,
   errori stimati e azioni in vantaggio/svantaggio, con intervalli.
4. **Stabilità individuale:** correlazione dello stile fra sessioni,
   variazione intra-persona, distribuzione della variabilità.
5. **Classificatore umano-vs-sintetico** allenato su partite complete
   mai viste, con valutazione su giocatori e semi nuovi; riportare
   AUC e intervallo di confidenza e confrontare con un riferimento
   umano-vs-umano. Obiettivo aspirazionale AUC vicino a 0,5, non prova
   assoluta di indistinguibilità.
6. **Blind test con valutatori** su decisioni o replay con interfaccia
   standardizzata; rilevare accuratezza e motivazioni.
7. **Copertura dei profili:** non basta imitare bene il "giocatore medio";
   calibrare separatamente principianti, intermedi ed esperti.

Un test che non riesce a distinguere i due gruppi può avere potenza
insufficiente: riportare sempre numerosità, incertezza e analisi di potenza.

## 8. Metriche di qualità del benchmark (secondarie)

- zero mosse illegali e zero fughe di informazioni;
- completamento regolare di ogni partita;
- riproducibilità a semi e versioni fissati;
- throughput di partite/minuto e latenza per scelta;
- win rate, pareggi e punti medi **per ciascuna fascia di abilità**;
- variabilità di forza tra seed e profili;
- ranking stabile delle versioni di Massima contro un pannello HumanSim
  congelato, con mazzi duplicati, posti invertiti e intervalli su mazzi
  (non su singole partite correlate);
- test di regressione rispetto ai motori storici e ai 20 profili esistenti.

Le vittorie contro HumanSim misurano la forza contro la popolazione simulata:
non vanno presentate come tasso di vittoria garantito contro esseri umani.

## 9. Roadmap senza scadenza artificiale

### M0 — Contratto e baseline
- Isolare in un nuovo namespace `humansim` il sistema di benchmark.
- Mappare le 20 `STYLES` al protocollo comune, con seed riproducibili.
- Inserire guardrail anti-cheating e registrare le distribuzioni di scelta.
- Stabilire i risultati attuali come baseline, senza cambiare Massima.

**Criterio di uscita:** intere partite riproducibili, mosse legali,
log strutturato, suite automatica verde.

### M1 — Raccolta e validazione dati
- Registratore di partite umane autorizzate con mano visibile
  al decisore; importatore JSONL con validazione e report scarti.
- Creare dataset di pilotaggio e un test set bloccato;
  dimensionare la raccolta ulteriore in base a copertura e
  intervalli, anziché fissare numeri arbitrari.

**Criterio di uscita:** ricostruzione verificabile delle partite,
splits per giocatore/sessione e dataset versionato.

### M2 — Calibrazione empirica dei 20 profili
- Stimare parametri e pesi della popolazione sui dati di training;
- aggiungere errori *strutturati* invece di `errori` random costante;
- produrre distribuzioni per mosse e profili.

**Criterio di uscita:** log-loss e calibrazione migliori dei modelli
handcrafted su test di validazione, senza fuga dati.

### M3 — Imitazione appresa
- Confrontare baseline interpretabili (logit condizionali / modelli
  gerarchici) e una rete di politica sequenziale;
- preservare identità persistente e memoria;
- testare in modo rigoroso condizioni mai viste e persone mai viste.

**Criterio di uscita:** miglioramento significativo su test bloccato,
senza collasso verso comportamenti uniformi.

### M4 — Test di realismo e stress
- Classificatori indipendenti, valutazione blind, errori per fase di
  partita, robustezza a mazzi/situazioni avverse;
- analisi degli errori per tipi di umano non ben rappresentati.

**Criterio di uscita:** evidenza ripetibile di forte somiglianza umana
entro il dominio testato; limiti espliciti.

### M5 — Benchmark permanente per Massima
- Creare un pannello HumanSim congelato per ogni release.
- Eseguire confronti *paired* su identici mazzi e seed.
- Conservare separatamente "forza", "realismo", "varietà" e "latenza".
- Pubblicare artefatti e report con hash della configurazione.

**Criterio di uscita:** una release nuova di Massima può essere
confrontata in modo riproducibile con tutte le precedenti.

## 10. Interfaccia proposta

```text
briscola-sim human-bench --engine massima --population humansim-v1 --deals N --seed S --out report/
briscola-sim human-bench --engine v0.6.0/ismcts+:20000 --population humansim-v1 --deals N --seed S
briscola-sim human-audit --dataset data/humans/test.jsonl --policy humansim-v1 --out report/
```

Questi sono **comandi target, non ancora implementati**.

## 11. Regole operative

- HumanSim deve restare un'implementazione indipendente: nessuna
  modifica non testata alla selezione delle mosse di Massima.
- Nessuna metrica sintetica da sola autorizza la parola "umano".
- Ogni calibrazione deve indicare se è basata su dati reali o artificiale.
- Ogni PR deve includere test, benchmark e note sulle limitazioni.
- Scopo: simulazione e valutazione controllata, non elusione di controlli
  anti-bot o uso occulto su piattaforme di gioco.

## 12. Riferimenti di ricerca

- Zhu et al., "Capturing the complexity of human strategic decision-making
  with machine learning", Nature Human Behaviour, 2025.
  https://www.nature.com/articles/s41562-025-02230-5
- "Synthetic User Generation in Games: Cloning Player Behavior with
  Transformer Models", Information, 2025.
  https://www.mdpi.com/2078-2489/16/4/329
- "The Many Challenges of Human-Like Agents in Virtual Game
  Environments", AAMAS, 2025.
  https://arxiv.org/abs/2505.20011
