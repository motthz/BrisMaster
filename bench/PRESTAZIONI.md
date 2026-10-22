# Prestazioni del motore

Tempo di una chiamata a `evaluate` (quella usata per i consigli, l'analisi e la mossa del computer),
su 400 posizioni prese da tutta la partita. Un solo thread, stessa macchina (Linux, 4 core).

```
cargo run --release -p briscola-sim --bin bench -- --positions 400 --samples 1000 --threads 1
```

| Mondi | Fase | Prima: media / max | Dopo: media / max |
|---:|---|---:|---:|
| 800 (consigli) | mazzo > 4 (rollout) | 1.8 / 3.0 ms | 2.0 / 5.9 ms |
| 800 (consigli) | mazzo 1-4 (alfa-beta) | 22.5 / 57.8 ms | **1.6 / 6.1 ms** |
| 1000 (Difficile) | mazzo > 4 (rollout) | 2.6 / 6.0 ms | 2.7 / 17.9 ms |
| 1000 (Difficile) | mazzo 1-4 (alfa-beta) | 33.8 / 104.0 ms | **1.6 / 6.7 ms** |
| 2000 (analisi) | mazzo > 4 (rollout) | 5.1 / 12.8 ms | 5.4 / 11.8 ms |
| 2000 (analisi) | mazzo 1-4 (alfa-beta) | 67.4 / 176.6 ms | **2.0 / 9.0 ms** |

Con 4 thread (come fa l'app, che usa tutti i core) la fase dei rollout a 1000 mondi scende a 1.0 ms di media.

**Le mosse non cambiano**: a un thread l'impronta delle valutazioni è identica prima e dopo
(per esempio `902f8e42b857eb7c` a 1000 mondi), quindi la forza di gioco è la stessa partita per partita.

## Cosa è cambiato

- **Memoria dei mondi nel finale.** Con 4 carte o meno nel mazzo i mondi possibili sono pochi
  (al più 120: mano avversaria e ordine del mazzo coperto), ma il motore ne campionava 1000 e
  risolveva ogni volta da capo. Ora ogni mondo già visto si riprende dalla memoria.
- **Risolutore alfa-beta più leggero.** Niente allocazioni per nodo (le mosse stanno in un array di 3)
  e taglio immediato quando i punti già presi o quelli ancora in gioco decidono già il confronto.
  Un test verifica che dia sempre lo stesso valore di un minimax completo.
- **Più thread.** `EngineConfig::threads` divide i mondi tra i core; l'app usa tutti i core del PC.
  I test massivi restano a 1 thread (parallelizzano già le partite) e restano riproducibili dal seme.

Provato e scartato: tabella delle trasposizioni nel risolutore (nessun guadagno, gli alberi sono piccoli)
e determinizzazione senza allocazioni (nessun guadagno misurabile).

## Margine per la forza di gioco

Il risolutore esatto a partire da 6 carte nel mazzo ora costa circa quanto costava prima quello da 4
(media 30 ms, max 150 ms a 1000 mondi). In una prova di 6000 partite in duplicato,
`engine:1000:0.1:6` contro `engine:1000` ha vinto il **52.1%** delle partite decise (IC 95%: 50.8% – 53.4%).
