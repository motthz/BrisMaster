## BrisMaster 0.6.9.1 · Anteprima Windows x64

Scarica `BrisMaster-0.6.9.1-Windows-x64.zip`, estrai l'archivio e avvia `BrisMaster-0.6.9.1.exe`. L'archivio include l'hash SHA-256 dell'eseguibile. Per confrontare le versioni, tieni conto che le installazioni possono condividere la memoria locale delle carte.

### Riconoscimento delle carte

- **Stampe classiche.** Le concordanze tra i riferimenti di stampe diverse rafforzano la fiducia quando confermano l'identità già più probabile. Su carte color avorio, quando la briscola manca dalla prima lettura, una seconda passata più selettiva può separarla dal dorso sottostante. La passata aggiunge solo una briscola con sufficiente margine e diversa dalle carte già lette in mano o sul tavolo.
- **Geometria della mano.** Una possibile seconda fila richiede anche l'allineamento orizzontale. Questo distingue una carta giocata vicino alla mano da una seconda fila vera.
- **Identità duplicate.** Una lettura incerta può cedere a un'alternativa plausibile se la sua prima scelta coincide con una carta della stessa mano identificata con maggiore sicurezza. La nuova proposta conserva la necessità di conferma.
- **Cambi di stile e memoria.** Il riconoscitore rivaluta lo stile quando più carte intere contraddicono quello precedente. Per le carte già apprese, può confrontare anche l'orientamento a 180° quando la grafica è poco simile ai riferimenti.
- **Carte regionali strette.** Un recupero per contorni cerca carte allineate e della stessa misura quando manca la lettura ordinaria della mano. Conserva le posizioni già lette. Senza una corrispondenza affidabile in memoria, il contorno porta a una proposta da confermare.

### Validazione

| Prova | Risultato |
|---|---|
| Stampe classiche con carta avorio, contrasto ridotto e illustrazione traslata | 132/132 corrette e automatiche, contro 128/132 con i nuovi criteri disattivati |
| Tavoli standard | 198/198 corrette e automatiche |
| Tavoli con scritte | 198/198 corrette e automatiche, contro 197 automatiche nella 0.6.9 |
| Scenari storici di partita intera | 17/17 completati, una richiesta di conferma in meno nello scenario con carte piccole |
| Partite di riserva | 4/4 completate, contro 3/4 nella 0.6.9 |
| Suite dell'app | 98 test superati |

Il caso `riserva-freddo` arriva a 20 prese con il punteggio corretto e zero interventi. I confronti sono stati eseguiti per scenario e per posizione della carta. I test di riconoscimento sono stati eseguiti su Linux, con Windows x64 come piattaforma di destinazione dell'eseguibile.

### Limiti dell'anteprima

Il riconoscimento al primo incontro resta limitato sulle grafiche molto diverse dai riferimenti. Nel banco con due mazzi esterni si osservano 45/132 letture corrette sulle Piacentine e 18/132 sulle Bresciane. La cattura dal vivo e le prestazioni su Windows restano da verificare. Questa versione va provata come prerelease.
