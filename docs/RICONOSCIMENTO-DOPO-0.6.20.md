# Riconoscimento Briscola dopo la 0.6.20

Riferimento: tag `v0.6.20`, commit `158f52f8958fc110963a76e009aceab23ac1a470`.

## Attribuzione di una carta isolata

Con le caselle già apprese, una singola carta riceveva il ruolo della casella
più vicina anche se era lontana da entrambe. Il test
`learned_slots_do_not_move_a_card_from_a_distant_table` crea la stessa sequenza
di letture A, B, A per entrambe le versioni. La 0.6.20 trasforma la giocata
avversaria del secondo tavolo in una propria. La candidata usa la stessa
assegnazione per una carta vicina alla casella appresa. Se il centro dista più
di una dimensione di carta, misurata lungo i due assi, conserva il ruolo del
rilevatore.

La prova isolata riguarda l'assegnazione tra le due caselle di gioco. La
chiave persistente di `tavolo.json` resta la dimensione dell'immagine. Un
cambio di sito con caselle vicine o sovrapposte richiede una prova diversa.
Il recupero di carte mancanti richiede misure su cattura, rilevamento e
aggiornamento della partita.

## Confronto individuale

Applicare le sole due istruzioni di stampa dei test di `assistant.rs` anche
al checkout di riferimento per emettere le righe `PARTITA`. Compilare i due
binari dei test in checkout distinti con
`cargo test --release --locked -p brismaster --no-run`.

```sh
"$BASE" siti_possibili --ignored --nocapture > base.log
"$BASE" tavoli_varianti --ignored --nocapture >> base.log
"$CANDIDATA" siti_possibili --ignored --nocapture > candidata.log
"$CANDIDATA" tavoli_varianti --ignored --nocapture >> candidata.log
python scripts/audit-brisco/compare_appearance_games.py base.log candidata.log
```

Le chiavi sono banco, configurazione, mazzo se applicabile e seme. Il
confronto fallisce su casi mancanti o duplicati e su ogni peggioramento
misurato. Le conferme di una partita interrotta non sono disponibili in
questo banco. Per la copertura dei checkpoint e delle giocate mancanti si
usano anche i replay in [docs/replay](replay/README.md).

I venti replay storici, incluse le varianti spoglio, display e ridotte,
richiedono il corpus `PARTITE` con immagini e `verita.json`. Il repository
contiene schermate di prova e i vecchi risultati aggregati. L'archivio
dell'audit contiene log e CSV senza la sequenza originale. Il filmato
già disponibile è di Scopa. La verifica storica completa della candidata
resta quindi da riprodurre.
