#!/usr/bin/env python3
"""Massima 2.0 - fase 2: grafici delle curve di miglioramento.

Legge i risultati del simulatore (ris/NOME.txt, una riga "Punteggio ... IC 95% sui
mazzi: A% - B%") e disegna un pannello per ogni parametro, con la banda di
tolleranza R10000 della fase 1 attorno al 50%, la linea di tendenza e il punto
di saturazione (il primo valore dopo il quale il raddoppio non porta più un
guadagno sopra il range). Massima stessa è il punto a 50% (pari contro se stessa).

Uso: fase2_grafici.py CARTELLA_RIS fase2_curve.png fase2_risultati.json
"""
import json, math, os, re, sys
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

R10000 = 0.3815
MASSIMA = {"iter": 20000, "mani": 1000, "ucb": 0.7, "minimo": 0.25}

def leggi(cartella):
    out = {}
    for f in sorted(os.listdir(cartella)):
        if not f.endswith(".txt"): continue
        t = open(os.path.join(cartella, f)).read()
        m = re.search(r"Punteggio \(pareggio = ½\): ([\d.]+)%\s+\(IC 95% sui mazzi: ([\d.]+)% - ([\d.]+)%", t)
        if m: out[f[:-4]] = tuple(float(x) for x in m.groups())
    return out

def serie(ris, pref, massima):
    pts = [(float(k[len(pref):]), *v) for k, v in ris.items() if k.startswith(pref)]
    if massima is not None: pts.append((float(massima), 50.0, 50.0, 50.0))
    return sorted(pts)

def saturazione(pts):
    # primo punto da cui nessun valore successivo supera il migliore finora di più di R10000
    best = None
    for i, (x, y, _, _) in enumerate(pts):
        if best is None or y > best[1] + R10000: best = (x, y)
    return best

def pannello(ax, pts, titolo, xlabel, logx, tendenza):
    xs = [p[0] for p in pts]; ys = [p[1] for p in pts]
    ax.axhspan(50 - R10000, 50 + R10000, color="#9aa5b1", alpha=0.25, label="range R10000 (±0,38%)")
    ax.axhline(50, color="#52606d", lw=0.8)
    ax.errorbar(xs, ys, yerr=[[p[1] - p[2] for p in pts], [p[3] - p[1] for p in pts]],
                fmt="o-", color="#2563eb", capsize=3, lw=1.5, label="punteggio (IC 95%)")
    for x, y, *_ in pts:
        ax.annotate(f"{y:.2f}", (x, y), textcoords="offset points", xytext=(0, 7), ha="center", fontsize=7)
    if tendenza:
        sel = [p for p in pts if p[0] >= tendenza]
        lx = [math.log2(p[0]) for p in sel]; ly = [p[1] for p in sel]
        n = len(lx); mx = sum(lx) / n; my = sum(ly) / n
        b = sum((a - mx) * (c - my) for a, c in zip(lx, ly)) / sum((a - mx) ** 2 for a in lx)
        a0 = my - b * mx
        gx = [2 ** (min(lx) + i * (max(lx) + 1 - min(lx)) / 50) for i in range(51)]
        ax.plot(gx, [a0 + b * math.log2(g) for g in gx], "--", color="#dc2626", lw=1,
                label=f"tendenza: {b:+.2f}% per raddoppio")
    s = saturazione(pts)
    if s and s[1] <= 50 + R10000:
        ax.text(0.02, 0.04, "curva piatta: nessun valore sopra il range", transform=ax.transAxes, fontsize=7, color="#16a34a")
    elif s and s[0] == xs[-1]:
        ax.axvline(s[0], color="#16a34a", ls=":", lw=1.2, label="non ancora saturo (cresce fino all'ultimo punto)")
    elif s:
        ax.axvline(s[0], color="#16a34a", ls=":", lw=1.2, label=f"saturazione ≈ {s[0]:g}")
    if logx: ax.set_xscale("log")
    ax.set_title(titolo, fontsize=10); ax.set_xlabel(xlabel, fontsize=8); ax.set_ylabel("% contro Massima", fontsize=8)
    ax.tick_params(labelsize=7); ax.grid(alpha=0.3); ax.legend(fontsize=6.5, loc="lower right")
    return s

def main():
    ris = leggi(sys.argv[1])
    fig, axs = plt.subplots(3, 2, figsize=(12, 13))
    iters = serie(ris, "iter_", MASSIMA["iter"])
    mani = serie(ris, "mani_", MASSIMA["mani"])
    ucb = serie(ris, "ucb_", MASSIMA["ucb"])
    minimo = serie(ris, "minimo_", MASSIMA["minimo"])
    sat = {
        "iterazioni": pannello(axs[0][0], iters, "Iterazioni ISMCTS (Massima = 20.000)", "iterazioni per mossa (scala log)", True, 10000),
        "mani": pannello(axs[0][1], mani, "Mani avversarie dedotte (Massima = 1.000)", "mani campionate (scala log)", True, None),
        "ucb": pannello(axs[1][0], ucb, "Esplorazione UCB (Massima = 0,7)", "costante c", False, None),
        "minimo": pannello(axs[1][1], minimo, "Minimo delle mani (Massima = 0,25)", "peso minimo", False, None),
    }
    reti = [(k, *v) for k, v in ris.items() if k.startswith("rete_")]
    reti.sort(key=lambda r: (r[0].startswith("rete_grande"), int(re.search(r"(\d+)M", r[0]).group(1))))
    ax = axs[2][0]
    ax.axhspan(50 - R10000, 50 + R10000, color="#9aa5b1", alpha=0.25)
    ax.axhline(50, color="#52606d", lw=0.8)
    nomi = [r[0].replace("rete_grande_", "grande ").replace("rete_", "") for r in reti]
    ax.bar(nomi, [r[1] for r in reti], yerr=[[r[1] - r[2] for r in reti], [r[3] - r[1] for r in reti]],
           color=["#f59e0b" if "grande" in n else "#8b5cf6" for n in nomi], capsize=3)
    for i, r in enumerate(reti): ax.annotate(f"{r[1]:.2f}", (i, r[3]), textcoords="offset points", xytext=(0, 3), ha="center", fontsize=7)
    ax.set_ylim(40, 52); ax.set_title("Rete autoallenata: partite di allenamento (milioni)", fontsize=10)
    ax.set_ylabel("% contro Massima", fontsize=8); ax.tick_params(labelsize=7); ax.grid(alpha=0.3, axis="y")
    ax = axs[2][1]; ax.axis("off")
    righe = [f"{k:<18} {v[0]:6.2f}%  [{v[1]:.2f} – {v[2]:.2f}]" for k, v in sorted(ris.items(), key=lambda kv: -kv[1][0])]
    ax.text(0, 1, "Classifica (10.000 partite ciascuno)\n\n" + "\n".join(righe), va="top", family="monospace", fontsize=7.5)
    fig.suptitle("Massima 2.0 · fase 2 · curve di miglioramento contro Massima (seme 20261117)", fontsize=12)
    fig.tight_layout(rect=(0, 0, 1, 0.97))
    fig.savefig(sys.argv[2], dpi=130)
    json.dump({"risultati": {k: dict(punteggio=v[0], ic_basso=v[1], ic_alto=v[2]) for k, v in ris.items()},
               "saturazione": {k: (v[0] if v else None) for k, v in sat.items()}, "range_R10000": R10000},
              open(sys.argv[3], "w"), ensure_ascii=False, indent=1)

main()
