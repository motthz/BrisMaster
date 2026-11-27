# Ripartenza senza riutilizzare le giocate precedenti

Quando il motore ripartiva con una carta visibile sul tavolo, conservava gli avvistamenti delle prese saltate. Una carta già vista poteva così diventare una risposta alla prima giocata dopo la ripartenza. Il recupero con il tavolo vuoto già azzerava quegli avvistamenti. La stessa routine ora vale per entrambe le ripartenze e mantiene gli avvistamenti delle carte effettivamente ancora in mano. L'apertura visibile rimane nella posizione corrente o nello storico quando è verificabile. Il test `resync_records_our_known_lead_in_the_opposite_slot` verifica che una vecchia risposta sicura venga scartata e una risposta nuova venga accettata.

## Risultati sui replay appaiati

| Gruppo | Partite complete | Attribuzioni errate | Controlli in ritardo |
|---|---:|---:|---:|
| Otto casi della PR #81 | 5/8 → 5/8 | 6 → 0 | 117 → 117 |
| Controlli su altri layout | 4/6 → 4/6 | 8 → 4 | 40 → 40 |
| Verifica con zone automatiche | 6/10 → 6/10 | 9 → 0 | 106 → 106 |
| Totale | 15/24 → 15/24 | 23 → 4 | 263 → 263 |

Riferimento `b0f3be2` (PR #82). Tutte le partite completate dal riferimento sono ancora complete. Si misura lo storico esatto oltre a 20 prese, punti, carte giocate, mano, tallone e briscola. Una carta conta come attribuita male anche se l'app corregge l'errore in seguito. I replay sono sintetici e ripetono due distribuzioni con e senza zone calibrate, come nella PR #82. Sulle otto distribuzioni condivise con la 0.6.20 la candidata chiude 5/8 (62,5%) contro 2/8 (25,0%) della 0.6.20.

Sul tavolo chiaro con app2, seme 91000, la ripartenza ora evita tutte e cinque le attribuzioni errate. Il contatore arriva a 20 prese anziché 18, ma i punti e lo storico restano incompleti. Nei due replay già falliti del seme 77031 (layout normale e briscola verticale), il motore si ferma a 15 prese invece di 19 e attribuisce due carte al giocatore sbagliato invece di quattro. Le partite ancora incomplete rimangono fallimenti in tutte le metriche di completamento.

Per il Cavallo in parte coperto, sulla quinta presa del replay chiaro, la rete propone Bastoni sul ritaglio intero (94,9–96,9%) e Spade sulla sola regione visibile (60,3–61,5%). Il ritaglio intero contiene pixel fuori dalla regione assegnata alla carta. La modifica alla classificazione resta fuori dalla candidata, vista la discordanza nel test.

Esiti cambiati nel formato `completata (1 o 0) / attribuzioni errate / controlli in ritardo`.

| Gruppo | Caso | Prima | Dopo |
|---|---|---:|---:|
| Otto casi della PR #81 | app3 app-mobile · 91000 | 0 / 1 / 37 | 0 / 0 / 37 |
| Otto casi della PR #81 | app2 tavolo-chiaro · 91000 | 0 / 5 / 28 | 0 / 0 / 28 |
| Controlli su altri layout | briscola-in-piedi · 77031 | 0 / 4 / 20 | 0 / 2 / 20 |
| Controlli su altri layout | normale · 77031 | 0 / 4 / 20 | 0 / 2 / 20 |
| Verifica con zone automatiche | app2 tavolo-chiaro · 91000 | 0 / 5 / 28 | 0 / 0 / 28 |
| Verifica con zone automatiche | app2 legno-piccole · 91000 | 0 / 4 / 28 | 0 / 0 / 28 |

## Riproduzione

Compilare la PR #82 e la candidata con `cargo test --locked --release -p brismaster --no-run`. Eseguire gli stessi nove comandi di [Recupero della briscola sotto il tallone](RECUPERO-BRISCOLA-SOTTO-TALLONE.md) per ciascun binario, con la rete attiva e senza l'interruttore di disattivazione del nuovo rilevamento della briscola.
