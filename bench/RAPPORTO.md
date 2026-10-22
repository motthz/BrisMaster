# Rapporto dei test massivi

Motore provato: **Motore PIMC (1000 mondi)** (`engine:1000`), commit `fbd3d28`, 2026-10-22.
Ogni riga: 5000 mazzi giocati due volte a posti invertiti (10000 partite), seme `20261022`. Gli stessi mazzi per tutti gli avversari.

## Riepilogo: obiettivo 55% contro ogni avversario

Sopra il 55% con certezza statistica (intero IC 95% sopra il 55%): **13 avversari su 24**. Il motore contro se stesso è escluso: lì il 50% è il risultato esatto per costruzione.

| Avversario | Punteggio | IC 95% | Obiettivo |
|---|---:|---|---|
| Motore PIMC (3000 mondi) | 48,3% | 47,5% – 49,2% | ❌ sotto il 55% |
| Motore PIMC (1000 mondi) · simulazioni con 10% di mosse casuali · finale esatto da 6 carte nel mazzo | 49,0% | 48,2% – 49,9% | ❌ sotto il 55% |
| Motore PIMC (1000 mondi) [v0.1.1] | 50,0% | 49,1% – 50,9% | ❌ sotto il 55% |
| Motore PIMC (1000 mondi) · simulazioni con 30% di mosse casuali | 50,1% | 49,2% – 51,0% | ❌ sotto il 55% |
| Motore PIMC (400 mondi) | 51,3% | 50,4% – 52,2% | ❌ sotto il 55% |
| Motore PIMC (1000 mondi) · simulazioni con 0% di mosse casuali | 51,4% | 50,5% – 52,3% | ❌ sotto il 55% |
| ISMCTS (20000 iterazioni) | 51,5% | 50,5% – 52,4% | ❌ sotto il 55% |
| ISMCTS (10000 iterazioni) | 53,3% | 52,3% – 54,2% | ❌ sotto il 55% |
| ISMCTS (5000 iterazioni) | 54,4% | 53,4% – 55,3% | ≈ al limite |
| Motore PIMC (150 mondi) | 54,8% | 53,9% – 55,7% | ≈ al limite |
| ISMCTS (2000 iterazioni) | 55,2% | 54,3% – 56,1% | ≈ al limite |
| Motore PIMC (400 mondi) con 10% di errori | 58,6% | 57,7% – 59,5% | ✅ sopra il 55% |
| ISMCTS (500 iterazioni) | 59,3% | 58,4% – 60,2% | ✅ sopra il 55% |
| Motore PIMC (50 mondi) | 59,9% | 59,0% – 60,8% | ✅ sopra il 55% |
| Contacarte | 68,4% | 67,6% – 69,3% | ✅ sopra il 55% |
| Euristico | 71,6% | 70,8% – 72,5% | ✅ sopra il 55% |
| Butta-carichi | 72,2% | 71,3% – 73,0% | ✅ sopra il 55% |
| Risparmiatore | 73,2% | 72,3% – 74,0% | ✅ sopra il 55% |
| Aggressivo | 74,7% | 73,9% – 75,6% | ✅ sopra il 55% |
| Briscolone | 75,9% | 75,1% – 76,8% | ✅ sopra il 55% |
| Rispondi di seme | 77,3% | 76,5% – 78,1% | ✅ sopra il 55% |
| Euristico con 30% di errori | 81,0% | 80,2% – 81,7% | ✅ sopra il 55% |
| Mai briscola | 87,4% | 86,7% – 88,1% | ✅ sopra il 55% |
| Casuale | 93,2% | 92,7% – 93,7% | ✅ sopra il 55% |

Come leggere le colonne:

