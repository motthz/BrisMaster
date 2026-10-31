//! Allena le reti sui dati di `selfplay`: valutazione (file `.bin`) o politica (`--kind policy`, file `.pol`).
//!
//! Uso:
//!   train --data CARTELLA[,CARTELLA...] --out FILE [--kind value|policy] [--epochs N] [--lr X] [--batch N] [--seed X]
//!         [--init FILE | --hidden 96x32]
//!
//! L'ultimo blocco di ogni cartella resta fuori dall'allenamento e serve da controllo:
//! sono partite intere mai viste, così la perdita di controllo non è falsata da posizioni
//! della stessa partita.

use briscola_core::net::*;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Sample {
    pos: Position,
    /// Valutazione: esito (0, ½, 1) e quota dei punti finali (0..1) per chi muove.
    /// Politica: `win` è l'indice della carta giocata.
    win: f32,
    pts: f32,
}

fn load(dirs: &str, policy: bool) -> (Vec<Sample>, Vec<Sample>) {
    let (ext, record) = if policy { ("pol", Position::BYTES + 1) } else { ("bin", Position::BYTES + 2) };
    let (mut train, mut val) = (Vec::new(), Vec::new());
    for dir in dirs.split(',') {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .expect("cartella dei dati")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == ext))
            .collect();
        files.sort();
        let last = files.len().saturating_sub(1);
        for (i, f) in files.iter().enumerate() {
            let b = std::fs::read(f).unwrap();
            let target = if i == last && files.len() > 1 { &mut val } else { &mut train };
            for r in b.chunks_exact(record) {
                target.push(if policy {
                    Sample { pos: Position::read(r), win: r[Position::BYTES] as f32, pts: 0.0 }
                } else {
                    Sample {
                        pos: Position::read(r),
                        win: r[Position::BYTES] as f32 / 2.0,
                        pts: r[Position::BYTES + 1] as f32 / 120.0,
                    }
                });
            }
        }
    }
    (train, val)
}

/// Gradienti con la stessa forma della rete.
fn zeros_like(n: &Net) -> Net {
    Net {
        n_out: n.n_out,
        h1: n.h1,
        h2: n.h2,
        w1: vec![0.0; n.w1.len()],
        b1: vec![0.0; n.b1.len()],
        w2: vec![0.0; n.w2.len()],
        b2: vec![0.0; n.b2.len()],
        w3: vec![0.0; n.w3.len()],
        b3: vec![0.0; n.b3.len()],
    }
}

fn params(n: &mut Net) -> [&mut Vec<f32>; 6] {
    [&mut n.w1, &mut n.b1, &mut n.w2, &mut n.b2, &mut n.w3, &mut n.b3]
}

fn bce(y: f32, t: f32) -> f32 {
    let y = y.clamp(1e-6, 1.0 - 1e-6);
    -(t * y.ln() + (1.0 - t) * (1.0 - y).ln())
}

/// Carte in mano a chi muove (indici con i semi rinumerati).
fn hand_of(p: &Position) -> impl Iterator<Item = usize> + '_ {
    p.loc.iter().enumerate().filter(|(_, &l)| l == 0).map(|(i, _)| i)
}

/// Politica: probabilità (softmax sulle carte in mano) e perdita di entropia incrociata.
fn policy_out(f: &Forward, s: &Sample) -> ([f32; POLICY_OUT], f32) {
    let mut p = [0f32; POLICY_OUT];
    let max = hand_of(&s.pos).map(|i| f.z[i]).fold(f32::MIN, f32::max);
    let mut sum = 0.0;
    for i in hand_of(&s.pos) {
        p[i] = (f.z[i] - max).exp();
        sum += p[i];
    }
    for i in hand_of(&s.pos) {
        p[i] /= sum;
    }
    let loss = -p[s.win as usize].max(1e-7).ln();
    (p, loss)
}

