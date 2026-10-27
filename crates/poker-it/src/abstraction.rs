//! Come il motore riassume quello che sa: la storia pubblica della mano più un "secchio"
//! che raggruppa le mani simili.
//!
//! - Prima del cambio il secchio è il percentile atteso dopo il miglior cambio (16 livelli
//!   a quantili uguali) e quante carte cambierebbe (0, 1, 2, 3 o più): 64 secchi.
//! - Dopo il cambio è la forza della mano servita, in 32 livelli più fitti verso le mani
//!   forti, dove le differenze contano di più.
//!
//! Nel cambio il motore sceglie solo quante carte cambiare (0..=5): quali carte lo dice
//! l'analisi del cambio (le carte che danno il percentile atteso più alto).

use crate::draw::DrawSummary;
use crate::game::{Action, Observation, Phase};

pub const PRE_LEVELS: usize = 16;
pub const POST_LEVELS: usize = 32;

pub fn pre_bucket(d: &DrawSummary, thresholds: &[f32]) -> u8 {
    let q = thresholds.partition_point(|&t| t <= d.max_ev()).min(PRE_LEVELS - 1);
    (q * 4 + d.natural().min(3)) as u8
}

pub fn post_bucket(strength: f32) -> u8 {
    let x = 1.0 - (1.0 - strength).max(0.0).sqrt();
    ((x * POST_LEVELS as f32) as usize).min(POST_LEVELS - 1) as u8
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
