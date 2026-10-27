# BrisMaster

Assistente di briscola per Windows. Inserisci tu come sta andando la partita (la briscola, le carte che peschi, quelle giocate dall'avversario) e il motore ti dice quale carta giocare, con la probabilità di vittoria di ogni alternativa.

![BrisMaster: l'Assistente](docs/assistente.png)

> Molte piattaforme di gioco online vietano nei loro regolamenti l'uso di programmi di aiuto durante le partite contro altre persone: controlla quelle della piattaforma su cui giochi.

## Installazione

1. Scarica `BrisMaster.exe` dall'[ultima release](https://github.com/motthz/BrisMaster/releases/latest).
2. Mettilo in una cartella tua (per esempio `Documenti\BrisMaster`) e avvialo.
3. Al primo avvio Windows SmartScreen può avvisare che l'app non è firmata: *Ulteriori informazioni → Esegui comunque*.

Gli aggiornamenti sono automatici: all'avvio l'app controlla le release di questo repository e, se ce n'è una più recente, mostra il pulsante **Installa**. L'app scarica la nuova versione, sostituisce l'eseguibile e si riavvia.

## Cosa fa

- **Assistente**: inserisci a mano la briscola, chi comincia, le tue carte, le carte che peschi e quelle giocate dall'avversario. Per ogni tua mossa il motore indica la carta da giocare, la probabilità di vittoria di ogni alternativa e una spiegazione a parole (prendere o lasciare, tagliare, quante briscole e carichi sono ancora fuori, quanti punti ti mancano). Le carte si inseriscono con un clic sulla griglia o da tastiera (`3c`, `AD`, `Rs`, `cavallo bastoni`, poi Invio), e ogni passo si annulla con Ctrl+Z. L'app tiene il conto delle carte uscite, dei punti e del mazzo, e sa da sola a chi va la briscola scoperta all'ultima pescata.
- **Carte uscite**: tabella delle carte già giocate e dei punti ancora in gioco.

## Come ragiona il motore

Il motore usa il **Monte Carlo determinizzato** (PIMC):

1. Dalle carte che conosce (la propria mano, la briscola, le carte uscite) genera centinaia di distribuzioni possibili delle carte coperte.
2. In ogni distribuzione prova ciascuna carta della mano e simula il resto della partita. Nelle simulazioni entrambi i giocatori vedono le carte di quella distribuzione: chi apre prevede la risposta, chi risponde prende quando conviene e non spreca le briscole. Quando nel mazzo restano due carte la simulazione si ferma e il resto si risolve in modo esatto.
3. Quando nel mazzo restano otto carte o meno risolve ogni distribuzione in modo esatto con alfa-beta.
4. Gioca la carta con la probabilità di vittoria più alta (a parità, quella con più punti attesi).

Nell'Assistente la precisione di serie (Massima) usa invece l'**ISMCTS potenziato**: una ricerca ad albero sugli insiemi di informazione che, a ogni iterazione, sceglie una distribuzione delle carte coperte, scende nell'albero delle mosse già provate e finisce la partita con le stesse simulazioni del motore. Con 20.000 iterazioni (circa 40 ms a mossa) batte il motore a 3000 mondi nel 53% delle partite. Le precisioni Veloce e Normale usano il motore a 1000 e 3000 mondi.

Il motore vede solo quello che vedrebbe un giocatore: le simulazioni non sbirciano mai la mano avversaria.

## Test massivi

Il programma `briscola-sim` gioca decine di migliaia di partite tra il motore e altri giocatori. Ogni mazzo si gioca due volte a posti invertiti, così la fortuna delle carte si compensa. Ogni risultato ha il suo intervallo di confidenza al 95%, calcolato sulle coppie di partite dello stesso mazzo.

```
# rapporto completo: 17 stili di avversario, contro se stesso, contro la versione precedente
cargo run --release -p briscola-sim -- report --deals 5000 --out bench/RAPPORTO.md
# un solo confronto, anche con una versione rilasciata
cargo run --release -p briscola-sim -- --player engine:1000 --opponent v0.1.1/engine:1000 --deals 5000
# pannello (aggiungi --extended per tutti gli stili)
cargo run --release -p briscola-sim -- --panel --deals 10000 --out tabella.md
```

L'ultimo rapporto completo è in [bench/RAPPORTO.md](bench/RAPPORTO.md).

**Quante partite servono.** `briscola-sim calibrate --deals 10000` fa giocare il motore contro se stesso: il risultato giusto è il 50%, e il numero di partite è sufficiente quando da lì in poi il punteggio resta entro un punto dal 50% e l'intervallo di confidenza è largo al più ±1 punto. Con meno partite anche due giocatori identici sembrano diversi.

**Controllo di regressione.** Su ogni pull request la CI gioca 10.000 partite tra il motore nuovo e quello dell'ultima release (job *regressione*) e fallisce se il nuovo è significativamente più debole. Dopo ogni release va aggiunta la nuova versione come riferimento: una riga in `crates/sim/Cargo.toml` e una voce in `crates/sim/src/agents.rs` (vedi il commento sulla macro `legacy!`).

Rapporto completo sulla v0.1.1 (livello Difficile), 10.000 partite per avversario, 260.000 in tutto. Punteggio: vittoria 1, pareggio ½; il 50% è la parità. Tabelle complete in [bench/RAPPORTO.md](bench/RAPPORTO.md).

| Avversari | Punteggio |
|---|---:|
| Stili umani (casuale, euristico, aggressivo, risparmiatore, briscolone, contacarte, principiante, rispondi di seme, butta-carichi, mai briscola) | 68% – 93% |
| Motore più debole e ISMCTS fino a 2000 iterazioni | 55% – 60% |
| ISMCTS 5000, 10.000 e 20.000 iterazioni | 54,4%, 53,3%, 51,5% |
| Varianti del motore stesso e versione precedente | 48% – 51% |

13 avversari su 24 sono sopra il 55% con certezza statistica.

Motore attuale, livello Difficile, 4.000 partite per avversario, tra parentesi la v0.1.1 (dettagli in [bench/RISULTATI.md](bench/RISULTATI.md)):

| Avversario | % vittorie | IC 95% |
|---|---:|---|
| Casuale | 94,4% (92,5) | 93,6 – 95,0 |
| Euristico | 70,2% (70,5) | 68,8 – 71,6 |
| Aggressivo | 80,7% (73,5) | 79,4 – 81,8 |
| Risparmiatore | 73,2% (72,5) | 71,8 – 74,5 |
| Briscolone | 75,3% (74,8) | 74,0 – 76,7 |
| Contacarte | 67,3% (67,3) | 65,8 – 68,7 |
| ISMCTS (2000 iterazioni) | 58,7% (54,1) | 57,2 – 60,2 |
| Motore livello Medio | 52,6% (53,8) | 51,0 – 54,1 |
| Motore della v0.1.1 (Difficile) | 52,6% su 20.000 partite | 51,9 – 53,3 |

Un limite importante: la briscola dipende molto dalle carte che arrivano. Contro avversari deboli o medi il motore vince nettamente più della metà delle partite, ma contro un giocatore forte quanto lui nessun algoritmo può garantire più del 50%: due copie dello stesso motore vincono per forza il 50% a testa.

## Struttura

| Cartella | Contenuto |
|---|---|
| `crates/core` | Regole, motore, avversari di riferimento, statistiche |
| `crates/sim` | Banco di prova per le simulazioni massive |
| `crates/app` | App Windows (egui) e aggiornamento automatico |

## Pubblicare una nuova versione

1. Aggiorna `version` in `Cargo.toml` (per esempio `0.2.0`).
2. Fai push su `main`.

GitHub Actions compila `BrisMaster.exe` su Windows, crea il tag `v0.2.0` e pubblica la release; le app già installate si aggiornano da sole. Il workflow si può anche lanciare a mano da *Actions → Release → Run workflow*.