- **% vittorie**: partite vinte (i pareggi 60-60 non contano).
- **Punteggio**: vittoria 1, pareggio ½, sconfitta 0. Il 50% è la parità: sopra il 50% il motore è più forte.
- **IC 95%**: intervallo di confidenza del punteggio, calcolato sulle coppie di partite dello stesso mazzo. Se è tutto sopra il 50%, il vantaggio non è dovuto al caso.
- **Elo**: la stessa differenza di forza espressa in punti Elo (+35 ≈ 55%, +100 ≈ 64% di punteggio).

## 1. Contro se stesso: quante partite servono

| Avversario | Partite | V / P / S | % vittorie | Punteggio | IC 95% | Elo | Punti medi |
|---|---:|---:|---:|---:|---|---:|---:|
| Motore PIMC (1000 mondi) (copia) | 20000 | 9747 / 608 / 9645 | 48,7% | **50,3%** | 49,6% – 50,9% | +2 | 60,1 |

Punteggio 50,3% (IC 49,6% – 50,9%): due copie dello stesso motore si dividono le partite a metà. Non è un difetto: in duplicato ogni mazzo viene giocato da entrambe le copie da entrambi i posti, quindi per simmetria nessuna delle due può fare meglio del 50%. Vale per qualsiasi programma: contro un avversario forte quanto lui nessuno può superare il 50%. Questo test serve a controllare che il banco di prova sia equo e a stabilire quante partite servono.

Chi apre la prima presa vince il 46,5% delle partite (IC 45,6% – 47,5%), chi risponde il 50,9%. Qualunque vantaggio dia il posto, il duplicato lo compensa: ogni mazzo si gioca da entrambi i posti.

**Quante partite servono.** Si gioca il motore contro se stesso aumentando le partite, finché il punteggio resta stabilmente vicino al 50% (entro 1 punto percentuale) e l'intervallo di confidenza è largo al più ±1 punto. Con meno partite il caso pesa troppo: anche due giocatori identici sembrano diversi.

| Mazzi | Partite | Punteggio | IC 95% | Distanza dal 50% | Criterio |
|---:|---:|---:|---|---:|:---:|
| 50 | 100 | 53,0% | 43,8% – 62,2% | 3,0 punti | — |
| 100 | 200 | 49,5% | 43,3% – 55,7% | 0,5 punti | — |
| 250 | 500 | 52,3% | 48,6% – 56,0% | 2,3 punti | — |
| 500 | 1000 | 50,0% | 47,2% – 52,9% | 0,0 punti | — |
| 1000 | 2000 | 49,8% | 47,9% – 51,7% | 0,2 punti | — |
| 1500 | 3000 | 50,4% | 48,8% – 52,0% | 0,4 punti | — |
| 2000 | 4000 | 50,4% | 49,0% – 51,8% | 0,4 punti | — |
| 3000 | 6000 | 50,2% | 49,0% – 51,3% | 0,2 punti | — |
| 4000 | 8000 | 50,4% | 49,4% – 51,4% | 0,4 punti | ✅ da qui |
| 5000 | 10000 | 50,0% | 49,1% – 50,9% | 0,0 punti | ✅ |
| 7500 | 15000 | 50,0% | 49,3% – 50,8% | 0,0 punti | ✅ |
| 10000 | 20000 | 50,3% | 49,6% – 50,9% | 0,3 punti | ✅ |

Servono almeno **4000 mazzi (8000 partite)** per confronto. Questo rapporto ne usa 5000 (10000 partite) per riga: le differenze di più di un punto percentuale non sono dovute al caso.

## 2. Contro tanti stili di gioco

