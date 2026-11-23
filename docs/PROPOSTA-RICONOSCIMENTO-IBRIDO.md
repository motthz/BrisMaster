# Proposta sperimentale: riconoscimento ibrido e temporale delle carte di Briscola

> **SOLO DOCUMENTAZIONE — NON IMPLEMENTATO.** Specifica per Claude: valutare, prototipare separatamente e confrontare con la pipeline attuale prima di integrare. Non modificare il comportamento di produzione senza benchmark e approvazione.

## 1. Contesto verificato nella repository

BrisMaster è un'app Windows principalmente in Rust. Il riconoscimento esiste già in `crates/app/src/vision/`, con acquisizione tramite `xcap` (`capture.rs`), coda frame (`mod.rs`), individuazione/normalizzazione delle carte (`detect.rs`), confronto con mazzi di riferimento (`reference.rs`), rete neurale (`nn.rs`), conteggio simboli (`pips.rs`), memoria locale (`memory.rs`) e tracking (`track.rs`, `live.rs`). `crates/app/src/assistant.rs` trasforma osservazioni in eventi, applica controlli di coerenza, recupero e conferme. Leggere **prima** `docs/RICONOSCIMENTO-CARTE.md` e il codice effettivo: il documento esistente descrive alcune versioni precedenti e potrebbe non essere aggiornato.

**Non sostituire indiscriminatamente il sistema attuale con Python, YOLO o OmniParser.** L'architettura corrente contiene già molti dei componenti desiderati: detection multi-tentativo, matching di più mazzi, CNN suit/rank, memoria, smoothing, recupero della cronologia e controllo delle regole. L'idea è introdurre candidati alternativi misurabili, solo dove correggono errori dimostrati.

## 2. Obiettivo e perimetro

Sorgente: partite online riprese esclusivamente dai pixel dello schermo. Modalità: **tracker passivo** (identificazione carte, posizione/ruolo, presa, briscola, pescate, punti, stato e incertezze), senza click, automazione delle mosse, intercettazione del traffico o lettura dei dati interni del sito. Funzionamento locale/offline; nessun caricamento automatico di screenshot o dati utente. Conservare gli attuali formati di memoria e i percorsi dei dati oppure predisporre migrazione reversibile.

Obiettivo misurabile: aumentare la precisione delle carte riconosciute al primo colpo e soprattutto la ricostruzione corretta di prese e partite intere, senza peggiorare i mazzi e i tavoli già supportati. 90%, 95%, 98% sono target sperimentali, non risultati ottenuti.

## 3. Proposta architetturale incrementale

### A. Benchmark prima di sviluppare

1. Fotografare lo stato del branch e della versione attuale; eseguire test esistenti e annotare commit SHA, hardware, dataset e metriche.
2. Usare diagnosi, video e test di partite reali con etichette verificate; separare **train**, **validation**, **test** per partita e per stile/sito, evitando leakage di frame quasi identici.
3. Annotare: carta vera (40 classi), posizione/ruolo (mano, briscola, mia giocata, avversario), visibilità parziale, mazzo/stile, dimensioni, occlusione, evento e timestamp. Rispettare licenze delle immagini; non committare materiale non autorizzato.
4. Costruire una matrice degli errori: carta non trovata, falso rettangolo, carta sbagliata, ruolo sbagliato, evento saltato, evento duplicato, nuova partita confusa con la precedente, lettura corretta ma persa durante il recupero.

### B. Detection alternativa, solo come esperimento

Mantenere `detect.rs` come baseline. Valutare un detector leggero di carte (es. YOLO tramite prototipo separato, con verifica delle condizioni di licenza e packaging) solo su screenshot nei quali il detector attuale fallisce: carte sovrapposte, bordi non bianchi, tavoli chiari, mazzi sconosciuti, carte tagliate. Confrontare recall/precision delle bounding box, costo CPU/GPU, memoria, dimensione installer e latenza. Se un detector nuovo non supera la baseline nei casi difficili senza regressioni, non integrarlo. Possibile esportazione ONNX e inferenza Rust/CPU, solo dopo prova reale.

### C. Identificazione ibrida delle 40 carte

