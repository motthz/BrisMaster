# Poker all'italiana: rapporto del motore

Testa a testa, limite fisso: invito 1 fiche, puntate da 2 prima del cambio e da 4 dopo, al massimo 3 puntate per giro. Mazzo da 32 carte. Ogni mazzo si gioca due volte a posti invertiti (duplicato) e il risultato è la media delle due mani, in fiche per mano: 0 è la parità. L'intervallo di confidenza al 95% è calcolato sulle medie per mazzo.

## Il motore che si adatta: sessioni contro lo stesso avversario

Il motore conta, mano dopo mano, cosa fa l'avversario (quanto lascia, quanto punta, quante carte cambia, che mani mostra) e sposta il gioco verso la risposta allenata per lo stile che gli somiglia di più. Per misurarlo si giocano **sessioni** di 100 e di 500 mani contro lo stesso avversario, alternando il mazziere; ogni sessione si rigioca con gli stessi mazzi a posti invertiti.

**Criterio (il 60%).** Nel poker contare le mani vinte non ha senso: si perdono tante mani piccole lasciando e se ne vincono poche grandi. Il 60% si legge come **almeno 6 sessioni su 10 chiuse in attivo**. Più la sessione è lunga, meno pesa la fortuna delle carte.

1000 coppie di sessioni da 100 mani e 300 da 500 per avversario, seme 20261023. Le fiche per mano (con l'intervallo al 95%) sono quelle delle sessioni da 500 mani.

| Avversario | Adattivo: fiche/mano | IC 95% | In attivo, 100 mani | In attivo, 500 mani | Solo equilibrio: fiche/mano | In attivo, 100 mani | 500 mani | 60% |
|---|---:|---|---:|---:|---:|---:|---:|---|
| Stazione di chiamata: vede sempre, non lascia e non rilancia mai (`station`) | **+1,200** | +1,188 .. +1,213 | 98,4% | 100,0% | +0,261 | 68,9% | 89,5% | sì, già in 100 mani |
| Maniaco: punta e rilancia sempre (`maniac`) | **+3,430** | +3,401 .. +3,458 | 99,6% | 100,0% | +0,433 | 70,2% | 89,7% | sì, già in 100 mani |
| Roccia: gioca solo le mani molto forti (`rock`) | **+0,482** | +0,474 .. +0,490 | 92,2% | 100,0% | +0,061 | 59,1% | 70,8% | sì, già in 100 mani |
| Giocatore da bar: regole di buon senso, bluffa di rado (`bar`) | **+0,165** | +0,155 .. +0,174 | 62,5% | 83,2% | +0,040 | 54,5% | 58,8% | sì, già in 100 mani |
| Stretto-aggressivo: poche mani, giocate con decisione, qualche bluff (`tag`) | **+0,217** | +0,208 .. +0,226 | 67,7% | 89,8% | +0,057 | 54,8% | 61,5% | sì, già in 100 mani |
| Largo-aggressivo: molte mani, molti bluff (`lag`) | **+0,446** | +0,433 .. +0,458 | 76,4% | 97,0% | +0,134 | 58,5% | 74,2% | sì, già in 100 mani |
| Largo-passivo: vede molto, punta solo con mani fortissime (`passive`) | **+0,448** | +0,441 .. +0,454 | 87,7% | 100,0% | +0,081 | 60,0% | 69,0% | sì, già in 100 mani |
| Bluffatore: bluffa spesso e resta servito con mani deboli (`bluffer`) | **+0,470** | +0,458 .. +0,482 | 80,0% | 98,3% | +0,129 | 61,0% | 75,0% | sì, già in 100 mani |
| Persone prese a caso: soglie, bluff e testardaggine scelti a caso per ogni sessione (stili mai visti in allenamento) (`pop`) | **+0,285** | +0,254 .. +0,316 | 69,3% | 85,2% | +0,123 | 60,3% | 71,7% | sì, già in 100 mani |
| Come sopra, ma distratte: fino a una mossa su tre a caso (`pop:0.3`) | **+0,341** | +0,318 .. +0,364 | 74,9% | 93,5% | +0,240 | 69,3% | 86,7% | sì, già in 100 mani |
| Cambia stile a metà: vede sempre per 100 mani, poi bluffa spesso (`switch:100:station>bluffer`) | **+0,504** | +0,493 .. +0,516 | 98,4% | 98,7% | +0,152 | 68,9% | 77,5% | sì, già in 100 mani |
| Il motore di equilibrio (un avversario quasi perfetto) (`engine`) | **+0,002** | -0,005 .. +0,010 | 47,9% | 51,0% | +0,003 | 47,9% | 50,5% | no |

Contro il motore di equilibrio il guadagno resta zero, come deve essere: contro chi gioca quasi alla perfezione nessun algoritmo guadagna. Il guadagno viene dagli errori costanti degli avversari. Attenzione: anche le "persone prese a caso" sono giocatori a regole, solo con parametri che il motore non ha mai visto; le persone vere sbagliano in modi più vari, quindi i numeri reali saranno più bassi di questi, soprattutto nelle prime mani contro un avversario nuovo.

## Sicurezza: quanto si rischia contro chi studia il motore

Ogni risposta a uno stile è allenata in modo prudente: durante l'allenamento l'avversario gioca quello stile solo il 55% delle volte, e il resto delle volte gioca la strategia che batte meglio la risposta stessa. La prima colonna dice quanto vincerebbe, per mano, un avversario allenato (10 milioni di iterazioni) apposta contro la risposta a quello stile, se il motore la giocasse sempre. La seconda è il caso più cattivo per il motore adattivo: un avversario che per 150 mani imita lo stile, così il motore si sposta sulla risposta, e poi gioca proprio la strategia che batte quella risposta (sessioni da 500 mani).

| Stile | Risposta bloccata contro chi la studia | IC 95% | Adattivo contro chi imita e poi colpisce | IC 95% |
|---|---:|---|---:|---|
| station | -0,274 | -0,293 .. -0,254 | +0,380 | +0,369 .. +0,391 |
| passive | -0,154 | -0,170 .. -0,138 | +0,168 | +0,159 .. +0,177 |
| rock | -0,294 | -0,319 .. -0,269 | +0,207 | +0,196 .. +0,218 |
| bar | -0,111 | -0,132 .. -0,089 | +0,084 | +0,074 .. +0,094 |
| tag | -0,138 | -0,160 .. -0,115 | +0,091 | +0,080 .. +0,102 |
| lag | -0,122 | -0,143 .. -0,101 | +0,151 | +0,140 .. +0,162 |
| bluffer | -0,205 | -0,228 .. -0,182 | +0,162 | +0,150 .. +0,174 |
| maniac | -0,395 | -0,421 .. -0,370 | +1,040 | +1,022 .. +1,057 |

## Il motore di base: l'equilibrio

Qui il motore non impara l'avversario: gioca sempre la strategia di equilibrio, che è il punto di partenza del motore adattivo e quello che gioca finché non ha capito chi ha davanti.

100000 mazzi per avversario (200000 mani), seme 20261023.

**Criterio.** Nella briscola si contano le partite vinte; nel poker conta quante fiche si vincono in media per mano (1 fiche = un invito), perché si possono perdere tante mani piccole e vincerne poche grandi. Il poker ha molta più varianza: con lo stesso criterio della briscola (8.000 partite) l'intervallo sarebbe largo ±0,04 fiche, più dei vantaggi da misurare. Per questo si giocano 100.000 mazzi per avversario (200.000 mani), che danno un intervallo di circa ±0,01 fiche per mano; la tabella del motore contro se stesso, in fondo, mostra che a questo numero di mazzi una copia identica dà 0 entro l'intervallo. **Vince** vuol dire che tutto l'intervallo è sopra lo zero.

| Avversario | Fiche per mano | IC 95% | Mani vinte | Piatti a showdown | |
|---|---:|---|---:|---:|---|
| Azioni a caso (`random`) | **+0,819** | +0,800 .. +0,838 | 45,6% | 39,8% | vince |
| Stazione di chiamata: vede sempre, non lascia e non rilancia mai (`station`) | **+0,245** | +0,231 .. +0,259 | 45,1% | 70,6% | vince |
| Maniaco: punta e rilancia sempre (`maniac`) | **+0,420** | +0,393 .. +0,448 | 23,5% | 31,3% | vince |
| Roccia: gioca solo le mani molto forti (`rock`) | **+0,056** | +0,048 .. +0,064 | 62,4% | 46,7% | vince |
| Giocatore da bar: regole di buon senso, bluffa di rado (`bar`) | **+0,030** | +0,021 .. +0,040 | 54,2% | 40,7% | vince |
| Stretto-aggressivo: poche mani, giocate con decisione, qualche bluff (`tag`) | **+0,048** | +0,038 .. +0,058 | 54,6% | 36,8% | vince |
| Largo-aggressivo: molte mani, molti bluff (`lag`) | **+0,125** | +0,111 .. +0,139 | 43,3% | 31,6% | vince |
| Largo-passivo: vede molto, punta solo con mani fortissime (`passive`) | **+0,084** | +0,074 .. +0,094 | 54,8% | 71,5% | vince |
| Bluffatore: bluffa spesso e resta servito con mani deboli (`bluffer`) | **+0,114** | +0,101 .. +0,126 | 44,9% | 28,1% | vince |
| Principiante: giocatore da bar che una volta su tre fa una mossa a caso (`noisy:0.3:bar`) | **+0,273** | +0,260 .. +0,286 | 51,6% | 40,2% | vince |
| Lo stesso motore, distratto: una mossa su dieci a caso (`noisy:0.1:engine`) | **+0,073** | +0,062 .. +0,084 | 49,2% | 41,9% | vince |
| Motore allenato con 200.000 iterazioni (un centesimo) (`cfr:200000`) | **+0,200** | +0,186 .. +0,214 | 49,7% | 42,9% | vince |
| Motore allenato con 2 milioni di iterazioni (un decimo) (`cfr:2000000`) | **+0,068** | +0,056 .. +0,080 | 49,3% | 42,1% | vince |
| Allenato con 4 milioni di iterazioni per battere proprio questo motore, conoscendone la strategia (`exploit:4000000`) | **+0,037** | +0,025 .. +0,049 | 47,1% | 38,5% | vince |
| Come sopra, con 20 milioni di iterazioni: l'avversario più forte possibile contro questo motore (`exploit:20000000`) | **+0,014** | +0,004 .. +0,025 | 48,5% | 40,6% | vince |

## Contro se stesso

Il motore contro una copia di se stesso deve dare 0 entro l'intervallo: serve a verificare che il numero di mazzi basti a distinguere un vantaggio vero dal caso.

| Mazzi | Fiche per mano | IC 95% |
|---:|---:|---|
| 1000 | -0,044 | -0,140 .. +0,052 |
| 4000 | -0,014 | -0,060 .. +0,033 |
| 16000 | -0,012 | -0,035 .. +0,011 |
| 50000 | -0,006 | -0,019 .. +0,007 |
| 100000 | -0,006 | -0,015 .. +0,003 |

Tempo totale: 671 s.
