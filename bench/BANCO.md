# Banco di prova della 0.6.0: il punto di partenza (v0.5.0)

Giocatore misurato: **Massima della v0.5.0** (`infer:20000:1:1000:0.25`, ISMCTS potenziato a 20.000 iterazioni
che deduce le carte avversarie con `crates/core/nets/politica.bin`), 28–29 ottobre 2026, seme `20261028`.
È il riferimento che il motore della 0.6.0 dovrà battere su tutto il banco.

Ogni prova è in duplicato (ogni mazzo giocato due volte a posti invertiti). Contro le persone la prova è sequenziale:
blocchi di 250 mazzi, ci si ferma appena il risultato rispetto al 60% è certo al 99,5% (al minimo 500 partite).
Contro i motori il numero di partite è fisso, da 500 a 2.000 secondo il costo dell'avversario: sono misure del punto di partenza,
non il rapporto finale. Il rapporto della 0.6.0 userà 8.000 partite per avversario.

## Riepilogo

| Famiglia | Avversari | Soglia | Sopra la soglia | Il più difficile |
|---|---:|---:|---:|---|
| Stili umani | 30 | 60% | 30 | Stile «memoria-perfetta» (ricorda tutto e gioca il finale perfetto) (71,1%) |
| Persone vere | 10 | 60% | 10 | Motore PIMC (400 mondi) con 10% di errori (64,3%) |
| Personaggi (i 20 più difficili su 200) | 20 | 60% | 20 | Personaggio n. 162 (72,6%) |
| Motori forti | 14 | 50% | 12 | Motore PIMC (1000 mondi) [v0.2.0] (52,8%) |
| Avversari cattivi | 3 | 50% | 3 | Imprevedibile: ISMCTS potenziato (20000 iterazioni) che sceglie a caso tra le mosse buone (54,5%) |
| Super-bari | 2 | — | misura | contro il super-baro la v0.5.0 fa il 4,1% |

Esame del finale: 20.000 posizioni a mazzo vuoto con la mossa giusta calcolata in modo esatto, **20.000 giuste su 20.000**.
A mazzo vuoto le carte sono tutte note e il motore calcola già in modo esatto; l'esame difficile sarà il pre-finale (fase 4).

Cosa dice il punto di partenza:

- **Contro le persone il 60% c'è già, con margine.** Ogni stile, ogni persona sotto pressione e ogni personaggio resta sopra il 70%,
  tranne il motore forte ma distratto (64,3%). Il lavoro della 0.6.0 qui serve ad allargare il margine.
- **Contro i motori forti il margine è piccolo.** Dal 52,8% (v0.2.0) al 63,6% (rete pura): contro ISMCTS da 50.000 e 100.000 iterazioni
  e contro l'ISMCTS potenziato si sta tra il 53% e il 54%. Contro la v0.2.0 (52,8%) e ISMCTS da 100.000 (54,0% su 500 partite)
  il vantaggio non è ancora certo statisticamente. È qui che la 0.6.0 deve fare la differenza.
- **Il margine teorico è enorme.** Contro chi vede la mano avversaria la v0.5.0 fa il 27,8%; contro chi vede anche il mazzo il 4,1%.
  Nessuno può vedere il mazzo, ma il baro conferma che il terreno da conquistare è la deduzione delle carte coperte.

## Stili umani

