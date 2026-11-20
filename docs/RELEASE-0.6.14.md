## Riconoscimento delle carte di briscola

Questa versione recupera più carte quando il sito scurisce il tavolo o comprime le proporzioni delle carte. Il riconoscitore compensa l'esposizione e le differenze tra i canali di colore. Applica la correzione delle proporzioni quando trova almeno due forme compatibili. Un controllo sui pannelli identici ripetuti riduce i casi in cui elementi dell'interfaccia vengono scambiati per carte.

## Confronto con la 0.6.13

L'audit ha confrontato le due versioni sugli stessi casi, misurando 43 indicatori su 32 fattori. Le percentuali seguenti descrivono il banco di prova. La percentuale mostrata durante una partita sul proprio sito dipende dalle immagini effettivamente catturate.

| Misura | 0.6.13 | 0.6.14 |
|---|---:|---:|
| Carte identificate correttamente | 1.610/1.896 (84,9%) | 1.709/1.896 (90,1%) |
| Carte corrette accettate automaticamente | 1.514 | 1.598 |
| Identificazioni errate dichiarate certe | 35 | 26 |
| Schermate vuote con falsi rilevamenti | 8/48 | 0/48 |
| Carte corrette con più fattori di stress combinati | 125/180 | 149/180 |

Nel confronto carta per carta, cinque identificazioni corrette che prima passavano automaticamente richiedono ora una verifica manuale. Nei casi confrontati nessuna carta identificata correttamente dalla 0.6.13 ha ricevuto un'identità errata dalla 0.6.14.

Su due partite simulate con luminosità ridotta al 20% tra la quinta e la quindicesima presa, le carte identificate correttamente salgono da 17/80 a 80/80. I turni con consiglio pronto e stato di gioco esatto salgono da 8/40 a 37/40.

## Limiti osservati

Sulle 44 partite standard i contatori monitorati hanno dato gli stessi risultati nelle due versioni. In 101 turni su 880 il consiglio risultava pronto con uno stato di gioco inesatto. Una presa persa fra due catture può ancora compromettere la ricostruzione. Nella prova dedicata alle carte minuscole sono state riconosciute 0/12 carte a 32 pixel e 9/12 a 64 pixel. Questi risultati lasciano aperti sia il problema delle carte molto piccole sia quello della continuità dei consigli.

## Installazione Windows

Scarica e apri **BrisMaster-Setup.exe** per installare l'app per il tuo utente. Per avviarla senza installazione, scarica **BrisMaster.exe**. Per ciascun eseguibile trovi il relativo file SHA-256 negli allegati.

[Modifiche al riconoscitore e all'audit](https://github.com/Motthz/brismaster/pull/63)
