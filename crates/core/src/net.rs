//! Reti neurali piccole, allenate su partite tra motori (self-play):
//!
//! - **valutazione**: da uno stato completo (mani e mazzo noti, come nei mondi campionati
//!   dalla ricerca) stima come finirà la partita giocata da giocatori forti che NON vedono
//!   le carte avversarie, quindi corregge l'ottimismo dei rollout "a carte scoperte";
//! - **politica**: da ciò che vede un giocatore, quanto è probabile che giochi ogni carta.
//!   Serve a dedurre le carte avversarie da come ha giocato.

use crate::cards::*;
use crate::game::{Game, Observation};

/// Dove si trova una carta, dal punto di vista di chi deve muovere.
pub const LOCS: usize = 8;
const LOC_MINE: u8 = 0;
const LOC_OPP: u8 = 1;
const LOC_PLAYED: u8 = 2;
const LOC_TABLE: u8 = 3;
const LOC_TOP: u8 = 4;
const LOC_SECOND: u8 = 5;
const LOC_DECK: u8 = 6;
const LOC_FACEUP: u8 = 7;

const DECK_BUCKETS: usize = 18;
/// Ingressi: carta×posto (una sola attiva per carta), lunghezza del mazzo, chi apre, punti.
pub const N_IN: usize = NUM_CARDS * LOCS + DECK_BUCKETS + 1 + 2;
const I_DECK: usize = NUM_CARDS * LOCS;
const I_FOLLOW: usize = I_DECK + DECK_BUCKETS;
const I_PTS: usize = I_FOLLOW + 1;

/// Posizione codificata in modo compatto: è anche il formato dei dati di allenamento.
/// I semi sono rinumerati: la briscola è sempre il seme 0, gli altri in ordine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// Posto di ogni carta (indice di carta con i semi rinumerati).
    pub loc: [u8; NUM_CARDS],
    pub deck_len: u8,
    /// Vero se chi muove risponde a una carta già sul tavolo.
    pub follow: bool,
    pub my_pts: u8,
    pub opp_pts: u8,
}

/// Indice della carta con i semi rinumerati (briscola = seme 0).
#[inline]
pub fn canon_index(c: Card, trump: u8) -> usize {
    (canon_suit(suit(c), trump) * 10 + rank(c)) as usize
}

/// Seme rinumerato: briscola 0, gli altri 1..3 nell'ordine originale.
#[inline]
fn canon_suit(s: u8, trump: u8) -> u8 {
    if s == trump {
        0
    } else if s < trump {
        s + 1
    } else {
        s
    }
}

impl Position {
    /// Codifica `g` dal punto di vista di chi deve muovere.
    pub fn of(g: &Game) -> Position {
        let me = g.to_move() as usize;
        let mut loc = [LOC_PLAYED; NUM_CARDS];
        let idx = |c: Card| (canon_suit(suit(c), g.trump) * 10 + rank(c)) as usize;
        for c in g.hands[me].iter() {
            loc[idx(c)] = LOC_MINE;
        }
        for c in g.hands[1 - me].iter() {
            loc[idx(c)] = LOC_OPP;
        }
        if let Some(t) = g.table {
            loc[idx(t)] = LOC_TABLE;
        }
        let n = g.deck_len as usize;
        for k in 0..n {
            loc[idx(g.deck[k])] = if k + 1 == n {
                LOC_TOP
            } else if k + 2 == n {
                LOC_SECOND
            } else if k == 0 {
                LOC_FACEUP
            } else {
                LOC_DECK
            };
        }
        Position {
            loc,
            deck_len: g.deck_len,
            follow: g.table.is_some(),
            my_pts: g.points[me],
            opp_pts: g.points[1 - me],
        }
    }

