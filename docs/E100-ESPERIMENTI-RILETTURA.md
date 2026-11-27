# Esperimenti E100 sulla rilettura

La coppia baseline e candidata usa siti, mazzi e semi identici. I 71 casi della candidata provengono dai 24 casi mirati e dalle partite disponibili del banco esteso, deduplicati per chiave. Le metriche sono relative ai replay sintetici. La modalità ordinaria ammette le correzioni simulate dal banco.

- **Rilettura in grigio.** Accetta il secondo passaggio solo se conserva almeno il numero di sagome rilevate e ripristina la modalità colore quando scarta il tentativo. Esiti identici su 12 partite e 234 ruoli di schermate con perturbazioni cromatiche.
- **Peso della rete sulle carte intere.** NN_DA_INTERE=0.85 produce esiti identici su 12 partite. Un'altra prova su schermate, NN_DA=0.90 e NN_K=1.35, dà 224 identificazioni corrette su 234 in entrambe le configurazioni. Con tinta -25°, una carta già corretta passa da incerta ad automatica.
- **Contorni selettivi.** La ricerca più permissiva si attiva anche per mazzi parzialmente riconosciuti, con almeno due sagome intere e lato lungo di almeno 110 px. Le 234 letture statiche coincidono. Sul mobile, app3 e seme 91000, i checkpoint in ritardo calano da 7 a 6 nella prova assistita. In modalità senza aiuti entrambi i semi app3 mobile falliscono con gli stessi risultati per entrambe le versioni. Il seme 91000 accetta una briscola errata e registra 12 prese. Il seme 91037 resta in attesa della briscola. [Ramo sperimentale](https://github.com/Motthz/BrisMaster/tree/codex/briscola-contorni-selettivi)

La candidata della PR #75 conserva le correzioni preesistenti. I nuovi tentativi non alzano il completamento senza aiuti sui casi confrontati. I 71 esiti appaiati non coprono l'intera matrice o le partite reali.

```text
current
Casi appaiati: 71
Riferimento completate 62 senza aiuti 35 giocate attribuite male 8 partite con attribuzioni errate 5
Candidata completate 63 senza aiuti 35 giocate attribuite male 8 partite con attribuzioni errate 5
siti_possibili app3 app-mobile seed=91000 (False, None, 0, 0) -> (True, 6, 0, 7)
new-experiments/gray/*.log
Casi appaiati: 12
Riferimento completate 10 senza aiuti 3 giocate attribuite male 2 partite con attribuzioni errate 2
Candidata completate 10 senza aiuti 3 giocate attribuite male 2 partite con attribuzioni errate 2
new-experiments/nn-*.log
Casi appaiati: 12
Riferimento completate 7 senza aiuti 0 giocate attribuite male 7 partite con attribuzioni errate 4
Candidata completate 7 senza aiuti 0 giocate attribuite male 7 partite con attribuzioni errate 4
new-experiments/evidence/*.log
Casi appaiati: 24
Riferimento completate 18 senza aiuti 5 giocate attribuite male 7 partite con attribuzioni errate 4
Candidata completate 18 senza aiuti 5 giocate attribuite male 7 partite con attribuzioni errate 4
siti_possibili app3 app-mobile seed=91000 (True, 6, 0, 7) -> (True, 6, 0, 6)
```
