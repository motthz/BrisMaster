"""Scrive i pesi di un modello allenato con allena.py nel formato letto da nn.rs: numeri a 32 bit
(little endian), strato per strato (c1..c4, fc, suit, rank), prima i pesi poi i bias."""
import sys, os, torch, numpy as np
sys.path.insert(0, os.path.dirname(__file__))
from allena import Net
m = Net(); m.load_state_dict(torch.load(sys.argv[1], weights_only=True)); m.eval()
parts = []
for name in ['c1', 'c2', 'c3', 'c4', 'fc', 'suit', 'rank']:
    l = getattr(m, name)
    parts += [l.weight.detach().numpy().astype('<f4').ravel(), l.bias.detach().numpy().astype('<f4').ravel()]
v = np.concatenate(parts)
v.tofile(sys.argv[2]); print(len(v), 'pesi', os.path.getsize(sys.argv[2]), 'byte')
