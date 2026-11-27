#!/usr/bin/env python3
"""Massima 2.0 - curva per dimensione della rete: stato per la pagina di avanzamento.

Legge lo stato dell'allenamento pubblicato dal PC (massima2-reti/stato.json sul ramo
claude/rl-stato) e, se ci sono, i risultati contro Massima giocati in cloud
(CARTELLA_RIS/rete_dim_<N>.txt). Scrive JSON {rows, meta} su stdout.
Uso: reti_stato.py STATO_PC.json CARTELLA_RIS
"""
import json, os, re, sys
from datetime import datetime, timezone

pc = json.load(open(sys.argv[1], encoding="utf-8-sig"))
ris = sys.argv[2]
pausa = len(sys.argv) > 3 and sys.argv[3] == "pausa"  # PC spento: la rete in corso è in pausa
rows = []
for r in pc["reti"]:
    n = r["hidden"]
    st = "in pausa" if pausa and r["stato"] == "in corso" else r["stato"]
    row = dict(rete=f"h{n}", neuroni=n, stato=st, partite=r["partite"], obiettivo=r["obiettivo"],
               completamento=round(100 * min(r["partite"], r["obiettivo"]) / r["obiettivo"], 1),
               partite_min=r.get("partite_min"), ore_rimaste=r.get("ore_rimaste"),
               inizio=r.get("inizio"), fine=r.get("fine"),
               punteggio=None, ic_basso=None, ic_alto=None, prova="da giocare")
    f = os.path.join(ris, f"rete_dim_{n}.txt")
    if os.path.exists(f):
        m = re.search(r"Punteggio \(pareggio = ½\): ([\d.]+)%\s+\(IC 95% sui mazzi: ([\d.]+)% - ([\d.]+)%", open(f).read())
        if m:
            row.update(punteggio=float(m[1]), ic_basso=float(m[2]), ic_alto=float(m[3]), prova="finita")
    elif r["stato"] == "finita":
        row["prova"] = "in corso" if os.path.exists(f[:-4] + ".tmp") else "in coda"
    rows.append(row)
agg = datetime.fromisoformat(pc["aggiornato"].replace("Z", "+00:00")).astimezone(timezone.utc)
json.dump({"rows": rows, "meta": [{"label": "Aggiornato", "value": agg.strftime("%d-%m-%Y %H:%M UTC")},
                                  {"label": "Fonte", "value": "massima2-reti/stato.json (ramo claude/rl-stato), scritto dal PC"}]},
          sys.stdout, ensure_ascii=False, indent=1)
