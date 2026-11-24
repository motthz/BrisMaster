## Tavoli diversi dal solito e bassa risoluzione

- Le carte dell'avversario non vengono più segnate come giocate da me: sul nuovo banco con 15 disposizioni del tavolo le giocate date al giocatore sbagliato passano da 15 (0.6.17) e 12 (0.6.15) a 0.
- Briscola messa in piedi accanto al mazzo, sul mio lato o su quello dell'avversario, e mazzo a destra: riconosciuti.
- Carte affiancate a metà tavolo, leggermente sovrapposte o storte: seguite. Una carta con un angolo coperto da un'altra si legge dalla parte che si vede; le carte coperte contano meno finché non si vedono bene.
- Bassa risoluzione: una carta letta per sbaglio in mano prima della distribuzione non viene più pescata dopo la prima presa.
- Chi apre la partita si decide alla prima lettura sicura, ma solo se la mano si legge pulita.

## Risultati di verifica

- Banco varianti (60 partite generate): partite perfette 39/60 (0.6.17: 24/56, 0.6.15: 23/56), nessuna partita fallita (prima 4).
- Partite registrate rimpicciolite al 60%: perfette 4/6 (0.6.15: 0/6), conferme da 9 a 2.
- Invariati partita per partita rispetto alla 0.6.17: 20 partite registrate (14 perfette), spoglio 20/20, display 19/20, 17 partite estreme, 129 prove automatiche superate.

## Unito in questa versione

- Il laboratorio per importare e validare partite (`crates/lab`), il simulatore di giocatori umani (`humansim`) e i nuovi agenti del simulatore.

## Ancora aperto

- Carte molto sovrapposte: l'app chiede conferma più spesso (1 partita perfetta su 4), perché della carta si vede troppo poco.
- I numeri vengono da partite di prova e generate, non da partite reali annotate.
