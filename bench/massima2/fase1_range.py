#!/usr/bin/env python3
"""Massima 2.0 - Fase 1: range di tolleranza.

Un "bot a testa o croce" gioca contro se stesso: ogni partita e' vinta con
probabilita' 0,5. Per ogni dimensione N (1.000, 10.000, 100.000 partite)
si giocano 500 set; per ogni set si misura la deviazione della percentuale
di vittoria dal 50%. Il range RN e' la media dei valori assoluti delle 500
deviazioni, usato come +/- RN.
"""
import random, statistics, math, json, sys

SEME = 20261117
SET = 500
random.seed(SEME)
ris = {}
for n in (1000, 10000, 100000):
    dev = []
    for _ in range(SET):
        vinte = sum(random.getrandbits(1) for _ in range(n))
        dev.append(100.0 * vinte / n - 50.0)
    mad = statistics.fmean(abs(d) for d in dev)
    teorico = 100 * 0.5 / math.sqrt(n) * math.sqrt(2 / math.pi)
    fuori = sum(abs(d) > mad for d in dev)
    ris[f"R{n}"] = dict(range=round(mad, 4), teorico=round(teorico, 4),
                        dev_min=round(min(dev), 3), dev_max=round(max(dev), 3),
                        set_fuori_range=fuori)
json.dump(dict(seme=SEME, set=SET, risultati=ris), sys.stdout, indent=2)
print()
