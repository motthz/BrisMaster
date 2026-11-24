# Massima 2.0, fase 3: partite vere contro Massima

Scopo della fase 3: raccogliere partite di persone vere contro Massima, **senza consigli**,
e con queste costruire un bot che giochi come le persone (il "bot umano"), da usare poi come
avversario nei test delle fasi 4 e 5. Il bot umano si costruisce **solo su dati reali**:
nessun bot umano provvisorio. Il prototipo HumanSim della PR #66 non è calibrato su persone
e non viene usato come bot umano.

Obiettivo di raccolta deciso: **50 persone, 2.000 partite**.

## Cosa c'è nel repo

| Percorso | Cosa fa |
|---|---|
| `crates/lab/src/tavolo.rs` | Arbitra una partita persona contro Massima e registra ogni carta giocata con quello che vedeva chi l'ha giocata. |
| `crates/lab/src/riga.rs` | Il formato `human_decision_v1`: una riga CSV per carta giocata. |
| `crates/lab/src/valida.rs` | Il validatore: rigioca ogni partita dall'ordine del mazzo e scarta, spiegando perché, ogni partita che non torna. |
| `crates/lab/src/wasm.rs` | Le funzioni che il sito chiama: lo stesso codice compilato in WebAssembly, senza dipendenze esterne. |
| `crates/lab/web/` | Il sito: `index.html`, `gioco.js`, `worker.js`, `config.js` e `lab.wasm` già compilato. |
| `crates/lab/costruisci.sh` | Ricompila `lab.wasm` dopo una modifica al motore. |

## Il motore nel sito

Massima nel sito è quella dell'app alla precisione "Massima": ISMCTS potenziato a 20.000
iterazioni, deduzione delle carte avversarie con 1.000 mani (`ai::evaluate_with_log`),
finale esatto. Gira in un Web Worker, quindi la pagina non si blocca mentre pensa.

Tempo per mossa misurato in WebAssembly (Node 22, una partita intera): **0,15–0,17 s di media,
massimo 0,3 s**, contro circa 0,12 s del codice nativo. La pagina aspetta comunque almeno 0,7 s
prima di mostrare la carta di Massima, solo per far vedere la mossa.

Il mazzo si mescola con un generatore portabile: dallo stesso seme esce lo stesso mazzo nel
browser e nel codice nativo. Le scelte di Massima invece usano `SmallRng`, che a 32 bit
(WebAssembly) è un generatore diverso da quello a 64 bit: l'algoritmo è identico, ma con lo
stesso seme le simulazioni casuali non coincidono carta per carta.

## Il formato `human_decision_v1`

Una riga per ogni carta giocata, sia dalla persona sia da Massima, così ogni partita si può
ricostruire per intero. Le carte sono numeri da 0 a 39: seme = carta / 10 (0 Denari, 1 Coppe,
2 Spade, 3 Bastoni), valore = carta % 10 (0 Asso, 1 Due, 2 Tre, …, 6 Sette, 7 Fante,
8 Cavallo, 9 Re). Gli elenchi di carte sono separati da spazi.

| Colonna | Significato |
|---|---|
| `schema` | sempre `human_decision_v1` |
| `partita` | codice casuale della partita |
| `giocatore` | codice casuale del browser (mai nome, email o IP) |
| `sessione` | codice casuale della visita al sito |
| `regole` | `briscola-1v1-v1` |
| `fonte` | da `config.js`, per esempio `sito-laboratorio` |
| `origine` | `umano` o `massima` |
| `mossa` | da 0 a 39 |
| `presa` | da 0 a 19 |
| `posto`, `apre` | posto di chi gioca e di chi apre la presa (0 o 1) |
| `briscola` | la carta scoperta in fondo al mazzo |
| `mano` | la mano di chi gioca, prima di giocare |
| `tavolo` | la carta già sul tavolo, vuota se chi gioca apre |
| `mazzo_rimasto` | carte ancora nel mazzo |
| `punti_miei`, `punti_avversario` | punti già fatti (si possono ricavare dalla storia) |
| `legali` | carte giocabili (nella briscola, tutta la mano) |
| `scelta` | la carta giocata |
| `storia` | tutte le carte giocate prima, in ordine |
| `consigli_visibili` | sempre `false`: il sito non dà consigli |
| `tempo_ms` | tempo della decisione della persona, misurato dalla pagina; vuoto per Massima |
| `qualita` | `exact`: lo stato viene dall'arbitro, non da una lettura dello schermo |
| `mazzo` | ordine completo del mazzo, solo sulla mossa 0 |
| `motore` | versione e impostazioni di Massima |

