"""Allena la piccola rete che riconosce seme e valore delle carte (vedi crates/app/src/vision/nn.rs).

Si allena SOLO sui mazzi di riferimento dell'app e sulle carte di quei mazzi come le vede l'app sui
tavoli finti; i mazzi di prova servono solo a misurare, mai ad allenare.

  1. Mazzi di riferimento (40 PNG per mazzo, 00.png..39.png, numero della carta = seme*10 + valore):
       ESPORTA=/tmp/rif cargo test --release -p brismaster esporta_riferimenti -- --ignored
  2. Le stesse carte raddrizzate dall'app su tavoli a caso (facoltativo, consigliato):
       VISTE=/tmp/viste VISTE_N=200 cargo test --release -p brismaster esporta_viste -- --ignored
  3. Allenamento (i pesi attuali: 60 epoche con le viste):
       python3 allena.py /tmp/rif/rif0,/tmp/rif/rif1,/tmp/rif/rif2 CARTELLA_MAZZI_DI_PROVA modello.pt 60 /tmp/viste
     CARTELLA_MAZZI_DI_PROVA contiene una sottocartella per mazzo, con 00.png..39.png: ogni 200 passi
     si stampa quante carte di ciascun mazzo la rete riconosce.
  4. Esportazione dei pesi nell'app:
       python3 esporta.py modello.pt ../../crates/app/assets/carte_rete.bin

Richiede torch, numpy e Pillow.
"""
import os, sys, math, random, time
import numpy as np
from PIL import Image, ImageFilter, ImageOps, ImageEnhance
import torch, torch.nn as nn, torch.nn.functional as F
torch.set_num_threads(4)
W, H = 40, 64
def load_deck(d):
    return [Image.open(f'{d}/{c:02d}.png').convert('RGB') for c in range(40)]
def to_t(im):
    im = im.resize((100, 160), Image.BOX) if im.size != (100, 160) else im
    a = np.asarray(im.resize((W, H), Image.BOX), dtype=np.float32) / 255.0
    return torch.from_numpy(a).permute(2, 0, 1)
def norm(t):
    # normalizzazione per carta: toglie luminosità e contrasto dello schermo
    m = t.mean(dim=(1, 2), keepdim=True); s = t.std() + 1e-3
    return (t - m) / s
