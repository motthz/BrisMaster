//! Come il motore riassume quello che sa: la storia pubblica della mano più un "secchio"
//! che raggruppa le mani simili.
//!
//! - Prima del cambio il secchio è il percentile atteso dopo il miglior cambio (in livelli
//!   a quantili uguali) e quante carte cambierebbe (0, 1, 2, 3 o più).
//! - Dopo il cambio è la forza della mano servita, in livelli più fitti verso le mani
//!   forti, dove le differenze contano di più.
//!
//! Il numero di livelli fa parte della strategia salvata (`Abstraction`), così strategie
//! allenate con astrazioni diverse possono giocare una contro l'altra.
//!
//! Nel cambio il motore sceglie solo quante carte cambiare (0..=5): quali carte lo dice
//! l'analisi del cambio (le carte che danno il percentile atteso più alto).

use crate::cards::all_hands;
use crate::draw::{summary, DrawSummary};
use crate::game::{Action, Observation, Phase};
use rayon::prelude::*;

/// Livelli usati dal motore attuale.
pub const PRE_LEVELS: usize = 32;
pub const POST_LEVELS: usize = 64;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Abstraction {
    /// Soglie dei livelli prima del cambio (uno in meno dei livelli).
    pub thresholds: Vec<f32>,
    pub post_levels: usize,
}

impl Abstraction {
    /// Soglie a quantili uguali del percentile atteso dopo il miglior cambio, su tutte le mani.
    pub fn new(pre_levels: usize, post_levels: usize) -> Abstraction {
        assert!(pre_levels * 4 <= 256 && post_levels < 256);
        let mut v: Vec<f32> = all_hands().par_iter().map(|h| summary(h).max_ev()).collect();
        v.sort_unstable_by(f32::total_cmp);
        let thresholds = (1..pre_levels).map(|i| v[i * v.len() / pre_levels]).collect();
        Abstraction { thresholds, post_levels }
    }

    pub fn standard() -> Abstraction {
        Abstraction::new(PRE_LEVELS, POST_LEVELS)
    }

    pub fn pre_levels(&self) -> usize {
        self.thresholds.len() + 1
    }

    /// Numero di secchi prima e dopo il cambio.
    pub fn pre_buckets(&self) -> usize {
        self.pre_levels() * 4
    }

    pub fn pre(&self, d: &DrawSummary) -> u8 {
        let q = self.thresholds.partition_point(|&t| t <= d.max_ev()).min(self.pre_levels() - 1);
        (q * 4 + d.natural().min(3)) as u8
    }

    pub fn post(&self, strength: f32) -> u8 {
        let x = 1.0 - (1.0 - strength).max(0.0).sqrt();
        ((x * self.post_levels as f32) as usize).min(self.post_levels - 1) as u8
    }
}

#[inline]
pub fn symbol(a: Action) -> u64 {
    match a {
        Action::Call => 1,
        Action::Raise => 2,
        Action::Draw(k) => 3 + k as u64,
        Action::Fold => 15,
    }
}

#[inline]
pub fn push(seq: u64, a: Action) -> u64 {
    (seq << 4) | symbol(a)
}

#[inline]
pub fn key(seq: u64, bucket: u8) -> u64 {
    (seq << 8) | bucket as u64
}

pub fn seq_of(history: &[Action]) -> u64 {
    history.iter().fold(0, |s, &a| push(s, a))
}

/// Le azioni astratte possibili, nell'ordine usato dall'allenamento.
/// Nel cambio `Draw(k)` indica il numero di carte da cambiare.
pub fn actions(obs: &Observation) -> Vec<Action> {
    match obs.phase {
        Phase::Draw => (0..6).map(Action::Draw).collect(),
        _ => {
            let mut v = Vec::with_capacity(3);
            if obs.to_call > 0 {
                v.push(Action::Fold);
            }
            v.push(Action::Call);
            if obs.can_raise {
                v.push(Action::Raise);
            }
            v
        }
    }
}
