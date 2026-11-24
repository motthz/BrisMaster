# Sperimentazione: audit del riconoscimento ed esperimenti misurati

Risposta operativa a `docs/PROPOSTA-RICONOSCIMENTO-IBRIDO.md`. Regola seguita: **il riconoscitore
attuale resta il default**. Ogni novità è dietro un interruttore che funziona solo nelle prove
(`cfg!(test)` + variabile d'ambiente) e viene integrata solo se supera i banchi esistenti.

Base del lavoro: `main` a `09674e7` (BrisMaster 0.6.16 + proposta), Linux, rustc stabile,
profilo release. Tutte le misure sono sullo stesso codice e sugli stessi dati; i binari sono
copiati per esperimento (vedi §6 per i comandi).

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
| E2b | la rete guarda anche la carta capovolta (media delle due) | `SPER_TTA=1` | +1 inferenza |
| E3 | proporzioni un po' più strette trovano le carte del nord senza rompere "strette" | `SPER_STRETTE=0.48` | nullo |

Gate per integrare: nessuna regressione su 20 partite (partita per partita), spoglio, display, 17
partite estreme, 129 prove; nessun aumento delle letture sbagliate date per sicure; miglioramento
misurabile su `prima_lettura` o `mazzi_sconosciuti`.

## 3. Risultati

(compilati man mano: vedi sotto)
