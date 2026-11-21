# Riconoscimento delle partite · PR 64

Punto di ripresa del 21 novembre 2026. Il riferimento iniziale è [4231a52](https://github.com/Motthz/BrisMaster/commit/4231a5206257a6c21d3409e54bc8f16b5906da61). Il lavoro prosegue sul branch `recognition/compact-tables-and-late-evidence` della [PR 64](https://github.com/Motthz/BrisMaster/pull/64). La versione pubblicata usata come riferimento è la 0.6.14.

## 1. Modifiche già implementate

1. **2af011b** migliora la ricerca delle carte piccole, la separazione dal panno e le conferme durante le animazioni. La cattura tiene conto dei cambi di colore a luminosità costante.
2. **e5f93ed** ricostruisce alcune prese interrotte confrontando la mano prima e dopo la pescata. Il pannello indica presa e giocatore mancanti.
3. **4231a52** estende il recupero al mazzo esaurito. La ricostruzione richiede una risposta compatibile univoca. Il motore conserva la distinzione fra carta dedotta e carta identificata visivamente.

## 2. Risultati su 44 partite simulate

Ogni versione affronta le stesse distribuzioni. Le 40 giocate per partita comprendono anche quelle perse. Una giocata corretta richiede carta, presa e giocatore corretti.

| Misura | 0.6.14 | 2af011b | e5f93ed | 4231a52 |
|---|---:|---:|---:|---:|
| Giocate corrette / 1.760 | 1.681 | 1.699 | 1.704 | 1.704 |
| Corrette e confermate / 1.760 | 1.448 | 1.562 | 1.569 | 1.569 |
| Errate e confermate | 22 | 15 | 15 | 15 |
| Consiglio pronto con posizione errata / 880 turni | 101 | 86 | 86 | 86 |

Sull’ultimo commit le conferme corrette raggiungono l’89,1%. Il controllo aggiuntivo su otto replay con buchi nel finale dà 271/320 conferme corrette sia prima sia dopo l’ultima modifica. I 128 test unitari locali sono passati. L’esito aggiornato dei job remoti è nella [CI del commit misurato](https://github.com/Motthz/BrisMaster/actions/runs/44877333382). Rimane da misurare il risultato su partite reali annotate per sito e mazzo.

## 3. Prima divergenza negli otto replay con interruzioni

Dati letti dai log di 4231a52. La prima divergenza è la prima giocata attesa mancante o errata nello storico **a fine partita**. Il confronto finale da solo non stabilisce il primo istante in cui il motore ha perso il tavolo.

| Scenario | Seme del simulatore | Prima presa divergente | Giocatore | Giocate mancanti | Turni pronti con stato errato | Di cui con avviso parziale |
|---|---:|---:|---|---:|---:|---:|
| CaptureLoss | 1 | 11 | Avversario | 1 | 8 | 8 |
| CaptureLoss | 2 | 3 | Avversario | 2 | 15 | 15 |
| CaptureLoss | 31 | 3 | Avversario | 4 | 14 | 14 |
| CaptureLoss | 42 | 4 | Nostro | 0 | 2 | 0 |
| Interrupted | 1 | 11 | Avversario | 4 | 8 | 8 |
| Interrupted | 2 | 3 | Avversario | 4 | 16 | 16 |
| Interrupted | 31 | 3 | Avversario | 8 | 13 | 13 |
| Interrupted | 42 | 11 | Nostro | 5 | 7 | 7 |

I semi 2 e 31 concentrano 58 degli 83 turni problematici. In entrambi gli scenari il primo errore nello storico finale riguarda l’avversario alla terza presa. CaptureLoss 42 è un secondo caso prioritario perché i due turni errati non hanno un avviso parziale, pur avendo registrato tutte le giocate attese.

## 4. Riprodurre il confronto

Dalla radice del repository, eseguire i comandi seguenti in Bash o Git Bash su ciascuna revisione. Scegliere una directory di output distinta per ogni revisione. Servono il toolchain Rust e le dipendenze di compilazione del progetto.

```bash
set -euo pipefail
out="replay-$(git rev-parse --short HEAD)"
mkdir -p "$out"
unset BRIS_STRESS_WINDOW BRIS_WINDOW_HOLDOUT
cargo test --release -p brismaster window_changes_during_whole_games -- --ignored --nocapture 2>&1 | tee "$out/base.log"
BRIS_WINDOW_HOLDOUT=1 cargo test --release -p brismaster window_changes_during_whole_games -- --ignored --nocapture 2>&1 | tee "$out/holdout.log"
```

Dalla revisione candidata, passare le due coppie di log allo script, sostituendo i percorsi di esempio.

```bash
python scripts/audit-brisco/compare_games.py --before percorso/prima/*.log --after percorso/dopo/*.log
```

Il confronto controlla sei contatori per ogni coppia scenario/seme e segnala il singolo peggioramento anche quando il totale migliora. Per isolare i semi 1 e 2 di uno scenario, impostare per quella esecuzione `BRIS_STRESS_WINDOW=CaptureLoss` oppure `Interrupted`.

## 5. Prossimo intervento e criteri di verifica

1. Riprodurre la terza presa dei semi 2 e 31. Confrontare carta attesa, carta osservata e storico subito prima della perdita di cattura e al ritorno del tavolo.
2. Tracciare separatamente il primo turno errato in tempo reale, l’eventuale ritorno a uno stato corretto e la durata della divergenza. Isolare anche i due turni senza avviso del seme 42.
3. Correggere la causa riprodotta e aggiungere un test che fallisca sulla revisione precedente. Verificare sia i consigli errati sia quelli corretti disponibili.
4. Rieseguire i 44 replay e i casi mirati. Registrare commit, risultati per caso, eventuali regressioni ed esito CI nella PR.
5. Verificare il traguardo del 90% su partite complete annotate di siti e mazzi indipendenti. Conservare in locale screenshot e correzioni dei siti. Pubblicare solo risultati aggregati e casi sintetici riproducibili.


### Recupero del quattro di coppe · 8c213f7

Nel replay CaptureLoss, seme 31, l’app vede una carta isolata dal lato avversario, ma quella stessa carta era nella mano del giocatore ed è scomparsa. Il recupero usa la faccia osservata per l’identità e la variazione della mano per attribuirla al giocatore. Si applica nel riallineamento di una singola presa con storico precedente coerente. Richiede carte leggibili e una mano compatibile; la conferma visiva richiede osservazioni ripetute. Il quattro di coppe risulta registrato alla quarta presa già a 17.200 ms. La risposta nascosta alla terza presa e i suoi punti restano incerti.

| Misura sui 44 replay | 4231a52 | 8c213f7 |
|---|---:|---:|
| Giocate corrette | 1.704 | 1.705 |
| Corrette e confermate | 1.569 | 1.570 |
| Mancanti | 28 | 27 |
| Errate, di cui confermate | 28 (15) | 28 (15) |
| Turni con consiglio pronto e posizione corretta | 707 | 707 |
| Turni con consiglio pronto e posizione errata | 86 | 86 |

Nessuna regressione sui sei indicatori per singola partita. Negli otto replay supplementari con interruzioni nel finale, entrambe le versioni confermano 271 giocate corrette su 320, con 148 turni pronti corretti e 6 errati. Superati 129 test unitari locali; 22 test esclusi dalla corsa standard. Il nuovo test verifica anche che il recupero non inventi punti o una giocata avversaria.

[Confronto per ciascuna delle 44 partite](https://github.com/Motthz/BrisMaster/blob/recognition/compact-tables-and-late-evidence/docs/audit-pr64/8c213f7-replay.csv) · [CI del codice misurato](https://github.com/Motthz/BrisMaster/actions/runs/44879857857)

Il passo successivo è riprodurre CaptureLoss, seme 42: il registro arriva a 40 giocate ma in due turni il consiglio risulta pronto con posizione errata senza avviso di storico parziale.


### Consigli con tavolo discordante · 1114cbe

1. In CaptureLoss, seme 42, a 26.800 e 31.600 ms mano e insieme delle carte uscite coincidono con il simulatore. La posizione del motore ha tavolo vuoto; nel replay sono presenti rispettivamente cavallo di bastoni (38) e quattro di spade (23).
2. Quando la posizione resta ferma per almeno due controlli discordanti, il pannello avvisa che le letture differiscono dalla posizione usata. Il consiglio resta disponibile. La traccia facoltativa si attiva con BRIS_ADVICE_TRACE=1. Il test del seme 42 verifica che tutti i consigli pronti errati siano segnalati e che i turni corretti pronti non ricevano avvisi in questo caso.
3. Sui 44 replay, rispetto a 8c213f7, i consigli pronti errati segnalati passano da 81/86 a 86/86. Due dei 707 consigli con posizione corretta presentano un avviso nella candidata. Il riferimento non misurava questo secondo contatore. Sul seme 42 gli avvisi passano da 0/2 a 2/2 con 0/14 sui turni corretti. Le 1.570 conferme corrette su 1.760, i 707 consigli pronti corretti e gli 86 errati restano invariati. Nessuna regressione sui sei indicatori confrontati partita per partita. Superati i 129 test standard e il replay mirato con le nuove asserzioni.
4. Resta da recuperare la carta già visibile prima che il motore elabori il consiglio, verificando che le aperture transitorie durante le animazioni non vengano conteggiate due volte.


### Prima cattura di ogni giocata · 46f9d24

Il replay misura il primo fotogramma consegnato dopo la giocata, prima delle tre letture ripetute. Su 44 partite sintetiche, 1.736 giocate sono visibili e 24 cadono interamente durante un’interruzione. In 1.667 casi il classificatore propone l’identità corretta in uno dei due posti sul tavolo; in 1.572 anche il lato coincide. Nello stesso istante 73 giocate sono già registrate con carta, presa e giocatore corretti, zero sono anche confermate. A fine partita le conferme corrette sono 1.570 su tutte le 1.760 giocate. Il dato della prima lettura riguarda carte appena giocate sul tavolo, anche se già viste in mano. [CSV per partita](https://github.com/Motthz/BrisMaster/blob/2feab9e3c78f2e183bc2e07c00cf4c97e9fb7e36/docs/audit-pr64/prima-cattura-46f9d24.csv).

Quattro esperimenti anticipano l’assegnazione del lato quando il centro della carta è entro il 20% della distanza tra le caselle rispetto al centro più vicino. Risultati nello stesso ordine: lato corretto subito, conferme corrette finali, conferme errate finali.

| Variante | Carta e lato al primo fotogramma | Conferme corrette finali | Conferme errate finali |
|---|---:|---:|---:|
| Riferimento | 1572 | 1570 | 15 |
| Posizione vicina | 1649 | 1584 | 16 |
| Carta intera e almeno 64 px | 1644 | 1580 | 16 |
| Anche tre coppie già osservate | 1624 | 1576 | 16 |
| Carta intera ≥64 px, due carte in mano, briscola presente, spostamento verso il proprio lato | 1644 | 1580 | 16 |

Tutte registrano correttamente 73 giocate sul primo fotogramma. In ciascuna il replay Interrupted con seme 42 peggiora da 9 a 10 conferme errate. Le regole sperimentali sono state scartate. Il codice conservato estende soltanto il banco di prova e riproduce i sei contatori precedenti su tutte le partite. Prossima verifica: conservare la prima lettura come ipotesi provvisoria e misurare quando una seconda evidenza indipendente (variazione della mano o nuova apertura) permette di confermarla; cronometrare anche il tempo alla prima registrazione corretta.