/// Accumula in `g` il gradiente della perdita su un esempio; restituisce le due perdite.
fn backprop(net: &Net, s: &Sample, g: &mut Net) -> (f32, f32) {
    let f = net.forward(&s.pos);
    let k = net.n_out;
    let mut dz = [0f32; POLICY_OUT];
    let losses = if k == POLICY_OUT {
        let (p, loss) = policy_out(&f, s);
        dz = p;
        dz[s.win as usize] -= 1.0;
        (loss, 0.0)
    } else {
        let yw = sigmoid(f.z[0]);
        let yp = sigmoid(f.z[1]);
        dz[0] = yw - s.win;
        dz[1] = yp - s.pts;
        (bce(yw, s.win), bce(yp, s.pts))
    };
    let (n1, n2) = (net.h1, net.h2);
    let mut dh2 = [0f32; MAX_H2];
    for i in 0..n2 {
        let mut acc = 0.0;
        for j in 0..k {
            g.w3[i * k + j] += f.h2[i] * dz[j];
            acc += net.w3[i * k + j] * dz[j];
        }
        if f.h2[i] > 0.0 {
            dh2[i] = acc;
        }
    }
    for j in 0..k {
        g.b3[j] += dz[j];
    }
    let mut dh1 = [0f32; MAX_H1];
    for i in 0..n1 {
        if f.h1[i] > 0.0 {
            let row = &net.w2[i * n2..(i + 1) * n2];
            let grow = &mut g.w2[i * n2..(i + 1) * n2];
            let mut acc = 0.0;
            for j in 0..n2 {
                grow[j] += f.h1[i] * dh2[j];
                acc += row[j] * dh2[j];
            }
            dh1[i] = acc;
        }
    }
    for j in 0..n2 {
        g.b2[j] += dh2[j];
    }
    let mut idx = [0u16; 42];
    let (dense, n) = s.pos.active(&mut idx);
    for &i in &idx[..n] {
        let row = &mut g.w1[i as usize * n1..(i as usize + 1) * n1];
        for j in 0..n1 {
            row[j] += dh1[j];
        }
    }
    let base = N_IN - 2;
    for (k, &x) in dense.iter().enumerate() {
        let row = &mut g.w1[(base + k) * n1..(base + k + 1) * n1];
        for j in 0..n1 {
            row[j] += dh1[j] * x;
        }
    }
    for j in 0..n1 {
        g.b1[j] += dh1[j];
    }
    losses
}

