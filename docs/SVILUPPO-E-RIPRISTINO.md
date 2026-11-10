# Sviluppo e ripristino senza Codex

Base di questo lavoro: tag `v0.6.0`, commit `c1e83b751d87b02e83dcbb7921b85ec2afdea372`.
Le modifiche sono sul ramo `improve/v0.6.0-recognition-engines-ui`, senza cambiare la release pubblicata.

## Avvio su un altro computer

Installa Git, Rust stable (verificato con 1.99.1) e un compilatore C.
Su Windows servono i Build Tools di Visual Studio con il carico di lavoro C++.
Su Linux servono un desktop X11/Wayland e le librerie OpenGL/XKB; acquisizione dello
schermo e overlay nativo si verificano su Windows. Non occorrono token o Codex.

```sh
git clone https://github.com/motthz/BrisMaster.git
cd BrisMaster
git switch improve/v0.6.0-recognition-engines-ui
cargo test --workspace --locked --release
cargo run --locked --release -p brismaster
```

Il primo download delle dipendenze richiede Internet. Non cancellare né rigenerare
`Cargo.lock` per aggirare problemi di compilazione. I modelli allenati e i mazzi di
riferimento sono già nel repository; avviare l'app non richiede di riallenarli.

Su Linux/macOS, `bash scripts/validate.sh` esegue test e build. In PowerShell usa
i due comandi Cargo equivalenti. Alcuni test di riconoscimento durano più di un minuto.
I test marcati `ignored` restano prove separate, non vengono conteggiati come passati.

## Copia autonoma e cronologia completa

Dopo un checkpoint Git pulito:

```sh
bash scripts/archive-work.sh /percorso/assoluto/nuovo-backup
```

Il comando non sovrascrive backup esistenti. Crea un Git bundle con tutti i rami
locali, un archivio dei sorgenti, istruzioni `RESTORE.txt` e checksum SHA-256.
Rifiuta una copia parziale se ci sono modifiche non salvate in Git.
Copia la cartella su un disco tuo: per ripristinarla bastano Git e Rust.

```sh
sha256sum -c SHA256SUMS
git clone -b improve/v0.6.0-recognition-engines-ui BrisMaster.bundle BrisMaster
```

Il bundle contiene il codice e la cronologia; non include cache Cargo, dipendenze
scaricate, credenziali o partite personali. Conserva separatamente i dati dell'app.
Su Windows si trovano in `Documenti\BrisMaster` (memorie delle carte, area dello
schermo e registrazioni scelte dall'utente). Le impostazioni egui sono salvate
nella cartella applicativa del sistema.

## Prove e confronti ripetibili

`bash scripts/bench-kernels.sh` misura quattro carichi a seme fisso, senza scrivere
modelli o rapporti esistenti. Esegui i confronti sulla stessa macchina, con lo stesso
compilatore, dopo il riscaldamento, evitando altri lavori pesanti.
I checksum servono a verificare che il lavoro sia stato eseguito. I tempi non sono
una misura della forza di gioco. Al river dell'Hold'em il checksum della vecchia
stima Monte Carlo può differire da quello nuovo, che è esatto.

Per la forza di gioco usa i simulatori e le prove in duplicato già documentati nel
README e in `bench/0.6.0/README.md`. Un test breve è uno smoke test: non dimostra un
miglioramento statistico. Conserva i risultati grezzi insieme a commit, comando,
seme, budget, avversari e intervalli di confidenza.

Vedi `STATO-LAVORO.md` per ciò che è pronto e ciò che richiede ulteriori prove.
