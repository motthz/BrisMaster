# Confronto con la 0.6.20

Il banco incrocia sei condizioni grafiche (mobile, legno e carte piccole, tavolo chiaro, angoli arrotondati, mano scurita, ombre), due mazzi e due semi. I 24 casi coincidono per configurazione e seme. Il riferimento usa il codice 0.6.20 con la sola stampa aggiuntiva dei risultati individuali. La candidata corrente è la PR #75. La ricerca della briscola dal dorso è nel commit ec9188b. La prova contorni usa SENZA_NOTO=1 sulla PR #75.

Ogni gruppo seguente riporta, nell'ordine, completate con stato finale verificato, perfette secondo il banco (zero interventi, ritardi e attribuzioni errate), partite con attribuzioni errate e completate senza interventi. La prima riga numerica è la 0.6.20. La seconda è la configurazione in prova. Le righe individuali riportano soltanto esiti differenti.

```text
results/candidate-*-*.log 172
158 91 7 97
158 91 7 97
results/recovery-comparison/* 24
17 5 4 5
18 5 4 5
siti_possibili app3 app-mobile seed=91000 ('false', 'None', '0', '0') ('true', 'Some(6)', '0', '7')
results/broad-comparison/* 24
17 5 4 5
18 5 4 6
siti_possibili app2 tavolo-chiaro seed=91000 ('true', 'Some(3)', '0', '3') ('true', 'Some(2)', '0', '2')
siti_possibili app2 tavolo-chiaro seed=91037 ('true', 'Some(1)', '0', '2') ('true', 'Some(0)', '0', '1')
siti_possibili app3 app-mobile seed=91000 ('false', 'None', '0', '0') ('true', 'Some(6)', '0', '7')
siti_possibili app3 tavolo-chiaro seed=91000 ('true', 'Some(11)', '0', '2') ('true', 'Some(8)', '0', '3')
siti_possibili app3 tavolo-chiaro seed=91037 ('false', 'None', '1', '0') ('false', 'None', '2', '0')
results/contour-comparison/* 24
17 5 4 5
19 4 3 4
siti_possibili app2 mano-scurita seed=91000 ('true', 'Some(0)', '0', '0') ('true', 'Some(2)', '0', '2')
siti_possibili app2 tavolo-chiaro seed=91037 ('true', 'Some(1)', '0', '2') ('true', 'Some(1)', '0', '1')
siti_possibili app3 app-mobile seed=91000 ('false', 'None', '0', '0') ('true', 'Some(4)', '0', '4')
siti_possibili app3 app-mobile seed=91037 ('false', 'None', '1', '0') ('true', 'Some(1)', '0', '2')
siti_possibili app3 arrotondate seed=91037 ('false', 'None', '0', '0') ('false', 'None', '1', '0')
siti_possibili app3 legno-piccole seed=91037 ('true', 'Some(15)', '4', '13') ('false', 'None', '5', '25')
siti_possibili app3 mano-scurita seed=91000 ('true', 'Some(2)', '0', '2') ('true', 'Some(3)', '0', '2')
siti_possibili app3 tavolo-chiaro seed=91000 ('true', 'Some(11)', '0', '2') ('true', 'Some(3)', '0', '4')
siti_possibili app3 tavolo-chiaro seed=91037 ('false', 'None', '1', '0') ('true', 'Some(1)', '0', '2')
```

Con BRIS_SENZA_INTERVENTI=1 l'esperimento del dorso chiude 1/2 casi su tavolo chiaro con app2 e 0/2 casi mobile con app3 e SENZA_NOTO=1. Il caso app3 su tavolo chiaro, seme 91037, passa da una a due attribuzioni errate. La prova contorni perde il caso app3 su legno con carte piccole, seme 91037, e introduce due interventi su app2 con mano scurita, seme 91000. Questi risultati mantengono entrambe le varianti del rilevamento in sperimentazione.

Le 172 partite del primo gruppo confrontano esclusivamente la correzione della posizione appresa della PR #73. Il confronto integrale sul codice della PR #75 resta da completare. I risultati misurano replay sintetici.
