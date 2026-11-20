# Audit del riconoscimento Briscola

Il riferimento è il tag v0.6.13, commit 3b14734. Il confronto usa identità annotate indipendentemente dal riconoscitore. I dati del sito restano sul computer. Per i CSV e le curve servono Python, Pillow, NumPy, pandas e matplotlib.

Compilare il binario dei test nel checkout di riferimento e in quello della candidata con `cargo test --locked --release -p brismaster --no-run`. Conservare i due percorsi distinti stampati da Cargo. I comandi seguenti mostrano l'esecuzione Linux. Su Windows usare le variabili di ambiente e il percorso dell'eseguibile corrispondenti.

```sh
python sweep.py "$BASE" prima.csv
python sweep.py "$NEW" finale.csv
python sweep.py "$BASE" holdout-prima.csv --holdout
python sweep.py "$NEW" holdout-finale.csv --holdout
BRIS_AUDIT_FIXTURES="$FIXTURES" "$NEW" export_card_audit --ignored
python probes.py "$BASE" fixtures carte-prima.csv --fixtures "$FIXTURES"
python probes.py "$NEW" fixtures carte-finale.csv --fixtures "$FIXTURES"
python probes.py "$BASE" controls misti-prima.csv
python probes.py "$NEW" controls misti-finale.csv
python probes.py "$NEW" alternatives tentativi.csv
"$BASE" window_changes_during_whole_games --ignored --nocapture > partite-prima.log
BRIS_WINDOW_HOLDOUT=1 "$BASE" window_changes_during_whole_games --ignored --nocapture > riserva-prima.log
"$NEW" window_changes_during_whole_games --ignored --nocapture > partite-finale.log
BRIS_WINDOW_HOLDOUT=1 "$NEW" window_changes_during_whole_games --ignored --nocapture > riserva-finale.log
```

Per misurare separatamente il tempo motore, in una copia del riferimento aggiungere solo il test `measure_advice_time` in `assistant/session_replay.rs`, ricompilare e conservare l'eseguibile in `BASE_TIMER`. Eseguire `python timing.py "$BASE_TIMER" "$NEW" tempi.csv`. Infine eseguire `python report.py DIRECTORY_DATI DIRECTORY_RAPPORTO`.

`frame_us` ed `extra_slots_in_frame` si ripetono per ogni ruolo della schermata. Contarli una sola volta per combinazione di fattore, livello e scena. Nella terza schermata sul mare sono annotate solo le tre carte della mano. Gli extra di questa schermata comprendono la briscola e sono esclusi dal conteggio dei falsi positivi. Per i fotogrammi con traslazione o copertura l'etichetta resta quella del tavolo prima della trasformazione. Le categorie grafiche non hanno un ordine di difficoltà intrinseco.
