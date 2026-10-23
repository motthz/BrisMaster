//! Forza assoluta delle mani e analisi del cambio.
//!
//! La forza di una mano è il suo percentile tra tutte le mani possibili del mazzo
//! (0 = la peggiore, 1 = la migliore). Per il cambio si calcola, per ogni insieme di
//! carte da scartare, il percentile atteso dopo la pescata: in modo esatto fino a due
//! carte cambiate, con 256 pescate simulate (sempre le stesse per la stessa mano) da tre in su.

use crate::cards::{all_hands, Card, NUM_CARDS};
use crate::eval::evaluate;
use crate::reference::POST_DRAW;
use rand::rngs::SmallRng;
use rand::seq::index::sample;
use rand::SeedableRng;
use std::sync::OnceLock;

const MC_SAMPLES: usize = 256;

fn sorted_values() -> &'static [u32] {
    static V: OnceLock<Vec<u32>> = OnceLock::new();
    V.get_or_init(|| {
        let mut v: Vec<u32> = all_hands().iter().map(evaluate).collect();
        v.sort_unstable();
        v
    })
}

/// Percentile del valore `v` tra tutte le mani di 5 carte del mazzo (servite, senza cambio).
#[inline]
pub fn raw_percentile(v: u32) -> f32 {
    let s = sorted_values();
    s.partition_point(|&x| x < v) as f32 / (s.len() - 1) as f32
}

/// Percentile del valore `v` in una tabella di quantili.
#[inline]
pub fn table_percentile(table: &[u32], v: u32) -> f32 {
    table.partition_point(|&x| x < v) as f32 / (table.len() - 1) as f32
}

/// Forza di una mano: il suo percentile tra le mani che si hanno davvero allo showdown,
/// cioè dopo un cambio ragionevole (tabella in `reference.rs`). Una coppia d'Assi è nella
/// media delle mani servite, ma sotto la media delle mani dopo il cambio.
#[inline]
pub fn percentile(v: u32) -> f32 {
    if POST_DRAW.is_empty() {
        raw_percentile(v)
    } else {
        table_percentile(POST_DRAW, v)
    }
}

#[inline]
pub fn strength(h: &[Card; 5]) -> f32 {
    percentile(evaluate(h))
}

/// Mano ordinata dopo aver sostituito le carte in `mask` (posizioni) con `new`.
pub fn replace(h: &[Card; 5], mask: u8, new: &[Card]) -> [Card; 5] {
    let mut out = *h;
    let mut j = 0;
    for (i, slot) in out.iter_mut().enumerate() {
        if mask & (1 << i) != 0 {
            *slot = new[j];
            j += 1;
        }
    }
    out.sort_unstable();
    out
}

#[derive(Clone, Copy, Debug)]
pub struct DrawInfo {
    /// Percentile atteso dopo il cambio per ogni maschera di scarto (bit i = posizione i).
    pub ev: [f32; 32],
    /// Lo scarto migliore per ogni numero di carte cambiate, 0..=5, e il suo valore atteso.
    pub best: [u8; 6],
    pub best_ev: [f32; 6],
}

impl DrawInfo {
    /// Il numero di carte da cambiare che dà il percentile atteso più alto.
    pub fn natural(&self) -> usize {
        (0..6).max_by(|&a, &b| self.best_ev[a].total_cmp(&self.best_ev[b])).unwrap()
    }
    pub fn max_ev(&self) -> f32 {
        self.best_ev[self.natural()]
    }
    /// Forza della mano se si resta serviti.
    pub fn stand(&self) -> f32 {
        self.best_ev[0]
    }
}

/// Analizza il cambio per una mano ordinata, senza conoscere le carte dell'avversario.
pub fn analyze(h: &[Card; 5]) -> DrawInfo {
    analyze_with(h, &strength)
}

