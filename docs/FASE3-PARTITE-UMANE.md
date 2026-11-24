# Massima 2.0, fase 3: partite vere contro Massima

Scopo della fase 3: raccogliere partite di persone vere contro Massima, **senza consigli**,
e con queste costruire un bot che giochi come le persone (il "bot umano"), da usare poi come
avversario nei test delle fasi 4 e 5. Il bot umano si costruisce **solo su dati reali**:
nessun bot umano provvisorio. Il prototipo HumanSim della PR #66 non è calibrato su persone
e non viene usato come bot umano.

Obiettivo di raccolta deciso: **50 persone, 2.000 partite**.

**Cambio di piano (24/11/2026):** le partite arrivano dal sito che Motthz ha già; il sito e il
server Cloudflare preparati in un primo momento sono stati tolti. Restano il formato, il
validatore e il tavolo di prova, su cui si appoggerà l'importazione dei dati di quel sito.
Niente partite ricavate in automatico da video di tavoli online.

## Cosa c'è nel repo

| Percorso | Cosa fa |
|---|---|
| `crates/lab/src/tavolo.rs` | Tavolo di prova: arbitra una partita contro Massima e scrive le righe nel formato (usato nei test). |
| `crates/lab/src/riga.rs` | Il formato `human_decision_v1`: una riga CSV per carta giocata. |
| `crates/lab/src/bin/lab-importa-sito.rs` | Converte l'esportazione JSON del sito di Motthz nel formato, rigiocando ogni partita. |
| `crates/lab/src/valida.rs` | Il validatore: rigioca ogni partita dall'ordine del mazzo e scarta, spiegando perché, ogni partita che non torna. |

## Il formato `human_decision_v1`

Una riga per ogni carta giocata, sia dalla persona sia da Massima, così ogni partita si può
ricostruire per intero. Le carte sono numeri da 0 a 39: seme = carta / 10 (0 Denari, 1 Coppe,
2 Spade, 3 Bastoni), valore = carta % 10 (0 Asso, 1 Due, 2 Tre, …, 6 Sette, 7 Fante,
8 Cavallo, 9 Re). Gli elenchi di carte sono separati da spazi.

| Colonna | Significato |
|---|---|
| `schema` | sempre `human_decision_v1` |
| `partita` | codice casuale della partita |
| `giocatore` | codice del giocatore (mai nome, email o IP) |
| `sessione` | codice della sessione di gioco |
| `regole` | `briscola-1v1-v1` |
| `fonte` | da dove viene la partita |
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
| `consigli_visibili` | `false` per le partite senza consigli (le altre sono scartate) |
| `tempo_ms` | tempo della decisione della persona, se misurato; vuoto per Massima |
| `qualita` | `exact`: lo stato viene dall'arbitro, non da una lettura dello schermo |
| `mazzo` | ordine completo del mazzo, solo sulla mossa 0 |
| `motore` | versione e impostazioni di Massima |

Colonne in più (per esempio `codice_ip`) sono ignorate dal validatore.

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

## Importare dal sito di Motthz

```
cargo run --release -p briscola-lab --bin lab-importa-sito -- esportazione.json > partite.csv
cargo run --release -p briscola-lab --bin lab-valida -- partite.csv
```

L'esportazione del sito ha, per ogni partita, l'ordine del mazzo (`state.order`, stessa
convenzione di `Game::from_order`), chi apre (`state.first`) e tutte le 40 carte giocate
(`state.moves`): la persona siede al posto 0 e la partita si rigioca per intero, con le mani
vere di entrambi. Il tempo di ogni carta della persona viene da `turns[].elapsed_ms`
(misurato dal server fra due richieste: comprende l'attesa della mossa di Massima, e una
partita lasciata a metà dà tempi di ore). Le partite di verifica (`cohort` diverso da
`human`) sono saltate. Il CSV aggiunge in fondo le colonne `codice_ip` e `consenso`.
I dati non vanno nel repo: stanno nella cartella del progetto.

Prima esportazione (24/11/2026): 75 partite, 73 di persone, tutte valide; 9 browser,
11 codici IP; le persone vincono 20, pareggiano 1, perdono 52 (27%). Un solo browser ha
35 partite su 73.

## Privacy

Il giocatore è sempre un codice, mai nome, email o IP (gli IP vanno trasformati in codici
prima di arrivare qui). I dati raccolti non vanno nel repo.

## Passi successivi

1. Raccogliere altre partite dal sito (obiettivo 2.000 partite, 50 persone).
2. Analisi delle partite vere: carichi, tagli, assi e tre tenuti, errori per fase della partita.
3. Bot umano allenato sui dati veri, con test su persone mai viste in allenamento.
4. Validazione del bot umano su 100.000 partite: il riferimento è la percentuale di Massima
   contro il bot umano.
