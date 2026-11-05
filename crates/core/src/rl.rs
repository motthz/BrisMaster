//! Rete auto-allenata (self-play con rinforzo): codifica di ciò che vede il giocatore,
//! storia compresa, e inferenza della rete (politica + valore).
//!
//! La stessa codifica serve all'ambiente di allenamento (crate `briscola-rl`, per Python)
//! e al giocatore `rl:` del banco di prova, così la rete vede in gara esattamente gli
//! ingressi visti in allenamento.

use crate::cards::*;
use crate::game::Observation;
use crate::net::canon_suit;

/// Posti di una carta dal punto di vista di chi muove.
pub const RL_LOCS: usize = 7;
const L_MINE: usize = 0;
const L_TABLE: usize = 1;
const L_PLAYED_ME: usize = 2;
const L_PLAYED_OPP: usize = 3;
const L_FACEUP: usize = 4;
const L_OPP_KNOWN: usize = 5;
const L_UNKNOWN: usize = 6;

/// Una presa chiusa nella storia: chi apre (io?), carta di apertura, carta di risposta.
pub const RL_TRICK: usize = 1 + 2 * NUM_CARDS;
pub const RL_TRICKS: usize = 20;
const I_HIST: usize = NUM_CARDS * RL_LOCS;
const I_SCAL: usize = I_HIST + RL_TRICKS * RL_TRICK;
/// Ingressi: posti delle carte, storia delle prese, 5 valori (mazzo, punti miei, punti suoi,
/// rispondo?, numero della presa).
pub const RL_IN: usize = I_SCAL + 5;
/// Uscite: 40 logit di politica (carte con i semi rinumerati) + valore.
pub const RL_OUT: usize = NUM_CARDS + 1;

/// Scala degli ingressi: la codifica è in byte, l'ingresso della rete è byte × scala.
pub fn rl_scale() -> [f32; RL_IN] {
    let mut s = [1.0f32; RL_IN];
    s[I_SCAL] = 1.0 / 40.0;
    s[I_SCAL + 1] = 1.0 / 120.0;
    s[I_SCAL + 2] = 1.0 / 120.0;
    s[I_SCAL + 4] = 1.0 / 20.0;
    s
}

#[inline]
fn canon(c: Card, trump: u8) -> usize {
    (canon_suit(suit(c), trump) * 10 + rank(c)) as usize
}

/// Carta vera dall'indice con i semi rinumerati.
#[inline]
pub fn uncanon(i: usize, trump: u8) -> Card {
    let cs = (i / 10) as u8;
    let s = if cs == 0 {
        trump
    } else if cs <= trump {
        cs - 1
    } else {
        cs
    };
    make(s, (i % 10) as u8)
}

#[inline]
pub fn canon_card(c: Card, trump: u8) -> usize {
    canon(c, trump)
}

/// Codifica ciò che vede `obs.me` (che deve muovere), con la storia pubblica `hist`:
/// tutte le carte giocate finora, in ordine, con chi le ha giocate.
pub fn encode(obs: &Observation, hist: &[(u8, Card)], out: &mut [u8]) {
    debug_assert_eq!(out.len(), RL_IN);
    out.fill(0);
    let t = obs.trump;
    let me = obs.me;
    let mut loc = [L_UNKNOWN; NUM_CARDS];
    for &(p, c) in hist {
        loc[canon(c, t)] = if p == me { L_PLAYED_ME } else { L_PLAYED_OPP };
    }
    if let Some(c) = obs.table {
        loc[canon(c, t)] = L_TABLE;
    }
    for c in obs.hand.iter() {
        loc[canon(c, t)] = L_MINE;
    }
    let tc = canon(obs.trump_card, t);
    if obs.deck_len > 0 {
        loc[tc] = L_FACEUP;
    } else if loc[tc] == L_UNKNOWN {
        loc[tc] = L_OPP_KNOWN;
    }
    for (c, &l) in loc.iter().enumerate() {
        out[c * RL_LOCS + l] = 1;
    }
    let tricks = hist.len() / 2;
    for k in 0..tricks.min(RL_TRICKS) {
        let (lp, lc) = hist[2 * k];
        let (_, fc) = hist[2 * k + 1];
        let b = I_HIST + k * RL_TRICK;
        out[b] = (lp == me) as u8;
        out[b + 1 + canon(lc, t)] = 1;
        out[b + 1 + NUM_CARDS + canon(fc, t)] = 1;
    }
    out[I_SCAL] = obs.deck_len;
    out[I_SCAL + 1] = obs.points[me as usize];
    out[I_SCAL + 2] = obs.points[1 - me as usize];
    out[I_SCAL + 3] = obs.table.is_some() as u8;
    out[I_SCAL + 4] = tricks as u8;
}

/// Rete a strati pieni con ReLU, l'ultimo strato dà `RL_OUT` valori.
/// Formato del file: "BRL1", numero di strati (u32), poi per strato: ingressi, uscite (u32),
/// pesi [ingresso][uscita] e bias (f32 little-endian).
pub struct RlNet {
    layers: Vec<Layer>,
    scale: [f32; RL_IN],
}

struct Layer {
    n_in: usize,
    n_out: usize,
    w: Vec<f32>,
    b: Vec<f32>,
}

/// Uscita della rete per una posizione.
pub struct RlOut {
    pub logits: [f32; NUM_CARDS],
    /// Stima dell'esito per chi muove, da −1 (sconfitta) a 1 (vittoria).
    pub value: f32,
}