/// Come `analyze`, con una misura di forza a scelta (serve a costruire la tabella di riferimento).
pub fn analyze_with(h: &[Card; 5], strength: &dyn Fn(&[Card; 5]) -> f32) -> DrawInfo {
    let mut used = 0u32;
    for &c in h {
        used |= 1 << c;
    }
    let rest: Vec<Card> = (0..NUM_CARDS as u8).filter(|c| used & (1 << c) == 0).collect();
    let mut rng = SmallRng::seed_from_u64(used as u64 ^ 0x5EED_CA4D);
    let mut ev = [0f32; 32];
    let mut new = [0 as Card; 5];
    for mask in 0u8..32 {
        let k = mask.count_ones() as usize;
        let mut sum = 0f64;
        let n;
        match k {
            0 => {
                sum = strength(h) as f64;
                n = 1;
            }
            1 => {
                for &a in &rest {
                    sum += strength(&replace(h, mask, &[a])) as f64;
                }
                n = rest.len();
            }
            2 => {
                let mut cnt = 0;
                for i in 0..rest.len() {
                    for j in i + 1..rest.len() {
                        sum += strength(&replace(h, mask, &[rest[i], rest[j]])) as f64;
                        cnt += 1;
                    }
                }
                n = cnt;
            }
            _ => {
                for _ in 0..MC_SAMPLES {
                    for (slot, idx) in new.iter_mut().zip(sample(&mut rng, rest.len(), k).into_iter()) {
                        *slot = rest[idx];
                    }
                    sum += strength(&replace(h, mask, &new[..k])) as f64;
                }
                n = MC_SAMPLES;
            }
        }
        ev[mask as usize] = (sum / n as f64) as f32;
    }
    let mut best = [0u8; 6];
    let mut best_ev = [-1f32; 6];
    for mask in 0u8..32 {
        let k = mask.count_ones() as usize;
        if ev[mask as usize] > best_ev[k] {
            best_ev[k] = ev[mask as usize];
            best[k] = mask;
        }
    }
    DrawInfo { ev, best, best_ev }
}

/// La parte dell'analisi che serve per giocare: scarto migliore e valore atteso per
/// ogni numero di carte cambiate.
#[derive(Clone, Copy, Debug)]
pub struct DrawSummary {
    pub best: [u8; 6],
    pub best_ev: [f32; 6],
}

impl DrawSummary {
    pub fn natural(&self) -> usize {
        (0..6).max_by(|&a, &b| self.best_ev[a].total_cmp(&self.best_ev[b])).unwrap()
    }
    pub fn max_ev(&self) -> f32 {
        self.best_ev[self.natural()]
    }
    pub fn stand(&self) -> f32 {
        self.best_ev[0]
    }
}

/// `analyze` con memoria condivisa tra thread: ogni mano si analizza una volta sola.
pub fn summary(h: &[Card; 5]) -> DrawSummary {
    static CACHE: OnceLock<Vec<OnceLock<DrawSummary>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| (0..crate::cards::NUM_HANDS).map(|_| OnceLock::new()).collect());
    *cache[crate::cards::hand_index(h)].get_or_init(|| {
        let d = analyze(h);
        DrawSummary { best: d.best, best_ev: d.best_ev }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::parse;

    fn h(s: &str) -> [Card; 5] {
        let mut v: Vec<Card> = s.split_whitespace().map(|x| parse(x).unwrap()).collect();
        v.sort_unstable();
        v.try_into().unwrap()
    }

    #[test]
    fn sensible_draws() {
        // Coppia d'assi: si tiene la coppia e si cambiano tre carte.
        let hand = h("Ac Ap 8f 9q Jc");
        let d = analyze(&hand);
        assert_eq!(d.natural(), 3, "{:?}", d.best_ev);
        let kept: Vec<Card> = (0..5).filter(|i| d.best[3] & (1 << i) == 0).map(|i| hand[i]).collect();
        assert!(kept.iter().all(|&c| crate::cards::rank(c) == crate::cards::ACE));
        // Full servito: si resta serviti.
        assert_eq!(analyze(&h("Ac Ap Af 8q 8c")).natural(), 0);
        // Doppia coppia: si cambia la carta spaiata.
        let d = analyze(&h("Kc Kp 9f 9q 7c"));
        assert_eq!(d.natural(), 1, "{:?}", d.best_ev);
        assert_eq!(d.best[1], 1);
        // Colore a quattro carte basso: col mazzo corto il colore entra di rado (4 carte su 27)
        // e conviene di più tenere le due figure e cambiarne tre.
        let d = analyze(&h("7c 9c Jc Kc 8p"));
        assert!(d.best_ev[3] > d.best_ev[1], "{:?}", d.best_ev);
        // Contro le mani dopo il cambio: il full d'Assi è tra le migliori, la coppia d'Assi è sotto la media.
        assert!(strength(&h("Ac Ap Af 8q 8c")) > 0.95, "{}", strength(&h("Ac Ap Af 8q 8c")));
        assert!(strength(&h("Ac Ap 8f 9q Jc")) < 0.5);
    }
}
