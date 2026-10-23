# Sette e mezzo: rapporto

Generato da `cargo run --release -p settemezzo -- --hands 1000000 --seed 20261023`.

**Regole.** Mazzo da 40 carte; Asso 1, dal Due al Sette il loro valore, figure ½. Il Re di Denari è la matta e vale quello che conviene (½ oppure da 1 a 7). Il giocatore riceve una carta coperta, chiede carte scoperte o si ferma; se supera 7½ perde subito. Poi gioca il banco: se sballa paga, altrimenti vince chi ha di più e **il pareggio va al banco**. Il sette e mezzo reale (7½ con due carte) batte il 7½ normale ed è pagato **il doppio**. Mazzo nuovo a ogni mano, un giocatore contro il banco.

## Criteri

Nel Sette e mezzo contro il banco non c'è un avversario da battere "più del 50% delle volte": il banco ha il vantaggio dei pareggi e del fatto che il giocatore sballa per primo. La misura giusta è la **vincita media per mano**, in percentuale della puntata (rendimento atteso).

1. **Valore esatto.** Per ogni strategia il rendimento si calcola enumerando tutte le carte che possono uscire, per il giocatore e per il banco: nessun errore di campionamento.
2. **Verifica con simulazione.** 1000000 mani simulate per ogni coppia strategia–banco, con mazzi mescolati davvero. Il valore esatto deve cadere nell'intervallo di confidenza al 95% della simulazione (su tante righe qualcuna può uscirne per caso, una su venti); se una riga si discosta di più di 3,5 deviazioni standard il programma esce con errore.
3. **Confronto appaiato.** Tutte le strategie giocano gli stessi mazzi, e la differenza dalla strategia ottimale ha il suo intervallo di confidenza, molto più stretto di quello delle singole strategie.
4. **Robustezza.** La strategia ottimale presuppone un modello di banco: si misura quanto si perde usando quella pensata per un banco diverso da quello reale.
5. **Quante mani servono.** Con le 8.000 partite usate per la briscola il rendimento di una strategia si conosce solo a ±2 punti; con 1000000 mani a circa ±0,2.

**Modelli di banco.**

- *Banco a soglia 5*: chiede carta finché ha meno di 5 e poi si ferma, senza guardare le tue carte.
- *Banco a soglia 5½*: chiede carta finché ha meno di 5½ e poi si ferma, senza guardare le tue carte.
- *Banco a soglia 6*: chiede carta finché ha meno di 6 e poi si ferma, senza guardare le tue carte.
- *Banco esperto*: guarda le tue carte scoperte e tira o si ferma per massimizzare la sua vincita.

## Contro il banco a soglia 5

| Strategia | Rendimento esatto | Vince | di cui reale | Sballa | Perde al confronto | Simulato (IC 95%) | Esatto nell'IC | Distanza dall'ottimale (IC 95%) |
|---|---:|---:|---:|---:|---:|---|:-:|---|
| **Ottimale (BrisMaster)** | **-11,45%** | 42,1% | 4,4% | 29,6% | 28,3% | -11,56% ± 0,21 | sì | — |
| Ottimale pensata per il banco esperto | **-12,39%** | 41,6% | 4,4% | 22,8% | 35,5% | -12,43% ± 0,21 | sì | -0,87% (-0,99% – -0,75%) |
| Ottimale pensata per il banco a soglia 5½ | **-15,19%** | 40,2% | 4,4% | 3,3% | 56,5% | -15,31% ± 0,21 | sì | -3,75% (-3,93% – -3,56%) |
| Chiede finché ha meno di 4 | **-13,60%** | 41,5% | 3,4% | 14,2% | 44,3% | -13,69% ± 0,20 | sì | -2,13% (-2,27% – -2,00%) |
| Chiede finché ha meno di 4½ | **-14,13%** | 41,2% | 3,4% | 20,8% | 37,9% | -14,19% ± 0,20 | sì | -2,63% (-2,76% – -2,50%) |
| Chiede finché ha meno di 5 | **-13,29%** | 41,6% | 3,4% | 24,8% | 33,6% | -13,29% ± 0,20 | sì | -1,73% (-1,85% – -1,61%) |
| Chiede finché ha meno di 5½ | **-13,08%** | 41,7% | 3,4% | 34,1% | 24,1% | -13,13% ± 0,20 | sì | -1,57% (-1,65% – -1,49%) |
| Chiede finché ha meno di 6 | **-15,78%** | 40,4% | 3,4% | 39,9% | 19,7% | -15,85% ± 0,20 | sì | -4,29% (-4,40% – -4,18%) |
| Chiede finché ha meno di 6½ | **-25,29%** | 35,6% | 3,4% | 52,5% | 11,9% | -25,30% ± 0,20 | sì | -13,74% (-13,89% – -13,58%) |
| A caso | **-41,36%** | 28,4% | 1,8% | 25,3% | 46,3% | -41,56% ± 0,18 | **NO** | -30,00% (-30,23% – -29,77%) |

