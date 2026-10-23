# Texas Hold'em: rapporto del motore

No Limit testa a testa, bui 50/100, stack di 100 grandi bui ricaricati a ogni mano. Ogni smazzata si gioca due volte a posti invertiti (duplicato); negli all-in prima del river si conta il valore atteso sulle carte mancanti. Risultato in grandi bui ogni 100 mani (bb/100) per il motore: 0 è la parità. Intervallo di confidenza al 95% sulle medie per smazzata.

40000 smazzate per avversario (80000 mani), seme 20261023.

| Avversario | bb/100 | IC 95% | Senza correzione all-in | Mani vinte | Showdown | |
|---|---:|---|---:|---:|---:|---|
| Azioni a caso (`random`) | **+227,8** | +215,6 .. +239,9 | +236,0 | 46,3% | 17,1% | vince |
| Stazione di chiamata: paga sempre, non rilancia e non lascia mai (`station`) | **+238,2** | +226,5 .. +249,9 | +243,6 | 46,5% | 95,3% | vince |
| Maniaco: rilancia il piatto a ogni occasione (`maniac`) | **+245,3** | +225,9 .. +264,6 | +253,4 | 10,5% | 13,8% | vince |
| Roccia: gioca solo il 12% delle mani migliori, punta solo con mani fortissime (`rock`) | **+25,9** | +21,1 .. +30,7 | +25,5 | 82,5% | 8,1% | vince |
| Stretto-aggressivo: apre metà delle mani, punte e rilanci con criterio, qualche bluff (`tag`) | **+18,9** | +11,1 .. +26,7 | +19,2 | 63,9% | 11,7% | vince |
| Largo-aggressivo: apre l'80% delle mani, bluffa spesso (`lag`) | **+82,9** | +71,6 .. +94,2 | +82,7 | 44,3% | 13,2% | vince |
| Largo-passivo: entra pagando con quasi tutto, punta solo con mani fortissime (`passive`) | **+31,7** | +23,4 .. +40,0 | +33,7 | 58,1% | 55,2% | vince |
| All-in o fold prima del flop con il 30% delle mani migliori (`shover`) | **+57,6** | +52,3 .. +62,9 | +56,0 | 63,2% | 14,8% | vince |
| Calcolatore: paga quando l'equità supera il prezzo, punta il piatto con le mani forti (`equity`) | **+31,4** | +22,4 .. +40,5 | +28,7 | 60,2% | 18,0% | vince |
| Puntate piccolissime (20% del piatto) e rilanci minimi, per mettere alla prova le misure fuori dall'albero (`tiny`) | **+28,0** | +20,5 .. +35,5 | +27,2 | 63,5% | 13,0% | vince |
| Puntate enormi (2,5 volte il piatto) con mani forti e bluff (`overbet`) | **+27,9** | +18,4 .. +37,3 | +27,9 | 57,2% | 6,6% | vince |
| Principiante: stretto-aggressivo che una volta su tre fa una mossa a caso (`noisy:0.3:tag`) | **+64,8** | +55,9 .. +73,8 | +66,5 | 59,3% | 13,0% | vince |
| Lo stesso motore, distratto: una mossa su dieci a caso (`noisy:0.1:engine`) | **+26,1** | +18,6 .. +33,7 | +24,9 | 49,0% | 16,9% | vince |
| Motore allenato con 2 milioni di iterazioni (`cfr:2000000`) | **+68,1** | +60,2 .. +75,9 | +66,3 | 57,6% | 8,7% | vince |
| Motore allenato con 10 milioni di iterazioni (`cfr:10000000`) | **+37,0** | +29,7 .. +44,3 | +39,3 | 54,4% | 11,9% | vince |
| Allenato con 20 milioni di iterazioni per battere proprio questo motore, conoscendone la strategia (`exploit:20000000`) | **+19,8** | +12,2 .. +27,4 | +21,3 | 50,7% | 7,5% | vince |

## Contro se stesso

Il motore contro una copia di se stesso deve dare 0 entro l'intervallo: verifica che il numero di smazzate basti a distinguere un vantaggio vero dal caso.

| Smazzate | bb/100 | IC 95% |
|---:|---:|---|
| 1000 | -9,6 | -50,0 .. +30,8 |
| 2000 | +4,7 | -22,5 .. +31,8 |
| 4000 | -6,6 | -26,6 .. +13,4 |
| 8000 | +1,2 | -12,8 .. +15,3 |
| 16000 | -6,4 | -16,4 .. +3,5 |
| 40000 | -7,6 | -14,1 .. -1,1 |

Altri semi, 40000 smazzate ciascuno:

| Seme | bb/100 | IC 95% |
|---:|---:|---|
| 7 | +2,2 | -4,4 .. +8,8 |
| 99 | +0,4 | -6,1 .. +7,0 |

Un intervallo al 95% esclude lo 0 per caso circa una volta su venti: per questo il controllo si ripete con più semi.

**L'avversario allenato per sfruttare il motore** gioca nella stessa astrazione e parte dalla strategia del motore come avversario fisso: in 20 milioni di iterazioni non trova un modo di batterlo. È un controllo di robustezza, non una misura completa di quanto il motore sia sfruttabile: un giocatore umano molto forte o un risolutore con un'astrazione più fine potrebbero trovare punti deboli che questo test non vede.

Tempo totale: 452 s.
