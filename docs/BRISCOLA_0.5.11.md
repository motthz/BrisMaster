# Briscola: verifica per la 0.5.11

## Provenienza e obiettivo

Base: v0.5.10 (`59da001`), più i commit recuperati dal branch
`claude/charming-carson-fnszwy`: `8688eca`, `7ed73f6`, `63ef5ef`.
Il lavoro riguarda la briscola. Il riconoscitore di immagini è condiviso con
la scopa: per questo si eseguono anche i test dell'app intera.

Le 148 registrazioni, le 30 partite di validazione e i risultati r6 citati
negli screenshot non sono versionati nella repo. I numeri riportati da Claude
non sono quindi presentati come risultati riprodotti in questa sessione.

## Esame di una partita completa

Il banco genera il mazzo con seme deterministico e segue distribuzione,
scelta del primo giocatore, prima giocata, risposta, permanenza della presa,
pescata, rimozione delle carte, esaurimento del mazzo, assegnazione della
briscola e ultima presa. Ogni stato è renderizzato e passa dal riconoscitore
reale; la verità è il motore `Game`, non l'output dell'assistente.
Le partite con coda verificano separatamente il trasporto delle letture e
l'aggiornamento dello stato; non sostituiscono il test dei pixel.

## Variabili e limiti delle prove

Nessun riconoscitore può recuperare dai pixel una carta mai visibile, una
schermata completamente nera o tutte le carte di prese giocate durante
un'assenza. L'obiettivo verificabile è allargare l'intervallo di riconoscimento,
non bloccarsi e dichiarare quando il recupero è incompleto. Non si promettono
estremi "irraggiungibili" per tutte le combinazioni possibili.

Le righe indicano il tipo di prova o la verifica ancora necessaria; non tutte
sono parametri spinti ai loro due limiti numerici. I risultati eseguiti sono
riportati separatamente sotto.

