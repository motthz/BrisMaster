# Poker all'italiana: rapporto del motore

Testa a testa, limite fisso: invito 1 fiche, puntate da 2 prima del cambio e da 4 dopo, al massimo 3 puntate per giro. Mazzo da 32 carte. Ogni mazzo si gioca due volte a posti invertiti (duplicato) e il risultato è la media delle due mani, in fiche per mano: 0 è la parità. L'intervallo di confidenza al 95% è calcolato sulle medie per mazzo.

100000 mazzi per avversario (200000 mani), seme 20261023.

**Criterio.** Nella briscola si contano le partite vinte; nel poker conta quante fiche si vincono in media per mano (1 fiche = un invito), perché si possono perdere tante mani piccole e vincerne poche grandi. Il poker ha molta più varianza: con lo stesso criterio della briscola (8.000 partite) l'intervallo sarebbe largo ±0,04 fiche, più dei vantaggi da misurare. Per questo si giocano 100.000 mazzi per avversario (200.000 mani), che danno un intervallo di circa ±0,01 fiche per mano; la tabella del motore contro se stesso, in fondo, mostra che a questo numero di mazzi una copia identica dà 0 entro l'intervallo. **Vince** vuol dire che tutto l'intervallo è sopra lo zero.

| Avversario | Fiche per mano | IC 95% | Mani vinte | Piatti a showdown | |
|---|---:|---|---:|---:|---|
| Azioni a caso (`random`) | **+0,822** | +0,803 .. +0,840 | 45,7% | 39,8% | vince |
| Stazione di chiamata: vede sempre, non lascia e non rilancia mai (`station`) | **+0,247** | +0,233 .. +0,261 | 45,2% | 70,7% | vince |
| Maniaco: punta e rilancia sempre (`maniac`) | **+0,422** | +0,394 .. +0,449 | 23,5% | 31,3% | vince |
| Roccia: gioca solo le mani molto forti (`rock`) | **+0,055** | +0,047 .. +0,063 | 62,5% | 46,7% | vince |
| Giocatore da bar: regole di buon senso, bluffa di rado (`bar`) | **+0,029** | +0,019 .. +0,039 | 54,2% | 40,8% | vince |
| Stretto-aggressivo: poche mani, giocate con decisione, qualche bluff (`tag`) | **+0,041** | +0,031 .. +0,051 | 54,6% | 37,0% | vince |
| Largo-aggressivo: molte mani, molti bluff (`lag`) | **+0,120** | +0,107 .. +0,134 | 43,3% | 31,6% | vince |
| Largo-passivo: vede molto, punta solo con mani fortissime (`passive`) | **+0,090** | +0,080 .. +0,100 | 54,9% | 71,4% | vince |
| Bluffatore: bluffa spesso e resta servito con mani deboli (`bluffer`) | **+0,108** | +0,096 .. +0,120 | 44,8% | 28,2% | vince |
| Principiante: giocatore da bar che una volta su tre fa una mossa a caso (`noisy:0.3:bar`) | **+0,272** | +0,259 .. +0,285 | 51,6% | 40,2% | vince |
| Lo stesso motore, distratto: una mossa su dieci a caso (`noisy:0.1:engine`) | **+0,082** | +0,070 .. +0,093 | 49,3% | 42,2% | vince |
| Motore allenato con 200.000 iterazioni (un centesimo) (`cfr:200000`) | **+0,164** | +0,151 .. +0,177 | 49,7% | 42,9% | vince |
| Motore allenato con 2 milioni di iterazioni (un decimo) (`cfr:2000000`) | **+0,056** | +0,044 .. +0,067 | 49,4% | 42,4% | vince |
| Allenato con 4 milioni di iterazioni per battere proprio questo motore, conoscendone la strategia (`exploit:4000000`) | **+0,027** | +0,016 .. +0,039 | 45,8% | 38,6% | vince |
| Come sopra, con 20 milioni di iterazioni: l'avversario più forte possibile contro questo motore (`exploit:20000000`) | **+0,010** | -0,000 .. +0,021 | 47,5% | 39,4% | pari |

## Contro se stesso

Il motore contro una copia di se stesso deve dare 0 entro l'intervallo: serve a verificare che il numero di mazzi basti a distinguere un vantaggio vero dal caso.

| Mazzi | Fiche per mano | IC 95% |
|---:|---:|---|
| 1000 | +0,029 | -0,069 .. +0,127 |
| 4000 | +0,011 | -0,037 .. +0,058 |
| 16000 | +0,002 | -0,022 .. +0,025 |
| 50000 | +0,002 | -0,012 .. +0,015 |
| 100000 | +0,001 | -0,009 .. +0,010 |

Tempo totale: 528 s.
