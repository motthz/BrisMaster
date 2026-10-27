# Scopa: rapporto dei test massivi

Motore provato: **Motore PIMC (400 mondi)** (`engine:400`), commit `e93dd5e`, 2026-10-23.
Ogni partita va a 11 punti (scopa classica a due: carte, denari, settebello, primiera e scope). Ogni riga: 4000 sequenze di mazzi giocate due volte a posti invertiti (8000 partite), seme `20261023`, gli stessi mazzi per tutti gli avversari.

## Riepilogo: obiettivo 55% contro ogni avversario

Sopra il 55% con certezza statistica (intero IC 95% sopra il 55%): **10 avversari su 16**. Il motore contro se stesso è escluso: lì il 50% è il risultato esatto per costruzione.

| Avversario | Partite vinte | IC 95% | Obiettivo |
|---|---:|---|---|
| Motore PIMC (1000 mondi) | 49,3% | 48,5% – 50,1% | ❌ sotto il 55% |
| ISMCTS (5000 iterazioni) | 49,6% | 48,7% – 50,4% | ❌ sotto il 55% |
| Motore PIMC (400 mondi) · gioca per la differenza punti | 50,2% | 49,4% – 51,1% | ❌ sotto il 55% |
| Motore ad albero (1000 iterazioni) | 52,8% | 51,9% – 53,7% | ❌ sotto il 55% |
| Motore PIMC (100 mondi) | 53,8% | 52,9% – 54,6% | ❌ sotto il 55% |
| ISMCTS (1000 iterazioni) | 55,4% | 54,5% – 56,2% | ≈ al limite |
| Motore PIMC (400 mondi) con 10% di errori | 57,8% | 56,9% – 58,6% | ✅ sopra il 55% |
| Contacarte | 62,7% | 61,8% – 63,6% | ✅ sopra il 55% |
| Contacarte con 10% di errori | 68,8% | 67,9% – 69,7% | ✅ sopra il 55% |
| Goloso | 77,5% | 76,7% – 78,3% | ✅ sopra il 55% |
| Primierista | 79,2% | 78,4% – 80,1% | ✅ sopra il 55% |
| Denarista | 79,8% | 79,0% – 80,6% | ✅ sopra il 55% |
| Euristico | 80,8% | 80,1% – 81,6% | ✅ sopra il 55% |
| Cacciatore di scope | 83,9% | 83,1% – 84,6% | ✅ sopra il 55% |
| Euristico con 30% di errori | 89,0% | 88,3% – 89,6% | ✅ sopra il 55% |
| Casuale | 98,2% | 97,9% – 98,5% | ✅ sopra il 55% |

Come leggere le colonne:

- **% vittorie**: partite a 11 vinte. Il 50% è la parità.
- **IC 95%**: intervallo di confidenza, calcolato sulle coppie di partite con gli stessi mazzi. Se è tutto sopra il 50%, il vantaggio non è dovuto al caso.
- **Elo**: la stessa differenza di forza in punti Elo (+35 ≈ 55%, +100 ≈ 64%).
- **Punti a smazzata**: media dei punti fatti e subiti dal motore in ogni smazzata.

## 1. Contro se stesso: quante partite servono

| Avversario | Partite | Vinte | % vittorie | IC 95% | Elo | Smazzate a partita | Punti a smazzata (fatti – subiti) | Scope a partita |
|---|---:|---:|---:|---|---:|---:|---:|---:|
| Motore PIMC (400 mondi) (copia) | 8000 | 4010 | **50,1%** | 49,3% – 50,9% | +1 | 4,25 | 2,30 – 2,29 | 2,03 |

Punteggio 50,1% (IC 49,3% – 50,9%): due copie dello stesso motore si dividono le partite a metà, come deve essere. In duplicato ogni sequenza di mazzi viene giocata da entrambe le copie da entrambi i posti, quindi nessuna delle due può fare meglio del 50%: contro un avversario forte quanto lui nessun programma lo supera. Il test controlla che il banco di prova sia equo e stabilisce quante partite servono.

Chi gioca per primo nella prima smazzata vince il 48,5% delle partite (IC 46,9% – 50,0%), chi distribuisce il 51,8%. Il duplicato compensa il vantaggio del posto.

**Quante partite servono.** Si aumentano le partite del motore contro se stesso finché il punteggio resta stabilmente entro 1 punto percentuale dal 50% e l'intervallo di confidenza è largo al più ±1 punto.

| Mazzi | Partite | Punteggio | IC 95% | Distanza dal 50% | Criterio |
|---:|---:|---:|---|---:|:---:|
| 50 | 100 | 52,0% | 45,8% – 58,2% | 2,0 punti | — |
| 100 | 200 | 51,0% | 46,8% – 55,2% | 1,0 punti | — |
| 250 | 500 | 48,2% | 45,1% – 51,3% | 1,8 punti | — |
| 500 | 1000 | 49,5% | 47,2% – 51,8% | 0,5 punti | — |
| 1000 | 2000 | 50,4% | 48,8% – 52,0% | 0,4 punti | — |
| 1500 | 3000 | 50,7% | 49,4% – 52,0% | 0,7 punti | — |
| 2000 | 4000 | 50,7% | 49,6% – 51,9% | 0,7 punti | — |
| 3000 | 6000 | 50,4% | 49,4% – 51,3% | 0,4 punti | ✅ da qui |
| 4000 | 8000 | 50,1% | 49,3% – 50,9% | 0,1 punti | ✅ |

Servono almeno **3000 mazzi (6000 partite)** per confronto. Questo rapporto ne usa 4000 (8000 partite) per riga: le differenze di più di un punto percentuale non sono dovute al caso.

## 2. Contro tanti stili di gioco