| Gruppo | Variabile | Estremi/scenari | Copertura prevista o disponibile |
|---|---|---|---|
| Geometria | Altezza carta | 70 / 280 px | partite estreme |
| Geometria | Rapporto larghezza/altezza | 0,44 / 0,90 | partite estreme |
| Geometria | Ventaglio | −12° / +12° | partite estreme |
| Geometria | Scala mazzo e briscola | 0,75 / 1 rispetto alla mano | partite estreme |
| Geometria | Briscola coperta dal mazzo | 45% / 65% | partite estreme |
| Geometria | Zoom browser | molto ridotto / molto ingrandito | banco registrazioni; dati non disponibili |
| Geometria | Scala DPI Windows | 100% / 300% | da verificare su Windows |
| Geometria | Risoluzione monitor | bassa / 4K | banco SCHERMO; registrazioni mancanti |
| Geometria | Posizione finestra | bordi / centro monitor | test inseguimento |
| Geometria | Spostamento finestra | ferma / trascinata durante il gioco | test inseguimento |
| Geometria | Numero monitor | uno / più monitor | da verificare su Windows |
| Geometria | Area scelta | intero desktop / ritaglio stretto | test geometria; verifica manuale Windows |
| Geometria | Carte tagliate dal bordo | integre / solo una parte visibile | test immagini parziali |
| Geometria | Sovrapposizione mano | carte separate / accavallate | ventaglio; limite non esaustivo |
| Geometria | Rotazione briscola | orizzontale / rovesciata | test riconoscimento |
| Geometria | Posizione briscola | sotto mazzo / vicina al tavolo | correzione layout esistente |
| Colore | Luminosità schermo | 100% / 45% RGB | partite estreme |
| Colore | Luce notturna | RGB 1/1/1 / 1/0,75/0,50 | partite estreme |
| Colore | Schermo freddo | RGB 1/1/1 / 0,55/0,75/1 | partite estreme |
| Colore | Saturazione | 0,2 / 1,8 | partite estreme |
| Colore | Scala di grigi | colore / luminanza identica sui canali | partite estreme |
| Colore | Gamma carte | 0,85 / 1,3 | test siti già presenti |
| Colore | Rotazione tinta | −15° / +15° | test siti già presenti |
| Colore | Contrasto carta/tavolo | panno quasi nero / crema quasi bianco | partite estreme |
| Colore | Trasparenza carte | opache / quasi invisibili | non quantificata; limite informativo |
| Colore | Carte disabilitate scurite | normali / velo scuro | ripieghe bianchi scuri; prova dedicata da estendere |
| Colore | Sovraesposizione | pixel integri / bianchi saturi | non quantificata; limite informativo |
| Colore | Colori del dorso | simile / diverso dalle carte | test falsi positivi |
| Sfondo | Tinta tavolo | verde / blu / rosso / quasi nero / crema | test siti e partite estreme |
| Sfondo | Texture | uniforme / legno / mare / cielo | test siti e partite |
| Sfondo | Loghi | assenti / opacità 0,9 | partite estreme |
| Sfondo | Chat e punteggi | assenti / numerose scritte | test falsi positivi e partite |
| Sfondo | Finestre di notifica | assenti / sopra le carte | layout prudente; copertura totale non ricostruibile |
| Sfondo | Interfaccia bianca simile alla carta | assente / pannelli bianchi | test scritte non carte |
| Sfondo | Altre carte sul desktop | assenti / fuori dal tavolo | filtraggio tavolo; verifica manuale da estendere |
| Immagine | Mazzo di riferimento | due disegni noti | test immagini e partite alternati |
| Immagine | Mazzo sconosciuto | mai visto / appreso | test unknown_deck_on_many_tables |
| Immagine | Valore della carta | asso / figura / numerale | 40 carte durante partite complete |
| Immagine | Seme | tutti e quattro | 40 carte durante partite complete |
| Immagine | Sfocatura | assente / filtro sintetico | partite estreme |
| Immagine | Compressione | PNG / JPEG | fixture esistenti; livelli non quantificati |
| Immagine | Antialiasing | campionamento normale / ridimensionato | render sintetico; non tutte le GPU |
| Immagine | Bordo bianco | integro / coperto | test immagini parziali |
| Immagine | Rumore | assente / molto forte | da estendere; non misurato |
| Tempo | Aggiornamento UI | frequente / una raccolta a fine partita | 100 partite in coda |
| Tempo | Durata carta in tavola | una lettura / molte letture | test avvistamenti e stati brevi |
| Tempo | Letture duplicate | assenti / ripetute | partite complete |
| Tempo | Piccoli cambiamenti | pochi pixel / cambio completo | test small_changes_are_kept |
| Tempo | Cattura lenta | 100 ms / analisi più lenta del gioco | banco CATTURA; registrazioni mancanti |
| Tempo | Tavolo fermo | breve / molte letture | controllore stallo |
| Tempo | Briscola intermittente | giusta / sbagliata alternata | test flickering e partite esistenti |
| Tempo | Lettura mano vecchia | recente / oltre finestra temporale | filtro avvistamenti recenti |
| Tempo | Cambio scheda | nessun cambio / più prese non viste | test recupero gap |
| Tempo | Errore cattura | assente / errore transitorio | gestione stato; verifica Windows da estendere |
| Partita | Chi comincia | io / avversario | semi pari e dispari |
| Partita | Chi apre ogni presa | io / avversario | partite complete casuali |
| Partita | Presa rimasta sul tavolo | tolta subito / resta dopo pescata | partite complete con linger |
| Partita | Ordine visuale della mano | stabile / nuova pescata in coda | partite complete |
| Partita | Carta fantasma | sparisce / resta disegnata dopo la giocata | test follows_the_screen |
| Partita | Pescata sbagliata | corretta / carta manuale errata | test corrects_a_wrong_draw_from_the_screen |
| Partita | Ultima pescata | briscola mia / avversaria | test regole e partite complete |
| Partita | Mazzo esaurito | 34 carte / zero | partite complete |
| Partita | Mano finale | tre / una / zero carte | partite complete |
| Partita | Nuova distribuzione | stesso / nuovo tavolo e briscola | test real_games_restart_on_a_new_table |
| Partita | Conferma utente | accetta / rifiuta carta | test richieste e memoria |
| Partita | Annullamento | nessuno / Ctrl+Z e reinserimento | test undone_cards_are_not_replayed |
| Partita | Ricostruzione | storico coerente / contraddizione schermo attuale | rebuild e controllore |
| Partita | Prese completamente non viste | zero / più prese | recupero dichiarato incompleto; nessuna invenzione dei punti |
| Memoria | Apprendimento | vuota / carte già confermate | test memoria |
| Memoria | Indizi annullati | presenti / rimossi | test Ctrl+Z |
| Memoria | Evento storico senza carta nota | nessuno / salto di riallineamento | test inference_survives_a_gap_in_the_game |
| Memoria | Persistenza dati | scrivibile / errore disco | stato Recorder; verifica Windows da estendere |

