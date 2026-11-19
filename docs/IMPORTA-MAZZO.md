# Importare un mazzo

Nella lettura a schermo di Briscola o Scopa, apri **Importa mazzo** e seleziona
la cartella digitandone il percorso. La cartella deve avere 40 ritagli PNG già
etichettati. I nomi vanno da `00.png` a `39.png`. Le decine rappresentano
denari, coppe, spade e bastoni. Le unità indicano in ordine asso, due, tre,
quattro, cinque, sei, sette, fante, cavallo e re. Per esempio, `28.png`
è il cavallo di spade.

Controlla l’anteprima carta per carta. La verifica segnala file mancanti,
ritagli senza disegno leggibile e immagini quasi identiche. L’anteprima
permette di controllare le etichette prima del salvataggio. Il tasto di
conferma importa le 40 carte in un unico aggiornamento della memoria locale.

Per raddrizzare una carta trapezoidale aggiungi, facoltativamente, un file
`prospettiva.json` nella cartella. Per ogni ritaglio da correggere, indica
i quattro angoli in senso orario partendo da quello in alto a sinistra.
Le coordinate sono frazioni della larghezza e dell’altezza del ritaglio.

```json
{"28.png": [[0.12, 0.06], [0.85, 0.12], [0.93, 0.94], [0.08, 0.89]]}
```

## Diagnosi e validazione

**Salva diagnosi** include i ritagli che l’app ha classificato, i cinque
candidati principali, la fascia visibile e i riquadri dei simboli separati.
I file restano sul computer. Lo screenshot di diagnosi è l’ultimo
disponibile e può precedere o seguire la scena riportata nel testo.

Con `BRIS_PROFILO_CARTE=1` l’app misura cattura, rilevamento e classificazione
separatamente. Per valutare una soglia di conferma, esegui il replay
annotato su due gruppi di fotogrammi distinti e passa i log a
`python scripts/calibra-conferme.py calibrazione.log verifica.log`.
Lo script confronta le conferme e gli errori automatici per ruolo rispetto
alla soglia corrente. Le soglie di produzione si cambiano solo dopo
una verifica su più partite indipendenti.