Il server può aggiungere colonne (per esempio `codice_ip`): il validatore le ignora.

## Il validatore

```
cargo run --release -p briscola-lab --bin lab-valida -- partite.csv [altre.csv ...]
```

Per ogni partita controlla: 40 carte, nessuna mossa mancante o ripetuta, mazzo con le 40
carte una volta sola, turno, chi apre, briscola, mano vera, carte giocabili, carta sul tavolo,
carte nel mazzo, punti, storia, nessun consiglio visibile, 120 punti a fine partita.
Una partita inviata due volte identica conta una volta. Ogni partita scartata ha il suo motivo.
Stampa anche persone diverse, decisioni umane (e quante obbligate, con una sola carta in mano)
e partite vinte, pari e perse dalle persone.

## Mettere online il sito (Cloudflare)

`crates/lab/web/` è un sito statico; `crates/lab/cloudflare/` lo pubblica su Cloudflare
Workers insieme a un piccolo server (`src/index.js`) che salva le partite in un Durable Object
con SQLite. Non c'è nessuna risorsa da creare a mano e tutto sta nel piano gratuito.

| Indirizzo | Cosa fa |
|---|---|
| `POST /api/partita` | Riceve il CSV di una partita finita (intestazione + 40 righe), fa un controllo leggero e lo salva. L'IP diventa subito un codice (`HMAC-SHA256` con il segreto `SALE`, 12 cifre esadecimali): l'IP non viene mai salvato. Una partita già ricevuta non si duplica. Al massimo 200 partite al giorno per rete. |
| `GET /api/conteggio` | Solo numeri: partite, giocatori, reti diverse. |
| `GET /api/esporta?chiave=…` | Tutte le partite in un CSV, con la colonna `codice_ip` davanti. Serve la chiave `CHIAVE_EXPORT`. |

La pubblicazione la fa il workflow GitHub **Laboratorio su Cloudflare**
(`.github/workflows/lab-cloudflare.yml`): a ogni modifica di `crates/lab` su `main`, o a mano
da Actions → Laboratorio su Cloudflare → Run workflow. Ricompila `lab.wasm`, pubblica e
imposta i segreti. Segreti del repository (Settings → Secrets and variables → Actions):

| Segreto | Da dove |
|---|---|
| `CLOUDFLARE_API_TOKEN` | Cloudflare → My Profile → API Tokens → Create Token → modello "Edit Cloudflare Workers" |
| `CLOUDFLARE_ACCOUNT_ID` | Cloudflare → Workers & Pages → Account ID nella colonna di destra |
| `LAB_SALE` | un testo casuale lungo, da non cambiare più (altrimenti gli stessi IP cambiano codice) |
| `LAB_CHIAVE_EXPORT` | un'altra parola segreta, per scaricare le partite |

L'indirizzo del sito compare nel riepilogo del workflow (`https://briscola-lab.<nome>.workers.dev`).

Prova in locale: `cd crates/lab/cloudflare && npm ci && npx wrangler dev` con un file
`.dev.vars` che contiene `SALE=...` e `CHIAVE_EXPORT=...`. Senza server basta
`python3 -m http.server -d crates/lab/web 8000`: le partite restano nel browser.

## Privacy

La pagina chiede il consenso prima della prima partita. Non chiede nome né email: il
giocatore è un codice casuale salvato nel browser. Gli IP, se il server li vede, vanno
trasformati in codici prima di salvarli e mai pubblicati. I dati raccolti non vanno nel repo.

## Limiti noti

- La partita è nel browser della persona: chi sa usare gli strumenti del browser potrebbe
  leggere la memoria del motore e vedere le carte di Massima. Per un laboratorio fra persone
  invitate è accettabile; nell'analisi conviene guardare le persone con risultati anomali.
- Durante la partita la pagina non mostra i punti, come al tavolo vero: si vedono a fine partita.
- `tempo_ms` parte quando le carte della persona diventano cliccabili, dopo la pausa che
  mostra la presa appena chiusa.

## Passi successivi

1. Mettere online il sito e cominciare la raccolta.
2. Analisi delle partite vere: carichi, tagli, assi e tre tenuti, errori per fase della partita.
3. Bot umano allenato sui dati veri, con test su persone mai viste in allenamento.
4. Validazione del bot umano su 100.000 partite: il riferimento è la percentuale di Massima
   contro il bot umano.
