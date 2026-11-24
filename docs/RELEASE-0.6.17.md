## Riconoscimento delle carte al primo colpo

- La rete che riconosce le carte guarda ogni carta due volte, anche capovolta, e fa la media delle due risposte. Le carte italiane sono quasi simmetriche, quindi è una seconda opinione gratuita: meno carte lette giuste ma con poca sicurezza, e quindi meno conferme. Il tempo di lettura non cambia in modo visibile.
- Inizio partita: un pezzo del sito letto per sbaglio come carta in mano, prima che arrivino le tre carte vere, non entra più nella mano. Prima l'app deduceva una presa mai giocata e perdeva il passo per tutta la partita.

## Risultati di verifica (confronto con la 0.6.16, stessi banchi)

- 20 partite registrate sul tavolo dell'utente: partite perfette da 8 a 14, conferme chieste da 14 a 6, carte scritte a mano da 10 a 3. La partita u13 passa da 8 carte scritte a mano a 1. Nessuna partita peggiora.
- 8 mazzi mai visti: carte giuste dal 74,3% al 74,9%; carte sbagliate date per sicure da 85 a 75.
- Invariati: spoglio 20/20, display 19/20 (identici partita per partita), 17 partite estreme tutte seguite, 129 prove automatiche superate.
- Tutti gli esperimenti, compresi quelli scartati, sono descritti con i numeri in `docs/SPERIMENTAZIONE-RICONOSCIMENTO.md`.

## Ancora aperto

- Le carte dell'avversario che sbiadiscono a fine presa restano il primo motivo di carte non viste. I due tentativi provati ritrovano le carte, ma trovano anche ombre e fanno più errori: non sono entrati.
- I numeri vengono da partite di prova e generate, non da partite reali annotate.
