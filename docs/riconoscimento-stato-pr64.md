# Riconoscimento delle partite · PR 64

Punto di ripresa del 21 novembre 2026. Il codice misurato è [4231a52](https://github.com/Motthz/BrisMaster/commit/4231a5206257a6c21d3409e54bc8f16b5906da61). Il lavoro prosegue sul branch `recognition/compact-tables-and-late-evidence` della [PR 64](https://github.com/Motthz/BrisMaster/pull/64). La versione pubblicata usata come riferimento è la 0.6.14.

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
