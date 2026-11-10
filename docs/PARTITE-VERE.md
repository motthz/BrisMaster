# Allenare il motore sulle partite vere

Il motore deduce le carte dell'avversario da come gioca, con una rete di politica
(`crates/core/nets/politica.bin`) allenata su partite tra motori. Le persone vere giocano in
modo diverso: con le loro partite la deduzione può imparare come giocano davvero.

## 1. Salvataggio (lo decidi tu)

La registrazione parte **spenta** a ogni partita. Nell'assistente di briscola la casella
**Registra questa partita** la accende: si salva tutta la partita, anche le carte inserite
prima di accenderla, in `Documenti\BrisMaster\Partite\partita_<data>.json`
(`crates/app/src/record.rs`). Spegnendola il file di quella partita si cancella. Si salvano
briscola, chi comincia, le tue pescate, ogni carta giocata da te e dall'avversario, il
consiglio del motore e il risultato. Solo carte: nessun nome, nessun dato del sito. Anche le
partite lasciate a metà servono.

Le partite salvate in automatico dalle versioni precedenti (fino alla 0.5.x) vengono
cancellate una volta sola al primo avvio della nuova versione (resta il segno
`Documenti\BrisMaster\.registrazione-a-scelta`); gli altri file della cartella non si toccano.

## 2. Consegna

Nell'assistente, pulsante **Prepara file da inviare**: raccoglie tutte le partite registrate in
un solo file, `Documenti\BrisMaster\partite-brismaster.json`, e lo mostra in Esplora risorse.
Si allega in un messaggio del progetto. Si può rinviare ogni volta tutto: le partite già
ricevute vengono riconosciute e contate una sola volta.

## 3. Dai file ai dati di allenamento

```
cargo run --release -p briscola-sim --bin reali -- --partite partite-brismaster.json --out dati-reali
```

Delle carte dell'avversario si sa quando le ha giocate ma non quando le ha pescate; a fine
partita però sono tutte uscite, quindi la sua mano a ogni giocata è quasi determinata. `reali`
campiona 512 ricostruzioni compatibili con il registro (uniformi sugli ordini del mazzo
possibili), le pesa con quanto la rete spiega le giocate vere e ne tiene 16 per partita. Le
partite incoerenti (carta inserita male) vengono scartate e segnalate. Un quinto delle partite,
scelte in modo fisso dal contenuto, resta fuori come controllo.

## 4. Allenamento

```
cargo run --release -p briscola-sim --bin train -- --kind policy --data dati-reali \
    --init crates/core/nets/politica.bin --out politica-reale.bin --epochs 10
```

Si parte dalla rete attuale e la si corregge (le partite vere sono poche rispetto a quelle tra
motori). Si può ripetere il passo 3 con `--rete politica-reale.bin` e riallenare (EM).

## 5. Verifica

1. **Previsione delle giocate vere** sulle partite di controllo, mai viste in allenamento:
   `reali --partite ... --confronta politica-reale.bin` stampa per le due reti la probabilità
   media data alla carta giocata davvero e quante volte meglio del caso.
2. **Vittorie** (nella sim della PR #33: avversario `reale:FILE[:TEMP]`, che gioca come la rete delle partite vere; deduzione con `BRISCOLA_POLICY=FILE` sul giocatore `infer:`): Massima con la nuova rete contro Massima con quella vecchia, contro un
   avversario che gioca come la rete delle partite vere e contro il banco di prova (umani
   simulati e motori), 8.000 partite. Entra nell'app solo se migliora contro gli "umani veri"
   senza peggiorare contro gli altri.

## Prova della catena con partite finte

`reali --finte N --partite finte.json` scrive N partite nel formato dell'app contro un
avversario "umano" simulato (euristico 60%, prendi-tutto 25%, a caso 15%) e le elabora.
Risultati (controllo = partite mai viste in allenamento; caso = 0,341 per giocata):

| Partite | Rete attuale | Rete riallenata |
|---|---|---|
| 100 | 0,89x il caso | 1,08x |
| 300 | 0,82x | 1,12x |
| 1.000 | 0,85x | 1,16x |

La rete attuale, allenata sui motori, prevede questo avversario peggio del caso; da qualche
centinaio di partite la rete riallenata lo prevede meglio. Con persone vere il guadagno può
essere diverso: si misura con il punto 5.

## Partite vere sul web (ricerca del 5 novembre 2026)

Non esiste una raccolta pubblica di partite di briscola tra persone. Quello che c'è:

- progetti di bot (BriscolaBot, briscolab, TextArena/EnvCommons, Briscola_engine_agents,
  briscolol): partite generate tra programmi, non persone;
- BriscolaPaperSourceCode (articolo arXiv 2605.17043): circa un milione di partite tra tre bot
  a regole fisse, utili al più come altri avversari simulati;
- BriscolaBot riporta 170 partite registrate contro persone, ma non le pubblica;
- Board Game Arena ha circa 185.000 partite di briscola, ma i registri si vedono solo da
  iscritti e la raccolta automatica non è permessa dalle sue condizioni d'uso.

Quindi la fonte di partite vere restano le partite registrate con l'app.