## Correzioni aggiuntive

- Il salto di riallineamento non viene interpretato come una giocata avversaria:
  l'inferenza riparte dagli eventi osservati successivi al salto.
- Le correzioni che richiedono di rigiocare carte mai viste vengono rifiutate;
  il programma mantiene la posizione recuperata e permette Ctrl+Z.
- L'ultima briscola pescata in mano non viene scartata come una lettura
  impossibile quando la risposta della diciassettesima presa era coperta.
- Le ricostruzioni lavorano su una copia isolata della memoria: un'ipotesi
  scartata non modifica le carte apprese o i file dell'utente.
- Ctrl+Z elimina anche la carta o il primo giocatore dagli indizi manuali usati
  per ricostruire la partita, così non possono tornare da uno storico obsoleto.
- Dopo prese non viste, un avviso persistente chiarisce che punti e carte
  mancanti non sono verificati, anche quando arriva una nuova lettura.

## Riproduzione

```sh
cargo test --release -p brismaster
cargo test --release -p brismaster briscola_extreme_games -- --ignored --nocapture
BRIS_STRESS_VALIDAZIONE=1 cargo test --release -p brismaster briscola_extreme_games -- --ignored --nocapture
# Un solo caso; il nome è stabile e il seme compare nell'output:
BRIS_STRESS_CASO=luce-calda cargo test --release -p brismaster briscola_extreme_games -- --ignored --nocapture
```

La validazione separata usa semi a partire da 99000, combinazioni diverse dalle
17 condizioni di sviluppo (semi 1200–1216), entrambi i mazzi e i due primi
giocatori. Per riprodurre 30 partite, eseguire i round da 0 a 7 (l'ultimo ne
contiene due):

```sh
for round in 0 1 2 3 4 5 6 7; do
  BRIS_STRESS_VALIDAZIONE=1 BRIS_STRESS_ROUND=$round cargo test --release -p brismaster briscola_extreme_games -- --ignored --nocapture || exit 1
done
```

Non è una validazione su browser reale o sulle registrazioni originali di Claude.
Il risultato finale richiede 20 prese, punti uguali alla verità, tutte e 40 le
carte giocate, mazzo e mani vuoti. Le discrepanze nei checkpoint intermedi sono
stampate separatamente: un recupero tardivo non equivale a riconoscere ogni
fotogramma correttamente. Il secondo numero di `Ok((20, N))` conta gli interventi
richiesti all'utente: conferme verificate sulla sequenza vera o inserimento della
briscola quando l'app la chiede esplicitamente. Non si inseriscono carte vere
senza che il programma chieda un intervento.

Il workflow di release esegue anche le 17 condizioni di sviluppo prima di
costruire l'eseguibile e pubblicare l'installatore.

## Risultati della sessione

In corso: aggiornare questa sezione con gli esiti effettivi prima del rilascio.
