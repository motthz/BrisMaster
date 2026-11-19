# Briscola a bassa risoluzione

Baseline pubblicata: `v0.6.11` (`dcd0bfa`). Il commit `45b94d0` aggiunge la
strumentazione dei test. Il rilevatore resta quello della baseline fino al
commit successivo, che contiene la candidata.

## Riproduzione

```sh
cargo test --locked --release -p brismaster banco_bassa_risoluzione -- --ignored --nocapture --test-threads=1
cargo test --locked --release -p brismaster briscola_extreme_games -- --ignored --nocapture --test-threads=1
BRIS_STRESS_VALIDAZIONE=1 cargo test --locked --release -p brismaster briscola_extreme_games -- --ignored --nocapture --test-threads=1
BRIS_STRESS_RIDUZIONE=4 BRIS_STRESS_CASO=normale cargo test --locked --release -p brismaster briscola_extreme_games -- --ignored --nocapture --test-threads=1
```

La riduzione ridimensiona tutto il fotogramma, non solo le carte. Il banco misura
48 posizioni attese per scala su due stili e quattro condizioni: tavolo
standard, tavolo chiaro, loghi con sfocatura e ventaglio con briscola coperta.
I controlli negativi includono tavoli senza carte scoperte e falsi riquadri.
`repeated_score_widgets_are_not_cards` riproduce i riquadri colorati ripetuti
scambiati per carte dalla baseline.

| Altezza nominale | Automatiche corrette v0.6.11 | Automatiche corrette candidata |
|---:|---:|---:|
| 160 px | 44/48 | 44/48 |
| 80 px | 44/48 | 44/48 |
| 53 px | 41/48 | 41/48 |
| 40 px | 30/48 | 48/48 |
| 32 px | 0/48 | 42/48 |

I 17 scenari storici e i quattro scenari di riserva completano tutti
20 prese, con esito, conferme e ritardi invariati caso per caso. La suite
ordinaria completa 116 test, con 18 test manuali o separati esclusi.

Nelle due schermate reali già in repository, ridotte a un quinto per lato,
la sola fase di rilevamento torna a proporre correttamente tutte le nove
identità attese (cinque nella prima e quattro in Solitalian; la baseline
non propone nessuna carta in entrambe). Alcune hanno margine basso.
Sono schermate statiche, non partite dal vivo.

Le 300 coppie fotogramma/posizione annotate sono state confrontate anche
singolarmente. Le due identità errate accettate a 80 px sono presenti in
entrambe le versioni.

Il conteggio automatico del banco è basato su identità vera e margine di 1,4.
Nel test di partita, `copertura` è il contatore dell'app; il successo del
test richiede anche che prese, carte uscite e punteggio coincidano con la
partita simulata.

La partita standard a 40 px, seme 1200, passa da 16 prese con copertura
19/33 a 20 prese e 38/40. A 32 px passa da nessuna presa a 20 prese
e 38/40. In entrambi i test la candidata finisce con i punteggi corretti e
zero interventi. Gli otto fotogrammi negativi esplorativi con quattro
riquadri colorati ripetuti passano da tre falsi candidati per fotogramma a zero.

A 40 px il ventaglio positivo passa dal blocco durante la presa 11 al
completamento di tutte le prese con 38/40 e zero interventi. Con scritte e
sfocatura, entrambe le versioni chiudono la partita. Gli interventi
scendono da 3 a 1 e la copertura sale da 33/40 a 35/40.

La candidata ha ancora difficoltà a seguire un'intera partita a 40 px su
tavolo molto chiaro: vede la mano iniziale ma non individua la prima
giocata al centro. Il rilevatore sperimentale dei pixel piccoli si attiva
sulla regione effettivamente analizzata se questa non supera 400×300 px;
non equivale al supporto di un tavolo minuscolo in qualunque viewport.
Le prove sono eseguite su immagini, senza convalida dal vivo sul sito
dell'utente in Windows.

Nel passaggio sequenziale su Linux, la mediana di analisi delle schermate
con carte è passata da 102 a 300 ms a 40 px e da 20 a 125 ms a 32 px.
La baseline era più rapida anche perché in questi casi scartava più carte.
Il banco non misura il ritardo dei consigli durante la cattura dal vivo.

## Recupero mirato sul tavolo chiaro

La candidata successiva, `d31d442`, cerca giocate vicino al centro solo
nelle regioni piccole con sfondo molto chiaro. La mano fornisce la misura
attesa e limita l’area della ricerca. La maschera usa i pixel quasi neutri
della cattura, senza il bilanciamento del bianco che univa carte e sfondo.
Se una parte della sagoma si stacca, il riquadro viene riesaminato alla
larghezza appresa. Il recupero richiede somiglianza forte con un
riferimento, distacco di almeno 1,4 e compatibilità con le carte già
presenti. Le coordinate e la misura della mano possono restare disponibili
per cinque letture senza mano, purché il ritaglio rimanga lo stesso.

```sh
BRIS_STRESS_RIDUZIONE=4 BRIS_STRESS_CASO=tavolo-chiaro cargo test --locked --release -p brismaster briscola_extreme_games -- --ignored --nocapture --test-threads=1
cargo test --locked --release -p brismaster recovers_the_first_play_on_a_small_light_table
```

Il tavolo chiaro a 40 px (seme 1207) passa da zero a 20 prese, 37/40
giocate identificate e zero interventi. Il punteggio finale è quello
della simulazione (44 a 76). L'analisi dei singoli fotogrammi del banco
passa da 42/48 a 46/48 identità corrette automatiche a 32 px; a 40 px
resta 48/48. I riquadri ripetuti restano esclusi.

### Confronto con la candidata precedente

- 300 casi a fotogramma: quattro recuperi, 296 invariati. Nessun nuovo
  candidato parassita nei controlli negativi.
- 198 posizioni del banco siti, 132 delle stampe avorio e 10 schermate
  reali a varie scale: identità invariate.
- 21 partite del banco ordinario e di riserva: esiti, interventi e ritardi
  invariati rispetto alla 0.6.11 pubblicata. Suite: 117 test passati.
- A 40 px restano complete la partita standard e quella con ventaglio,
  entrambe senza interventi. Con scritte e sfocatura resta un intervento.
  A 32 px la partita standard termina senza interventi.
- Una misurazione sequenziale sullo stesso ambiente Linux vede la mediana
  salire da 130 a 152 ms a 32 px e da 281 a 290 ms a 40 px rispetto
  alla candidata precedente. Le letture sono di fotogrammi sintetici;
  la latenza dal vivo su Windows resta da verificare.