| Avversario | Partite | V / P / S | % vittorie | Punteggio | IC 95% | Elo | Punti medi |
|---|---:|---:|---:|---:|---|---:|---:|
| Casuale | 10000 | 9284 / 68 / 648 | 92,8% | **93,2%** | 92,7% – 93,7% | +454 | 83,8 |
| Euristico | 10000 | 7048 / 231 / 2721 | 70,5% | **71,6%** | 70,8% – 72,5% | +161 | 66,5 |
| Aggressivo | 10000 | 7374 / 195 / 2431 | 73,7% | **74,7%** | 73,9% – 75,6% | +188 | 69,5 |
| Risparmiatore | 10000 | 7205 / 228 / 2567 | 72,0% | **73,2%** | 72,3% – 74,0% | +174 | 67,3 |
| Briscolone | 10000 | 7494 / 201 / 2305 | 74,9% | **75,9%** | 75,1% – 76,8% | +200 | 69,3 |
| Contacarte | 10000 | 6727 / 235 / 3038 | 67,3% | **68,4%** | 67,6% – 69,3% | +135 | 65,1 |
| ISMCTS (2000 iterazioni) | 10000 | 5373 / 295 / 4332 | 53,7% | **55,2%** | 54,3% – 56,1% | +36 | 62,7 |
| Motore PIMC (150 mondi) | 10000 | 5341 / 274 / 4385 | 53,4% | **54,8%** | 53,9% – 55,7% | +33 | 61,7 |
| Euristico con 30% di errori | 10000 | 8004 / 185 / 1811 | 80,0% | **81,0%** | 80,2% – 81,7% | +251 | 71,9 |
| Rispondi di seme | 10000 | 7635 / 198 / 2167 | 76,3% | **77,3%** | 76,5% – 78,1% | +213 | 69,8 |
| Butta-carichi | 10000 | 7113 / 206 / 2681 | 71,1% | **72,2%** | 71,3% – 73,0% | +165 | 67,2 |
| Mai briscola | 10000 | 8682 / 118 / 1200 | 86,8% | **87,4%** | 86,7% – 88,1% | +337 | 78,0 |
| Motore PIMC (400 mondi) con 10% di errori | 10000 | 5729 / 266 / 4005 | 57,3% | **58,6%** | 57,7% – 59,5% | +61 | 63,1 |
| Motore PIMC (50 mondi) | 10000 | 5872 / 227 / 3901 | 58,7% | **59,9%** | 59,0% – 60,8% | +69 | 63,5 |
| Motore PIMC (400 mondi) | 10000 | 4987 / 279 / 4734 | 49,9% | **51,3%** | 50,4% – 52,2% | +9 | 60,5 |
| ISMCTS (500 iterazioni) | 10000 | 5783 / 292 / 3925 | 57,8% | **59,3%** | 58,4% – 60,2% | +65 | 64,0 |
| ISMCTS (5000 iterazioni) | 10000 | 5291 / 289 / 4420 | 52,9% | **54,4%** | 53,4% – 55,3% | +30 | 62,3 |

