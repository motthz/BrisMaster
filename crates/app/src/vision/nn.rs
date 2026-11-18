//! Una piccola rete neurale che riconosce seme e valore di una carta raddrizzata, allenata solo sui
//! mazzi di riferimento dell'app (con colori, tratti, sfocature e coperture cambiati a caso) per
//! riconoscere anche i mazzi mai visti: il disegno dei semi e quanti sono, non il disegno esatto di
//! un mazzo. I pesi stanno in `assets/carte_rete.bin` (numeri a 32 bit; per riallenarla vedi `scripts/rete-carte/allena.py`).

use super::{Img, CARD_H, CARD_W};
use std::sync::OnceLock;

/// La carta in ingresso alla rete: larghezza e altezza.
const W: usize = 40;
const H: usize = 64;

struct Conv {
    cin: usize,
    cout: usize,
    w: Vec<f32>,
    b: Vec<f32>,
}

struct Dense {
    nin: usize,
    nout: usize,
    w: Vec<f32>,
    b: Vec<f32>,
}

struct Net {
    convs: [Conv; 4],
    fc: Dense,
    suit: Dense,
    rank: Dense,
}

fn net() -> Option<&'static Net> {
    static NET: OnceLock<Option<Net>> = OnceLock::new();
    NET.get_or_init(|| {
        let bytes: &[u8] = include_bytes!("../../assets/carte_rete.bin");
        let vals: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        let mut at = 0;
        let mut take = |n: usize| -> Option<Vec<f32>> {
            let v = vals.get(at..at + n)?.to_vec();
            at += n;
            Some(v)
        };
        let mut conv = |cin: usize, cout: usize| -> Option<Conv> { Some(Conv { cin, cout, w: take(cout * cin * 9)?, b: take(cout)? }) };
        let convs = [conv(3, 16)?, conv(16, 32)?, conv(32, 48)?, conv(48, 64)?];
        let mut dense = |nin: usize, nout: usize| -> Option<Dense> { Some(Dense { nin, nout, w: take(nout * nin)?, b: take(nout)? }) };
        let fc = dense(64 * 4 * 2, 96)?;
        let suit = dense(96, 4)?;
        let rank = dense(96, 10)?;
        Some(Net { convs, fc, suit, rank })
    })
    .as_ref()
}

/// Rimpicciolisce di un fattore qualsiasi facendo la media sull'area (come il filtro "box").
fn shrink(img: &Img) -> Vec<[f32; 3]> {
    let (sx, sy) = (CARD_W as f32 / W as f32, CARD_H as f32 / H as f32);
    let weights = |o: usize, s: f32, n: usize| -> Vec<(usize, f32)> {
        let (a, b) = (o as f32 * s, (o + 1) as f32 * s);
        (a.floor() as usize..(b.ceil() as usize).min(n))
            .map(|i| (i, ((i + 1) as f32).min(b) - (i as f32).max(a)))
            .filter(|w| w.1 > 0.0)
            .collect()
    };
    let mut out = vec![[0.0f32; 3]; W * H];
    for y in 0..H {
        let wy = weights(y, sy, CARD_H);
        for x in 0..W {
            let wx = weights(x, sx, CARD_W);
            let (mut acc, mut tot) = ([0.0f32; 3], 0.0f32);
            for &(j, fy) in &wy {
                for &(i, fx) in &wx {
                    let p = img.at(i, j);
                    let f = fx * fy;
                    for c in 0..3 {
                        acc[c] += p[c] as f32 * f;
                    }
                    tot += f;
                }
            }
            out[y * W + x] = acc.map(|v| v / tot / 255.0);
        }
    }
    out
}

/// Convoluzione 3x3 con bordo di zeri, poi ReLU.
fn conv(l: &Conv, x: &[f32], h: usize, w: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; l.cout * h * w];
    for o in 0..l.cout {
        let plane = &mut out[o * h * w..(o + 1) * h * w];
        plane.iter_mut().for_each(|v| *v = l.b[o]);
        for i in 0..l.cin {
            let k = &l.w[(o * l.cin + i) * 9..(o * l.cin + i) * 9 + 9];
            let src = &x[i * h * w..(i + 1) * h * w];
            for ky in 0..3 {
                for kx in 0..3 {
                    let wv = k[ky * 3 + kx];
                    if wv == 0.0 {
                        continue;
                    }
                    for y in 0..h {
                        let sy = y as isize + ky as isize - 1;
                        if sy < 0 || sy >= h as isize {
                            continue;
                        }
                        let row = &src[sy as usize * w..sy as usize * w + w];
                        let dst = &mut plane[y * w..y * w + w];
                        let (x0, x1) = (if kx == 0 { 1 } else { 0 }, if kx == 2 { w - 1 } else { w });
                        for xx in x0..x1 {
                            dst[xx] += wv * row[xx + kx - 1];
                        }
                    }
                }
            }
        }
        plane.iter_mut().for_each(|v| *v = v.max(0.0));
    }
    out
}

