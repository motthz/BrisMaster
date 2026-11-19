# Verificare il riconoscimento con un replay

Il banco di prova può leggere fotogrammi e annotazioni locali, senza caricarli
su GitHub. Imposta `BRIS_REPLAY_CARTE` sul percorso del JSON di annotazione e
`BRIS_REPLAY_FRAMES` sulla cartella dei fotogrammi.

```sh
BRIS_REPLAY_CARTE=/percorso/annotazioni.json BRIS_REPLAY_FRAMES=/percorso/fotogrammi \
  cargo test --release --locked -p brismaster replay_carte_annotate -- --ignored --nocapture
```

Il formato delle annotazioni e i ruoli delle carte sono definiti in
`crates/app/src/vision/replay.rs`. Conserva filmato, fotogrammi e annotazioni
nella tua cartella locale.
