# Blackjack: rapporto dei test massivi

Commit `459a861`, seme `20261023`. Ogni riga: **100 milioni di mani** simulate da un sabot vero (il consigliere dell'app: 1.0 milioni di mani, confrontato sulle stesse carte con la strategia di base, perché ogni sua decisione richiede un calcolo esatto).

Il blackjack si gioca contro il banco, non contro un avversario: il criterio non può essere "vincere più del 50% delle partite". Le misure sono:

- **Rendimento per mano**: guadagno medio per mano in percentuale della puntata. Negativo = si perde. Con l'IC 95%, l'intervallo in cui cade il valore vero con il 95% di probabilità.
- **Vantaggio del banco**: lo stesso numero cambiato di segno, la misura standard dei casinò.
- **Mani vinte / pari / perse**: le mani con raddoppi e divisioni contano una volta, col risultato netto.
- **Sessioni da 100 mani in attivo**: quante serate da 100 mani si chiudono in guadagno. È la cosa più vicina alla "percentuale di partite vinte" degli altri giochi.
- **Risultato atteso in 100 mani da 10 €**: quanto si perde in media.

Perché così tante mani: lo scarto tipo di una mano è circa 1,15 puntate, quindi con le 8.000 partite usate per la briscola l'incertezza sarebbe ± 2,5%, cinque volte il vantaggio del banco. Con 100 milioni di mani è ± 0,02%.

## 1. Strategie a confronto: Europeo (online tipico)

Regole: 6 mazzi, S17, senza carta coperta, raddoppio solo 9, 10, 11 anche dopo divisione, fino a 2 mani, BJ paga 3:2, rimescola a ogni mano.

| Strategia | Rendimento per mano | IC 95% | Vantaggio del banco | Mani vinte / pari / perse | Sessioni da 100 mani in attivo | Risultato atteso in 100 mani da 10 € |
|---|---:|---:|---:|---|---:|---:|
| Consigliere dell'app (scelta esatta per le carte sul tavolo) | -0,650% | ± 0,040% | 0,650% | 43,4% / 8,8% / 47,8% | — | -6,50 € |
| Strategia di base | -0,673% | ± 0,022% | 0,673% | 43,3% / 8,8% / 47,8% | 46,6% | -6,73 € |
| Strategia di base con 5% di errori | -2,088% | ± 0,022% | 2,088% | 42,8% / 8,6% / 48,6% | 41,5% | -20,88 € |
| Strategia di base con 20% di errori | -6,322% | ± 0,022% | 6,322% | 41,1% / 8,0% / 50,9% | 27,5% | -63,22 € |
| Mai sballare (sta da 12 in su) | -6,026% | ± 0,019% | 6,026% | 42,6% / 6,6% / 50,9% | 26,3% | -60,26 € |
| Imita il banco (carta fino a 17) | -5,678% | ± 0,019% | 5,678% | 41,1% / 9,8% / 49,1% | 27,2% | -56,78 € |
| Casuale (carta o stai a caso) | -29,846% | ± 0,019% | 29,846% | 31,9% / 4,1% / 64,0% | 0,1% | -298,46 € |

Il consigliere calcola per ogni decisione il valore atteso esatto di ogni azione con il sabot pieno meno le carte sul tavolo: con il rimescolamento a ogni mano è la strategia ottima. Guadagno sulla strategia di base, misurato sulle stesse carte: +0,022% ± 0,033% per mano. Le sessioni non sono calcolate per questa riga (confronto mano per mano).

Calcolo esatto per la strategia di base (EV del giro con la scelta migliore per ogni prima mano): -0,661%; simulazione: -0,673% ± 0,022%.

## 2. Quanto contano le regole del tavolo (strategia di base)

| Regole | Calcolo esatto | Simulazione | IC 95% | Sessioni da 100 mani in attivo | Dettaglio |
|---|---:|---:|---:|---:|---|
| Europeo (online tipico) | -0,661% | -0,673% | ± 0,022% | 46,6% | 6 mazzi, S17, senza carta coperta, raddoppio solo 9, 10, 11 anche dopo divisione, fino a 2 mani, BJ paga 3:2, rimescola a ogni mano |
| Atlantic City | -0,409% | -0,372% | ± 0,022% | 47,7% | 8 mazzi, S17, carta coperta (peek), raddoppio su qualsiasi coppia anche dopo divisione, fino a 4 mani, resa, BJ paga 3:2, rimescola al 75% |
| Las Vegas Strip | -0,580% | -0,561% | ± 0,022% | 47,0% | 6 mazzi, H17, carta coperta (peek), raddoppio su qualsiasi coppia anche dopo divisione, fino a 4 mani, resa, BJ paga 3:2, rimescola al 75% |
| Las Vegas 6:5 (sconsigliato) | -1,939% | -1,919% | ± 0,022% | 43,0% | 6 mazzi, H17, carta coperta (peek), raddoppio su qualsiasi coppia anche dopo divisione, fino a 4 mani, resa, BJ paga 6:5, rimescola al 75% |

Il calcolo esatto valuta ogni prima mano con le probabilità esatte del sabot e approssima le divisioni (niente ridivisioni); la simulazione gioca tutto, ridivisioni comprese. Le differenze tra le regole corrispondono a quelle pubblicate: il banco che chiede carta sul 17 morbido costa circa 0,2 punti, il blackjack pagato 6:5 circa 1,4 punti.

## 3. Contare le carte (Hi-Lo) serve solo con un sabot vero

Strategia di base (senza deviazioni) con puntata variabile secondo il conteggio Hi-Lo: 1 unità fino a conteggio vero 1, poi 2 unità per ogni punto in più, fino a 12. Il rendimento è per mano, in unità minime, e per unità effettivamente puntata.

| Situazione | Rendimento per mano (unità) | IC 95% | Puntata media | Rendimento per unità puntata |
|---|---:|---:|---:|---:|
| Sabot da 8 mazzi rimescolato al 75%, puntata fissa | -0,372% | ± 0,022% | 1,00 | -0,372% |
| Sabot da 8 mazzi rimescolato al 75%, Hi-Lo 1-12 | +0,386% | ± 0,042% | 1,37 | +0,282% |
| Rimescolamento a ogni mano (online), Hi-Lo 1-12 | -0,376% | ± 0,022% | 1,00 | -0,376% |

Nei giochi online a generatore casuale (e nelle macchine a rimescolamento continuo) il mazzo è completo a ogni mano: il conteggio resta a zero e non dà alcun vantaggio. Nei casinò fisici con sabot il conteggio può rendere positivo il rendimento, ma i casinò lo contrastano (rimescolamenti anticipati, limiti di puntata, allontanamento).

## Conclusioni

- Contro il banco non esiste una strategia che vinca nel lungo periodo con un mazzo rimescolato a ogni mano: anche la strategia perfetta, quella del consigliere, perde circa 0,650% per mano con le regole online tipiche.
- La strategia di base, la tabella classica, è quasi perfetta (perde solo 0,022% per mano rispetto al consigliere, che usa le carte esatte della mano).
- Giocare "a sensazione" costa molto: imitare il banco o non sballare mai perde da 5 a 10 volte di più.
- Le regole contano: cerca tavoli 3:2, con il banco che sta sul 17 morbido e con la resa; evita il 6:5.
- Nelle sessioni brevi la fortuna conta più della strategia: anche giocando perfettamente circa il 46,6% delle sessioni da 100 mani si chiude in attivo, ma la media resta negativa.

Tempo di calcolo: 549 s.
