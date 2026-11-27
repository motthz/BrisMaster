# Briscola sotto il tallone su tavoli chiari

La PR #81 ha isolato quattro partite in cui il motore sbaglia o non acquisisce la briscola iniziale. Su panno chiaro, il bordo bianco del dorso e la carta scoperta formano un unico contorno. La nuova ricerca localizza l’interno del dorso, cerca il bordo esterno della carta ai lati e confronta solo la parte visibile con i riferimenti, nei due versi. La ricerca richiede tre carte intere nella mano, il centro del tavolo libero da giocate riconosciute, un dorso delle dimensioni attese e un riscontro sul bordo chiaro. Un risultato deve migliorare la somiglianza della lettura precedente.

## Protocollo

Riferimento `7a8db6d` (PR #81). I due binari eseguono le stesse distribuzioni con `BRIS_SENZA_INTERVENTI=1`. Per completare un replay, devono coincidere briscola, 20 prese, punti, carte giocate, lunghezza del tallone, mano finale e ordine delle giocate per giocatore. I ritardi contano le verifiche intermedie in cui il motore è rimasto indietro. Le attribuzioni errate contano le carte registrate al giocatore sbagliato durante il replay, anche quando l’errore viene corretto in seguito.

Sugli otto casi della PR #81, attivare le zone sintetiche delle giocate. Gli altri controlli usano le zone automatiche. La rete resta attiva in entrambe le versioni. La prova è sintetica e calibra ruoli di giocata su layout fissi. Il test ordinario `reads_opening_trump_under_a_red_back_on_light_felt` controlla identità, confidenza e il caso con il solo dorso e nessuna briscola scoperta.

## Risultati

| Gruppo | Completate | Controlli in ritardo | Attribuzioni errate |
|---|---:|---:|---:|
| Matrice della PR #81 | 4/8 → 5/8 | 169 → 117 | 1 → 6 |
| Controlli su altri layout | 4/6 → 4/6 | 40 → 40 | 8 → 8 |
| Verifica estesa con zone automatiche | 1/10 → 6/10 | 332 → 106 | 0 → 9 |
| Totale | 9/24 → 15/24 | 541 → 263 | 9 → 23 |

Sono 24 esecuzioni appaiate, con due distribuzioni di app2 su tavolo chiaro ripetute con e senza zone calibrate. Tutte le partite completate dal riferimento si completano anche nella candidata. Sulle otto distribuzioni della PR #81, le letture con carta e ruolo corretti passano da 904/960 a 905/960 e restano 2 inversioni. Sulle stesse otto, la 0.6.20 aveva completato 2/8 (25,0%), la PR #81 4/8 (50,0%) e questa candidata 5/8. Per la 0.6.20 valgono i risultati appaiati già registrati in [Validazione zone e rete](VALIDAZIONE-ZONE-RETE.md).

La suite ordinaria ha 137 test superati, 25 ignorati e zero fallimenti.

Il conteggio delle attribuzioni errate aumenta. Alcuni replay che prima restavano fermi a zero prese ora entrano in partita ed espongono errori di riconoscimento e recupero. Nel caso tavolo chiaro con app2, seme 91000, la prima divergenza è alla quinta presa. Il Cavallo di Bastoni viene letto come Cavallo di Spade, con margine debole. Il banco registra cinque attribuzioni errate prima di fermarsi a 18 prese. Portare la soglia d’ingresso della rete sulle carte intere a 0,85 o 0,90, o disattivarla nel test, lascia invariato questo esito. I due replay mobili con app3 conservano i problemi della PR #81.

Esiti modificati, nel formato `completata (1 o 0) / attribuzioni errate / controlli in ritardo`.

| Gruppo | Caso | Prima | Dopo |
|---|---|---:|---:|
| Matrice della PR #81 | app2 tavolo-chiaro · 91000 | 0 / 0 / 40 | 0 / 5 / 28 |
| Matrice della PR #81 | app2 tavolo-chiaro · 91037 | 0 / 0 / 40 | 1 / 0 / 0 |
| Verifica estesa con zone automatiche | app0 tavolo-chiaro · 91000 | 0 / 0 / 40 | 1 / 0 / 0 |
| Verifica estesa con zone automatiche | app0 tavolo-chiaro · 91037 | 0 / 0 / 40 | 1 / 0 / 1 |
| Verifica estesa con zone automatiche | app0 tavolo-chiaro · 91074 | 0 / 0 / 40 | 1 / 0 / 0 |
| Verifica estesa con zone automatiche | app2 tavolo-chiaro · 91000 | 0 / 0 / 40 | 0 / 5 / 28 |
| Verifica estesa con zone automatiche | app2 tavolo-chiaro · 91037 | 0 / 0 / 40 | 1 / 0 / 0 |
| Verifica estesa con zone automatiche | app2 tavolo-chiaro · 91074 | 0 / 0 / 40 | 1 / 0 / 1 |
| Verifica estesa con zone automatiche | app2 legno-piccole · 91000 | 0 / 0 / 40 | 0 / 4 / 28 |
| Verifica estesa con zone automatiche | app2 legno-piccole · 91037 | 0 / 0 / 26 | 0 / 0 / 22 |

## Riproduzione

Compilare il riferimento e la candidata con `cargo test --locked --release -p brismaster --no-run`. Lanciare i seguenti comandi per ciascuno dei due eseguibili indicati da Cargo, assegnandone il percorso a `BIN`. L’interruttore `BRIS_SENZA_BORDO_TALLONE=1` permette anche di ripetere il controllo nella sola candidata, disattivando il nuovo rilevamento nel binario dei test.

```bash
export BRIS_SENZA_INTERVENTI=1 BRIS_TEST_MISURA_RUOLI=1
BRIS_TEST_ZONE=1 SITO=app-mobile SITI_MAZZI=app0,app3 SITI_SEMI=2 "$BIN" siti_possibili --ignored --nocapture
BRIS_TEST_ZONE=1 SITO=tavolo-chiaro SITI_MAZZI=app2 SITI_SEMI=2 "$BIN" siti_possibili --ignored --nocapture
BRIS_TEST_ZONE=1 VARIANTE=affiancate VARIANTI_SEMI=2 "$BIN" tavoli_varianti --ignored --nocapture
VARIANTE=normale VARIANTI_SEMI=2 "$BIN" tavoli_varianti --ignored --nocapture
VARIANTE=briscola-in-piedi VARIANTI_SEMI=2 "$BIN" tavoli_varianti --ignored --nocapture
SITO=tavolo-scuro SITI_MAZZI=app2 SITI_SEMI=2 "$BIN" siti_possibili --ignored --nocapture
SITO=tavolo-chiaro SITI_MAZZI=app0,app2 SITI_SEMI=3 "$BIN" siti_possibili --ignored --nocapture
SITO=dorsi-blu SITI_MAZZI=app1 SITI_SEMI=2 "$BIN" siti_possibili --ignored --nocapture
SITO=legno-piccole SITI_MAZZI=app2 SITI_SEMI=2 "$BIN" siti_possibili --ignored --nocapture
```