| Avversario | Partite | Vinte | % vittorie | IC 95% | Elo | Smazzate a partita | Punti a smazzata (fatti – subiti) | Scope a partita |
|---|---:|---:|---:|---|---:|---:|---:|---:|
| Casuale | 8000 | 7853 | **98,2%** | 97,9% – 98,5% | +691 | 3,59 | 3,48 – 1,07 | 2,07 |
| Goloso | 8000 | 6199 | **77,5%** | 76,7% – 78,3% | +215 | 3,94 | 2,91 – 1,93 | 3,06 |
| Euristico | 8000 | 6468 | **80,8%** | 80,1% – 81,6% | +250 | 4,17 | 2,76 – 1,74 | 1,63 |
| Cacciatore di scope | 8000 | 6711 | **83,9%** | 83,1% – 84,6% | +287 | 4,18 | 2,79 – 1,65 | 1,33 |
| Primierista | 8000 | 6339 | **79,2%** | 78,4% – 80,1% | +233 | 4,14 | 2,76 – 1,79 | 1,89 |
| Denarista | 8000 | 6381 | **79,8%** | 79,0% – 80,6% | +238 | 4,10 | 2,80 – 1,80 | 2,13 |
| Contacarte | 8000 | 5016 | **62,7%** | 61,8% – 63,6% | +90 | 4,24 | 2,47 – 2,09 | 2,26 |
| Euristico con 30% di errori | 8000 | 7118 | **89,0%** | 88,3% – 89,6% | +363 | 4,01 | 2,97 – 1,53 | 1,77 |
| Contacarte con 10% di errori | 8000 | 5505 | **68,8%** | 67,9% – 69,7% | +137 | 4,21 | 2,58 – 1,98 | 2,27 |

| Avversario | Come gioca |
|---|---|
| Casuale | Carta e presa a caso |
| Goloso | Prende sempre la presa più ricca, non guarda cosa lascia in tavola |
| Euristico | Regole da bar: prende il più possibile, non lascia in tavola meno di 11 (scopa facile) |
| Cacciatore di scope | Punta tutto sulle scope |
| Primierista | Insegue sette e sei per la primiera, non li butta mai |
| Denarista | Raccoglie denari |
| Contacarte | Conta le carte uscite per stimare cosa può prendere l'avversario e gioca il finale in modo perfetto |
| Euristico con 30% di errori | Principiante: regole da bar, ma una mossa su tre è a caso |
| Contacarte con 10% di errori | Contacarte distratto: sbaglia una mossa su dieci |

## 3. Contro gli avversari più forti

Ricerca ad albero Monte Carlo (ISMCTS), che è un algoritmo diverso dal motore, e varianti del motore stesso. Contro le proprie varianti il motore parte vicino al 50% per costruzione.

| Avversario | Partite | Vinte | % vittorie | IC 95% | Elo | Smazzate a partita | Punti a smazzata (fatti – subiti) | Scope a partita |
|---|---:|---:|---:|---|---:|---:|---:|---:|
| ISMCTS (1000 iterazioni) | 8000 | 4428 | **55,4%** | 54,5% – 56,2% | +37 | 4,19 | 2,39 – 2,24 | 2,15 |
| ISMCTS (5000 iterazioni) | 8000 | 3965 | **49,6%** | 48,7% – 50,4% | -3 | 4,25 | 2,28 – 2,32 | 2,02 |
| Motore ad albero (1000 iterazioni) | 8000 | 4224 | **52,8%** | 51,9% – 53,7% | +19 | 4,21 | 2,37 – 2,27 | 2,19 |
| Motore PIMC (100 mondi) | 8000 | 4300 | **53,8%** | 52,9% – 54,6% | +26 | 4,25 | 2,35 – 2,23 | 2,04 |
| Motore PIMC (1000 mondi) | 8000 | 3946 | **49,3%** | 48,5% – 50,1% | -5 | 4,26 | 2,28 – 2,31 | 2,03 |
| Motore PIMC (400 mondi) con 10% di errori | 8000 | 4620 | **57,8%** | 56,9% – 58,6% | +54 | 4,25 | 2,42 – 2,15 | 2,07 |
| Motore PIMC (400 mondi) · gioca per la differenza punti | 8000 | 4020 | **50,2%** | 49,4% – 51,1% | +2 | 4,24 | 2,28 – 2,31 | 1,99 |

| Avversario | Come gioca |
|---|---|
| ISMCTS (1000 iterazioni) | Ricerca ad albero Monte Carlo (famiglia diversa dal motore), 1000 iterazioni |
| ISMCTS (5000 iterazioni) | ISMCTS con 5000 iterazioni |
| Motore ad albero (1000 iterazioni) | Ricerca ad albero potenziata con il finale esatto e il valore della partita |
| Motore PIMC (100 mondi) | Lo stesso motore con 100 mondi |
| Motore PIMC (1000 mondi) | Lo stesso motore con 1000 mondi (più del doppio del calcolo) |
| Motore PIMC (400 mondi) con 10% di errori | Lo stesso motore ma distratto: sbaglia una mossa su dieci |
| Motore PIMC (400 mondi) · gioca per la differenza punti | Lo stesso motore che gioca per la differenza punti della smazzata e non per la partita |

## Come riprodurlo

```
cargo run --release -p scopa-sim -- report --player engine:400 --deals 4000 --self-deals 4000 --seed 20261023 --out bench/SCOPA.md
```

136000 partite in tutto, circa 5 ore e mezza su 4 core (calcolo ripreso a blocchi con `SCOPA_SIM_CACHE`).

Nell'app l'assistente usa di base 1000 mondi (regolabili): un po' più forte del motore provato qui, che contro la versione a 1000 mondi fa il 49,3%.
