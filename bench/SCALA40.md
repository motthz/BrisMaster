# Scala 40: rapporto del banco di prova

Giocatore: `engine`. Ogni riga sono **8000 partite** complete (smazzate fino a 101 punti): 4000 coppie giocate in duplicato, cioè gli stessi mazzi a posti invertiti. Punteggio: vittoria 1, pari ½; il 50% è la parità. L'intervallo di confidenza al 95% è calcolato sulle coppie. Seme 20261023.

| Avversario | Punteggio | IC 95% | Smazzate chiuse | Punti presi / dati per smazzata | Obiettivo 55% |
|---|---:|---:|---:|---:|:---:|
| `random` | 95.6% | 95.2–96.1% | 75.5% | 4.7 / 35.5 | ✅ |
| `noisy:0.3:basic` | 82.1% | 81.3–82.8% | 62.6% | 7.3 / 19.6 | ✅ |
| `basic` | 72.1% | 71.2–72.9% | 58.2% | 8.2 / 15.0 | ✅ |
| `pairs` | 62.6% | 61.7–63.5% | 53.4% | 9.8 / 13.7 | ✅ |
| `defensive` | 58.1% | 57.2–59.0% | 51.7% | 10.2 / 12.7 | ✅ |
| `hoarder` | 62.2% | 61.3–63.2% | 53.3% | 10.0 / 13.8 | ✅ |
| `hoarder:2` | 66.5% | 65.6–67.5% | 53.3% | 10.0 / 15.7 | ✅ |
| `noisy:0.1:engine` | 57.6% | 56.8–58.4% | 52.2% | 10.0 / 12.1 | ✅ |
| `engine:look=0` | 55.7% | 54.8–56.6% | 52.5% | 9.9 / 11.4 | ✅ |
| `engine:pair=14,outs=0.5,dpartner=1` | 54.3% | 53.4–55.2% | 51.9% | 9.8 / 10.9 | ❌ |

- `random`: Carta a caso: cala quando può, scarta a caso, non prende mai dal pozzo
- `noisy:0.3:basic`: Principiante: regole semplici, ma una mossa su tre è a caso
- `basic`: Regole semplici: prende dal pozzo se può usarlo, cala appena può, scarta la carta spaiata più alta
- `pairs`: Tiene le coppie: protegge le carte che possono diventare combinazioni, scarta la spaiata più alta
- `defensive`: Difensivo: non scarta mai carte che l'avversario può attaccare, preferisce valori che l'avversario ha già scartato
- `hoarder`: Chiude in mano: tiene le combinazioni nascoste e cala tutto insieme, a meno che l'avversario non abbia 5 carte o meno
- `hoarder:2`: Chiude in mano, più azzardato: cala prima solo se l'avversario ha 2 carte o meno
- `noisy:0.1:engine`: Motore distratto: lo stesso motore che sbaglia una mossa su dieci
- `engine:look=0`: Versione precedente del motore: stessa valutazione, ma senza guardare la pescata successiva
- `engine:pair=14,outs=0.5,dpartner=1`: Variante del motore con i pesi scritti a mano prima della taratura (valuta diversamente coppie e rischi)

## Commento

Il motore supera il 55% contro 9 avversari su 10. L'unico sotto è la variante con i pesi scritti a mano: è lo stesso
motore (stessa ricerca, stessa previsione sulla pescata) con una valutazione delle coppie un po' diversa, quindi quasi una
copia; il 54,3% (IC fino al 55,2%) dice che la taratura dei pesi vale circa 4 punti, non che esista uno stile che lo batte.
Contro una copia identica il risultato è il 50% per simmetria (vedi sotto): nessun motore può garantire più del 50%
contro chi gioca esattamente come lui.

Il vantaggio più grande arriva contro i giocatori umani tipici: chi scarta sempre la carta più alta, chi tiene tutto per
chiudere in mano, chi sbaglia ogni tanto. Il guadagno viene soprattutto dalle smazzate perse: il motore paga circa 10 punti
a smazzata contro i 13-15 degli avversari umani simulati, e resta senza aprire meno spesso.

## Contro se stesso

Il motore contro una sua copia, con mazzi diversi nelle due partite di ogni coppia (in duplicato verrebbe 50% esatto per costruzione): **49.4%** (IC 95% 48.3–50.5%) su 8000 partite. Il risultato atteso è il 50%: serve a controllare che il banco di prova non favorisca un posto.

Andamento al crescere delle partite:

| Partite | Punteggio | IC 95% |
|---:|---:|---:|
| 500 | 52.4% | 48.0–56.8% |
| 1000 | 49.5% | 46.3–52.7% |
| 2000 | 49.4% | 47.2–51.5% |
| 3000 | 49.4% | 47.6–51.2% |
| 4000 | 49.5% | 48.0–51.1% |
| 6000 | 49.1% | 47.9–50.4% |
| 8000 | 49.4% | 48.3–50.5% |

Per rifarlo:

```
cargo run --release -p scala40-sim -- report --player engine --matches 4000 --seed 20261023 --out bench/SCALA40.md
```
