## Briscola a bassa risoluzione

- Il rilevatore usa la maschera a risoluzione originale nelle regioni piccole per preservare i bordi delle carte e mantiene un secondo passaggio per la briscola coperta.
- Sui tavoli molto chiari, posizione e misura della mano guidano il recupero delle giocate al centro. Se una parte della sagoma manca, BrisMaster prova anche la larghezza appresa dalle altre carte. La geometria della mano viene conservata brevemente per coprire le ultime prese.
- I gruppi di riquadri quasi identici con debole somiglianza alle carte di riferimento vengono scartati. La soglia di conferma delle identità resta la stessa.

## Verifiche

- Nella partita simulata su fondo chiaro con carte alte 40 px, la 0.6.12 completa 20 prese con punteggio corretto, 37 giocate identificate su 40 e zero interventi. La 0.6.11 non superava l’inizio della partita.
- Sul banco di fotogrammi, 48/48 identità corrette in automatico a 40 px e 46/48 a 32 px; zero falsi candidati nelle otto schermate con riquadri colorati ripetuti.
- 117 test della suite app passati e 21 partite di regressione con esiti, interventi e ritardi invariati. Nei test sequenziali su Linux, il recupero comporta più lavoro sui fotogrammi molto piccoli. La latenza della cattura dal vivo in Windows resta da misurare.

## Installazione

Scarica **BrisMaster-Setup.exe** da **Assets** per installare o aggiornare l’app in Windows. **BrisMaster.exe** è la versione portatile.