fn pool(x: &[f32], c: usize, h: usize, w: usize) -> Vec<f32> {
    let (h2, w2) = (h / 2, w / 2);
    let mut out = vec![0.0f32; c * h2 * w2];
    for k in 0..c {
        for y in 0..h2 {
            for xx in 0..w2 {
                let p = |dy: usize, dx: usize| x[k * h * w + (2 * y + dy) * w + 2 * xx + dx];
                out[k * h2 * w2 + y * w2 + xx] = p(0, 0).max(p(0, 1)).max(p(1, 0)).max(p(1, 1));
            }
        }
    }
    out
}

fn dense(l: &Dense, x: &[f32], relu: bool) -> Vec<f32> {
    (0..l.nout)
        .map(|o| {
            let v = l.b[o] + l.w[o * l.nin..(o + 1) * l.nin].iter().zip(x).map(|(a, b)| a * b).sum::<f32>();
            if relu { v.max(0.0) } else { v }
        })
        .collect()
}

fn log_softmax(v: &[f32]) -> Vec<f32> {
    let m = v.iter().copied().fold(f32::MIN, f32::max);
    let s = v.iter().map(|x| (x - m).exp()).sum::<f32>().ln() + m;
    v.iter().map(|x| x - s).collect()
}

/// Per ogni carta, il logaritmo della probabilità che la rete le dà (None se la rete non c'è).
pub fn log_probs(card: &Img) -> Option<[f32; 40]> {
    let net = net()?;
    if card.w != CARD_W || card.h != CARD_H {
        return None;
    }
    // Ingresso: per canale, meno la media del canale e diviso la deviazione di tutto.
    let px = shrink(card);
    let n = (W * H) as f32;
    let mean: [f32; 3] = std::array::from_fn(|c| px.iter().map(|p| p[c]).sum::<f32>() / n);
    let all = 3.0 * n;
    let avg = px.iter().flat_map(|p| p.iter()).sum::<f32>() / all;
    let var = px.iter().flat_map(|p| p.iter()).map(|v| (v - avg) * (v - avg)).sum::<f32>() / (all - 1.0);
    let sd = var.sqrt() + 1e-3;
    let mut x = vec![0.0f32; 3 * W * H];
    for (i, p) in px.iter().enumerate() {
        for c in 0..3 {
            x[c * W * H + i] = (p[c] - mean[c]) / sd;
        }
    }
    let (mut h, mut w) = (H, W);
    for (k, l) in net.convs.iter().enumerate() {
        x = conv(l, &x, h, w);
        if k < 3 {
            x = pool(&x, l.cout, h, w);
            h /= 2;
            w /= 2;
        }
    }
    // Media su 4 x 2 zone (come la media adattiva: zone che si sovrappongono se non divisibili).
    let c = net.convs[3].cout;
    let zone = |i: usize, n: usize, out: usize| (i * n / out, ((i + 1) * n).div_ceil(out));
    let mut f = Vec::with_capacity(c * 8);
    for k in 0..c {
        for zy in 0..4 {
            for zx in 0..2 {
                let ((y0, y1), (x0, x1)) = (zone(zy, h, 4), zone(zx, w, 2));
                let mut s = 0.0;
                for y in y0..y1 {
                    for xx in x0..x1 {
                        s += x[k * h * w + y * w + xx];
                    }
                }
                f.push(s / ((y1 - y0) * (x1 - x0)) as f32);
            }
        }
    }
    let hidden = dense(&net.fc, &f, true);
    let ls = log_softmax(&dense(&net.suit, &hidden, false));
    let lr = log_softmax(&dense(&net.rank, &hidden, false));
    Some(std::array::from_fn(|c| ls[c / 10] + lr[c % 10]))
}
