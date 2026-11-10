# Motore di briscola auto-allenato: archivio (chiuso il 10/11/2026)

## Dove sta tutto
- **Codice**: PR #43 (ramo `claude/project-thread-sj98vx`, base 0.6.0).
  - `crates/core/src/rl.rs`: codifica e rete.
  - `crates/rl-py`: ambiente Python e `python/allena.py`, allenamento PPO con lega.
  - Giocatori del banco: `rl:FILE`, `rlcerca:FILE:MONDI:PROF` e `prova:rlroll/rlmix/rlmixfine/rlguida/rldeduz/rlspareggio=FILE@infer:...`. Il FILE va indicato senza "/": il banco va lanciato dalla cartella della rete.
- **Stati di allenamento** (ramo `claude/rl-stato`):
  - `stato_ridotto.pt`: rete piccola, 81 milioni di partite.
  - `stato_grande.pt`: rete grande, ultimo stato salvato dal PC.
  - `rete_grande_finale.brl`: rete grande, se esportata dal PC.
- **In questa cartella**:
  - `reti/`: reti piccole a 17, 28, 35, 38, 51 e 81 milioni di partite; rete grande a 49 milioni.
  - `stato_rete_piccola_90M.pt` e `stato_grande_cloud.pt`: stati completi dal cloud.
  - `risultati/`: output delle prove e log di allenamento.
- Rete grande finale: 88,16 milioni di partite (giro 15410), ramo claude/rl-stato commit 2bed2e2 (stato_grande.pt + rete_grande_finale.brl).
- **Sul PC**: `C:\Users\stemo\BrisMaster\run1` (rete piccola) e `run_grande` (rete grande), con stati completi e storia delle reti.

## Risultati contro Massima v0.5.0
Massima è `infer:20000:1:1000:0.25`.

| Prova | Partite | Punteggio |
|---|---|---|
| Rete piccola da sola, 17M / 35M / 38,6M / 51,65M | 2.000 | 46,3 / 47,3 / 48,35 / 47,1% |
| Rete piccola con ricerca (rlcerca 32:6, beta 0,03) | 2.000 | 48,4-49,8% |
| Rete grande da sola, 49,5M | 2.000 | 44,85% |
| Unione: valore della rete nelle foglie | 2.000 | 47,65% |
| Unione: metà rete, metà simulazioni | 2.000 | 48,6% |
| A: peso della rete che cala con il mazzo | 2.000 | 49,1% |
| B: la rete guida l'albero (rlguida) | 15.500 | 50,44% |
| C: la rete deduce le carte avversarie | 2.000 | 49,4% |
| D: la rete decide tra mosse alla pari | 2.000 | 49,0% |

La prova B non è finita: si è fermata a 15.500 partite delle 30.000 previste.

Nessuna variante supera Massima in modo misurabile.

Possibili riprese:
- misurare la rete grande oltre i 100 milioni di partite;
- ripetere la prova B con una rete più forte.
