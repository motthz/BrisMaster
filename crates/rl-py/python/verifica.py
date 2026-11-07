"""Controlla che l'inferenza Rust (giocatore `rl:`) dia gli stessi numeri di PyTorch."""
import os, sys, tempfile
import numpy as np, torch
sys.path.insert(0, os.path.dirname(__file__))
import briscola_rl as B
from allena import Rete, esporta

torch.manual_seed(1)
net = Rete(int(sys.argv[1]) if len(sys.argv) > 1 else 512)
for p in net.parameters():
    p.data.normal_(0, 0.05)
path = os.path.join(tempfile.mkdtemp(), "r.brl")
esporta(net, path)
env = B.VecEnv(256, 3)
env.reset()
worst = 0.0
for t in range(40):
    xb, mb, _ = env.obs()
    x = np.frombuffer(xb, np.uint8).reshape(256, B.RL_IN)
    m = np.frombuffer(mb, np.uint8).reshape(256, 40)
    with torch.no_grad():
        lg, v = net(torch.from_numpy(x.copy()))
    r = np.frombuffer(B.rust_forward(open(path, "rb").read(), xb), "<f4").reshape(256, 41)
    worst = max(worst, np.abs(r[:, :40] - lg.numpy()).max(), np.abs(r[:, 40] - v.numpy()).max())
    a = np.array([np.random.choice(np.flatnonzero(row)) for row in m], np.uint8)
    env.step(a.tobytes())
print("differenza massima Rust/PyTorch:", worst)
assert worst < 1e-4
