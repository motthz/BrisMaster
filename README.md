# BrisMaster

Allenatore e assistente di briscola per Windows. Giochi contro un motore forte che ti mostra la mossa migliore con la probabilità di vittoria di ogni carta, a fine partita analizza tutte le tue scelte, e in modalità **Assistente** ti consiglia la mossa in una partita che stai giocando, a partire dalle carte che inserisci tu.

![BrisMaster: partita contro il motore con i consigli](docs/screenshot.png)

![BrisMaster: modalità Assistente](docs/assistente.png)

> Molte piattaforme di gioco online vietano nei loro regolamenti l'uso di programmi di aiuto durante le partite contro altre persone: controlla quelle della piattaforma su cui giochi.

## Installazione

1. Scarica `BrisMaster.exe` dall'[ultima release](https://github.com/motthz/BrisMaster/releases/latest).
2. Mettilo in una cartella tua (per esempio `Documenti\BrisMaster`) e avvialo.
3. Al primo avvio Windows SmartScreen può avvisare che l'app non è firmata: *Ulteriori informazioni → Esegui comunque*.

Gli aggiornamenti sono automatici: all'avvio l'app controlla le release di questo repository e, se ce n'è una più recente, mostra il pulsante **Installa**. L'app scarica la nuova versione, sostituisce l'eseguibile e si riavvia.

## Cosa fa

- **Partita contro il computer** a tre livelli: Facile (regole pratiche), Medio (motore con 150 simulazioni per mossa), Difficile (motore con 1000 simulazioni per mossa).
- **Allenamento**: sotto ogni carta della tua mano compaiono la probabilità di vincere la partita e i punti attesi giocandola; la carta consigliata è bordata di verde.
- **Analisi a fine partita**: ogni tua scelta viene rivalutata con 2000 simulazioni e classificata come ottima, imprecisione o errore, con la mossa migliore indicata.
- **Assistente**: inserisci a mano la briscola, chi comincia, le tue carte, le carte che peschi e quelle giocate dall'avversario. Per ogni tua mossa il motore indica la carta da giocare, la probabilità di vittoria di ogni alternativa e una spiegazione a parole (prendere o lasciare, tagliare, quante briscole e carichi sono ancora fuori, quanti punti ti mancano). Le carte si inseriscono con un clic sulla griglia o da tastiera (`3c`, `AD`, `Rs`, `cavallo bastoni`, poi Invio), e ogni passo si annulla con Ctrl+Z. L'app tiene il conto delle carte uscite, dei punti e del mazzo, e sa da sola a chi va la briscola scoperta all'ultima pescata.
- **Carte uscite**: tabella delle carte già giocate e dei punti ancora in gioco, per allenarti a contare.

## Come ragiona il motore

Il motore usa il **Monte Carlo determinizzato** (PIMC):

1. Dalle carte che conosce (la propria mano, la briscola, le carte uscite) genera centinaia di distribuzioni possibili delle carte coperte.
2. In ogni distribuzione prova ciascuna carta della mano e simula il resto della partita.
3. Quando nel mazzo restano quattro carte o meno risolve la partita in modo esatto con alfa-beta.
4. Gioca la carta con la probabilità di vittoria più alta (a parità, quella con più punti attesi).

Il motore vede solo quello che vedrebbe un giocatore: le simulazioni non sbirciano mai la mano avversaria.

## Test massivi

Il programma `briscola-sim` gioca migliaia di partite tra il motore e avversari di riferimento. Ogni mazzo si gioca due volte a posti invertiti, così la fortuna delle carte si compensa. Le percentuali sono accompagnate dall'intervallo di confidenza al 95% (Wilson).

```
cargo run --release -p briscola-sim -- --panel --deals 10000 --out tabella.md
cargo run --release -p briscola-sim -- --player engine:1000 --opponent ismcts:2000 --deals 5000
```

Risultati del livello Difficile, 20.000 partite per avversario (dettagli in [bench/RISULTATI.md](bench/RISULTATI.md)):

| Avversario | % vittorie | IC 95% |
|---|---:|---|
| Casuale | 92,5% | 92,2 – 92,9 |
| Euristico | 70,5% | 69,9 – 71,1 |
| Aggressivo | 73,5% | 72,9 – 74,1 |
| Risparmiatore | 72,5% | 71,9 – 73,1 |
| Briscolone | 74,8% | 74,2 – 75,4 |
| Contacarte | 67,3% | 66,7 – 68,0 |
| ISMCTS (2000 iterazioni) | 54,1% | 53,4 – 54,8 |
| Motore livello Medio | 53,8% | 53,1 – 54,5 |

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