    /// Codifica ciò che vede chi deve muovere (per la rete di politica): le carte che non
    /// vede sono tutte "nel mazzo", tranne la briscola scoperta se sa chi l'ha pescata.
    pub fn of_obs(o: &Observation) -> Position {
        let mut loc = [LOC_PLAYED; NUM_CARDS];
        for c in o.unknown().iter() {
            loc[canon_index(c, o.trump)] = LOC_DECK;
        }
        for c in o.hand.iter() {
            loc[canon_index(c, o.trump)] = LOC_MINE;
        }
        if let Some(t) = o.table {
            loc[canon_index(t, o.trump)] = LOC_TABLE;
        }
        let tc = canon_index(o.trump_card, o.trump);
        if o.deck_len > 0 {
            loc[tc] = LOC_FACEUP;
        } else if !o.hand.contains(o.trump_card) && !o.played.contains(o.trump_card) && o.table != Some(o.trump_card) {
            loc[tc] = LOC_OPP;
        }
        let me = o.me as usize;
        Position { loc, deck_len: o.deck_len, follow: o.table.is_some(), my_pts: o.points[me], opp_pts: o.points[1 - me] }
    }

    /// Indici degli ingressi attivi (valore 1) e i due ingressi continui (punti).
    #[inline]
    pub fn active(&self, out: &mut [u16; NUM_CARDS + 2]) -> ([f32; 2], usize) {
        let mut n = 0;
        for (c, &l) in self.loc.iter().enumerate() {
            out[n] = (c * LOCS + l as usize) as u16;
            n += 1;
        }
        out[n] = (I_DECK + (self.deck_len as usize / 2).min(DECK_BUCKETS - 1)) as u16;
        n += 1;
        if self.follow {
            out[n] = I_FOLLOW as u16;
            n += 1;
        }
        ([self.my_pts as f32 / 120.0, self.opp_pts as f32 / 120.0], n)
    }

    /// Stessa posizione con i tre semi non di briscola permutati (aumento dei dati).
    pub fn permute_suits(&self, perm: [u8; 3]) -> Position {
        let mut p = *self;
        for s in 1..4usize {
            let t = perm[s - 1] as usize;
            p.loc[t * 10..t * 10 + 10].copy_from_slice(&self.loc[s * 10..s * 10 + 10]);
        }
        p
    }

    pub const BYTES: usize = NUM_CARDS + 4;

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.loc);
        out.extend_from_slice(&[self.deck_len, self.follow as u8, self.my_pts, self.opp_pts]);
    }

    pub fn read(b: &[u8]) -> Position {
        let mut loc = [0u8; NUM_CARDS];
        loc.copy_from_slice(&b[..NUM_CARDS]);
        let r = &b[NUM_CARDS..];
        Position { loc, deck_len: r[0], follow: r[1] != 0, my_pts: r[2], opp_pts: r[3] }
    }
}

/// Strati nascosti di una rete nuova (quelli di un file si leggono dal file).
pub const H1: usize = 96;
pub const H2: usize = 32;
/// Strati nascosti più larghi ammessi.
pub const MAX_H1: usize = 512;
pub const MAX_H2: usize = 128;

/// Uscite della rete di valutazione: probabilità di vincere e quota dei punti finali.
pub const VALUE_OUT: usize = 2;
/// Uscite della rete di politica: un punteggio per carta (semi rinumerati).
pub const POLICY_OUT: usize = NUM_CARDS;

/// Percettrone a due strati nascosti.
#[derive(Clone)]
pub struct Net {
    pub n_out: usize,
    /// Larghezza dei due strati nascosti.
    pub h1: usize,
    pub h2: usize,
    /// Pesi del primo strato per riga d'ingresso: `w1[i * H1 + j]`.
    pub w1: Vec<f32>,
    pub b1: Vec<f32>,
    pub w2: Vec<f32>,
    pub b2: Vec<f32>,
    pub w3: Vec<f32>,
    pub b3: Vec<f32>,
}

/// Valori intermedi del calcolo in avanti (servono anche all'allenamento).
pub struct Forward {
    pub h1: [f32; MAX_H1],
    pub h2: [f32; MAX_H2],
    /// Uscite prima della sigmoide (valutazione) o della softmax (politica); valgono le prime `n_out`.
    pub z: [f32; POLICY_OUT],
}

#[inline]
pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

const MAGIC: &[u8; 4] = b"BRN2";

/// Rete di politica allenata, inclusa nel programma (`nets/politica.bin`, da `train --kind policy`).
pub fn policy() -> &'static Net {
    static NET: std::sync::OnceLock<Net> = std::sync::OnceLock::new();
    NET.get_or_init(|| Net::from_bytes(include_bytes!("../nets/politica.bin")).expect("rete di politica inclusa"))
}

