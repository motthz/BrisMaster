## Riconoscimento della briscola

- Migliora l’individuazione delle carte piccole e la separazione dal panno sui tavoli compatti.
- Recupera alcune prese perse durante brevi interruzioni della cattura. Le giocate dedotte restano distinguibili dalle carte confermate visivamente.
- Recupera la nostra apertura quando la carta isolata appare dal lato avversario e il cambio della mano permette di attribuirla.
- Mostra un avviso quando il tavolo visibile e la posizione usata per il consiglio continuano a divergere.

## Risultati di verifica

Su 44 partite sintetiche appaiate, le giocate corrette e confermate salgono da 1.448 a 1.570 su 1.760 rispetto alla 0.6.14 (82,3% → 89,2%). Le conferme errate scendono da 22 a 15 e i turni con consiglio disponibile su una posizione errata da 101 a 86. L’avviso compare in tutti gli 86 casi errati e in 2 dei 707 casi corretti con consiglio disponibile.

Sulle 1.736 giocate visibili, la prima cattura contiene una lettura corretta della carta in 1.667 casi (96,0%). Carta e lato coincidono in 1.572 casi e 73 giocate sono già registrate nella presa corretta. Rimane da misurare la copertura su partite reali annotate. [Dati e metodo](https://github.com/Motthz/BrisMaster/pull/64)

## Prossimi interventi

1. Misurare il tempo dal primo fotogramma utile alla registrazione corretta e alla conferma, distinguendo aperture e risposte.
2. Conservare la prima lettura come ipotesi e confermarla quando il cambio della mano o una successiva apertura fornisce un riscontro indipendente.
3. Riprodurre i due turni del seme 42 in cui il motore vede ancora il tavolo vuoto e verificare il recupero dell’apertura prima del consiglio.
4. Confrontare ogni candidata partita per partita, includendo falsi riconoscimenti, duplicati e consigli corretti disponibili. Estendere la verifica a siti e mazzi indipendenti con partite reali annotate per misurare il traguardo del 90%.