impl RlNet {
    pub fn from_bytes(b: &[u8]) -> Result<RlNet, String> {
        if b.len() < 8 || &b[..4] != b"BRL1" {
            return Err("file di rete rl non valido".into());
        }
        let rd = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
        let n = rd(4);
        let mut o = 8;
        let mut layers = Vec::new();
        for _ in 0..n {
            if b.len() < o + 8 {
                return Err("file di rete rl troncato".into());
            }
            let (n_in, n_out) = (rd(o), rd(o + 4));
            o += 8;
            let need = (n_in * n_out + n_out) * 4;
            if b.len() < o + need {
                return Err("file di rete rl troncato".into());
            }
            let f: Vec<f32> = b[o..o + need].chunks_exact(4).map(|x| f32::from_le_bytes(x.try_into().unwrap())).collect();
            o += need;
            layers.push(Layer { n_in, n_out, w: f[..n_in * n_out].to_vec(), b: f[n_in * n_out..].to_vec() });
        }
        if layers.is_empty() || layers[0].n_in != RL_IN || layers.last().unwrap().n_out != RL_OUT {
            return Err("dimensioni della rete rl diverse da quelle attese".into());
        }
        for w in layers.windows(2) {
            if w[0].n_out != w[1].n_in {
                return Err("strati della rete rl incompatibili".into());
            }
        }
        Ok(RlNet { layers, scale: rl_scale() })
    }

    /// Valuta la codifica `x` (da `encode`).
    pub fn forward(&self, x: &[u8]) -> RlOut {
        let first = &self.layers[0];
        let mut h = first.b.clone();
        for (i, &v) in x.iter().enumerate() {
            if v != 0 {
                let a = v as f32 * self.scale[i];
                let row = &first.w[i * first.n_out..(i + 1) * first.n_out];
                for (hj, &wj) in h.iter_mut().zip(row) {
                    *hj += a * wj;
                }
            }
        }
        for l in &self.layers[1..] {
            for v in h.iter_mut() {
                *v = v.max(0.0);
            }
            let mut nh = l.b.clone();
            for (i, &a) in h.iter().enumerate() {
                if a != 0.0 {
                    let row = &l.w[i * l.n_out..(i + 1) * l.n_out];
                    for (hj, &wj) in nh.iter_mut().zip(row) {
                        *hj += a * wj;
                    }
                }
            }
            debug_assert_eq!(nh.len(), l.n_out);
            h = nh;
        }
        let mut logits = [0f32; NUM_CARDS];
        logits.copy_from_slice(&h[..NUM_CARDS]);
        RlOut { logits, value: h[NUM_CARDS].tanh() }
    }

    /// Probabilità di giocare ciascuna carta della mano (nell'ordine di `obs.hand.iter()`),
    /// con temperatura `temp` (0 = solo la migliore).
    pub fn policy(&self, obs: &Observation, hist: &[(u8, Card)], temp: f32) -> Vec<(Card, f32)> {
        let mut x = [0u8; RL_IN];
        encode(obs, hist, &mut x);
        let out = self.forward(&x);
        let cards: Vec<Card> = obs.hand.iter().collect();
        let z: Vec<f32> = cards.iter().map(|&c| out.logits[canon(c, obs.trump)]).collect();
        let max = z.iter().cloned().fold(f32::MIN, f32::max);
        let mut p: Vec<f32> = if temp <= 0.0 {
            z.iter().map(|&v| if v == max { 1.0 } else { 0.0 }).collect()
        } else {
            z.iter().map(|&v| ((v - max) / temp).exp()).collect()
        };
        let s: f32 = p.iter().sum();
        for v in p.iter_mut() {
            *v /= s;
        }
        cards.into_iter().zip(p).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Game;
    use rand::rngs::SmallRng;
    use rand::{Rng, SeedableRng};

    #[test]
    fn uncanon_inverts_canon() {
        for t in 0..4u8 {
            for c in 0..NUM_CARDS as u8 {
                assert_eq!(uncanon(canon(c, t), t), c);
            }
        }
    }

    #[test]
    fn encode_one_location_per_card() {
        let mut rng = SmallRng::seed_from_u64(7);
        for _ in 0..200 {
            let first = rng.gen_range(0..2);
            let mut g = Game::new(&mut rng, first);
            let mut hist = Vec::new();
            while !g.is_over() {
                let p = g.to_move();
                let obs = Observation::of(&g, p);
                let mut x = [0u8; RL_IN];
                encode(&obs, &hist, &mut x);
                for c in 0..NUM_CARDS {
                    let s: u32 = x[c * RL_LOCS..(c + 1) * RL_LOCS].iter().map(|&v| v as u32).sum();
                    assert_eq!(s, 1);
                }
                let unknown = (0..NUM_CARDS).filter(|&c| x[c * RL_LOCS + L_UNKNOWN] == 1).count();
                let opp = g.hands[1 - p as usize].len() as usize;
                let known = (0..NUM_CARDS).filter(|&c| x[c * RL_LOCS + L_OPP_KNOWN] == 1).count();
                let deck_hidden = if g.deck_len > 0 { g.deck_len as usize - 1 } else { 0 };
                assert_eq!(unknown + known, opp + deck_hidden);
                let moves = g.legal_moves().to_vec();
                let c = moves[rng.gen_range(0..moves.len())];
                hist.push((p, c));
                g.play(c);
            }
        }
    }
}
