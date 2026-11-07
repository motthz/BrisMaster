"""Allenamento in self-play della rete di briscola (PPO con lega di versioni passate).

Uso:
    python allena.py --dir CARTELLA [--minutes 110] [--games 2048] [--hidden 512]

Riprende da CARTELLA/stato.pt se c'è. Ogni giro: `--games` partite intere giocate in parallelo
(tutte durano 40 mosse, 20 per giocatore). In una parte delle partite (`--league`) un posto è
occupato da una versione passata congelata e si allena solo l'altro. Esporta la rete in formato
Rust (BRL1) in CARTELLA/rete.brl e una copia per ogni istantanea in CARTELLA/storia/.
"""

import argparse
import json
import os
import random
import struct
import time

import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

import briscola_rl as B

RL_IN, RL_OUT = B.RL_IN, B.RL_OUT
NC = 40
SCALE = torch.tensor(B.SCALE, dtype=torch.float32)
DEV = torch.device("cpu")


class Rete(nn.Module):
    def __init__(self, hidden):
        super().__init__()
        self.l1 = nn.Linear(RL_IN, hidden)
        self.l2 = nn.Linear(hidden, hidden)
        self.l3 = nn.Linear(hidden, hidden // 2)
        self.out = nn.Linear(hidden // 2, RL_OUT)
        self.register_buffer("scale", SCALE.clone(), persistent=False)

    def forward(self, x_u8):
        x = x_u8.float() * self.scale
        h = F.relu(self.l1(x))
        h = F.relu(self.l2(h))
        h = F.relu(self.l3(h))
        z = self.out(h)
        return z[:, :NC], torch.tanh(z[:, NC])


def esporta(net, path):
    layers = [net.l1, net.l2, net.l3, net.out]
    b = bytearray(b"BRL1") + struct.pack("<I", len(layers))
    for l in layers:
        w = l.weight.detach().cpu().t().contiguous().numpy().astype("<f4")  # [in][out]
        b += struct.pack("<II", w.shape[0], w.shape[1])
        b += w.tobytes() + l.bias.detach().cpu().numpy().astype("<f4").tobytes()
    tmp = path + ".tmp"
    with open(tmp, "wb") as f:
        f.write(b)
    os.replace(tmp, path)


def scegli(logits, mask, greedy=False):
    logits = logits.masked_fill(mask == 0, -1e9)
    if greedy:
        a = logits.argmax(1)
    else:
        a = torch.distributions.Categorical(logits=logits).sample()
    logp = F.log_softmax(logits, 1).gather(1, a[:, None])[:, 0]
    return a, logp


def giro(env, net, rivali, args):
    """Gioca un giro di partite. Restituisce i dati per l'allenamento (solo mosse della rete)."""
    n = args.games
    env.reset()
    # Partite di lega: posto occupato da una versione passata (-1 = nessuna).
    rival_seat = np.full(n, -1)
    rival_id = np.zeros(n, dtype=np.int64)
    if rivali:
        k = int(n * args.league)
        rival_seat[:k] = np.random.randint(0, 2, k)
        rival_id[:k] = np.random.randint(0, len(rivali), k)
    X = np.zeros((40, n, RL_IN), np.uint8)
    M = np.zeros((40, n, NC), np.uint8)
    P = np.zeros((40, n), np.uint8)
    A = np.zeros((40, n), np.int64)
    LP = np.zeros((40, n), np.float32)
    V = np.zeros((40, n), np.float32)
    mine = np.zeros((40, n), bool)
    with torch.no_grad():
        for t in range(40):
            xb, mb, pb = env.obs()
            x = np.frombuffer(xb, np.uint8).reshape(n, RL_IN)
            m = np.frombuffer(mb, np.uint8).reshape(n, NC)
            p = np.frombuffer(pb, np.uint8)
            learner = rival_seat != p
            act = np.zeros(n, np.int64)
            xt, mt = torch.from_numpy(x.copy()).to(DEV), torch.from_numpy(m.copy()).to(DEV)
            idx_t = torch.from_numpy(np.flatnonzero(learner)).to(DEV)
            idx = np.flatnonzero(learner)
            lg, v = net(xt[idx_t])
            a, lp = scegli(lg, mt[idx_t])
            act[idx] = a.cpu().numpy()
            LP[t, idx] = lp.cpu().numpy()
            V[t, idx] = v.cpu().numpy()
            for r in range(len(rivali)):
                ridx = np.flatnonzero(~learner & (rival_id == r))
                if len(ridx):
                    rt = torch.from_numpy(ridx).to(DEV)
                    lg, _ = rivali[r](xt[rt])
                    a, _ = scegli(lg, mt[rt])
                    act[ridx] = a.cpu().numpy()
            X[t], M[t], P[t], A[t], mine[t] = x, m, p, act, learner
            env.step(act.astype(np.uint8).tobytes())
    pts = np.frombuffer(env.points(), np.uint8).reshape(n, 2).astype(np.float32)
    # Esito per ciascun giocatore: +1 vittoria, 0 pareggio, -1 sconfitta, più un po' di punti.
    diff = pts[:, 0] - pts[:, 1]
    res0 = np.sign(diff) + args.shaping * diff / 120.0
    res = np.stack([res0, -res0], 1)  # [n, 2]
    # Vantaggi (GAE, gamma 1) sulla sequenza delle 20 mosse di ciascun giocatore.
    adv = np.zeros((40, n), np.float32)
    ret = np.zeros((40, n), np.float32)
    for pl in (0, 1):
        sel = P == pl  # [40, n], 20 vere per colonna
        order = np.argsort(~sel, axis=0, kind="stable")[:20]  # passi del giocatore, in ordine
        cols = np.arange(n)[None, :]
        v = V[order, cols]  # [20, n]
        last = np.zeros(n, np.float32)
        nxt_v = res[:, pl].astype(np.float32)
        for k in range(19, -1, -1):
            delta = nxt_v - v[k]
            last = delta + args.lam * last
            adv[order[k], np.arange(n)] = last
            ret[order[k], np.arange(n)] = last + v[k]
            nxt_v = v[k]
    keep = mine & (M.sum(2) > 1)  # mosse della rete con più di una scelta
    d = dict(x=X[keep], m=M[keep], a=A[keep], lp=LP[keep], adv=adv[keep], ret=ret[keep])
    # Statistica: risultato della rete contro le versioni passate.
    lw = rival_seat >= 0
    vs = None
    if lw.any():
        seat = 1 - rival_seat[lw]
        vs = float(((res[lw, :][np.arange(lw.sum()), seat] > 0) + 0.5 * (diff[lw] == 0)).mean())
    return d, vs


def aggiorna(net, opt, d, args):
    n = len(d["a"])
    x = torch.from_numpy(d["x"])
    m = torch.from_numpy(d["m"])
    a = torch.from_numpy(d["a"])
    lp0 = torch.from_numpy(d["lp"])
    adv = torch.from_numpy(d["adv"])
    adv = (adv - adv.mean()) / (adv.std() + 1e-8)
    ret = torch.from_numpy(d["ret"])
    stats = []
    for _ in range(args.epochs):
        perm = torch.randperm(n)
        for s in range(0, n, args.batch):
            i = perm[s : s + args.batch]
            xi, mi, ai = x[i].to(DEV), m[i].to(DEV), a[i].to(DEV)
            advi, lp0i, reti = adv[i].to(DEV), lp0[i].to(DEV), ret[i].to(DEV)
            lg, v = net(xi)
            lg = lg.masked_fill(mi == 0, -1e9)
            logp_all = F.log_softmax(lg, 1)
            lp = logp_all.gather(1, ai[:, None])[:, 0]
            ratio = torch.exp(lp - lp0i)
            pg = -torch.min(ratio * advi, ratio.clamp(1 - args.clip, 1 + args.clip) * advi).mean()
            vl = F.mse_loss(v, reti)
            prob = logp_all.exp() * (mi > 0)
            ent = -(prob * logp_all.clamp(min=-30) * (mi > 0)).sum(1).mean()
            loss = pg + args.vcoef * vl - args.ent * ent
            opt.zero_grad()
            loss.backward()
            nn.utils.clip_grad_norm_(net.parameters(), 0.5)
            opt.step()
            stats.append((pg.item(), vl.item(), ent.item()))
    return np.mean(stats, 0)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", required=True)
    ap.add_argument("--minutes", type=float, default=110)
    ap.add_argument("--games", type=int, default=2048)
    ap.add_argument("--hidden", type=int, default=512)
    ap.add_argument("--lr", type=float, default=3e-4)
    ap.add_argument("--epochs", type=int, default=3)
    ap.add_argument("--batch", type=int, default=4096)
    ap.add_argument("--clip", type=float, default=0.2)
    ap.add_argument("--vcoef", type=float, default=0.5)
    ap.add_argument("--ent", type=float, default=0.01)
    ap.add_argument("--lam", type=float, default=0.95)
    ap.add_argument("--shaping", type=float, default=0.1)
    ap.add_argument("--league", type=float, default=0.2)
    ap.add_argument("--snap", type=int, default=25, help="giri tra due istantanee per la lega")
    ap.add_argument("--pool", type=int, default=20)
    ap.add_argument("--seed", type=int, default=20261105)
    ap.add_argument("--device", default="auto", help="auto | cpu | cuda")
    args = ap.parse_args()
    global DEV
    dev = args.device
    if dev == "auto":
        dev = "cuda" if torch.cuda.is_available() else "cpu"
    DEV = torch.device(dev)
    print("dispositivo:", DEV, flush=True)
    torch.set_num_threads(os.cpu_count())
    os.makedirs(os.path.join(args.dir, "storia"), exist_ok=True)
    stato = os.path.join(args.dir, "stato.pt")
    net = Rete(args.hidden).to(DEV)
    opt = torch.optim.Adam(net.parameters(), lr=args.lr)
    it, games, pool = 0, 0, []
    if os.path.exists(stato):
        s = torch.load(stato, weights_only=False, map_location=DEV)
        net.load_state_dict(s["net"])
        opt.load_state_dict(s["opt"])
        for g in opt.param_groups:  # il passo scelto ora vale anche ripartendo
            g["lr"] = args.lr
        it, games, pool = s["it"], s["games"], s["pool"]
        print(f"ripreso dal giro {it}, {games} partite", flush=True)
    seed = args.seed + it
    random.seed(seed)
    np.random.seed(seed % 2**32)
    torch.manual_seed(seed)
    env = B.VecEnv(args.games, seed)

    def carica(sd):
        r = Rete(args.hidden)
        r.load_state_dict(sd)
        r.to(DEV)
        r.eval()
        return r

    rivali = [carica(sd) for sd in pool]
    t0 = time.time()
    log = open(os.path.join(args.dir, "log.jsonl"), "a")
    while time.time() - t0 < args.minutes * 60:
        ts = time.time()
        d, vs = giro(env, net, rivali, args)
        tr = time.time() - ts
        pg, vl, ent = aggiorna(net, opt, d, args)
        it += 1
        games += args.games
        rec = dict(it=it, games=games, pg=round(pg, 4), vl=round(vl, 4), ent=round(ent, 4),
                   vs_lega=None if vs is None else round(vs, 4), t_gioco=round(tr, 1),
                   t_tot=round(time.time() - ts, 1))
        print(json.dumps(rec), flush=True)
        log.write(json.dumps(rec) + "\n")
        log.flush()
        if it % args.snap == 0:
            sd = {k: v.clone() for k, v in net.state_dict().items()}
            pool.append(sd)
            pool = pool[-args.pool :]
            rivali = [carica(s) for s in pool]
            esporta(net, os.path.join(args.dir, "storia", f"rete_{it:06d}.brl"))
        if it % 5 == 0:
            esporta(net, os.path.join(args.dir, "rete.brl"))
            torch.save(dict(net=net.state_dict(), opt=opt.state_dict(), it=it, games=games, pool=pool,
                            args=vars(args)), stato + ".tmp")
            os.replace(stato + ".tmp", stato)
    esporta(net, os.path.join(args.dir, "rete.brl"))
    torch.save(dict(net=net.state_dict(), opt=opt.state_dict(), it=it, games=games, pool=pool,
                    args=vars(args)), stato + ".tmp")
    os.replace(stato + ".tmp", stato)
    print("fine sessione", it, games, flush=True)


if __name__ == "__main__":
    main()
