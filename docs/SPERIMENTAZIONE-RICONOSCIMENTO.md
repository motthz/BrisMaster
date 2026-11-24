# Sperimentazione: audit del riconoscimento ed esperimenti misurati

Risposta operativa a `docs/PROPOSTA-RICONOSCIMENTO-IBRIDO.md`. Regola seguita: **il riconoscitore
attuale resta il default**. Ogni novità è dietro un interruttore che funziona solo nelle prove
(`cfg!(test)` + variabile d'ambiente) e viene integrata solo se supera i banchi esistenti.

Base del lavoro: `main` a `09674e7` (BrisMaster 0.6.16 + proposta), Linux, rustc stabile,
profilo release. Tutte le misure sono sullo stesso codice e sugli stessi dati; i binari sono
copiati per esperimento (vedi §4 per i comandi).

---

## 1. Audit del codice (stato reale, 0.6.16)

### 1.1 Pipeline effettiva

| fase | dove | cosa fa davvero |
|---|---|---|
| cattura | `capture.rs`, `screen.rs` | `xcap` ogni 16 ms; nelle prove legge `BRISMASTER_SCHERMO` |
| coda | `mod.rs` (`Frames`, `FRAMES`=8) | tiene solo schermate cambiate nella zona carte; scarta le più ridondanti, mai l'ultima |
| zona | `mod.rs` (`Focus`, `FULL_EVERY`=24), `layout.rs` | analizza solo la zona delle carte, ogni 24 letture tutto lo schermo; salva `tavolo.json` |
| ricerca carte | `detect.rs` (`analyze` → `analyze_once`, `TRIES`=7) | aree chiare con 7 definizioni di "bianco", rettangoli ruotati, erosione per separare il ventaglio, carte parziali, regole per mazzi sconosciuti, rilettura senza di esse |
| ruoli | `detect.rs::assign_briscola` | mano = riga più bassa di carte intere; briscola = di traverso o parziale vicino al mazzo; tavolo = carte in piedi sopra la mano (0.6.16: mai una carta nella riga della mano) |
| confronto | `reference.rs` | miniatura 16×26 luminosità + 2 differenze di colore, correlazione normalizzata, ritagli/spostamenti, firma dei colori, voto dello stile del sito; **4 mazzi** (3 napoletani + piacentine) |
| punteggio | `mod.rs::recognize` | `sim·8` (+1 EXACT) + rete pesata se `top` < 0.65 + accordo rete (+0.8) + simboli (−3) + memoria (+6, decisiva sotto 0.75) |
| rete | `nn.rs` | CNN 40×64, testa seme (4) + valore (10), allenata solo sui mazzi dell'app |
| tempo | `assistant.rs::smooth` | media 50/50 dei punteggi di una carta ferma con la lettura precedente nello stesso posto |
| partita | `assistant.rs` | avvistamenti (`SIGHT_MARGIN` 0.8), certezza (`sure`), inserimento a distacco ≥ 1.4 (`AUTO_MARGIN`), conferma, `auto_accept` dopo 6 letture, regole come controllo, recuperi (`swap_misread`, `catch_up`/`rebuild`, `resync`, `adopt_screen` a 30 tick) |

### 1.2 Differenze fra documentazione e codice

- `RICONOSCIMENTO-CARTE.md` dice "come nella 0.6.9", "92 prove", rete "sui 3 mazzi di
  riferimento", tabella dei risultati della 0.6.9: oggi sono **129 prove**, **4 mazzi**, rete
  riallenata sui 4 (0.6.16), accordo rete/confronto, regola della riga della mano, prove che non si
  fermano a una carta debole.
- Le modifiche 0.6.12–0.6.15 (giocate dedotte, avviso di divergenza, recupero aperture) stanno in
  `assistant.rs` e in `docs/riconoscimento-stato-pr64.md`, non nel documento del riconoscimento.
- Lo schema dati proposto (`track_id`, `candidates[40]`, `quality`, `provisional`) **non esiste**:
  la visione passa a `assistant.rs` una `Scene` con `CardView` (punteggi, `sim`, `vis`, `at`), senza
  identità di traccia; la continuità è ricostruita da `smooth` per posizione.

### 1.3 Limiti principali (con evidenza)

1. **Manca una misura della prima lettura.** I banchi misurano la partita (conferme, carte a mano,
   partite perfette) o la carta su un tavolo singolo (`mazzi_sconosciuti`), non "la carta è giusta
   e sicura nella prima schermata in cui compare". Aggiunto il banco `prima_lettura` (E0).
2. **Punteggi non calibrati e sommati a pezzi.** `sim·8`, log-probabilità della rete con un peso
   a rampa, bonus fissi (+1, +0.8, +6, −3): soglie scelte a mano. Il distacco 1.4 non corrisponde a
   una probabilità d'errore misurata.
3. **La rete conta quasi solo sui mazzi sconosciuti** (`top` < 0.65). Con la vecchia rete,
   aprirla di più (0.85) aveva rotto il caso "grandi"; la rete è stata riallenata in 0.6.16 e la
   soglia non è stata rimisurata.
4. **Copertura dei mazzi.** Mancano trevigiane, bergamasche, siciliane, ecc. Sui 8 mazzi mai
   visti: 74,3% giuste al primo colpo (0.6.16); i due mazzi trevigiani restano sotto il 25%.
5. **Proporzioni.** `card_shaped` accetta 0.52–0.76: le carte del nord sono più strette (piacentine
   ≈ 0.50, trevigiane ≈ 0.53–0.55). Nel 2025 lo 0.46 aveva rotto "strette": mai provato un valore
   intermedio.
6. **Tempo.** `smooth` fa una media con la lettura precedente: una lettura cattiva (carta che
   scivola, pescata in arrivo) sporca quella buona. Gli avvistamenti contano schermate quasi uguali
   come osservazioni indipendenti (lo segnala anche la proposta).
7. **Carte coperte in parte nella mano** (es. cavallo di bastoni coperto dal contatore del mazzo
   nella partita u04): letta come carta intera, distacco basso, conferma.
8. **Inizio partita.** u13: mano iniziale letta male → 8 carte a mano. È il singolo caso peggiore
   del banco delle 20 partite.
9. **Dati.** Tutti i banchi sono sintetici o generati (20 partite dal tavolo di prova dell'utente,
   spoglio/display, partite estreme, mazzi di terzi solo in locale). Non ci sono partite reali
   annotate: nessuna cifra qui va letta come accuratezza su siti reali.

### 1.4 Cosa NON serve rifare

Rilevazione multi-tentativo, confronto multi-mazzo, CNN seme/valore, memoria locale, smoothing,
recupero dalla cronologia e regole come controllo esistono già e sono coperti da prove. Un detector
YOLO o OmniParser non è giustificato finché i falsi negativi di rilevazione non risultano la causa
dominante (vedi E0).

---

## 2. Esperimenti e ordine di priorità

Priorità: (a) carte giuste **e sicure** alla prima lettura, (b) partite ricostruite per intero.

| id | ipotesi | interruttore (solo prove) | costo |
|---|---|---|---|
| E0 | baseline con la nuova misura della prima lettura | — (banco `prima_lettura`) | nessuno |
| E1 | la lettura della schermata migliore di una carta ferma è meglio della media | `SPER_MIGLIORE=1` | nullo a runtime |
| E2a | la rete riallenata può contare anche su carte che somigliano di più ai mazzi noti | `NN_DA=0.75`, `NN_DA=0.85` (già esistente) | +1 inferenza per carta |
| E2b | la rete guarda anche la carta capovolta (media delle due) | `SPER_TTA=1` (ora default; `SENZA_TTA=1` per spegnerlo) | +1 inferenza |
| E6 | le carte giocate che sbiadiscono a fine presa si ritrovano togliendo il colore del tavolo | `SPER_SBIADITE=1` | 2 ricerche in più a tavolo vuoto |
| E3 | proporzioni un po' più strette trovano le carte del nord senza rompere "strette" | `SPER_STRETTE=0.48` | nullo |

Gate per integrare: nessuna regressione su 20 partite (partita per partita), spoglio, display, 17
partite estreme, 129 prove; nessun aumento delle letture sbagliate date per sicure; miglioramento
misurabile su `prima_lettura` o `mazzi_sconosciuti`.

## 3. Risultati

Banchi: **u20** = 20 partite registrate dal tavolo di prova (`partite_registrate`), **spoglio** e
**display** = 20 + 20 partite generate, **estreme** = 17 partite agli estremi
(`briscola_extreme_games`), **mazzi** = 8 mazzi mai visti, 1074 carte (`mazzi_sconosciuti`, solo
in locale: i mazzi di terzi non sono nel repository). "Sicura" = distacco ≥ 1.4, cioè la carta che
l'app inserisce senza chiedere.

### 3.1 E0 — baseline 0.6.16 (prima lettura)

| banco | comparse | giuste alla prima lettura | giuste e sicure | sbagliate (sicure) | mancate | identità sbagliate date per sicure |
|---|---|---|---|---|---|---|
| u20 | 1220 | 95,2% | 93,7% | 10 (9) | 48 (47 in tavola) | 122 su 83 020 letture (0,15%) |
| spoglio | 1220 | 97,5% | 97,5% | 0 | 31 | 0 |
| display | 1220 | 97,5% | 97,5% | 0 (0) | 31 | 1 su 15 000 |

Partite: u20 **8/20 perfette** (19/20 seguite fino in fondo; u13 la peggiore con 8 carte scritte a
mano), spoglio 20/20, display 19/20, estreme 17/17, mazzi 74,3% giuste (70,5% senza conferma, 85
sbagliate date per sicure).

Errori dominanti: (1) carte in tavola **non trovate** (47 delle 48 mancate su u20), quasi tutte
carte giocate che sbiadiscono a fine presa; (2) carte della mano lette giuste ma con distacco basso
(93,8% sicure contro 97,5% giuste) → conferme.

### 3.2 Esperimenti

| id | u20 partite perfette | u20 prima lettura (sicure) | spoglio / display | estreme | mazzi giuste · senza conferma · sicure sbagliate | decisione |
|---|---|---|---|---|---|---|
| base | 8/20 | 93,7% | 20/20 · 19/20 | 17/17 | 74,3% · 70,5% · 85 | — |
| E1 miglior lettura invece della media | 8/20, identico partita per partita | — | — | — | — | **REVERT** (nessun effetto) |
| E2a rete da 0.75 | — | — | — | — | 74,3% · 70,9% · **92** | **REVERT** (più errori sicuri) |
| E2a rete da 0.85 | (vedi C1) | | **display 18/20**: `sovrapposte_2` 3 carte a mano | 17/17 | 74,8% · 71,4% · 85 | **REVERT** (regressione display) |
| **E2b carta capovolta** | **14/20**, nessuna partita peggiore | 93,4% sui 13 letti per intero (base 92,9%; mano 94,2% vs 92,7%), errori sicuri invariati (79) | 20/20 · 19/20, identici | 17/17 | 74,9% · 70,5% · **75** | **KEEP** |
| C1 = E2b + rete da 0.85 | 15/20 | 94,9% (mano 97,5%), ma errori sicuri 130 vs 122 | 20/20 · **18/20** | 17/17 | 74,9% · 71,2% · 72 | REVERT (per la parte 0.85) |
| E3 proporzioni 0.48 | — | — | — | **rompe "strette"** | 75,3% · 71,5% · 81 | **REVERT** |
| E6 carte sbiadite | u01: 4 conferme e 4 a mano (base 0 e 0) | u01+u02: errori sicuri 44 vs 6 | — | — | — | **REVERT** |

Note:
- **E2b** toglie conferme in 9 partite su 20 (u05, u06, u09, u11, u13, u14, u16, u20, u24: 0 conferme,
  u16 da 2 a 1) senza cambiare nessun'altra cifra. Il tempo di analisi di una schermata
  intera resta nel rumore (650–710 ms con e senza, su questa macchina): la rete lavora su miniature
  40×64, una passata in più costa poco rispetto alla ricerca delle carte.
- La prima lettura di E2b sulle 20 partite è stata interrotta dal limite di tempo dopo 13 partite;
  il confronto è fatto sulle stesse 13 partite per tutte le varianti (base, C1, E2b).
- **Rete da 0.85**: dà la spinta maggiore sulla mano (sicure 97,5%), ma in `sovrapposte_2` l'app si
  convince di avere il Sei di Bastoni invece del Sette di Bastoni (carte sovrapposte, la rete vede
  solo una parte della carta) e servono 3 carte a mano; e aumenta le identità sbagliate date per
  sicure (u20 130 vs 122, display 3 vs 1). Non passa il gate. Da riprovare solo con la rete limitata
  alle carte intere (`vis` pieno).
- **E6** ritrova davvero le carte sbiadite nel caso che falliva (u06, schermate 38–39: Re di Denari
  e Cavallo di Bastoni), ma togliere il colore del tavolo fa uscire anche ombre e resti
  dell'animazione che somigliano a carte: le letture sicure sbagliate salgono di 7 volte e le
  partite peggiorano. Il problema resta il primo per numero di carte mancate; serve un criterio
  diverso (per esempio cercare solo dove la presa precedente aveva appena mostrato una carta).
- I numeri sono su banchi sintetici o generati, non su partite reali annotate (§1.3 punto 9).

### 3.3 Integrato

Solo **E2b**: `nn::log_probs` fa la media delle probabilità della carta e della carta capovolta.
Prove unitarie: 129 passate. Nessun'altra soglia cambia. Gli altri esperimenti restano nel codice
spenti, attivabili solo nelle prove, per poterli ripetere.

Rollback: nelle prove `SENZA_TTA=1` riporta la lettura di prima; per l'app basta rimettere il
ritorno anticipato in `nn::log_probs` (un `return Some(one);`).

### 3.4 Prossimi esperimenti suggeriti

1. Carte sbiadite (E6) con una ricerca limitata alla casella del tavolo dove la carta era attesa.
2. Rete da 0.85 solo sulle carte viste per intero.
3. Inizio partita (u13): la prima mano letta male costa da sola 7–8 carte a mano.

### 3.5 Secondo giro (0.6.17)

I tre passi di §3.4, provati sulla 0.6.16 + E2b.

| id | cosa | risultato | decisione |
|---|---|---|---|
| E6b | carte sbiadite cercate solo nelle caselle del tavolo già imparate (centro entro 5% dello schermo, somiglianza ≥ 0.85) | u06: carte mancate da 6 a 4, ma ancora 2 scritte a mano; u01: 4 conferme e 4 a mano (prima 0 e 0), 26 letture sicure sbagliate (prima 0) | **REVERT** |
| E2c | rete da 0.85 solo sulle carte viste per intero (`NN_DA_INTERE=0.85`) | `sovrapposte_2` ancora con 3 carte a mano: la carta letta male risulta intera | **REVERT** |
| R1 | inizio partita: alla distribuzione si pescano prima le carte viste nell'ultima lettura | vedi sotto | **KEEP** |

**R1, la causa di u13.** Nelle prime schermate un pezzo del pannello bianco dei punti (in alto a
destra) viene letto come "Asso di Spade" in mano, mentre le tre carte vere sono ancora in
animazione. La pescata iniziale prendeva le carte viste in mano da più tempo: entrava l'asso
finto, e quando comparivano le tre carte vere la quarta faceva dedurre "asso giocato, presa chiusa,
pescata": una presa in più, mazzo a 32, e la partita persa dal primo minuto (8 carte a mano).
Correzione in `next_draw`: solo alla prima distribuzione (`tricks == 0`), una carta che non c'è
nell'ultima lettura e non è mai comparsa in tavola passa dopo quelle viste adesso.

Risultati della 0.6.17 (E2b + R1) contro 0.6.16:

| banco | 0.6.16 | 0.6.17 |
|---|---|---|
| u20 partite perfette | 8/20 | **14/20** |
| u20 conferme chieste | 14 | **6** |
| u20 carte scritte a mano | 10 | **3** |
| u13 | 8 a mano | 1 a mano (la briscola letta male alla prima schermata) |
| spoglio / display | 20/20 · 19/20 | 20/20 · 19/20 (identici partita per partita) |
| estreme | 17/17 | 17/17 |
| prove unitarie | 129 | 129 |

Rollback di R1: togliere il filtro `stale` in `next_draw` (`assistant.rs`).

### 3.6 Dati reali (passo 4)

Non fatto da qui, di proposito: servono schermate di siti veri con la verità controllata da chi
gioca, e per scelta dell'utente schermate e immagini dei siti restano sul suo computer. Procedura
locale: registrare le schermate di una partita in una cartella (`0000.png`, ...), scrivere
`verita.json` nello stesso formato delle partite registrate (per schermata: `frame`, `hand`,
`trump`, `trick`), poi lanciare `partite_registrate` e `prima_lettura` sulla cartella e
`scripts/prima-lettura.py`. Solo i numeri riassuntivi, non le immagini, servono per confrontare
le versioni.

## 4. Riproduzione

```
cargo test --release -p brismaster --no-run          # binario in target/release/deps/brismaster-*
PARTITE=cartella ./BIN assistant::tests::partite_registrate --ignored --exact --nocapture
PARTITE=cartella PRIMA_CSV=letture.csv ./BIN assistant::tests::prima_lettura --ignored --exact --nocapture
python3 scripts/prima-lettura.py cartella letture.csv [--finestra 10]
./BIN assistant::tests::briscola_extreme_games --ignored --exact --nocapture
MAZZI=cartella ./BIN vision::other_sites::mazzi_sconosciuti --ignored --exact --nocapture
```
Interruttori (solo nelle prove): `SENZA_TTA=1`, `NN_DA=0.85`, `SPER_MIGLIORE=1`,
`SPER_STRETTE=0.48`, `SPER_SBIADITE=1`, `NN_DA_INTERE=0.85`. Diagnosi: `DEBUG_PASSI=1`
(ogni passo dedotto dagli avvistamenti), `DEBUG_TAVOLA=1`.
