# Verificare il riconoscimento con un replay

Il banco di prova può leggere fotogrammi e annotazioni locali, senza caricarli
su GitHub. Imposta `BRIS_REPLAY_CARTE` sul percorso del JSON di annotazione e
`BRIS_REPLAY_FRAMES` sulla cartella dei fotogrammi.

```sh
BRIS_REPLAY_CARTE=/percorso/annotazioni.json BRIS_REPLAY_FRAMES=/percorso/fotogrammi \
  cargo test --release --locked -p brismaster replay_carte_annotate -- --ignored --nocapture
```

Il formato delle annotazioni e i ruoli delle carte sono definiti in
`crates/app/src/vision/replay.rs`. Conserva filmato, fotogrammi e annotazioni
nella tua cartella locale.

## Replay di Briscola dall'apertura all'ultima presa

`replay_briscola_annotata` legge una sequenza locale di fotogrammi del tavolo,
riproduce le letture con timestamp espliciti e passa i risultati allo stesso
ingresso dell'assistente usato dall'app. Usa una memoria vuota e procede
senza interventi manuali.

```sh
BRIS_REPLAY_SESSIONE=/percorso/partita.json \
  cargo test --release --locked -p brismaster replay_briscola_annotata -- --ignored --nocapture
```

Su PowerShell si imposta il percorso con
`$env:BRIS_REPLAY_SESSIONE = 'C:\replay\partita.json'`.

Il manifest JSON contiene `version: 1`, `plays` e `frames`. `plays` contiene
40 righe con campi `trick` (1–20), `player` (0 per chi usa l'assistente,
1 per l'avversario) e `card` (ID da 0 a 39, seme × 10 + indice del valore
da 0 a 9). Le righe seguono l'ordine reale delle giocate. Annotale a partire
dalla partita registrata, indipendentemente dal log prodotto da BrisMaster.

`frames` contiene righe con `ms` strettamente crescente e un percorso `image`
riferito alla cartella del manifest. Per una cattura interrotta, usa
`capture_error: true` senza indicare un'immagine. Sono disponibili anche
`geometry_changed: true` dopo un cambio di area catturata e
`queue_delay_ms` per simulare un ritardo al momento del checkpoint. Indica
`check_turn` sul fotogramma immediatamente precedente a ognuna delle 20
giocate dell'utente, con `hand` (lista degli ID visibili in mano), `table`
(ID della carta avversaria se ha aperto la presa, altrimenti `null`) e
`trump` (ID della briscola iniziale). Per esempio, una singola riga di
`frames` potrebbe essere:

```json
{"ms":82400,"image":"frame-0231.png","geometry_changed":true,"check_turn":{"hand":[7,19,24],"table":1,"trump":33}}
```

Il rapporto JSON misura quante delle 40 giocate sono corrette, identificate
con certezza, errate, mancanti o duplicate. Conta separatamente le carte
errate marcate come certe. Per ogni turno dell'utente verifica se si può
calcolare il consiglio partendo da mano, carta in tavola, briscola e carte
delle prese già chiuse coerenti con la partita annotata. Riporta anche il
primo checkpoint problematico e il numero massimo di turni consecutivi
senza una posizione utilizzabile. La misura riguarda la disponibilità
dello stato di gioco. Il calcolo dell'IA e la latenza dei worker Windows
richiedono prove dedicate.

## Stress test delle finestre

```sh
cargo test --release --locked -p brismaster window_changes_during_whole_games -- --ignored --nocapture
```

Si ripetono le stesse partite con finestra stabile, cambio di scala e
posizione, tavolo chiaro con carte nominali di 40 px inserito tra bordi
grigi, perdita di cattura alla chiusura di una presa e combinazione di
tutte le condizioni. I cambi avvengono nelle prime prese, a metà e verso
la fine. Nelle interruzioni viene scartato il fotogramma che mostra la
risposta della presa, poi tornano i fotogrammi successivi. Il test
controlla anche che oscuramento ed errori di cattura da soli non
mutino le carte e il numero di prese.

Il banco simula pixel e tempi riproducibili su Linux. Serve affiancargli
un test Windows del percorso di cattura, comprendente trascinamento tra
monitor a DPI misti, minimizzazione e ripristino, cambio scheda, ritardi
del lettore e riapparizione della finestra. Serve inoltre una partita
completa annotata sul sito dove è stato osservato il calo di copertura.

## Ordine degli interventi e criteri di confronto