/// Perdite medie, errore quadratico sulla vittoria e accuratezza (esito previsto giusto, pareggi esclusi).
/// Per la politica: perdita, 0, 0 e quota di mosse indovinate.
fn validate(net: &Net, val: &[Sample]) -> (f64, f64, f64, f64) {
    let r: Vec<(f64, f64, f64, f64, f64)> = val
        .par_iter()
        .map(|s| {
            let f = net.forward(&s.pos);
            if net.n_out == POLICY_OUT {
                let (p, loss) = policy_out(&f, s);
                let best = hand_of(&s.pos).max_by(|&a, &b| p[a].partial_cmp(&p[b]).unwrap()).unwrap();
                return (loss as f64, 0.0, 0.0, (best == s.win as usize) as u8 as f64, 1.0);
            }
            let (yw, yp) = (sigmoid(f.z[0]), sigmoid(f.z[1]));
            let decisive = (s.win != 0.5) as u8 as f64;
            let right = if (yw > 0.5) == (s.win > 0.5) { decisive } else { 0.0 };
            (bce(yw, s.win) as f64, bce(yp, s.pts) as f64, ((yw - s.win) as f64).powi(2), right, decisive)
        })
        .collect();
    let n = r.len() as f64;
    let sum = |k: usize| r.iter().map(|t| [t.0, t.1, t.2, t.3, t.4][k]).sum::<f64>();
    (sum(0) / n, sum(1) / n, sum(2) / n, sum(3) / sum(4))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str, d: &str| -> String {
        args.iter().position(|a| a == k).map(|i| args[i + 1].clone()).unwrap_or(d.to_string())
    };
    let data = get("--data", "");
    let out = get("--out", "");
    assert!(!data.is_empty() && !out.is_empty(), "servono --data e --out");
    let epochs: usize = get("--epochs", "10").parse().unwrap();
    let lr0: f32 = get("--lr", "0.001").parse().unwrap();
    let batch: usize = get("--batch", "512").parse().unwrap();
    let seed: u64 = get("--seed", "1").parse().unwrap();
    let wd: f32 = get("--wd", "0.0001").parse().unwrap();

    let policy = get("--kind", "value") == "policy";
    let hidden: (usize, usize) = {
        let h = get("--hidden", &format!("{H1}x{H2}"));
        let (a, b) = h.split_once('x').expect("--hidden LARGO1xLARGO2");
        (a.parse().unwrap(), b.parse().unwrap())
    };
    let (mut train, val) = load(&data, policy);
    eprintln!("{} posizioni di allenamento, {} di controllo", train.len(), val.len());
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut net = match args.iter().position(|a| a == "--init") {
        Some(i) => Net::from_bytes(&std::fs::read(&args[i + 1]).unwrap()).unwrap(),
        None => Net::random_sized(seed, if policy { POLICY_OUT } else { VALUE_OUT }, hidden.0, hidden.1),
    };
    let (lw, lp, mse, acc) = validate(&net, &val);
    eprintln!("inizio: perdita vittoria {lw:.4}, punti {lp:.4}, errore² {mse:.4}, esiti giusti {:.1}%", acc * 100.0);

    let mut m = zeros_like(&net);
    let mut v = zeros_like(&net);
    let (b1, b2, eps) = (0.9f32, 0.999f32, 1e-8f32);
    let mut step = 0i32;
    let mut best = f64::INFINITY;
    let total_steps = (epochs * train.len() / batch).max(1) as f32;
    let perms: [[u8; 3]; 6] = [[1, 2, 3], [1, 3, 2], [2, 1, 3], [2, 3, 1], [3, 1, 2], [3, 2, 1]];
    for epoch in 0..epochs {
        train.shuffle(&mut rng);
        // Aumento dei dati: ogni esempio con una permutazione a caso dei semi non di briscola.
        for s in train.iter_mut() {
            let perm = perms[rng.gen_range(0..6)];
            s.pos = s.pos.permute_suits(perm);
            if policy {
                // Anche la carta giocata cambia seme.
                let (su, r) = (s.win as usize / 10, s.win as usize % 10);
                if su > 0 {
                    s.win = (perm[su - 1] as usize * 10 + r) as f32;
                }
            }
        }
        let mut seen = 0.0f64;
        let mut loss = 0.0f64;
        for b in train.chunks(batch) {
            let parts: Vec<(Net, f64)> = b
                .par_chunks(batch.div_ceil(rayon::current_num_threads()).max(1))
                .map(|part| {
                    let mut g = zeros_like(&net);
                    let mut l = 0.0f64;
                    for s in part {
                        let (a, c) = backprop(&net, s, &mut g);
                        l += (a + c) as f64;
                    }
                    (g, l)
                })
                .collect();
            let mut grad = zeros_like(&net);
            for (g, l) in &parts {
                loss += l;
                for (dst, src) in params(&mut grad).into_iter().zip([&g.w1, &g.b1, &g.w2, &g.b2, &g.w3, &g.b3]) {
                    for (d, s) in dst.iter_mut().zip(src.iter()) {
                        *d += s;
                    }
                }
            }
            seen += b.len() as f64;
            step += 1;
            // Adam con decadimento dei pesi separato e tasso che scende a coseno.
            let lr = lr0 * 0.5 * (1.0 + (std::f32::consts::PI * step as f32 / total_steps).cos()).max(0.02);
            let scale = 1.0 / b.len() as f32;
            let (c1, c2) = (1.0 - b1.powi(step), 1.0 - b2.powi(step));
            for ((p, g), (mm, vv)) in params(&mut net)
                .into_iter()
                .zip(params(&mut grad))
                .zip(params(&mut m).into_iter().zip(params(&mut v)))
            {
                for k in 0..p.len() {
                    let gk = g[k] * scale;
                    mm[k] = b1 * mm[k] + (1.0 - b1) * gk;
                    vv[k] = b2 * vv[k] + (1.0 - b2) * gk * gk;
                    p[k] -= lr * ((mm[k] / c1) / ((vv[k] / c2).sqrt() + eps) + wd * p[k]);
                }
            }
        }
        let (lw, lp, mse, acc) = validate(&net, &val);
        let mark = if lw + lp < best {
            best = lw + lp;
            std::fs::write(&out, net.to_bytes()).unwrap();
            " (salvata)"
        } else {
            ""
        };
        eprintln!(
            "epoca {}: allenamento {:.4} | controllo: perdita {lw:.4}, punti {lp:.4}, errore² {mse:.4}, giusti {:.1}%{mark}",
            epoch + 1,
            loss / seen,
            acc * 100.0
        );
    }
}