impl Net {
    /// Rete con pesi casuali piccoli (inizializzazione di He), per l'allenamento.
    pub fn random(seed: u64, n_out: usize) -> Net {
        Net::random_sized(seed, n_out, H1, H2)
    }

    /// Come `random`, con strati nascosti larghi `h1` e `h2`.
    pub fn random_sized(seed: u64, n_out: usize, h1: usize, h2: usize) -> Net {
        assert!(h1 <= MAX_H1 && h2 <= MAX_H2, "strati nascosti troppo larghi");
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
        let mut init = |n: usize, fan_in: usize| -> Vec<f32> {
            let s = (2.0 / fan_in as f32).sqrt();
            (0..n).map(|_| (rng.gen::<f32>() * 2.0 - 1.0) * s * 1.7).collect()
        };
        Net {
            n_out,
            h1,
            h2,
            w1: init(N_IN * h1, NUM_CARDS + 3),
            b1: vec![0.0; h1],
            w2: init(h1 * h2, h1),
            b2: vec![0.0; h2],
            w3: init(h2 * n_out, h2),
            b3: vec![0.0; n_out],
        }
    }

    pub fn forward(&self, p: &Position) -> Forward {
        let (n1, n2) = (self.h1, self.h2);
        let mut idx = [0u16; NUM_CARDS + 2];
        let (dense, n) = p.active(&mut idx);
        let mut h1 = [0f32; MAX_H1];
        h1[..n1].copy_from_slice(&self.b1);
        for &i in &idx[..n] {
            let row = &self.w1[i as usize * n1..(i as usize + 1) * n1];
            for j in 0..n1 {
                h1[j] += row[j];
            }
        }
        for (k, &x) in dense.iter().enumerate() {
            let row = &self.w1[(I_PTS + k) * n1..(I_PTS + k + 1) * n1];
            for j in 0..n1 {
                h1[j] += row[j] * x;
            }
        }
        for v in h1[..n1].iter_mut() {
            *v = v.max(0.0);
        }
        let mut h2 = [0f32; MAX_H2];
        h2[..n2].copy_from_slice(&self.b2);
        for (i, &x) in h1[..n1].iter().enumerate() {
            if x != 0.0 {
                let row = &self.w2[i * n2..(i + 1) * n2];
                for j in 0..n2 {
                    h2[j] += row[j] * x;
                }
            }
        }
        for v in h2[..n2].iter_mut() {
            *v = v.max(0.0);
        }
        let k = self.n_out;
        let mut z = [0f32; POLICY_OUT];
        z[..k].copy_from_slice(&self.b3);
        for (i, &x) in h2[..n2].iter().enumerate() {
            if x != 0.0 {
                let row = &self.w3[i * k..(i + 1) * k];
                for j in 0..k {
                    z[j] += row[j] * x;
                }
            }
        }
        Forward { h1, h2, z }
    }

    /// Per chi deve muovere in `g`: probabilità di vincere (pareggio = ½) e punti finali attesi.
    #[inline]
    pub fn eval(&self, g: &Game) -> (f32, f32) {
        let f = self.forward(&Position::of(g));
        (sigmoid(f.z[0]), sigmoid(f.z[1]) * 120.0)
    }

    /// Come `eval`, ma dalla sola osservazione di chi deve muovere (rete onesta).
    #[inline]
    pub fn eval_obs(&self, g: &Game) -> (f32, f32) {
        let f = self.forward(&Position::of_obs(&Observation::of(g, g.to_move())));
        (sigmoid(f.z[0]), sigmoid(f.z[1]) * 120.0)
    }

