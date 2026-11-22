#!/usr/bin/env python3
"""Massima 2.0 - stato della fase 2 per la pagina di avanzamento.

Legge punti.txt e la cache del simulatore (una riga per mazzo duplicato:
vinte pareggi perse punti posto0 posto1 punteggio) e scrive un JSON
{rows, meta} con completamento e punteggio parziale di ogni punto.
Uso: stato.py CARTELLA_CODA [CARTELLA_CACHE_EXTRA ...] > stato.json
"""
import json, math, os, re, sys, datetime

MAZZI = 5000
SEME = 20261117
MASSIMA = "infer:20000:1:1000:0.25"
R10000 = 0.3815

def chiave(spec):
    s = f"{spec}_{MASSIMA}_{SEME}_"
    return "".join(c if c.isalnum() or c in ".-" else "_" for c in s)

def gruppo(nome):
    if nome.startswith("iter_"): return "Iterazioni", float(nome[5:])
    if nome.startswith("mani_"): return "Mani dedotte", float(nome[5:])
    if nome.startswith("ucb_"): return "Esplorazione", float(nome[4:])
    if nome.startswith("minimo_"): return "Minimo delle mani", float(nome[7:])
    m = re.match(r"rete_(grande_)?(\d+)M", nome)
    if m: return ("Rete grande" if m.group(1) else "Rete piccola"), float(m.group(2))
    return "Altro", 0.0

def main():
    coda = sys.argv[1]
    cartelle = [os.path.join(coda, "cache")] + sys.argv[2:]
    files = []
    for c in cartelle:
        if os.path.isdir(c):
            files += [os.path.join(c, f) for f in os.listdir(c)]
    punti = [l.split("|", 1) for l in open(os.path.join(coda, "punti_tutti.txt") if os.path.exists(os.path.join(coda, "punti_tutti.txt")) else os.path.join(coda, "punti.txt")).read().splitlines() if "|" in l]
    in_corso = None
    rows = []
    for nome, spec in punti:
        k = chiave(spec)
        visti, somme = set(), []
        for f in files:
            b = os.path.basename(f)
            if not b.startswith(k): continue
            m = re.match(r"(\d+)-(\d+)", b[len(k):])
            if not m: continue
            a, z = int(m.group(1)), int(m.group(2))
            for d, riga in zip(range(a, z), open(f).read().splitlines()):
                if d in visti or d >= MAZZI: continue
                visti.add(d)
                somme.append(float(riga.split()[6]))
        n = len(somme)
        finito = os.path.exists(os.path.join(coda, "ris", nome + ".txt"))
        if n >= MAZZI: finito = True
        stato = "finito" if finito else ("in corso" if n > 0 and in_corso is None else ("in pausa" if n > 0 else "in coda"))
        if stato == "in corso": in_corso = nome
        g, v = gruppo(nome)
        r = dict(punto=nome, gruppo=g, valore=v, spec=spec, partite=2 * n, partite_totali=2 * MAZZI,
                 completamento=round(100.0 * n / MAZZI, 1), stato=stato,
                 punteggio=None, ic_basso=None, ic_alto=None, esito=None)
        if n >= 2:
            mu = sum(somme) / n
            var = sum((x - mu) ** 2 for x in somme) / (n - 1)
            h = 1.959964 * math.sqrt(var / n)
            r.update(punteggio=round(100 * mu, 2), ic_basso=round(100 * (mu - h), 2), ic_alto=round(100 * (mu + h), 2))
            if finito:
                r["esito"] = "meglio" if 100 * mu > 50 + R10000 else ("peggio" if 100 * mu < 50 - R10000 else "pari")
        rows.append(r)
    ora = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    meta = [
        {"label": "Aggiornato", "value": ora},
        {"label": "Avversario", "value": f"Massima {MASSIMA}, 5000 mazzi in duplicato = 10.000 partite, seme {SEME}"},
        {"label": "Range R10000 (fase 1)", "value": f"±{R10000}%"},
        {"label": "stato.py", "value": open(__file__).read(), "type": "code", "lang": "python"},
    ]
    json.dump({"rows": rows, "meta": meta}, sys.stdout, ensure_ascii=False, indent=1)

main()
