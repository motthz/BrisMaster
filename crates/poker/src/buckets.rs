//! Raggruppamento delle mani in "bucket" per giro.
//!
//! - Preflop: le 169 classi di mani iniziali (nessuna perdita di informazione).
//! - Flop e turn: E[HS²], la media del quadrato della forza al river, che premia le mani
//!   che possono migliorare molto (progetti di colore e di scala) rispetto all'equità semplice.
//! - River: la forza esatta contro una mano a caso.
//!
//! I bucket dopo il flop sono a frequenze uguali: le soglie sono i quantili della misura
//! su situazioni casuali, calcolati una volta e salvati con la strategia.

use crate::abstraction::BUCKETS;
use crate::cards::{preflop_class, Card, CardSet};
use crate::equity::{hs2, river_hs, Rest};
use rand::rngs::SmallRng;
use rand::SeedableRng;

/// Precisione delle stime: durante l'allenamento si usano poche simulazioni (servono milioni di
/// stime), quando si gioca molte di più.
#[derive(Clone, Copy, Debug)]
pub struct Quality {
    pub runouts: u32,
    pub opps: u32,
    pub river_opps: u32,
}

pub const TRAIN: Quality = Quality { runouts: 24, opps: 3, river_opps: 60 };
pub const PLAY: Quality = Quality { runouts: 400, opps: 3, river_opps: 0 };

/// Valore grezzo della mano nel giro (board di 3, 4 o 5 carte).
pub fn measure(hole: [Card; 2], board: &[Card], q: Quality, rng: &mut SmallRng) -> f32 {
    match board.len() {
        5 if q.river_opps == 0 => river_hs(hole, board) as f32,
        5 => river_hs_sampled(hole, board, q.river_opps, rng),
        _ => hs2(hole, board, q.runouts, q.opps, rng) as f32,
    }
}

/// Forza al river contro `n` mani avversarie estratte a caso.
pub fn river_hs_sampled(hole: [Card; 2], board: &[Card], n: u32, rng: &mut SmallRng) -> f32 {
    let mut used = CardSet::of(board);
    used.add(hole[0]);
    used.add(hole[1]);
    let mut rest = Rest::without(used);
    let mut a = [0u8; 7];
    a[..5].copy_from_slice(board);
    a[5] = hole[0];
    a[6] = hole[1];
    let me = crate::cards::eval(&a);
    let mut tot = 0.0;
    for _ in 0..n {
        let d = rest.draw(rng, 2);
        a[5] = d[0];
        a[6] = d[1];
        let op = crate::cards::eval(&a);
        tot += if me > op { 1.0 } else if me == op { 0.5 } else { 0.0 };
    }
    tot / n as f32
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bucketer {
    /// Soglie per flop, turn e river (`BUCKETS[s] - 1` valori crescenti).
    pub cutoffs: [Vec<f32>; 3],
}

impl Bucketer {
    /// Calcola le soglie su `samples` situazioni casuali per giro.
    pub fn compute(samples: usize, seed: u64) -> Bucketer {
        use rayon::prelude::*;
        let cut = |street: usize| -> Vec<f32> {
            let n_board = [3, 4, 5][street - 1];
            let mut v: Vec<f32> = (0..samples)
                .into_par_iter()
                .map(|i| {
                    let mut rng = SmallRng::seed_from_u64(seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ street as u64);
                    let mut rest = Rest::without(CardSet(0));
                    let d = rest.draw(&mut rng, 2 + n_board).to_vec();
                    measure([d[0], d[1]], &d[2..], TRAIN, &mut rng)
                })
                .collect();
            v.sort_unstable_by(f32::total_cmp);
            let b = BUCKETS[street];
            (1..b).map(|k| v[k * v.len() / b]).collect()
        };
        Bucketer { cutoffs: [cut(1), cut(2), cut(3)] }
    }

    pub fn from_value(&self, street: usize, x: f32) -> u16 {
        self.cutoffs[street - 1].partition_point(|&c| c <= x) as u16
    }

    pub fn bucket(&self, hole: [Card; 2], board: &[Card], q: Quality, rng: &mut SmallRng) -> u16 {
        match board.len() {
            0 => preflop_class(hole[0], hole[1]) as u16,
            n => {
                let street = n - 2;
                self.from_value(street, measure(hole, board, q, rng))
            }
        }
    }

    /// Secchio con un generatore casuale ricavato dalle carte: la stessa situazione dà sempre
    /// lo stesso bucket (utile nell'app e nei test ripetibili).
    pub fn bucket_det(&self, hole: [Card; 2], board: &[Card], q: Quality) -> u16 {
        let mut s = CardSet::of(board).0 ^ (1u64 << hole[0]) ^ (1u64 << hole[1]).rotate_left(7);
        s = s.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ board.len() as u64;
        let mut rng = SmallRng::seed_from_u64(s);
        self.bucket(hole, board, q, &mut rng)
    }
}