    /// Rete di politica: probabilità che chi deve muovere giochi ciascuna carta della sua mano
    /// (nell'ordine di `obs.hand.iter()`), con temperatura `temp` (1 = come allenata).
    pub fn policy(&self, obs: &Observation, temp: f32, out: &mut [f32; 3]) -> usize {
        let f = self.forward(&Position::of_obs(obs));
        let mut n = 0;
        let mut max = f32::MIN;
        for c in obs.hand.iter() {
            out[n] = f.z[canon_index(c, obs.trump)] * temp;
            max = max.max(out[n]);
            n += 1;
        }
        let mut sum = 0.0;
        for v in out[..n].iter_mut() {
            *v = (*v - max).exp();
            sum += *v;
        }
        for v in out[..n].iter_mut() {
            *v /= sum;
        }
        n
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for d in [N_IN, self.h1, self.h2, self.n_out] {
            out.extend_from_slice(&(d as u32).to_le_bytes());
        }
        for v in [&self.w1, &self.b1, &self.w2, &self.b2, &self.w3, &self.b3] {
            for x in v.iter() {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        out
    }

    pub fn from_bytes(b: &[u8]) -> Result<Net, String> {
        if b.len() < 20 || &b[..4] != MAGIC {
            return Err("file della rete non valido".into());
        }
        let dim = |k: usize| u32::from_le_bytes(b[4 + 4 * k..8 + 4 * k].try_into().unwrap()) as usize;
        let (h1, h2) = (dim(1), dim(2));
        if dim(0) != N_IN || h1 == 0 || h2 == 0 || h1 > MAX_H1 || h2 > MAX_H2 {
            return Err(format!("dimensioni della rete diverse: {} {} {}", dim(0), h1, h2));
        }
        let n_out = dim(3);
        if n_out != VALUE_OUT && n_out != POLICY_OUT {
            return Err(format!("numero di uscite non valido: {n_out}"));
        }
        let mut f = b[20..].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap()));
        let mut take = |n: usize| -> Result<Vec<f32>, String> {
            let v: Vec<f32> = f.by_ref().take(n).collect();
            if v.len() == n { Ok(v) } else { Err("file della rete troncato".into()) }
        };
        Ok(Net {
            n_out,
            h1,
            h2,
            w1: take(N_IN * h1)?,
            b1: take(h1)?,
            w2: take(h1 * h2)?,
            b2: take(h2)?,
            w3: take(h2 * n_out)?,
            b3: take(n_out)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::SmallRng;
    use rand::{Rng, SeedableRng};

    #[test]
    fn position_roundtrip_and_symmetry() {
        let mut rng = SmallRng::seed_from_u64(4);
        let net = Net::random(1, VALUE_OUT);
        let pol = Net::random(2, POLICY_OUT);
        for _ in 0..200 {
            let first = rng.gen_range(0..2);
            let mut g = Game::new(&mut rng, first);
            for _ in 0..rng.gen_range(0..39) {
                let m = g.legal_moves().to_vec();
                g.play(m[rng.gen_range(0..m.len())]);
            }
            if g.is_over() {
                continue;
            }
            let p = Position::of(&g);
            let mut b = Vec::new();
            p.write(&mut b);
            assert_eq!(b.len(), Position::BYTES);
            assert_eq!(Position::read(&b), p);
            // Ogni carta ha un solo posto, e quella permutata torna uguale con la permutazione identica.
            assert_eq!(p.permute_suits([1, 2, 3]), p);
            assert_eq!(p.loc.iter().filter(|&&l| l == LOC_MINE).count(), g.hands[g.to_move() as usize].len() as usize);
            let (w, pts) = net.eval(&g);
            assert!((0.0..=1.0).contains(&w) && (0.0..=120.0).contains(&pts));
            let obs = Observation::of(&g, g.to_move());
            let mut pr = [0f32; 3];
            let n = pol.policy(&obs, 1.0, &mut pr);
            assert_eq!(n, obs.hand.len() as usize);
            assert!((pr[..n].iter().sum::<f32>() - 1.0).abs() < 1e-4);
            let q = Position::of_obs(&obs);
            assert_eq!(q.loc.iter().filter(|&&l| l == LOC_MINE).count(), n);
        }
        let back = Net::from_bytes(&net.to_bytes()).unwrap();
        assert_eq!(back.w1, net.w1);
        let back = Net::from_bytes(&pol.to_bytes()).unwrap();
        assert_eq!((back.n_out, back.w3.len()), (POLICY_OUT, pol.w3.len()));
        // Una rete più larga si salva, si rilegge e calcola le stesse uscite.
        let wide = Net::random_sized(3, POLICY_OUT, 256, 64);
        let back = Net::from_bytes(&wide.to_bytes()).unwrap();
        assert_eq!((back.h1, back.h2), (256, 64));
        let g = Game::new(&mut rng, 0);
        let p = Position::of(&g);
        assert_eq!(back.forward(&p).z, wide.forward(&p).z);
    }
}
