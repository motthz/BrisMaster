# Briscola 0.6.0: tutti i tentativi sul motore

La 0.6.0 esce con lo **stesso motore di gioco della 0.5.0** (Massima = `infer:20000:1:1000:0.25` con
`crates/core/nets/politica.bin`). Qui sotto ci sono tutti i tentativi fatti tra il 28 ottobre e il 10 novembre 2026,
con i numeri, le reti e i file dei risultati, così si possono riprendere in futuro.

**In sintesi:** 71 idee provate. Nessuna batte la Massima 0.5.0 in modo misurabile. Le idee che a 2.000 iterazioni
sembravano vincenti (fino a +3 punti) al livello Massima rientrano nel rumore.

## Come si leggono i numeri

- Punteggio = percentuale di punti partita (pareggio = ½), su mazzi in duplicato (ogni mazzo si gioca a posti invertiti).
- «Δ» = differenza in punti percentuali rispetto allo stesso motore senza l'idea, sullo stesso banco e con le stesse carte.
- Banco di 9 avversari, 500 mazzi (1.000 partite) ciascuno: stile circolo, memoria perfetta, domenica, nonno,
  circolo con il tempo che stringe (`fretta:0.4`), spavaldo in tilt (`tilt:0.3`), personaggio 162, counter, `ismcts+:2000`.
- **Rumore misurato:** lo stesso motore rigiocato con una minima variazione del generatore (`prova:ucb=0.70001`) dà
  −0,6 di media, da −1,9 a +1,2 sul singolo avversario. Uno scarto sotto ~1 punto di media non vuol dire nulla.

## La 0.5.0 su questo banco (riferimento)

| Avversario | Seme 20261101 | Seme 20261106 |
|---|---:|---:|
| Stile circolo | 71,10% | 68,75% |
| Memoria perfetta | 70,05% | 67,80% |
| Domenica | 79,20% | 77,45% |
| Nonno | 77,70% | 75,55% |
| Circolo con il tempo che stringe | 76,70% | 78,85% |
| Spavaldo in tilt | 76,20% | 76,65% |
| Personaggio 162 | 71,35% | 69,90% |
| Counter | 73,70% | 73,75% |
| ISMCTS potenziato 2.000 | 58,15% | 55,85% |

Avversari costruiti per battere la Massima (motore forte a 20.000 iterazioni che nella ricerca fa giocare la Massima
come la rete `reti/guida256x64.bin`, distillata da 10.000 partite della Massima), seme 20261107, 1.000 partite:
la Massima fa **51,05%** con modello al 50% e **48,40%** con modello al 100% (`sfruttatori.tsv`).

## Fase 1: selezione a 2.000 iterazioni (48 idee)

