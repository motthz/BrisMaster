# Validazione E100 delle zone e della rete locale

La prova parte da `8b947e9` (PR #80) e mantiene fisso il riconoscitore. I due interruttori esistono nei test. Le zone sono fissate una volta dalla geometria del tavolo, tramite gli stessi rettangoli normalizzati e la stessa funzione di assegnazione usati dall’app. Si calibrano i due ruoli delle giocate. Le annotazioni delle identità servono al conteggio dei risultati. Tutte le partite sono eseguite con `BRIS_SENZA_INTERVENTI=1`.

## Partite con layout mobile e tavolo chiaro

I sei replay incrociano il layout mobile con i mazzi app0 e app3 e il tavolo chiaro con app2, ciascuno con i semi 91000 e 91037. Il denominatore conta ogni carta giocata e visibile per fotogramma, inclusi i fotogrammi ripetuti. Una lettura corretta richiede identità e ruolo esatti. Una lettura invertita ha l’identità giusta nello slot opposto e manca dallo slot atteso. “Attribuzioni errate” e “controlli in ritardo” riguardano lo stato interno della partita e sono due metriche del banco preesistente.

| Configurazione | Ruolo e identità corretti | Ruoli invertiti | Attribuzioni errate | Controlli in ritardo | Partite completate con stato finale verificato |
|---|---:|---:|---:|---:|---:|
| Senza zone e senza rete | 509/720 | 167 | 2 | 169 | 2/6 |
| Rete | 509/720 | 167 | 1 | 169 | 2/6 |
| Zone | 674/720 | 2 | 2 | 169 | 2/6 |
| Zone e rete | 674/720 | 2 | 1 | 169 | 2/6 |

| Layout · mazzo · seme | Senza zone, corretti / invertiti | Con zone, corretti / invertiti | Osservazioni attese |
|---|---:|---:|---:|
| Mobile · app0 · 91000 | 71 / 63 | 134 / 0 | 140 |
| Mobile · app0 · 91037 | 59 / 37 | 95 / 1 | 100 |
| Mobile · app3 · 91000 | 91 / 33 | 123 / 1 | 140 |
| Mobile · app3 · 91037 | 60 / 34 | 94 / 0 | 100 |
| Tavolo chiaro · app2 · 91000 | 131 / 0 | 131 / 0 | 140 |
| Tavolo chiaro · app2 · 91037 | 97 / 0 | 97 / 0 | 100 |

La rete riduce da 2 a 1 le attribuzioni errate su app3, seme 91000. Nessuna delle quattro combinazioni completa i due replay con app3 o i due replay su tavolo chiaro. I due replay app0 completano in tutte le combinazioni.

## Due replay con giocate affiancate

La rete resta attiva. Nei semi 77000 e 77031, le zone portano rispettivamente i ruoli corretti da 43/140 a 134/140 e da 58/100 a 96/100. Le inversioni passano da 129 a 0 su 240 osservazioni. Le due partite completano con zero attribuzioni sbagliate e zero controlli in ritardo in entrambe le configurazioni.

Sugli otto replay appaiati con rete attiva, la correttezza delle giocate passa da 610/960 a 904/960 (63,5% → 94,2%, +30,6 punti percentuali su valori non arrotondati). Le inversioni scendono da 296 a 2 (−99,3%). Il tasso di partite completate resta 4/8.

## Confronto appaiato con la 0.6.20

| Esito sugli stessi otto replay, senza aiuti | 0.6.20 | Candidata con zone e rete |
|---|---:|---:|
| Partite completate con stato finale e storico verificati | 2/8 (25,0%) | 4/8 (50,0%) |
| Controlli in ritardo | 237 | 169 |
| Attribuzioni errate | 1 | 1 |

Le due partite mobili app0 sono quelle recuperate rispetto alla 0.6.20. Le due affiancate continuano a chiudersi e quattro casi continuano a fallire. I ritardi calano del 28,7%. Il confronto con la 0.6.20 riguarda l’intera evoluzione del motore. La matrice sulla candidata mostra che i due recuperi di app0 sono presenti anche disattivando entrambe le funzioni.

## Identità e confidenza della rete

Si usano due schermate annotate del repository e 26 trasformazioni ciascuna, per 234 osservazioni degli slot. Saturazione, tinta, luminosità e attenuazione dei canali rosso e blu variano individualmente. Una lettura automatica supera la soglia di confidenza del rilevatore. Le schermate costituiscono un campione di due tavoli e non misurano l’esito di una partita.

| Esito | Senza rete | Con rete |
|---|---:|---:|
| Identità giusta al primo posto | 221/234 (94,4%) | 224/234 (95,7%) |
| Lettura giusta sopra la soglia automatica | 187/234 (79,9%) | 213/234 (91,0%) |
| Lettura errata sopra la soglia automatica | 1/234 | 0/234 |
| Rilevamenti extra nelle 52 schermate | 2 | 2 |

Le tre correzioni di identità avvengono con tinta −45° o −60° nella schermata Sito Briscola. Nessuna identità corretta al primo posto passa a errata nel campione. Il test `unknown_deck_on_many_tables` supera le sue asserzioni con entrambe le configurazioni. I due test unitari delle zone coprono l’assegnazione dei quattro ruoli e il ripiego sulla lettura automatica in presenza di ambiguità e ruoli mancanti.

L’efficacia delle zone spostate rispetto ai riquadri, della persistenza da interfaccia e del passaggio tra finestre resta da misurare su sessioni complete. I replay misurano layout fissi e calibrati, fotogrammi annotati e differenze della rete esistente. Tre replay si fermano sulla briscola iniziale senza una carta acquisita. Il quarto (app3, seme 91000) termina a 12 prese e ha acquisito l’Asso di Denari al posto del Fante di Denari come briscola. La priorità per migliorare l’autonomia è quindi la verifica della briscola iniziale, anche tramite la zona briscola già disponibile.

La suite ordinaria del binario candidato termina con 136 test superati e 25 esclusi per impostazione del repository. I replay completi esclusi dalla suite ordinaria vengono lanciati separatamente con i comandi sotto.

## Riproduzione

Dalla radice, compilare con `cargo test --locked --release -p brismaster --no-run` e assegnare a `BIN` il percorso dell’eseguibile dei test stampato da Cargo. Usare un ambiente pulito dalle altre variabili di stress. Il banco usa i mazzi interni e non richiede materiale esterno.

```bash
(
  unset BRIS_TEST_ZONE BRIS_TEST_SENZA_RETE
  export BRIS_SENZA_INTERVENTI=1 BRIS_TEST_MISURA_RUOLI=1 SITI_SEMI=2
  for mode in nessuna rete zone entrambe; do
    (
      case "$mode" in zone|entrambe) export BRIS_TEST_ZONE=1;; esac
      case "$mode" in nessuna|zone) export BRIS_TEST_SENZA_RETE=1;; esac
      SITO=app-mobile SITI_MAZZI=app0,app3 "$BIN" siti_possibili --ignored --nocapture > "$mode-mobile.log"
      SITO=tavolo-chiaro SITI_MAZZI=app2 "$BIN" siti_possibili --ignored --nocapture > "$mode-chiaro.log"
    )
  done
  VARIANTE=affiancate VARIANTI_SEMI=2 "$BIN" tavoli_varianti --ignored --nocapture > rete-affiancate.log
  BRIS_TEST_ZONE=1 VARIANTE=affiancate VARIANTI_SEMI=2 "$BIN" tavoli_varianti --ignored --nocapture > entrambe-affiancate.log
  "$BIN" unknown_deck_on_many_tables
  BRIS_TEST_SENZA_RETE=1 "$BIN" unknown_deck_on_many_tables
  "$BIN" vision::zones::tests
)
python scripts/audit-brisco/sweep_zones_network.py "$BIN" risultati-zone-rete
```

Per confrontare la 0.6.20, usare un worktree del commit `158f52f`. Applicare `scripts/audit-brisco/baseline-0620-no-assistance.patch` al worktree 0.6.20, compilare lì e ripetere i tre comandi dei replay con il suo binario, senza gli interruttori di zone o rete. La patch rende esplicite le conferme nei log e applica lo stesso controllo dello storico in modalità senza interventi.

I log riportano i contatori `RUOLI` e `PARTITA` per seme. Nei CSV, confrontare `truth`, `prediction` e il flag `automatic` per la stessa scena, fattore, livello e slot. Per i CSV servono Pillow e NumPy.