1000000 mani per strategia simulate in 4 s.

## Contro il banco a soglia 5½

| Strategia | Rendimento esatto | Vince | di cui reale | Sballa | Perde al confronto | Simulato (IC 95%) | Esatto nell'IC | Distanza dall'ottimale (IC 95%) |
|---|---:|---:|---:|---:|---:|---|:-:|---|
| **Ottimale (BrisMaster)** | **-7,24%** | 44,2% | 4,4% | 3,3% | 52,5% | -7,42% ± 0,21 | sì | — |
| Ottimale pensata per il banco esperto | **-11,94%** | 41,9% | 4,4% | 22,8% | 35,3% | -12,02% ± 0,21 | sì | -4,60% (-4,78% – -4,43%) |
| Chiede finché ha meno di 4 | **-10,32%** | 43,1% | 3,4% | 14,2% | 42,7% | -10,41% ± 0,20 | sì | -2,99% (-3,14% – -2,85%) |
| Chiede finché ha meno di 4½ | **-12,96%** | 41,8% | 3,4% | 20,8% | 37,4% | -13,06% ± 0,20 | sì | -5,65% (-5,82% – -5,48%) |
| Chiede finché ha meno di 5 | **-13,66%** | 41,5% | 3,4% | 24,8% | 33,8% | -13,71% ± 0,20 | sì | -6,30% (-6,48% – -6,12%) |
| Chiede finché ha meno di 5½ | **-17,89%** | 39,3% | 3,4% | 34,1% | 26,5% | -18,00% ± 0,20 | sì | -10,59% (-10,78% – -10,39%) |
| Chiede finché ha meno di 6 | **-18,93%** | 38,8% | 3,4% | 39,9% | 21,3% | -19,04% ± 0,20 | sì | -11,62% (-11,83% – -11,42%) |
| Chiede finché ha meno di 6½ | **-27,09%** | 34,7% | 3,4% | 52,5% | 12,8% | -27,10% ± 0,20 | sì | -19,69% (-19,91% – -19,46%) |
| A caso | **-33,28%** | 32,4% | 1,8% | 25,3% | 42,2% | -33,51% ± 0,19 | **NO** | -26,10% (-26,31% – -25,89%) |

1000000 mani per strategia simulate in 4 s.

## Contro il banco a soglia 6

| Strategia | Rendimento esatto | Vince | di cui reale | Sballa | Perde al confronto | Simulato (IC 95%) | Esatto nell'IC | Distanza dall'ottimale (IC 95%) |
|---|---:|---:|---:|---:|---:|---|:-:|---|
| **Ottimale (BrisMaster)** | **-2,36%** | 46,6% | 4,4% | 3,3% | 50,1% | -2,53% ± 0,21 | sì | — |
| Ottimale pensata per il banco esperto | **-11,66%** | 42,0% | 4,4% | 22,8% | 35,2% | -11,74% ± 0,21 | sì | -9,21% (-9,38% – -9,03%) |
| Ottimale pensata per il banco a soglia 5½ | **-2,36%** | 46,6% | 4,4% | 3,3% | 50,1% | -2,53% ± 0,21 | sì | -0,00% (-0,00% – +0,00%) |
| Chiede finché ha meno di 4 | **-8,26%** | 44,2% | 3,4% | 14,2% | 41,7% | -8,36% ± 0,21 | sì | -5,83% (-5,98% – -5,68%) |
| Chiede finché ha meno di 4½ | **-12,33%** | 42,1% | 3,4% | 20,8% | 37,0% | -12,43% ± 0,20 | sì | -9,90% (-10,07% – -9,73%) |
| Chiede finché ha meno di 5 | **-13,80%** | 41,4% | 3,4% | 24,8% | 33,8% | -13,84% ± 0,20 | sì | -11,31% (-11,49% – -11,13%) |
| Chiede finché ha meno di 5½ | **-19,78%** | 38,4% | 3,4% | 34,1% | 27,5% | -19,89% ± 0,20 | sì | -17,35% (-17,55% – -17,16%) |
| Chiede finché ha meno di 6 | **-22,39%** | 37,1% | 3,4% | 39,9% | 23,0% | -22,47% ± 0,20 | sì | -19,94% (-20,15% – -19,73%) |
| Chiede finché ha meno di 6½ | **-28,60%** | 34,0% | 3,4% | 52,5% | 13,6% | -28,60% ± 0,20 | sì | -26,07% (-26,30% – -25,85%) |
| A caso | **-28,49%** | 34,8% | 1,8% | 25,3% | 39,8% | -28,73% ± 0,19 | **NO** | -26,20% (-26,41% – -25,99%) |

