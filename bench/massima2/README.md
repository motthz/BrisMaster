# Massima 2.0

Lavoro per il nuovo motore di briscola Massima 2.0, diviso in 6 fasi.
Si passa a una fase nuova solo dopo l'ok dell'utente.

## Fase 1 — Range di tolleranza (17-11-2026)

Un bot "a testa o croce" gioca contro se stesso (ogni partita vinta al 50%).
Per ogni dimensione si giocano 500 set; il range è la media del valore assoluto
delle 500 deviazioni dal 50%, usato sia in positivo che in negativo.
Script: `fase1_range.py` (seme 20261117), dati: `fase1_range.json`.

| Range | Partite per set | Valore misurato | Valore teorico | Set fuori range |
|---|---|---|---|---|
| R1000 | 1.000 | ±1,20% | ±1,26% | 194 / 500 |
| R10000 | 10.000 | ±0,38% | ±0,40% | 210 / 500 |
| R100000 | 100.000 | ±0,13% | ±0,13% | 207 / 500 |

Regole di punteggio (fase 5): per ogni validatore −1 se sotto il range,
0 se dentro, +1 se sopra. Contro Massima il centro è 50%; contro il bot umano
il centro è la percentuale di Massima contro il bot umano.

## Fase 2 — Curve di miglioramento (17–22-11-2026)

Ogni punto: 5.000 mazzi in duplicato = 10.000 partite contro Massima
(`infer:20000:1:1000:0.25`, politica `politica.bin`), seme 20261117.
Massima contro se stessa vale 50%, quindi è il punto di riferimento di ogni curva.
Grafici: `fase2_curve.png` (script `fase2_grafici.py`), numeri: `fase2_risultati.json`.
Coda: `fase2_punti_cloud.txt`, pagina di avanzamento: `stato.py`.

| Parametro | Valori provati (% contro Massima) | Lettura |
|---|---|---|
| Iterazioni (Massima 20.000) | 2k 44,50 · 5k 48,54 · 10k 49,50 · 40k 49,77 · 80k 50,69 · **160k 51,38** | cresce ancora, circa +0,45% per raddoppio dopo 20k: non saturo |
| Mani dedotte (Massima 1.000) | 100 49,07 · 300 50,26 · **3.000 50,87** · 10.000 49,73 | picco a 3.000, poi cala: saturazione ≈ 3.000 |
| Esplorazione UCB (Massima 0,7) | 0,3 48,69 · 0,5 50,21 · 1,0 50,19 · 1,4 48,68 | piatta tra 0,5 e 1,0: Massima è già all'ottimo |
| Minimo delle mani (Massima 0,25) | 0 49,89 · 0,1 49,83 · 0,5 50,23 · 1 48,26 | piatta tra 0 e 0,5: nessun guadagno |
| Rete autoallenata (partite) | 17M 46,54 · 28M 45,64 · 35M 45,80 · 38M 45,67 · 51M 46,79 · 81M 46,73 · grande 49M 45,57 | ferma a ~46% da 17M a 81M partite |

Solo tre punti superano il range R10000 (+0,38%): 160.000 iterazioni (+1,38%),
3.000 mani (+0,87%) e 80.000 iterazioni (+0,69%).

Tempo per mossa misurato sul cloud (1 core): Massima ≈ 0,12 s, 160.000 iterazioni
≈ 0,9 s, 3.000 mani ≈ 0,13 s. Tutti sotto il limite di 2 s per mossa.

Non fatta: la curva per dimensione della rete (serve il PC, su indicazione dell'utente).