| Avversario | Partite | Punteggio | IC 95% | Obiettivo |
|---|---:|---:|---|---|
| Casuale | 500 | **94,7%** | 92,7% – 96,7% | ✅ sopra il 60% |
| Euristico | 500 | **77,6%** | 74,1% – 81,1% | ✅ sopra il 60% |
| Aggressivo | 500 | **82,1%** | 78,6% – 85,6% | ✅ sopra il 60% |
| Risparmiatore | 500 | **79,8%** | 76,2% – 83,4% | ✅ sopra il 60% |
| Briscolone | 500 | **82,2%** | 78,8% – 85,6% | ✅ sopra il 60% |
| Contacarte | 500 | **74,6%** | 70,9% – 78,3% | ✅ sopra il 60% |
| Euristico con 30% di errori | 500 | **84,1%** | 81,1% – 87,1% | ✅ sopra il 60% |
| Rispondi di seme | 500 | **83,6%** | 80,3% – 86,9% | ✅ sopra il 60% |
| Butta-carichi | 500 | **77,1%** | 73,3% – 80,9% | ✅ sopra il 60% |
| Mai briscola | 500 | **86,8%** | 83,7% – 89,9% | ✅ sopra il 60% |
| Stile «liscio» (liscia sempre: non prende quasi mai, butta la carta più piccola) | 500 | **75,7%** | 72,0% – 79,4% | ✅ sopra il 60% |
| Stile «caricatore» (carica sempre: apre con assi e tre e non protegge i punti) | 500 | **80,6%** | 77,2% – 84,0% | ✅ sopra il 60% |
| Stile «asso-di-briscola» (tiene l'asso e il tre di briscola fino alla fine, a ogni costo) | 500 | **80,0%** | 76,6% – 83,4% | ✅ sopra il 60% |
| Stile «conta-briscole» (ricorda bene le carte uscite ma gioca senza fantasia) | 500 | **76,1%** | 72,6% – 79,6% | ✅ sopra il 60% |
| Stile «tilt» (va in tilt: quando è sotto comincia a sbagliare) | 500 | **78,3%** | 74,9% – 81,7% | ✅ sopra il 60% |
| Stile «finale-alla-cieca» (gioca il finale senza contare) | 500 | **76,7%** | 73,2% – 80,2% | ✅ sopra il 60% |
| Stile «memoria-corta» (dimentica quasi tutte le carte uscite) | 500 | **76,4%** | 72,9% – 79,9% | ✅ sopra il 60% |
| Stile «memoria-perfetta» (ricorda tutto e gioca il finale perfetto) | 500 | **71,1%** | 67,4% – 74,8% | ✅ sopra il 60% |
| Stile «prende-tutto» (vuole prendere ogni presa, briscole comprese) | 500 | **77,8%** | 74,4% – 81,2% | ✅ sopra il 60% |
| Stile «avaro» (non molla una briscola nemmeno sotto tortura) | 500 | **80,9%** | 77,5% – 84,3% | ✅ sopra il 60% |
| Stile «apre-di-briscola» (apre di briscola per stanare quelle avversarie) | 500 | **76,2%** | 72,9% – 79,5% | ✅ sopra il 60% |
| Stile «domenica» (giocatore della domenica: distratto, ricorda poco) | 500 | **77,4%** | 73,8% – 81,0% | ✅ sopra il 60% |
| Stile «circolo» (esperto da circolo: ricorda quasi tutto, sbaglia di rado) | 500 | **75,4%** | 71,8% – 79,0% | ✅ sopra il 60% |
| Stile «frettoloso» (il tempo stringe: verso la fine sbaglia sempre di più) | 500 | **80,0%** | 76,5% – 83,5% | ✅ sopra il 60% |
| Stile «cacciatore» (caccia i carichi: briscola appena vede punti in tavola) | 500 | **75,8%** | 72,3% – 79,3% | ✅ sopra il 60% |
| Stile «prudente» (prudente: protegge i punti, prende solo quando costa poco) | 500 | **75,0%** | 71,4% – 78,6% | ✅ sopra il 60% |
| Stile «spavaldo» (spavaldo: apre forte e prende tutto quello che può) | 500 | **76,0%** | 72,4% – 79,6% | ✅ sopra il 60% |
| Stile «nonno» (il nonno: lento e attento, tiene le briscole, sbaglia poco) | 500 | **78,7%** | 75,3% – 82,1% | ✅ sopra il 60% |
| Stile «impulsivo» (impulsivo: prende tutto, sbaglia, ha fretta) | 500 | **80,0%** | 76,5% – 83,5% | ✅ sopra il 60% |
| Stile «calcolatore-distratto» (calcolatore distratto: ricorda tutto ma una mossa su otto è a caso) | 500 | **76,5%** | 72,9% – 80,1% | ✅ sopra il 60% |

## Persone vere

| Avversario | Partite | Punteggio | IC 95% | Obiettivo |
|---|---:|---:|---|---|
| Contacarte · il tempo stringe (fino a 40% di errori) | 500 | **79,7%** | 76,2% – 83,2% | ✅ sopra il 60% |
| Stile «circolo» (esperto da circolo: ricorda quasi tutto, sbaglia di rado) · il tempo stringe (fino a 40% di errori) | 500 | **76,7%** | 73,2% – 80,2% | ✅ sopra il 60% |
| Contacarte · va in tilt quando è sotto (40% di errori) | 500 | **76,2%** | 72,5% – 79,9% | ✅ sopra il 60% |
| Stile «circolo» (esperto da circolo: ricorda quasi tutto, sbaglia di rado) · va in tilt quando è sotto (40% di errori) | 500 | **77,4%** | 74,0% – 80,8% | ✅ sopra il 60% |
| Contacarte con 5% di errori | 500 | **77,0%** | 73,4% – 80,6% | ✅ sopra il 60% |
| Stile «memoria-perfetta» (ricorda tutto e gioca il finale perfetto) con 10% di errori | 500 | **74,4%** | 70,9% – 77,9% | ✅ sopra il 60% |
| Stile «nonno» (il nonno: lento e attento, tiene le briscole, sbaglia poco) · il tempo stringe (fino a 30% di errori) | 500 | **80,1%** | 76,8% – 83,4% | ✅ sopra il 60% |
| Stile «spavaldo» (spavaldo: apre forte e prende tutto quello che può) · va in tilt quando è sotto (30% di errori) | 500 | **79,2%** | 75,8% – 82,6% | ✅ sopra il 60% |
| Stile «domenica» (giocatore della domenica: distratto, ricorda poco) · il tempo stringe (fino a 50% di errori) | 500 | **83,2%** | 79,9% – 86,5% | ✅ sopra il 60% |
| Motore PIMC (400 mondi) con 10% di errori | 2000 | **64,3%** | 62,3% – 66,4% | ✅ sopra il 60% |

## Personaggi

Vaglio: 200 personaggi, 200 partite ciascuno contro ISMCTS potenziato (2000 iterazioni). Punteggio minimo 64,0%, primo quartile 72,2%, mediana 75,5%, massimo 88,8%; sopra il 60%: 200 su 200. I 20 più difficili per lui sono qui sotto, contro la Massima.

| Avversario | Partite | Punteggio | IC 95% | Obiettivo |
|---|---:|---:|---|---|
| Personaggio n. 41 | 500 | **73,2%** | 69,6% – 76,8% | ✅ sopra il 60% |
| Personaggio n. 70 | 500 | **75,0%** | 71,6% – 78,4% | ✅ sopra il 60% |
| Personaggio n. 47 | 500 | **78,3%** | 74,8% – 81,8% | ✅ sopra il 60% |
| Personaggio n. 162 | 500 | **72,6%** | 68,8% – 76,4% | ✅ sopra il 60% |
| Personaggio n. 197 | 500 | **74,5%** | 70,9% – 78,1% | ✅ sopra il 60% |
| Personaggio n. 109 | 500 | **76,1%** | 72,5% – 79,7% | ✅ sopra il 60% |
| Personaggio n. 178 | 500 | **77,3%** | 73,9% – 80,7% | ✅ sopra il 60% |
| Personaggio n. 66 | 500 | **76,1%** | 72,4% – 79,8% | ✅ sopra il 60% |
| Personaggio n. 154 | 500 | **79,2%** | 75,9% – 82,5% | ✅ sopra il 60% |
| Personaggio n. 38 | 500 | **76,2%** | 72,9% – 79,5% | ✅ sopra il 60% |
| Personaggio n. 130 | 500 | **79,9%** | 76,5% – 83,3% | ✅ sopra il 60% |
| Personaggio n. 63 | 500 | **73,7%** | 70,1% – 77,3% | ✅ sopra il 60% |
| Personaggio n. 81 | 500 | **74,3%** | 70,7% – 77,9% | ✅ sopra il 60% |
| Personaggio n. 61 | 500 | **80,3%** | 77,0% – 83,6% | ✅ sopra il 60% |
| Personaggio n. 60 | 500 | **74,5%** | 70,8% – 78,2% | ✅ sopra il 60% |
| Personaggio n. 53 | 500 | **73,4%** | 69,7% – 77,1% | ✅ sopra il 60% |
| Personaggio n. 62 | 500 | **77,1%** | 73,4% – 80,8% | ✅ sopra il 60% |
| Personaggio n. 58 | 500 | **74,6%** | 71,0% – 78,2% | ✅ sopra il 60% |
| Personaggio n. 98 | 500 | **75,7%** | 72,1% – 79,3% | ✅ sopra il 60% |
| Personaggio n. 119 | 500 | **77,6%** | 74,2% – 81,0% | ✅ sopra il 60% |

## Motori forti

La Massima della v0.5.0 non compare: è il giocatore stesso. ISMCTS da 100.000 iterazioni: 500 partite; ISMCTS da 50.000 e gli ISMCTS
potenziati da 20.000 e 50.000: 1.000 partite; gli altri 2.000 (PIMC 3.000 e 6.000 mondi e le versioni vecchie: 1.000).

| Avversario | Partite | Punteggio | IC 95% | Obiettivo |
|---|---:|---:|---|---|
| ISMCTS (5000 iterazioni) | 2000 | **58,8%** | 56,6% – 60,9% | ✅ sopra il 50% |
| ISMCTS (20000 iterazioni) | 2000 | **59,0%** | 56,9% – 61,0% | ✅ sopra il 50% |
| ISMCTS (50000 iterazioni) | 1000 | **54,0%** | 51,1% – 57,0% | ✅ sopra il 50% |
| ISMCTS (100000 iterazioni) | 500 | **54,0%** | 49,7% – 58,3% | ≈ al limite |
| ISMCTS potenziato (2000 iterazioni) | 2000 | **56,5%** | 54,4% – 58,6% | ✅ sopra il 50% |
| ISMCTS potenziato (20000 iterazioni) | 1000 | **53,5%** | 50,7% – 56,3% | ✅ sopra il 50% |
| ISMCTS potenziato (50000 iterazioni) | 1000 | **53,3%** | 50,6% – 56,0% | ✅ sopra il 50% |
| Motore PIMC (400 mondi) | 2000 | **56,5%** | 54,4% – 58,5% | ✅ sopra il 50% |
| Motore PIMC (3000 mondi) | 1000 | **55,2%** | 52,3% – 58,1% | ✅ sopra il 50% |
| Motore PIMC (6000 mondi) | 1000 | **56,5%** | 53,5% – 59,4% | ✅ sopra il 50% |
| Rete di politica pura (istinto, senza ricerca) | 2000 | **63,6%** | 61,6% – 65,6% | ✅ sopra il 50% |
| ISMCTS potenziato (20000 iterazioni) [v0.3.0] | 1000 | **53,5%** | 50,7% – 56,3% | ✅ sopra il 50% |
| Motore PIMC (1000 mondi) [v0.2.0] | 1000 | **52,8%** | 50,0% – 55,6% | ≈ al limite |
| Motore PIMC (1000 mondi) [v0.1.1] | 1000 | **60,5%** | 57,6% – 63,3% | ✅ sopra il 50% |

## Avversari cattivi

Gli sfruttatori allenati contro il motore e l'ingannatore arrivano con la fase 8.

| Avversario | Partite | Punteggio | IC 95% | Obiettivo |
|---|---:|---:|---|---|
| Camaleonte n. 1: cambia stile ogni 4 prese tra 7 stili | 1000 | **77,9%** | 75,4% – 80,4% | ✅ sopra il 50% |
| Camaleonte n. 2: cambia stile ogni 4 prese tra 7 stili | 1000 | **77,4%** | 74,9% – 79,9% | ✅ sopra il 50% |
| Imprevedibile: ISMCTS potenziato (20000 iterazioni) che sceglie a caso tra le mosse buone | 1000 | **54,5%** | 51,8% – 57,3% | ✅ sopra il 50% |

## Super-bari (tetto)

| Avversario | Partite | Punteggio | IC 95% | Obiettivo |
|---|---:|---:|---|---|
| Baro: vede le carte avversarie, ISMCTS potenziato (20000 iterazioni) | 500 | **27,8%** | 24,0% – 31,6% | misura |
| Super-baro: vede mano avversaria e mazzo, ISMCTS potenziato (20000 iterazioni) | 500 | **4,1%** | 2,5% – 5,7% | misura |

## Come riprodurlo

```
export BRISCOLA_POLICY=crates/core/nets/politica.bin BRISCOLA_SIM_CACHE=cache
P=infer:20000:1:1000:0.25
cargo run --release -p briscola-sim -- banco --player $P --famiglie umani,persone --min-deals 250 --seed 20261028
cargo run --release -p briscola-sim -- banco --player $P --famiglie personaggi --screen ismcts+:2000 --screen-deals 100 --top 20 --min-deals 250 --seed 20261028
cargo run --release -p briscola-sim -- banco --player $P --famiglie motori --solo 0-1 --min-deals 1000 --max-deals 1000 --seed 20261028   # un motore alla volta
cargo run --release -p briscola-sim -- banco --player $P --famiglie cattivi --min-deals 500 --max-deals 500 --seed 20261028
cargo run --release -p briscola-sim -- banco --player $P --famiglie bari --min-deals 250 --max-deals 250 --seed 20261028
cargo run --release -p briscola-sim -- esame --player $P --positions 20000 --seed 20261028
```

Circa 20 ore di calcolo su 4 core, spezzate in prove da meno di due ore riprese dalla cache.
