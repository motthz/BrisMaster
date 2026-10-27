# Risultati dei test massivi

## Motore attuale (rollout a carte scoperte)

Motore: **PIMC con 1000 mondi per mossa**, rollout a carte scoperte che si fermano a 2 carte nel mazzo per risolvere il resto con alfa-beta, risolutore esatto da 8 carte nel mazzo (`EngineConfig::strength`).
Per ogni avversario: 2.000 mazzi giocati due volte a posti invertiti, quindi **4.000 partite**, seme `20261022`. Il "Motore PIMC (150 mondi)" è il nuovo motore a livello *Medio*.

| Avversario | Partite | Vinte | Pari | Perse | % vittorie | IC 95% | Punti medi |
|---|---:|---:|---:|---:|---:|---|---:|
| Casuale | 4000 | 3775 | 20 | 205 | **94.4%** | 93.6% – 95.0% | 85.3 |
| Euristico | 4000 | 2808 | 108 | 1084 | **70.2%** | 68.8% – 71.6% | 66.4 |
| Aggressivo | 4000 | 3226 | 46 | 728 | **80.7%** | 79.4% – 81.8% | 72.9 |
| Risparmiatore | 4000 | 2926 | 84 | 990 | **73.2%** | 71.8% – 74.5% | 67.8 |
| Briscolone | 4000 | 3014 | 84 | 902 | **75.3%** | 74.0% – 76.7% | 69.7 |
| Contacarte | 4000 | 2691 | 115 | 1194 | **67.3%** | 65.8% – 68.7% | 65.0 |
| ISMCTS (2000 iterazioni) | 4000 | 2349 | 113 | 1538 | **58.7%** | 57.2% – 60.2% | 64.0 |
| Motore PIMC (150 mondi) | 4000 | 2103 | 102 | 1795 | **52.6%** | 51.0% – 54.1% | 61.1 |

Contro il motore della v0.1.1 (`classic:1000`), 10.000 mazzi in duplicato (**20.000 partite**), seme `77`: **52,6%** di vittorie (IC 95% 51,9% – 53,3%), 61,1 punti medi.

```
cargo run --release -p briscola-sim -- --panel --deals 2000 --player engine:1000 --seed 20261022 --out tabella.md
cargo run --release -p briscola-sim -- --player engine:1000 --opponent classic:1000 --deals 10000 --seed 77
```

### Come si è arrivati alla configurazione

Ogni variante è stata provata a 200 mondi contro il motore della v0.1.1 a 200 mondi, su 8.000-10.000 partite in duplicato (IC 95% circa ±1 punto).

| Variante | % vittorie |
|---|---:|
| Rollout a carte scoperte, senza mosse casuali | 51,3% |
| + risolutore esatto da 8 carte | 53,3% |
| Solo finale risolto nei rollout (da 4 carte) | 53,1% |
| Carte scoperte + risolutore da 8 + finale nei rollout da 4 | 54,5% |
| Come sopra, finale nei rollout da 2 (metà del tempo) | 54,3% |
| Come sopra, pesi della briscola 8/2 (scelta finale) | 55,0% |
| Pesi della briscola 24/6 | 51,2% |
| Risolutore da 10 carte | 54,0% (3 volte più lento) |

Scartate:

| Variante | % vittorie | Perché |
|---|---:|---|
| Finale con pescate casuali (expectimax sulle carte del mazzo) | 47,7% | corretto rispetto a un riferimento esaustivo, ma gioca peggio |
| Mondi pesati in base alle risposte passate dell'avversario | 41-48% | il modello dell'avversario sbaglia contro un motore |
| Stessi numeri casuali per tutte le mosse candidate | 48,7% | nessun guadagno |
| Solo risolutore da 6, 8 o 10 carte, rollout euristici | 49-50% | nessun guadagno senza rollout migliori |

## Avversari più forti e nuove prove (ottobre 2026, dopo la v0.2.0)

Tutte le righe: 4.000 mazzi giocati due volte a posti invertiti (**8.000 partite**), seme `20261022`. Punteggio: vittoria 1, pareggio ½.

**Nuovo avversario: ISMCTS potenziato** (`ismcts+:N`). La stessa ricerca ad albero dell'ISMCTS, ma con le simulazioni del motore: a carte scoperte, senza mosse casuali, finale risolto in modo esatto da 2 carte nel mazzo. A parità di tempo di calcolo gioca alla pari con il nostro motore, e con più iterazioni lo batte: è l'avversario più forte che il banco di prova sa costruire.

| Partita | Punteggio | IC 95% |
|---|---:|---|
| Motore 1000 mondi contro ISMCTS 20.000 | 54,5% | 53,5 – 55,6 |
| ISMCTS potenziato 3.000 (stesso tempo per mossa) contro motore 1000 mondi | 49,4% | 48,4 – 50,4 |
| Motore 1000 mondi contro ISMCTS potenziato 10.000 | **47,5%** | 46,5 – 48,5 |

**Più mondi.** Il calcolo in più aiuta fino a circa 3000 mondi, poi non più:

| Partita | Punteggio | IC 95% |
|---|---:|---|
| Motore 1000 mondi contro ISMCTS 20.000 | 54,5% | 53,5 – 55,6 |
| Motore 3000 mondi contro ISMCTS 20.000 | **55,6%** | 54,5 – 56,6 |
| Motore 6000 mondi contro ISMCTS 20.000 | 54,4% | 53,4 – 55,5 |
| Motore 3000 mondi contro ISMCTS potenziato 10.000 | 48,1% | 47,1 – 49,1 |

Per questo l'Assistente ora usa 3000 mondi di serie (scelte: Veloce 1000, Normale 3000, Massima 6000). Tempi di una mossa con 4 core (`bench --threads 4`, 400 posizioni):