Mantenere il confronto con riferimenti (`reference.rs`), la CNN (`nn.rs`), i simboli (`pips.rs`) e la memoria (`memory.rs`). Sperimentare:
- classificatore dedicato a 40 classi, eventualmente multi-head seme/valore, su crop normalizzati;
- confronto multi-scala e feature locali sulle parti visibili;
- calibrazione delle probabilità e fusione dei punteggi esistenti con quelli nuovi;
- routing condizionale: riconoscimento veloce per mazzi noti, modello aggiuntivo solo per casi ambigui;
- separazione netta tra `unknown`, `not_visible` e `candidate_card`; mai trasformare un'incertezza in una certezza senza evidenza.

**Nota tecnica:** gli score correnti sono log-likelihood-like, non probabilità calibrate. Non sommare ingenuamente valori eterogenei; calibrare su validation e misurare reliability diagram/ECE, top-1, top-3 e tasso di errori ad alta confidenza.

### D. Tracking temporale ed eventi

Integrare con `track.rs`, `live.rs` e sightings/smoothing di `assistant.rs`. Per ogni carta fisica visibile mantenere track ID temporaneo, coordinate, ruolo, timestamp, qualità crop, distribuzione candidati e stato (provvisorio/confermato/ambiguo). Aggregare evidenza **senza contare frame quasi identici come osservazioni indipendenti**. Distinguere animazione, nuova carta, carta spostata, carta scomparsa e fine presa. Usare un event log append-only con correzioni versionate, per consentire replay deterministico e rollback.

Se una carta viene mostrata per pochi frame, conservare il miglior crop e rivalutarlo. Evitare che la coda scarti gli unici frame informativi durante prese veloci; valutare un buffer circolare e trigger su cambiamento. Nessuna inferenza di identità di carte nascoste come se fossero osservate.

### E. Vincoli della Briscola e ricostruzione

Riutilizzare il motore di regole già esistente, senza duplicarlo. Verificare: unicità delle 40 carte, massimo tre carte nella mano ordinaria, due carte per presa completa, ordine delle giocate, decremento del mazzo, briscola scoperta, sequenza di pescata, punti e inizio di nuova partita. Le regole devono **scartare ipotesi impossibili**, non forzare una carta specifica quando più ipotesi sono legali. Separare eventi osservati, deduzioni certe e ipotesi probabilistiche.

Consentire correzione retroattiva: se un'identità provvisoria diventa incompatibile con osservazioni successive, ricalcolare lo stato tramite replay, registrando motivo e confidenza; non alterare silenziosamente la cronologia originale.

### F. Profili per siti diversi e apprendimento controllato

Conservare il layout locale (`tavolo.json`) e la memoria immagini (`carte_viste.json`). Aggiungere, se utile, profili versionati per geometria, scaling, zone del tavolo, mazzo/stile, zoom, colore e qualità di cattura. Non introdurre un riconoscitore universale generico come dipendenza obbligatoria. Le etichette automatiche **non** devono contaminare la memoria o il training: usare solo conferme utente o verifiche indipendenti sufficientemente solide. Consentire cancellazione, esportazione e revisione degli esempi.

## 4. Esperimenti in ordine di valore atteso

1. **E0 — Baseline riproducibile:** benchmark completo senza cambiare algoritmo.
2. **E1 — Temporal best-frame:** scegliere e rivalutare i migliori crop già acquisiti; confrontare con smoothing attuale.
3. **E2 — Fusion calibrata:** modello 40 classi in shadow mode, con output registrato ma non applicato.
4. **E3 — Detector alternativo:** solo se i falsi negativi di detection dominano; confronto A/B su stessi frame.
5. **E4 — Event log + replay:** robustezza su prese rapide, finestra nascosta, frame mancanti, correzioni.
6. **E5 — Generalizzazione multi-sito:** holdout di siti/mazzi mai visti e verifica delle regressioni.

Per ogni esperimento preparare: ipotesi, dataset, baseline, modifica isolata, test, metriche, costi runtime, risultati, decisione KEEP/REVERT. Usare feature flag; non cambiare default prima del superamento dei gate.

## 5. Metriche e criteri di accettazione

