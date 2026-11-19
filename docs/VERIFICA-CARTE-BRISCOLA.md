# Verifica delle carte di Briscola

Nella schermata Briscola, seleziona una delle due percentuali in basso a destra.
La finestra mostra le carte visibili al momento dell'apertura, i campioni
dell'archivio locale condiviso e le giocate della partita in corso. Per acquisire
un'altra lettura, usa **Aggiorna dal tavolo**. Lo storico mostra il ritaglio
associato alla giocata quando il lettore lo ha acquisito.

- **Salva etichetta** aggiorna il campione nella memoria locale per il
  riconoscimento successivo.
- **Conferma** verifica l'identità di una giocata nello storico. Se cambi
  carta, l'app verifica che possa rigiocare le azioni successive a partire
  dalla stessa posizione. Le prese senza un evento registrato sono segnalate
  nella lista.
- **Esporta immagini e dati** crea nella cartella BrisMaster una cartella
  con PNG, `indice.json` e una copia della memoria `carte_viste.json`.
  L'esportazione non avvia un trasferimento di rete.

## Confronto dei consigli

```sh
cargo test --release -p brismaster audit_consiglio_presa_oscurata -- --nocapture
```

Il test confronta 12 turni riproducibili dello stesso motore con storico
completo e con le identità delle due carte della presa precedente oscurate.
Il punteggio e il tavolo rimangono uguali in entrambe le valutazioni. Valuta
la scelta della seconda simulazione usando le stime della prima.

## Prova su siti nuovi in Windows

Su ogni sito, partire con un profilo Windows senza campioni del sito e
seguire una partita da 20 prese con la lettura dello schermo attiva. Controllare
la mano iniziale, la briscola, le giocate e le ultime prese dopo l'esaurimento
del tallone. Segnare per ogni turno le carte lette correttamente, la mossa
consigliata e gli interventi richiesti. Conservare la sequenza consecutiva più
lunga di turni senza consiglio, quando era il proprio turno e c'erano almeno
due carte in mano. Ripetere sulla versione pubblicata con la stessa
registrazione quando disponibile. I banchi sintetici e la compilazione
incrociata per Windows coprono scenari diversi dalla cattura live.
