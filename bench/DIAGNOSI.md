# Diagnosi: dove perde la Massima della v0.5.0

Fase 2 del piano per la 0.6.0. Domanda: in quali momenti della partita conta di più sapere le carte avversarie?
Giocatore: Massima della v0.5.0 (`infer:20000:1:1000:0.25`). Seme `20261029`, 29 ottobre 2026.

## 1. L'oracolo a momenti

La Massima gioca contro se stessa, ma una delle due copie vede la mano avversaria (non il mazzo) solo in un momento
della partita. Ogni riga: 500 mazzi in duplicato (1.000 partite), IC 95% circa ±2,7 punti.

| La copia vede la mano avversaria… | Punteggio contro la Massima | IC 95% |
|---|---:|---|
| sempre (il baro, dal banco di prova) | 72,2% | 68,4% – 76,0% |
| **solo quando apre la presa** | **71,9%** | 69,3% – 74,5% |
| solo a metà partita (mazzo da 12 a 23 carte) | 60,4% | 57,6% – 63,2% |
| solo a inizio partita (mazzo da 24 carte in su) | 59,7% | 56,8% – 62,6% |
| solo nel pre-finale (mazzo da 1 a 11 carte) | 52,2% | 49,6% – 54,8% |
| solo quando risponde | 52,1% | 49,5% – 54,7% |

## 2. La mappa degli errori

500 partite della Massima contro se stessa: 17.000 decisioni col mazzo non vuoto, ognuna giudicata da un ISMCTS potenziato
da 20.000 iterazioni che vede la mano avversaria. «Vittorie perse» è la probabilità di vincere che la mossa scelta perde
rispetto a quella dell'oracolo (contiene anche un po' di rumore della ricerca).

| Momento | Decisioni | Mosse diverse dall'oracolo | Vittorie perse per decisione | Quota del totale |
|---|---:|---:|---:|---:|
| Mazzo 30–33 (prime prese) | 3000 | 38,9% | 1,4% | 17,3% |
| Mazzo 24–29 | 3000 | 33,0% | 1,4% | 17,5% |
| Mazzo 18–23 | 3000 | 31,4% | 1,7% | 20,3% |
| Mazzo 12–17 | 3000 | 30,2% | 1,5% | 18,6% |
| Mazzo 6–11 | 3000 | 29,5% | 1,6% | 19,3% |
| Mazzo 1–5 (pre-finale) | 2000 | 26,2% | 0,9% | 7,0% |
| **Quando apre** | 8500 | **48,1%** | **2,5%** | **86,9%** |
| Quando risponde | 8500 | 15,7% | 0,4% | 13,1% |

## Conclusioni

- **Il punto debole è l'apertura.** Vedere le carte avversarie solo quando si apre la presa vale quasi quanto vederle sempre
  (71,9% contro 72,2%), e l'87% delle vittorie perse viene da lì. Quando si risponde l'avversario ha già scoperto la sua carta e
  la Massima sbaglia poco (52,1%).
- **Il pre-finale pesa poco.** Vederlo vale solo il 52,2% e il 7% delle vittorie perse. Risolvere il pre-finale in modo esatto
  (fase 4 del piano) può dare al massimo un punto o due: passa in fondo alla lista.
- **Le perdite sono sparse su tutta la partita** (dal 17% al 20% per ogni tratto di mazzo): non basta migliorare un momento solo,
  serve dedurre meglio la mano avversaria a ogni apertura.

Nuovo ordine dei lavori: prima la deduzione (esatta e con una rete di politica migliore), poi la ricerca che non bara con se stessa
e il rinforzo, entrambe misurate soprattutto sulle aperture. Il pre-finale perfetto viene dopo.

## Come riprodurlo

```
export BRISCOLA_POLICY=crates/core/nets/politica.bin
P=infer:20000:1:1000:0.25
cargo run --release -p briscola-sim -- --player oracolo:apre:$P --opponent $P --deals 500 --seed 20261029
#   momenti: sempre, apre, risponde, inizio, meta, prefinale
cargo run --release -p briscola-sim -- diagnosi --player $P --deals 500 --oracolo 20000 --seed 20261029
```
