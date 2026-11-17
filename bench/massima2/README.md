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