1000000 mani per strategia simulate in 5 s.

## Contro il banco esperto

| Strategia | Rendimento esatto | Vince | di cui reale | Sballa | Perde al confronto | Simulato (IC 95%) | Esatto nell'IC | Distanza dall'ottimale (IC 95%) |
|---|---:|---:|---:|---:|---:|---|:-:|---|
| **Ottimale (BrisMaster)** | **-6,09%** | 44,8% | 4,3% | 22,8% | 32,3% | -6,10% ± 0,21 | sì | — |
| Ottimale pensata per il banco a soglia 5½ | **-19,90%** | 37,9% | 4,3% | 3,3% | 58,8% | -19,93% ± 0,20 | sì | -13,83% (-13,98% – -13,67%) |
| Chiede finché ha meno di 4 | **-8,74%** | 44,0% | 3,3% | 14,2% | 41,9% | -8,81% ± 0,20 | sì | -2,71% (-2,81% – -2,60%) |
| Chiede finché ha meno di 4½ | **-7,59%** | 44,5% | 3,3% | 20,8% | 34,6% | -7,62% ± 0,21 | sì | -1,52% (-1,58% – -1,46%) |
| Chiede finché ha meno di 5 | **-7,32%** | 44,7% | 3,3% | 24,8% | 30,5% | -7,29% ± 0,21 | sì | -1,19% (-1,25% – -1,13%) |
| Chiede finché ha meno di 5½ | **-11,49%** | 42,6% | 3,3% | 34,1% | 23,3% | -11,50% ± 0,20 | sì | -5,40% (-5,51% – -5,28%) |
| Chiede finché ha meno di 6 | **-14,64%** | 41,0% | 3,3% | 39,9% | 19,1% | -14,65% ± 0,20 | sì | -8,55% (-8,68% – -8,42%) |
| Chiede finché ha meno di 6½ | **-26,36%** | 35,2% | 3,3% | 52,5% | 12,4% | -26,32% ± 0,20 | sì | -20,22% (-20,39% – -20,05%) |
| A caso | **-49,88%** | 24,2% | 1,8% | 25,3% | 50,5% | -50,07% ± 0,18 | **NO** | -43,97% (-44,18% – -43,76%) |

1000000 mani per strategia simulate in 4 s.

## Robustezza: strategia pensata per un banco, giocata contro un altro

| Strategia pensata per ↓ / banco reale → | Banco a soglia 5 | Banco a soglia 5½ | Banco a soglia 6 | Banco esperto |
|---|---:|---:|---:|---:|
| Banco a soglia 5 | **-11,45%** | -14,60% | -15,60% | -10,86% |
| Banco a soglia 5½ | -15,19% | **-7,24%** | -2,36% | -19,90% |
| Banco a soglia 6 | -15,19% | -7,24% | **-2,36%** | -19,90% |
| Banco esperto | -12,39% | -11,94% | -11,66% | **-6,09%** |

In grassetto la strategia giusta per quel banco, il massimo della colonna.

## Varianti di regole (valori esatti, strategia ottimale)

| Regole | Banco a soglia 5 | Banco a soglia 5½ | Banco a soglia 6 | Banco esperto |
|---|---:|---:|---:|---:|
| Con matta, reale pagato doppio (quelle sopra) | -11,45% | -7,24% | -2,36% | -6,09% |
| Con matta, reale pagato semplice | -15,81% | -11,60% | -6,72% | -10,35% |
| Senza matta, reale pagato doppio | -13,12% | -5,99% | +0,45% | -7,72% |
| Senza matta, reale pagato semplice | -16,12% | -8,99% | -2,56% | -10,65% |

## In sintesi

| Banco | Ottimale | Migliore strategia a soglia fissa | Guadagno dell'ottimale |
|---|---:|---|---:|
| Banco a soglia 5 | -11,45% | Chiede finché ha meno di 5½: -13,08% | +1,63% |
| Banco a soglia 5½ | -7,24% | Chiede finché ha meno di 4: -10,32% | +3,08% |
| Banco a soglia 6 | -2,36% | Chiede finché ha meno di 4: -8,26% | +5,90% |
| Banco esperto | -6,09% | Chiede finché ha meno di 5: -7,32% | +1,23% |

Controlli: simulazione entro 3,5 deviazioni standard dal valore esatto in ogni riga e nessuna strategia sopra l'ottimale: **superati**. Tempo totale 21 s.