Misurare separatamente:
- detection precision/recall, falsi positivi per minuto e recall carte parziali;
- card top-1/top-3, first-sighting accuracy, accuracy dopo aggregazione, errori ad alta confidenza;
- correttezza dei ruoli, prese ricostruite, eventi mancati/duplicati, percentuale partite perfette;
- richieste di conferma per partita, tempo al riconoscimento, latenza p50/p95, frame persi, CPU/RAM;
- performance per sito, mazzo, zoom, risoluzione, sovrapposizione, oscuramento e transizioni.

Gate proposti (da calibrare sulla baseline): nessuna regressione statisticamente rilevante sui mazzi noti, miglioramento misurabile dei casi difficili, nessun incremento degli errori silenziosi ad alta confidenza, replay deterministico dei test, utilizzo risorse compatibile con Windows senza GPU dedicata. Non dichiarare 99% senza intervalli di confidenza e set di test indipendente.

## 6. Test obbligatori

- Le 40 carte per ciascun mazzo di riferimento, inclusi crop, rotazioni, variazioni cromatiche e scale.
- Mani sovrapposte, carte parziali, briscola sotto il mazzo, pannelli e annunci simili a carte.
- Tavolo chiaro/scuro, zoom browser, multi-monitor, finestra spostata, overlay, schermo grigio.
- Prese molto rapide, perdita frame, app in background, fine partita, nuova partita e disconnessione.
- Carta già uscita che riappare visivamente per animazione; due candidate molto simili; memoria con etichetta errata.
- Replay end-to-end confrontando ogni evento e stato con ground truth; test di regressione su tutti i casi esistenti.

## 7. Integrazione con il sistema attuale

Progettare interfaccia dati tra visione e `assistant.rs` che esponga `timestamp`, `role`, `bbox`, `candidates[40]`, `quality`, `track_id`, `source`, `provisional`. È uno schema concettuale, **non** una API già esistente. Fare adapter e test di compatibilità, non riscrivere `assistant.rs` o il motore strategico. Preferire prototipi e benchmark isolati, poi integrare in Rust se giustificato; Python può essere usato offline per training/esperimenti ma non deve diventare obbligatorio per l'utente.

Non toccare UI, suggerimenti di gioco, aggiornamenti automatici, installer o logica strategica salvo stretta necessità dimostrata. Non aggiungere rete o telemetria.

## 8. Istruzioni operative per Claude

1. Leggi `README.md`, `docs/RICONOSCIMENTO-CARTE.md`, tutti i moduli pertinenti di `crates/app/src/vision/`, `assistant.rs`, test e script esistenti.
2. Controlla la versione **effettiva** del codice e segnala differenze rispetto alla documentazione; non assumere che i dettagli sopra siano sempre attuali.
3. Presenta prima un audit: pipeline attuale, colli di bottiglia, benchmark disponibili, errori ricorrenti e opportunità concrete. Evita di reimplementare funzioni già presenti.
4. Proponi il **minimo esperimento reversibile** con maggiore probabilità di migliorare il first-sighting e la precisione dell'intera partita.
5. Esegui baseline e test prima/dopo sullo stesso dataset, con report quantitativo e campioni di fallimento.
6. Mantieni ogni novità dietro feature flag e non abilitare in produzione senza confronto e conferma.
7. Documenta ogni esperimento, comandi di riproduzione, risultati, limiti, commit e istruzioni per rollback.
8. Se manca ground truth, chiedi esempi o prepara strumenti di annotazione: non inventare metriche o dichiarare successi.

## 9. Repository di riferimento da valutare (non dipendenze approvate)

- Ultralytics: https://github.com/ultralytics/ultralytics — detector alternativo; controllare licenza e costi.
- OpenCV: https://github.com/opencv/opencv — feature matching, tracking, preprocessing.
- RapidOCR: https://github.com/RapidAI/RapidOCR — solo se il riconoscimento dei testi è realmente necessario.
- OmniParser: https://github.com/microsoft/OmniParser — utile per UI generiche, **non consigliato** per leggere le 40 carte a ogni frame.

**Decisione architetturale:** miglioramento incrementale della pipeline Rust esistente, con benchmark prima di qualunque modifica. Questo documento è una proposta di ricerca e integrazione, non una richiesta di cambiare ora il codice.
