# Risultati dei test massivi

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