def augment(im):
    w, h = im.size
    # ritaglio e prospettiva leggera
    dx, dy = int(w * random.uniform(-0.06, 0.06)), int(h * random.uniform(-0.05, 0.05))
    sc = random.uniform(0.92, 1.06)
    im = im.transform(im.size, Image.AFFINE, (sc, random.uniform(-0.03, 0.03), dx, random.uniform(-0.03, 0.03), sc, dy), resample=Image.BILINEAR, fillcolor=(255, 255, 255))
    im = im.rotate(random.uniform(-6, 6), resample=Image.BILINEAR, fillcolor=(255, 255, 255))
    if random.random() < 0.5:
        im = im.rotate(180)
    # colori: rotazione della tinta, saturazione, contrasto, luminosità, a volte grigio
    hsv = np.asarray(im.convert('HSV')).copy()
    hsv[..., 0] = (hsv[..., 0].astype(int) + random.randint(0, 255)) % 256 if random.random() < 0.6 else hsv[..., 0]
    im = Image.fromarray(hsv, 'HSV').convert('RGB')
    im = ImageEnhance.Color(im).enhance(random.uniform(0.3, 1.6))
    im = ImageEnhance.Contrast(im).enhance(random.uniform(0.6, 1.4))
    im = ImageEnhance.Brightness(im).enhance(random.uniform(0.75, 1.2))
    if random.random() < 0.15:
        im = ImageOps.grayscale(im).convert('RGB')
    # tratto più spesso o più sottile, sfocatura, posterizzazione (pixel art)
    r = random.random()
    if r < 0.2: im = im.filter(ImageFilter.MinFilter(3))
    elif r < 0.35: im = im.filter(ImageFilter.MaxFilter(3))
    if random.random() < 0.3: im = im.filter(ImageFilter.GaussianBlur(random.uniform(0.3, 1.5)))
    if random.random() < 0.15:
        small = im.resize((random.randint(18, 30), random.randint(28, 48)), Image.NEAREST)
        im = small.resize(im.size, Image.NEAREST)
    if random.random() < 0.15: im = ImageOps.posterize(im, random.randint(2, 4))
    t = to_t(im)
    # parti coperte
    if random.random() < 0.3:
        x0 = random.randint(0, W - 8); y0 = random.randint(0, H - 8)
        t[:, y0:y0 + random.randint(6, 20), x0:x0 + random.randint(6, 16)] = torch.rand(3, 1, 1)
    if random.random() < 0.35:
        # Le parti coperte visibili al classificatore in Rust sono neutralizzate.
        axis = random.choice((1, 2))
        length = t.shape[axis]
        hidden = random.randint(length // 8, length // 3)
        start = random.choice((0, length - hidden))
        t.narrow(axis, start, hidden).fill_(245.0 / 255.0)
    t = t + torch.randn_like(t) * random.uniform(0, 0.04)
    return t
class Net(nn.Module):
    def __init__(s):
        super().__init__()
        s.c1 = nn.Conv2d(3, 16, 3, padding=1); s.c2 = nn.Conv2d(16, 32, 3, padding=1)
        s.c3 = nn.Conv2d(32, 48, 3, padding=1); s.c4 = nn.Conv2d(48, 64, 3, padding=1)
        s.fc = nn.Linear(64 * 4 * 2 + 0, 96)
        s.suit = nn.Linear(96, 4); s.rank = nn.Linear(96, 10)
        s.drop = nn.Dropout(0.3)
    def feats(s, x):
        x = F.max_pool2d(F.relu(s.c1(x)), 2)   # 32x20
        x = F.max_pool2d(F.relu(s.c2(x)), 2)   # 16x10
        x = F.max_pool2d(F.relu(s.c3(x)), 2)   # 8x5
        x = F.relu(s.c4(x))
        x = F.adaptive_avg_pool2d(x, (4, 2))
        return s.drop(F.relu(s.fc(x.flatten(1))))
    def forward(s, x):
        f = s.feats(x)
        return s.suit(f), s.rank(f)
def logp40(model, x):
    su, ra = model(x)
    ls, lr = F.log_softmax(su, 1), F.log_softmax(ra, 1)
    return (ls[:, :, None] + lr[:, None, :]).reshape(-1, 40)
def evaluate(model, decks):
    model.eval(); res = {}
    with torch.no_grad():
        for name, cards in decks.items():
            x = torch.stack([norm(to_t(im)) for im in cards])
            p = logp40(model, x).argmax(1).numpy()
            res[name] = int((p == np.arange(40)).sum())
    model.train(); return res

def load_export(model, path):
    values = np.fromfile(path, dtype="<f4")
    offset = 0
    with torch.no_grad():
        for name in ("c1", "c2", "c3", "c4", "fc", "suit", "rank"):
            layer = getattr(model, name)
            for tensor in (layer.weight, layer.bias):
                end = offset + tensor.numel()
                tensor.copy_(torch.from_numpy(values[offset:end].copy()).reshape_as(tensor))
                offset = end
    if offset != len(values):
        raise ValueError(f"Numero di pesi inatteso in {path}")


def measure_masks(model, decks):
    masks = {
        "intera": (slice(0, 0), slice(None)),
        "alto": (slice(0, H // 4), slice(None)),
        "basso": (slice(H * 3 // 4, H), slice(None)),
        "sinistra": (slice(None), slice(0, W // 4)),
        "destra": (slice(None), slice(W * 3 // 4, W)),
    }
    model.eval()
    records = []
    with torch.no_grad():
        for deck, images in decks.items():
            full = torch.stack([to_t(img) for img in images])
            for mask, (rows, cols) in masks.items():
                batch = full.clone()
                batch[:, :, rows, cols] = 245.0 / 255.0
                suit, rank = model(torch.stack([norm(card) for card in batch]))
                for card, (su, rk) in enumerate(zip(suit.argmax(1), rank.argmax(1), strict=True)):
                    records.append((deck, mask, card, int(su), int(rk)))
    return records

if __name__ == '__main__':
    train_dirs = sys.argv[1].split(','); test_root = sys.argv[2]; out = sys.argv[3]; epochs = int(sys.argv[4])
    train_paths = {os.path.realpath(d) for d in train_dirs}
    eval_paths = {os.path.realpath(os.path.join(test_root, d)) for d in os.listdir(test_root)}
    if train_paths & eval_paths:
        raise ValueError("I mazzi di valutazione devono essere separati dall'allenamento")
    train = [(c, im) for d in train_dirs for c, im in enumerate(load_deck(d))]
    views = []
    if len(sys.argv) > 5:
        vd = sys.argv[5]
        for f in sorted(os.listdir(vd)):
            views.append((int(f[:2]), Image.open(os.path.join(vd, f)).convert('RGB')))
        print('viste', len(views), flush=True)
    tests = {d: load_deck(os.path.join(test_root, d)) for d in sorted(os.listdir(test_root))}
    random.seed(0); torch.manual_seed(0)
    model = Net()
    if warm_start := os.environ.get("BRIS_RETE_PESI_INIZIALI"):
        load_export(model, warm_start)
    opt = torch.optim.AdamW(model.parameters(), 2e-3, weight_decay=1e-3)
    steps = epochs * 40
    sched = torch.optim.lr_scheduler.OneCycleLR(opt, 3e-3, total_steps=steps)
    t0 = time.time()
    for step in range(steps):
        batch = random.choices(train, k=48) + (random.choices(views, k=48) if views else random.choices(train, k=48))
        x = torch.stack([norm(augment(im)) for _, im in batch]); y = torch.tensor([c for c, _ in batch])
        su, ra = model(x)
        loss = F.cross_entropy(su, y // 10) + F.cross_entropy(ra, y % 10)
        opt.zero_grad(); loss.backward(); opt.step(); sched.step()
        if (step + 1) % 200 == 0:
            r = evaluate(model, tests)
            print(f'passo {step+1} perdita {loss.item():.3f} {int(time.time()-t0)}s', r, 'tot', sum(r.values()), flush=True)
    torch.save(model.state_dict(), out)
    if report := os.environ.get("BRIS_RETE_RISULTATI"):
        import csv
        with open(report, "w", newline="", encoding="utf-8") as stream:
            writer = csv.writer(stream)
            writer.writerow(("mazzo", "maschera", "carta", "seme_predetto", "valore_predetto"))
            writer.writerows(measure_masks(model, tests))
