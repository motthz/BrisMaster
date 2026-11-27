## Diagnosi delle prese perse nel replay

```sh
BRIS_STRESS_WINDOW=CaptureLoss BRIS_WINDOW_TRACE=1 \
  cargo test --release --locked -p brismaster window_changes_during_whole_games -- --ignored --nocapture
```

Il replay rimuove il fotogramma che chiude le prese 3, 11 e 18 e inietta
due errori di cattura per ciascuna. Questo isola il recupero dello storico
quando la risposta è invisibile. Non misura una perdita interna alla coda.

| Seme | Giocate corrette | Mancanti | Errate | Prese registrate | Prima giocata errata o mancante |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 | 39/40 | 1 | 0 | 20 | 11, avversario |
| 2 | 36/40 | 2 | 2 | 19 | 3, avversario |

Nel seme 1, alla presa 11 l'apertura propria (carta 12) è visibile e
registrata. Quando torna la cattura, a 53.600 ms il tavolo è già vuoto
e la mano include la pescata 11. La risposta avversaria resta assente
anche quando l'apertura della presa 12 viene registrata correttamente.
Una ricerca più permissiva sul fotogramma successivo non può recuperare una
carta che non vi compare. Il caso successivo da riprodurre è una risposta
effettivamente catturata per un solo fotogramma, sotto pressione della coda.

Il seme 1 mostra anche un errore di ruolo senza perdita della giocata.
Alla presa 4, a 17.200 ms, la carta 19 è letta nella casella avversaria pur
essendo la carta appena giocata dall'utente. L'assistente la registra
comunque a suo nome partendo dalla mano. Quando entrambe le carte sono
visibili, il lettore assegna di nuovo la 19 alla casella corretta.
Questo caso richiede una misura sul ruolo visivo e una sullo storico.

## Una carta catturata e poi scartata

Il test `unique_card_frame_survives_a_full_queue` usa nove immagini di un
tavolo sintetico con le carte di riferimento dell'app. Solo la seconda
mostra la giocata avversaria, visibile per 16 ms. Le altre catture
alternano una lieve variazione di luminosità. L'analisi inizia a coda piena.

Con la politica della v0.6.20 il fotogramma è ammesso e poi espulso.
Con la candidata, i fotogrammi con meno di due celle di differenza
significativa rispetto a un vicino sono i primi candidati allo scarto.
Si conserva il fotogramma con la carta e il riconoscitore identifica la
giocata. Passano anche i sei test preesistenti eseguiti per schermate brevi,
cambi di seme a luminosità simile, cambi locali, timestamp ed errori di cattura.

Il caso riprodotto riguarda una coda già piena. La validazione su Windows
e il confronto su sequenze lunghe sotto carico sono ancora necessari per
misurare l'effetto end-to-end. Il test di cattura interrotta sopra resta
il controllo per una carta che l'app non ha avuto modo di vedere.