1. Riprodurre i guasti su sessioni fisse. Conservare per ogni sessione
   le 40 giocate attese e i 20 checkpoint. Annotare il primo fotogramma
   utile per distinguere un errore visivo da una perdita nel tracciamento.
2. Correggere i guasti che interrompono la partita. Per lo storico,
   una correzione riparte dall'ultimo tratto ricostruibile, successivo a
   eventuali salti o sostituzioni dirette della mano. Dopo una partita
   completa osservata, ciascun giocatore ha 20 giocate, ogni carta appare
   una volta e la somma dei punti è 120. Dopo un intervallo senza
   osservazioni si tengono distinte le giocate verificate e quelle ignote.
3. Isolare il riconoscimento a 40 px sottraendo e aggiungendo gli stessi
   bordi alla stessa immagine. Misurare separatamente gli effetti della
   qualità dell'immagine, degli elementi dell'interfaccia e delle transizioni
   delle finestre prima di cambiare le soglie.
4. Aggiungere la registrazione diagnostica locale su Windows, esplicitamente
   attivata, con ritaglio del tavolo, timestamp ed eventi di cattura. Validare
   su partite annotate tenute fuori dallo sviluppo, su siti e mazzi diversi.

Confrontare riferimento e candidata per singola sessione. Le metriche
vincolanti sono giocate corrette identificate, false certezze, giocate
mancanti, turni con stato utilizzabile, durata del blocco e punteggio
finale per le partite osservabili per intero. Un miglioramento medio può
convivere con il peggioramento di una singola partita. I casi sintetici
stabili e con solo ridimensionamento e spostamento richiedono tutte le
40 giocate, il punteggio esatto e zero duplicati.

## Risultati della prima fase

La 0.6.12 si arresta nel caso `SmallTableWithChrome` con seme 1. Dopo
una sostituzione diretta della mano, lo storico contiene una posizione
per Annulla senza un evento corrispondente. Una correzione successiva
tentava di leggere l'evento all'indice oltre la fine del registro. Il
test `corrections_only_replay_events_after_a_direct_hand_replacement`
verifica che il tratto precedente alla sostituzione rimanga intatto e
che una correzione nel tratto successivo continui a riuscire.

Con la correzione, tutte le dieci sessioni terminano l'esecuzione del
banco. Risultati aggregati per modalità sui semi 1 e 2:

| Modalità | Giocate giuste su 80 | Giuste marcate certe | Sbagliate marcate certe | Turni con stato corretto e utilizzabile su 40 |
| --- | ---: | ---: | ---: | ---: |
| Stabile | 80 | 73 | 0 | 37 |
| Ridimensionamento e spostamento | 80 | 73 | 0 | 37 |
| Tavolo chiaro a 80 px con bordi | 5 | 2 | 30 | 1 |
| Interruzioni a finestra stabile | 73 | 63 | 2 | 13 |
| Tavolo piccolo, bordi e interruzioni | 5 | 4 | 19 | 1 |

Su finestra stabile e con solo spostamento e ridimensionamento si
ottengono tutte le prese con i punteggi reali. Le interruzioni possono
lasciare lo storico incompleto anche quando il motore riprende il turno.
Nei due casi con tavolo piccolo e bordi il primo errore compare alla
seconda presa. Il caso combina risoluzione, bordi, sfondo chiaro e scritte.
La prova a fattori isolati deve verificare il peso di ciascuno.

La suite ordinaria conclude 120 test. Il banco a bassa risoluzione
riproduce le stesse letture e gli stessi candidati aggiuntivi della
0.6.12 voce per voce. La prova precedente su tavolo chiaro a 40 px
completa ancora 20 prese con 37 giocate identificate su 40 e zero
interventi.
Le 17 partite storiche e le quattro di riserva conservano esiti,
copertura, interventi manuali e checkpoint in ritardo caso per caso.

## Verifica del ritaglio dei bordi

Sui due semi di riferimento, a 40 px con bordi scuri uniformi, le giocate corrette passano da 5/80 a 78/80, le false certezze da 19 a zero e i turni utilizzabili da 1/40 a 37/40. Con interruzioni si passa da 5 a 69 giocate corrette, da 19 a una falsa certezza e da 1 a 13 turni utilizzabili. A 80 px con tavolo chiaro, scritte e bordi restano 5/80 e 30 false certezze. La matrice confronta 16 replay sintetici per versione.
