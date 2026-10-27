# Burraco: rapporto dei test massivi

Motore provato: **Motore (16 simulazioni)** (`engine:16`), commit `1a2b3eb` (le modifiche successive non cambiano l'algoritmo). Ogni riga: 4000 mazzi giocati due volte a posti invertiti (8000 smazzate), seme `20261023`. Gli stessi mazzi per tutti gli avversari.

Riprodurre: `cargo run --release -p burraco -- report --deals 4000` (con `BURRACO_SIM_CACHE=cartella` un'esecuzione interrotta riparte dall'ultimo blocco). Con 4 core servono circa 4 ore.

Nell'app l'assistente usa lo stesso motore con 32, 96 o 300 simulazioni per scelta (veloce, normale, alta): più simulazioni del motore provato qui, quindi almeno altrettanto forte.

Regole: burraco a due giocatori, 11 carte, due pozzetti, chiusura con almeno un burraco e scarto finale. Una partita del test è una **smazzata**: vince chi fa più punti nella smazzata (carte calate, burraco, chiusura, meno le carte in mano e il pozzetto non preso).

## Riepilogo: obiettivo 55% contro ogni avversario

Sopra il 55% con certezza statistica (intero IC 95% sopra il 55%): **9 avversari su 9**. Il motore contro se stesso è escluso: lì il 50% è il risultato esatto per costruzione.

| Avversario | Punteggio | IC 95% | Obiettivo |
|---|---:|---|---|
| Motore (4 simulazioni) | 58,2% | 57,1% – 59,2% | ✅ sopra il 55% |
| Esperto | 60,8% | 59,7% – 61,8% | ✅ sopra il 55% |
| Chiude subito | 67,2% | 66,2% – 68,2% | ✅ sopra il 55% |
| Raccoglitore | 67,9% | 66,9% – 68,9% | ✅ sopra il 55% |
| Esperto distratto | 68,2% | 67,2% – 69,2% | ✅ sopra il 55% |
| Trattenitore | 69,7% | 68,7% – 70,7% | ✅ sopra il 55% |
| Prudente | 71,4% | 70,4% – 72,3% | ✅ sopra il 55% |
| Principiante | 82,4% | 81,6% – 83,2% | ✅ sopra il 55% |
| Casuale | 86,9% | 86,1% – 87,6% | ✅ sopra il 55% |

Come leggere le colonne:

- **Punteggio**: vittoria 1, pareggio ½, sconfitta 0. Il 50% è la parità.
- **IC 95%**: intervallo di confidenza del punteggio, calcolato sulle coppie di smazzate dello stesso mazzo.
- **Elo**: la stessa differenza di forza in punti Elo (+35 ≈ 55%).
- **Scarto medio**: punti del motore meno punti dell'avversario, in media per smazzata.

## 1. Contro se stesso

| Avversario | Smazzate | V / P / S | % vittorie | Punteggio | IC 95% | Elo | Punti medi | Scarto medio | Chiude lui / l'avversario |
|---|---:|---:|---:|---:|---|---:|---:|---:|---:|
| Motore (16 simulazioni) (copia) | 8000 | 3987 / 5 / 4008 | 49,8% | **49,9%** | 48,8% – 50,9% | -1 | 413 | -0 | 49,8% / 50,0% |

Punteggio 49,9% (IC 48,8% – 50,9%): due copie dello stesso motore si dividono le smazzate a metà, come deve essere in duplicato. Serve a controllare che il banco di prova sia equo: contro un avversario forte quanto lui nessun programma può superare il 50%.

Chi inizia vince il 48,2% delle smazzate, chi è secondo il 51,4%: il duplicato compensa il vantaggio del posto.

## 2. Contro gli avversari

| Avversario | Smazzate | V / P / S | % vittorie | Punteggio | IC 95% | Elo | Punti medi | Scarto medio | Chiude lui / l'avversario |
|---|---:|---:|---:|---:|---|---:|---:|---:|---:|
| Casuale | 8000 | 6948 / 3 / 1049 | 86,9% | **86,9%** | 86,1% – 87,6% | +328 | 658 | +400 | 82,8% / 16,6% |
| Principiante | 8000 | 6588 / 9 / 1403 | 82,3% | **82,4%** | 81,6% – 83,2% | +268 | 718 | +410 | 76,6% / 20,8% |
| Chiude subito | 8000 | 5370 / 17 / 2613 | 67,1% | **67,2%** | 66,2% – 68,2% | +125 | 431 | +20 | 71,0% / 26,0% |
| Raccoglitore | 8000 | 5420 / 18 / 2562 | 67,8% | **67,9%** | 66,9% – 68,9% | +130 | 429 | +26 | 70,5% / 27,0% |
| Trattenitore | 8000 | 5574 / 0 / 2426 | 69,7% | **69,7%** | 68,7% – 70,7% | +145 | 410 | +296 | 72,5% / 21,3% |
| Prudente | 8000 | 5708 / 4 / 2288 | 71,4% | **71,4%** | 70,4% – 72,3% | +159 | 581 | +276 | 70,4% / 28,6% |
| Esperto distratto | 8000 | 5452 / 4 / 2544 | 68,2% | **68,2%** | 67,2% – 69,2% | +132 | 415 | +105 | 68,8% / 30,8% |
| Esperto | 8000 | 4857 / 7 / 3136 | 60,7% | **60,8%** | 59,7% – 61,8% | +76 | 389 | +8 | 62,2% / 37,0% |
| Motore (4 simulazioni) | 8000 | 4652 / 2 / 3346 | 58,1% | **58,2%** | 57,1% – 59,2% | +57 | 451 | +75 | 57,9% / 41,8% |

Gli avversari:

- **Casuale** (`random`): Pesca e scarta a caso, cala quello che vede.
- **Principiante** (`beginner`): Principiante: usa subito le matte, raccoglie gli scarti solo se servono subito, non guarda cosa regala.
- **Chiude subito** (`closer`): Chiude appena può, usa subito le matte.
- **Raccoglitore** (`hoarder`): Raccoglie sempre gli scarti e usa subito le matte.
- **Trattenitore** (`holder`): Tiene in mano le combinazioni e le cala solo per andare al pozzetto.
- **Prudente** (`classic`): Giocatore attento ma prudente: raccoglie gli scarti solo quando gli servono davvero.
- **Esperto distratto** (`noisy`): Esperto distratto: gioca come l'esperto, ma uno scarto su quattro è a caso.
- **Esperto** (`expert`): Esperto: lo stile tarato con decine di migliaia di smazzate, usato dal motore nelle simulazioni.
- **Motore (4 simulazioni)** (`engine:4`): Lo stesso motore con 4 simulazioni per scelta invece di 16.