File: `idee.txt` (id | variabili d'ambiente | giocatore) e `selezione.tsv` (id, avversario, punteggio).
L'id 0 è la base `infer:2000:1:1000:0.25`. Le idee sono descritte nel documento «Briscola 0.6.0: 50 idee da provare».

Passavano la soglia (media ≥ +0,7, nessun avversario sotto −1,5): 27 rete guida 0,5 (+2,5), 7 avversario umano medio
nell'albero al 20% (+2,3), 26 rete guida 1 (+2,0), 8 umano medio al 40% (+1,5), 13 minimo di deduzione 50% (+1,3),
20 temperatura per fase (+1,2), 43 e 10 memoria dello stile (+1,0, +0,9), 11 deduzione esatta (+0,8), 42 rischio quando
si è sotto (+0,7). Combinazione 103 (7+27+20+13): +3,0. Varianti 191-194 (velo e ombra, idea 19): bocciate.

## Fase 2: conferma al livello Massima (20.000 iterazioni)

File: `conferma.tsv` (seme 20261101) e `conferma2.tsv` (seme 20261106). Δ medio sui 7 avversari umani e Δ contro `ismcts+:2000`:

| Id | Giocatore | Δ umani | Δ motore |
|---|---|---:|---:|
| M103 | `prova:modello=umano-medio,alfa=0.2,dove=albero,guida=0.5,tinizio=0.5,tfine=1.5@infer:20000:1:1000:0.5` (con `BRISCOLA_PRIOR=politica.bin`) | +0,5 | −5,2 |
| M7 | `prova:modello=umano-medio,alfa=0.2,dove=albero@infer:20000:1:1000:0.25` | +0,3 | −1,3 |
| M27 | `prova:guida=0.5@infer:20000:1:1000:0.25` | −3,9 (1 avversario, interrotta) | — |
| M13 | `infer:20000:1:1000:0.5` | −0,6 | +0,6 |
| M20 | `prova:tinizio=0.5,tfine=1.5@infer:20000:1:1000:0.25` | −0,4 | −2,3 |
| M105 | `prova:modello=umano-medio,alfa=0.2,dove=albero,tinizio=0.5,tfine=1.5@infer:20000:1:1000:0.5` | −0,6 | — |
| C2 | `infer:20000:1:3000:0.25` (3.000 mani dedotte) | −1,7 | −1,4 |
| C3 | `prova:prefin=4,mondi=2000@infer:20000:1:1000:0.25` (finale esatto da 4 carte) | +0,3 (seme 2: −0,3) | +0,05 (seme 2: −0,6) |
| D3b | `prova:prefin=4,mondi=5000@...` | +0,2 | +0,05 |
| D3c | `prova:prefin=6,mondi=3000@...` | −0,1 | −0,3 |
| C5 | `prova:prefin=8,mondi=2000@...` | −1,4 | −0,7 |
| C4 | `prova:apre=2@...` (doppio tempo sulle aperture) | −0,9 (5 avversari) | — |
| C7 | `prova:dubbio=1@...` (seconda ricerca se incerto) | −1,4 | −1,65 |
| C10 | `prova:rsolve=4@...` (simulazioni esatte da 4 carte) | −1,0 | +0,4 |
| C19 | `prova:ombra=0.3@infer:28600:1:1000:0.25` (l'avversario simulato non sfrutta la mia mano) | −3,0 (4 avversari) | — |
| C21 | `infer:10000:1:1000:0.25` | −2,0 | −0,35 |
| R0 | `prova:ucb=0.70001@...` (stesso motore: misura del rumore) | −0,6 | +0,1 |
| D14 | `prova:sotto=1@...` | −0,1 | −2,55 |
| D14b | `prova:avanti=1,sotto=1@...` | −0,3 | −1,15 |
| D6 | `prova:mix=0.25@...` con `BRISCOLA_NET=reti/valore.bin` | −3,2 (2 avversari) | — |
| D11 | `prova:modello=umano-errori,alfa=0.2,dove=albero@...` | +0,8 | −2,0 |
| D11b | come D11 con `alfa=0.1` | −0,1 | −0,6 |
| E1 | D11 + `prefin=4,mondi=2000` | 0,0 | −1,9 |
| D12 | `infer:20000:1:1000:0.15` | −1,6 | — |
| D13 | `infer:20000:1:1000:0.4` con `BRISCOLA_POLICY=reti/misti.bin` | −1,1 | — |
| D15 | `prova:pref=1.6,apre=0.9,risp=0.9@...` (tempo dosato) | −0,8 | +0,85 |

Testa a testa: `infer:80000:1:1000:0.25` contro la Massima = **48,75%** (1.000 partite, seme 20261104). Sopra le 20.000
iterazioni il motore non migliora; sotto sì (a 10.000 perde 2 punti).

Prove precedenti (29-30 ottobre): deduzione esatta 50,2%; reti di deduzione più grandi senza guadagno; rete di valutazione
onesta 45,9% / 50,1%; ricerca guidata a 20.000 = 50,0%, guida distillata dalla Massima 48,7%; memoria dello stile per
avversario +0,6 al livello Massima (scartata: richiedeva di sapere contro chi si gioca).

## Le reti (`reti/`)

| File | Cosa è |
|---|---|
| `p96.bin`, `p96x32.bin` | reti di politica della forma attuale |
| `p256x64.bin`, `p512x128.bin` | reti di politica più grandi (deduzione: nessun guadagno) |
| `onesta256x64.bin`, `onesta512x128.bin` | reti di valutazione che vedono solo ciò che vede il giocatore |
| `valore.bin` | rete di valutazione per `prova:mix=` |
| `guida96x32.bin`, `guida256x64.bin` | reti distillate dalle mosse della Massima (guida della ricerca e modello degli sfruttatori) |
| `umani.bin`, `misti.bin` | reti allenate su partite di umani simulati, e su un misto di umani e motori |
| `v050.bin` | copia della rete della 0.5.0 |

## Come si rilancia

- Opzioni nel banco: `prova:CHIAVE=VALORE,...@GIOCATORE` (vedi `crates/sim/src/agents.rs`: ucb, pts, eps, riuso, rsolve,
  velo, ombra, modello, alfa, dove, guida, mix, tinizio, tfine, prefin, mondi, apre, risp, pref, dubbio, avanti, sotto, ...),
  `sfrutta:ALFA:MODELLO@GIOCATORE`, `adatta:`, `esatta:`, `oracolo:`, `reale:FILE[:TEMP]` (giocatore o modello imitato da una rete).
- Esempio: `BRISCOLA_POLICY=crates/core/nets/politica.bin briscola-sim --player prova:prefin=4,mondi=2000@infer:20000:1:1000:0.25 --opponent stile:circolo --deals 500 --seed 20261101`.

## Motore auto-allenato (rete allenata giocando contro se stessa)

Il codice è nella 0.6.0 ma è **spento di default**: il motore dell'app resta la Massima 0.5.0.

- `crates/core/src/rl.rs`: codifica della posizione e rete (formato `.brl`).
- `crates/rl-py`: ambiente Python (pyo3) e `python/allena.py`, allenamento PPO con una lega di avversari passati.
- Giocatori del banco: `rl:FILE[:TEMP]`, `rlcerca:FILE:MONDI:PROF` e
  `prova:rlroll/rlmix/rlmixfine/rlguida/rldeduz/rlspareggio=FILE@infer:...`
  (FILE senza "/": il banco va lanciato dalla cartella della rete).

Due reti: piccola (allenata fino a ~90 milioni di partite) e grande (`--hidden 1024`, allenata sul PC con GPU
fino a ~88 milioni). Risultati contro la Massima 0.5.0 (`infer:20000:1:1000:0.25`), seme 20261105:

| Prova | Partite | Punteggio |
|---|---:|---:|
| Rete piccola da sola, 17M / 35M / 38,6M / 51,65M partite | 2.000 | 46,3 / 47,3 / 48,35 / 47,1% |
| Rete piccola con ricerca (`rlcerca` 32:6, beta 0,03) | 2.000 | 48,4–49,8% |
| Rete grande da sola, 49,5M partite | 2.000 | 44,85% |
| Massima + valore della rete nelle foglie | 2.000 | 47,65% |
| Massima + metà rete, metà simulazioni | 2.000 | 48,6% |
| A: peso della rete che cala con il mazzo | 2.000 | 49,1% |
| B: la rete guida l'albero (`rlguida`) | 15.500 | 50,44% |
| C: la rete deduce le carte avversarie (`rldeduz`) | 2.000 | 49,4% |
| D: la rete decide tra mosse alla pari (`rlspareggio`) | 2.000 | 49,0% |

Contro `ismcts+:2000` la rete piccola da sola fa 50,1%. Nessuna variante batte la Massima in modo misurabile
(B, la migliore, è +0,4 punti: dentro il rumore).

**Dove sono le reti e gli stati di allenamento** (per riprendere il lavoro):
- release GitHub [`archivio-0.6.0`](https://github.com/motthz/BrisMaster/releases/tag/archivio-0.6.0) (non è una
  versione dell'app): reti `.brl` a 17, 28, 35, 38, 51, 81 milioni di partite e rete grande a 49 milioni, stati
  completi di allenamento `.pt`, log e risultati delle prove;
- ramo `claude/rl-stato`: stati ridotti `stato_ridotto.pt` (rete piccola) e `stato_grande.pt` (rete grande), da cui
  `allena.py` riparte.

Possibili riprese: misurare la rete grande oltre i 100 milioni di partite; ripetere la prova B con una rete più forte.

## Partite vere

Registrazione a scelta nell'app (spenta per ogni partita finché non la accendi), strumento `reali` per allenare la
deduzione sulle proprie partite, guida in `docs/PARTITE-VERE.md`.
