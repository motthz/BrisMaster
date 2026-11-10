# Stato del lavoro sulla base 0.6.0

Richiesta: riconoscimento e Psy Fly in tutti gli otto giochi, interfaccia più curata,
valutazioni efficienti e lavoro riprendibile senza servizi esterni.
Mazzi scelti: napoletane per Briscola, Scopa e Sette e mezzo; francesi per gli altri.

## Comportamento implementato

- Briscola e Scopa conservano ricostruzione automatica, cronologia e tracking esistenti.
- Gli altri sei giochi hanno un lettore comune con conferma dell'inserimento e
  scelta/correzione dell'etichetta. Il ruolo della carta è quello selezionato
  nell'assistente: non si inventano puntate, calate o turni dai soli pixel.
- Riferimenti francesi: 52 carte e due disegni di jolly, che valgono lo stesso tipo
  di carta. Le doppie copie rimangono distinte sullo schermo. Un disegno non
  riconosciuto può essere etichettato e appreso; non viene inserito automaticamente.
- Memorie separate per napoletane e francesi. Esempi danneggiati vengono scartati con
  segnalazione, senza alterare il file originale durante il caricamento.
- Psy Fly riceve il consiglio del gioco attivo. Per decisioni come carta/stai o
  puntate mostra il testo; evidenzia scarti/cambi soltanto quando tutte le carte
  richieste sono localizzate senza ambiguità e la cattura è fresca. Nel lettore
  confermato occorre segnare «La mia mano»: non si assume che una carta unica sul
  tavolo sia una propria carta da scartare. Nessun clic
  automatico. Un clic non anticipa più i cuori di una giocata confermata.
- Palette comune, controlli distribuiti su righe distinte, aggiornamenti nel piè
  di pagina: un errore di rete non copre più la selezione del gioco.

## Motori

- Briscola/Scopa: iteratori di carte con dimensione esatta per evitare riallocazioni.
  La ricerca Scopa distribuisce anche il resto del budget tra i thread.
- Blackjack: riuso immediato dei risultati della ricorsione, senza interrogare di
  nuovo il banco su una posizione già risolta.
- Burraco: accumulo per thread, senza conservare una matrice per tutte le simulazioni;
  confronto a stessi semi tra percorso seriale e parallelo.
- Hold'em: stato del board riutilizzato nelle valutazioni; al river, equità esatta
  su tutte le 990 mani avversarie, invece di 4.000 campioni Monte Carlo.
- Poker italiano: eliminazione dell'ordinamento intermedio quando la misura di
  forza interna è indipendente dall'ordine; invariati tutti i valori delle 32
  maschere di cambio. I callback pubblici continuano a ricevere mani ordinate.
- Scala 40: riduzione delle allocazioni nella ricerca e nella valutazione delle
  connessioni. Verificare i tempi prima di attribuire un guadagno quantitativo.
- Sette e mezzo: mantenuto il risolutore esatto; la nuova integrazione usa gli
  stessi EV e lo stesso modello del banco già selezionato nell'app.

Non sono stati sostituiti i modelli allenati, né attribuiti nuovi rating o
percentuali di vittoria. Le prove della 0.6.0 mostrano che aumentare le iterazioni
non rende automaticamente più forte la Briscola.

## Verifiche eseguite

- Suite del workspace in release: **215 passati, 0 falliti, 10 ignorati**; build
  dell'app completata. Log e metadati in `bench/quality-0.6.0/validation.log` e
  `validation.json`. I due avvisi di compilazione preesistenti non bloccano la build.
- Test di conversione dei mazzi, conferma nei sei nuovi giochi, jolly, doppie copie,
  memoria danneggiata, ruolo della mano e coordinate obsolete superati.
- Prova GUI con immagine di riferimento: tre carte francesi rilevate, confermate
  nel Blackjack, EV mostrati e Psy Fly con consiglio «Stai». È una prova della
  catena software, non della cattura nativa Windows o di un sito reale.
- Riferimenti ricostruiti da revisione fissata, con verifica dei 54 checksum.
- Backup Git bundle verificato e clonato senza accesso a GitHub; integrità Git
  controllata con `git fsck --full`.

## Verifiche da riprendere

- Eseguire `scripts/validate.sh` dopo ogni modifica, conservando il risultato.
- Primo confronto dei kernel eseguito contro il tag 0.6.0: dati grezzi e limiti
  in `bench/quality-0.6.0/README.md`. Ripeterlo con `scripts/bench-kernels.sh`.
- Verificare Windows: cattura, click-through, esclusione dell'overlay dalle foto,
  DPI 100/125/150/200%, monitor a scale diverse e coordinate negative.
- Provare i mazzi francesi su tavoli reali: font e disegni diversi, ventagli,
  sovrapposizioni, rotazioni, animazioni e scritte sulle carte. Le prove sintetiche
  sui riferimenti inclusi non sono una certificazione di tutti i siti.
- Completare la ricostruzione *automatica* dei turni degli altri sei giochi con
  replay annotati di partite complete. L'inserimento confermato è intenzionale:
  l'estensione attuale non equivale al lettore automatico di Briscola/Scopa.
- Prima di dichiarare motori più forti: banco in duplicato con più semi e avversari,
  intervalli di confidenza e confronto contro la 0.6.0 invariata.

Il progetto non è dichiarato “flawless”: il comportamento completo su Windows e
su disegni diversi deve essere dimostrato dalle prove sopra.