| Mondi | Media a metà partita | Media / max con 1-8 carte nel mazzo |
|---:|---:|---:|
| 1000 | 5 ms | 6 / 37 ms |
| 3000 | 13 ms | 14 / 90 ms |
| 6000 | 24 ms | 26 / 187 ms |

**Varianti del motore provate e scartate** (a 300 mondi contro lo stesso motore a 300 mondi, 8.000 partite ciascuna; il 50% è la parità):

| Variante | Punteggio | IC 95% |
|---|---:|---|
| Chi apre nelle simulazioni non guarda la mano avversaria (risposta attesa su una mano a caso) | 48,5% | 47,5 – 49,5 |
| Peso dei punti 0,004 invece di 1/1200 | 49,4% | 48,3 – 50,4 |
| Solo la vittoria conta (peso dei punti 0) | 49,3% | 48,3 – 50,3 |
| Costo base della briscola 4 invece di 8 | 50,2% | 49,2 – 51,3 |
| Costo base della briscola 12 | 50,1% | 49,1 – 51,1 |
| Costo per forza della briscola 1 invece di 2 | 50,1% | 49,1 – 51,1 |
| Costo per forza della briscola 3 | 49,1% | 48,1 – 50,1 |
| Finale esatto nelle simulazioni da 4 carte invece di 2 | 49,8% | 48,8 – 50,9 |
| Ibrido: media delle stime del motore (500 mondi) e dell'ISMCTS potenziato (1.500), contro il motore a 1000 mondi | 50,2% | 49,2 – 51,2 |

I parametri attuali sono già al loro ottimo: nessuna variante si stacca dal 50%. Il motore si rafforza solo con più calcolo.

**Risolutore più veloce.** Il risolutore esatto dei mondi del finale ora usa una tabella delle trasposizioni: calcola i punti che restano da fare (che non dipendono da quelli già presi), così ritrova le posizioni raggiunte per strade diverse e, a mazzo finito, quelle comuni a mondi diversi. Con 1000 mondi e 1-8 carte nel mazzo una mossa scende da 113 a 22 ms di media (max da 738 a 152 ms) su un thread. Le mosse sono identiche: l'impronta delle valutazioni di `bench` resta `21bca65cad5db0ff`. Nel finale con più di 4 carte nel mazzo il motore ora usa anche tutti i core.

## Motore della v0.1.1

Motore: **PIMC con 1000 mondi per mossa** e risolutore esatto con 4 carte o meno nel mazzo (il livello *Difficile* dell'app).
Per ogni avversario: 10.000 mazzi giocati due volte a posti invertiti, quindi **20.000 partite** (160.000 in totale), seme `20261022`.
IC 95% = intervallo di confidenza di Wilson sulla percentuale di vittorie. I pareggi (60-60) non contano come vittorie.

| Avversario | Partite | Vinte | Pari | Perse | % vittorie | IC 95% | Punti medi |
|---|---:|---:|---:|---:|---:|---|---:|
| Casuale | 20000 | 18506 | 148 | 1346 | **92.5%** | 92.2% – 92.9% | 83.7 |
| Euristico | 20000 | 14099 | 423 | 5478 | **70.5%** | 69.9% – 71.1% | 66.5 |
| Aggressivo | 20000 | 14707 | 406 | 4887 | **73.5%** | 72.9% – 74.1% | 69.6 |
| Risparmiatore | 20000 | 14505 | 417 | 5078 | **72.5%** | 71.9% – 73.1% | 67.4 |
| Briscolone | 20000 | 14964 | 399 | 4637 | **74.8%** | 74.2% – 75.4% | 69.4 |
| Contacarte | 20000 | 13465 | 466 | 6069 | **67.3%** | 66.7% – 68.0% | 65.1 |
| ISMCTS (2000 iterazioni) | 20000 | 10826 | 582 | 8592 | **54.1%** | 53.4% – 54.8% | 62.7 |
| Motore PIMC (150 mondi) | 20000 | 10752 | 547 | 8701 | **53.8%** | 53.1% – 54.5% | 61.7 |

## Gli avversari

| Avversario | Come gioca |
|---|---|
| Casuale | Carta a caso |
| Euristico | Regole pratiche da bar: liscio quando non c'è niente da prendere, prende i carichi con la briscola più piccola |
| Aggressivo | Prova sempre a prendere, apre con le carte forti |
| Risparmiatore | Tiene le briscole fino alla fine, prende solo quando costa poco |
| Briscolone | Usa la briscola appena ci sono punti in tavola, apre di briscola |
| Contacarte | Euristico che conta le carte uscite, apre i carichi quando sono sicuri e gioca il finale in modo perfetto |
| ISMCTS | Ricerca ad albero Monte Carlo sugli insiemi di informazione: un algoritmo di famiglia diversa, l'avversario più forte del pannello |
| Motore PIMC (150 mondi) | Lo stesso motore con meno simulazioni (livello *Medio* dell'app) |

## Come leggerli

- Contro tutti gli 8 avversari il motore vince più del 50% delle partite, e anche il limite basso dell'intervallo di confidenza è sopra il 50%.
- Contro gli stili umani tipici il margine è ampio (67-75%). Contro gli avversari algoritmici forti è piccolo ma netto (circa 54%).
- Contro un avversario forte quanto lui il risultato tende al 50%: due copie dello stesso motore vincono metà delle partite a testa. Nessun algoritmo può garantire più del 50% contro qualsiasi giocatore, perché la briscola dipende molto dalle carte.

Per riprodurre la tabella:

```
cargo run --release -p briscola-sim -- --panel --deals 10000 --player engine:1000 --seed 20261022 --out tabella.md
```