| Avversario | Come gioca |
|---|---|
| Casuale | Carta a caso |
| Euristico | Regole pratiche da bar: liscia quando non c'è niente da prendere, prende i carichi con la briscola più piccola |
| Aggressivo | Prova sempre a prendere, apre con le carte forti |
| Risparmiatore | Tiene le briscole fino alla fine, prende solo quando costa poco |
| Briscolone | Usa la briscola appena ci sono punti in tavola, apre di briscola |
| Contacarte | Conta le carte uscite, apre i carichi quando sono sicuri e gioca il finale in modo perfetto |
| ISMCTS (2000 iterazioni) | Ricerca ad albero Monte Carlo: algoritmo di famiglia diversa, forte |
| Motore PIMC (150 mondi) | Lo stesso motore con meno simulazioni (livello Medio dell'app) |
| Euristico con 30% di errori | Principiante: regole da bar, ma una mossa su tre è a caso |
| Rispondi di seme | Crede di dover rispondere al seme, come in altri giochi di carte |
| Butta-carichi | Apre con Assi e Tre per fare punti subito, senza contare le briscole |
| Mai briscola | Non usa mai la briscola finché c'è il mazzo |
| Motore PIMC (400 mondi) con 10% di errori | Giocatore forte ma distratto: motore a 400 simulazioni che sbaglia una mossa su dieci |
| Motore PIMC (50 mondi) | Motore con 50 simulazioni |
| Motore PIMC (400 mondi) | Motore con 400 simulazioni |
| ISMCTS (500 iterazioni) | ISMCTS con 500 iterazioni |
| ISMCTS (5000 iterazioni) | ISMCTS con 5000 iterazioni, l'avversario algoritmico più forte |

## 3. Contro gli avversari più forti

Gli avversari più difficili che il banco di prova sa costruire: ricerca ad albero con molte più iterazioni, e varianti del motore stesso con più simulazioni o un finale calcolato prima. Contro le varianti di se stesso il motore parte vicino al 50% per costruzione: superarle del 55% richiede un motore nettamente migliore, non solo più calcolo.

| Avversario | Partite | V / P / S | % vittorie | Punteggio | IC 95% | Elo | Punti medi |
|---|---:|---:|---:|---:|---|---:|---:|
| ISMCTS (10000 iterazioni) | 10000 | 5172 / 311 / 4517 | 51,7% | **53,3%** | 52,3% – 54,2% | +23 | 61,9 |
| ISMCTS (20000 iterazioni) | 10000 | 5009 / 274 / 4717 | 50,1% | **51,5%** | 50,5% – 52,4% | +10 | 61,4 |
| Motore PIMC (3000 mondi) | 10000 | 4681 / 306 / 5013 | 46,8% | **48,3%** | 47,5% – 49,2% | -12 | 59,5 |
| Motore PIMC (1000 mondi) · simulazioni con 0% di mosse casuali | 10000 | 4992 / 294 / 4714 | 49,9% | **51,4%** | 50,5% – 52,3% | +10 | 60,4 |
| Motore PIMC (1000 mondi) · simulazioni con 30% di mosse casuali | 10000 | 4866 / 285 / 4849 | 48,7% | **50,1%** | 49,2% – 51,0% | +1 | 60,1 |
| Motore PIMC (1000 mondi) · simulazioni con 10% di mosse casuali · finale esatto da 6 carte nel mazzo | 10000 | 4763 / 284 / 4953 | 47,6% | **49,0%** | 48,2% – 49,9% | -7 | 59,9 |

| Avversario | Come gioca |
|---|---|
| ISMCTS (10000 iterazioni) | ISMCTS con 10.000 iterazioni |
| ISMCTS (20000 iterazioni) | ISMCTS con 20.000 iterazioni |
| Motore PIMC (3000 mondi) | Lo stesso motore con il triplo delle simulazioni |
| Motore PIMC (1000 mondi) · simulazioni con 0% di mosse casuali | Lo stesso motore con simulazioni senza mosse casuali |
| Motore PIMC (1000 mondi) · simulazioni con 30% di mosse casuali | Lo stesso motore con simulazioni più varie (30% di mosse casuali) |
| Motore PIMC (1000 mondi) · simulazioni con 10% di mosse casuali · finale esatto da 6 carte nel mazzo | Lo stesso motore che calcola il finale esatto già con 6 carte nel mazzo |

## 4. Contro la versione precedente

| Avversario | Partite | V / P / S | % vittorie | Punteggio | IC 95% | Elo | Punti medi |
|---|---:|---:|---:|---:|---|---:|---:|
| Motore PIMC (1000 mondi) [v0.1.1] | 10000 | 4844 / 309 / 4847 | 48,4% | **50,0%** | 49,1% – 50,9% | -0 | 60,0 |

Contro Motore PIMC (1000 mondi) [v0.1.1]: nessuna differenza significativa tra le due versioni.
La stessa prova gira in automatico su ogni pull request (job *regressione* in `.github/workflows/ci.yml`) e blocca le modifiche che rendono il motore significativamente più debole della release v0.1.1.

## Come riprodurlo

```
cargo run --release -p briscola-sim -- report --player engine:1000 --deals 5000 --self-deals 10000 --seed 20261022 --out bench/RAPPORTO.md
```

260.000 partite in tutto, circa 5 ore di calcolo su 4 core (il rapporto è stato ripreso dalla cache dopo un'interruzione).
