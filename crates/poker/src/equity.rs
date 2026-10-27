//! Forza della mano: equità contro una mano casuale, E[HS²] per le astrazioni, tabelle preflop.

use crate::cards::{eval, preflop_class, Card, CardSet};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::sync::OnceLock;

/// Mazzo delle carte non ancora viste, per estrazioni rapide senza ripetizioni.
pub struct Rest {
    cards: [Card; 52],
    n: usize,
}

impl Rest {
    pub fn without(used: CardSet) -> Rest {
        let mut cards = [0; 52];
        let mut n = 0;
        for c in 0..52 {
            if !used.has(c) {
                cards[n] = c;
                n += 1;
            }
        }
        Rest { cards, n }
    }
    /// Estrae `k` carte distinte: si trovano in `self.cards[..k]` dopo la chiamata.
    pub fn draw<R: Rng>(&mut self, rng: &mut R, k: usize) -> &[Card] {
        for i in 0..k {
            let j = rng.gen_range(i..self.n);
            self.cards.swap(i, j);
        }
        &self.cards[..k]
    }
    pub fn all(&self) -> &[Card] {
        &self.cards[..self.n]
    }
}

fn score(hole: [Card; 2], board: &[Card]) -> u32 {
    let mut a = [0u8; 7];
    a[..board.len()].copy_from_slice(board);
    a[board.len()] = hole[0];
    a[board.len() + 1] = hole[1];
    eval(&a[..board.len() + 2])
}

/// Equità contro una mano a caso, con le carte comuni mancanti estratte a caso (Monte Carlo).
pub fn equity_vs_random<R: Rng>(hole: [Card; 2], board: &[Card], samples: u32, rng: &mut R) -> f64 {
    let mut used = CardSet::of(board);
    used.add(hole[0]);
    used.add(hole[1]);
    let mut rest = Rest::without(used);
    let need = 5 - board.len();
    let mut full = [0u8; 5];
    full[..board.len()].copy_from_slice(board);
    let mut tot = 0.0;
    for _ in 0..samples {
        let d = rest.draw(rng, need + 2);
        full[board.len()..].copy_from_slice(&d[2..]);
        let (me, op) = (score(hole, &full), score([d[0], d[1]], &full));
        tot += if me > op { 1.0 } else if me == op { 0.5 } else { 0.0 };
    }
    tot / samples as f64
}

/// Forza esatta al river: quota di mani avversarie battute (pareggi a metà).
pub fn river_hs(hole: [Card; 2], board: &[Card]) -> f64 {
    debug_assert_eq!(board.len(), 5);
    let mut used = CardSet::of(board);
    used.add(hole[0]);
    used.add(hole[1]);
    let rest = Rest::without(used);
    let r = rest.all();
    let me = score(hole, board);
    // Il board è comune: si valuta una sola volta per mano avversaria.
    let (mut tot, mut n) = (0.0, 0u32);
    for i in 0..r.len() {
        for j in i + 1..r.len() {
            let op = score([r[i], r[j]], board);
            tot += if me > op { 1.0 } else if me == op { 0.5 } else { 0.0 };
            n += 1;
        }
    }
    tot / n as f64
}

/// E[HS²] al flop o al turn: media del quadrato della forza al river su `runouts` sviluppi,
/// ciascuno misurato contro `opps` mani avversarie. Premia le mani che possono diventare molto forti.
pub fn hs2<R: Rng>(hole: [Card; 2], board: &[Card], runouts: u32, opps: u32, rng: &mut R) -> f64 {
    let mut used = CardSet::of(board);
    used.add(hole[0]);
    used.add(hole[1]);
    let mut rest = Rest::without(used);
    let need = 5 - board.len();
    let mut full = [0u8; 5];
    full[..board.len()].copy_from_slice(board);
    let mut tot = 0.0;
    for _ in 0..runouts {
        let d = rest.draw(rng, need + 2 * opps as usize);
        full[board.len()..].copy_from_slice(&d[..need]);
        let me = score(hole, &full);
        let mut hs = 0.0;
        for k in 0..opps as usize {
            let op = score([d[need + 2 * k], d[need + 2 * k + 1]], &full);
            hs += if me > op { 1.0 } else if me == op { 0.5 } else { 0.0 };
        }
        let hs = hs / opps as f64;
        tot += hs * hs;
    }
    tot / runouts as f64
}

/// Equità preflop di ogni classe (169) contro una mano a caso.
pub fn preflop_equity() -> &'static [f64; 169] {
    static T: OnceLock<[f64; 169]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0.0; 169];
        let mut rng = SmallRng::seed_from_u64(169);
        for cls in 0..169 {
            let (i, j) = (cls / 13, cls % 13);
            let (h, l) = if i >= j { (i as u8, j as u8) } else { (j as u8, i as u8) };
            let hole = if i > j { [h * 4, l * 4] } else { [h * 4, l * 4 + 1] };
            debug_assert_eq!(preflop_class(hole[0], hole[1]), cls);
            t[cls] = equity_vs_random(hole, &[], 6000, &mut rng);
        }
        t
    })
}

/// Percentile della classe nella classifica preflop (0 = la peggiore, 1 = AA), pesata per combinazioni.
pub fn preflop_percentile(cls: usize) -> f64 {
    static T: OnceLock<[f64; 169]> = OnceLock::new();
    T.get_or_init(|| {
        let eq = preflop_equity();
        let combos = |c: usize| if c / 13 == c % 13 { 6.0 } else if c / 13 > c % 13 { 4.0 } else { 12.0 };
        let mut t = [0.0; 169];
        for c in 0..169 {
            let below: f64 = (0..169).filter(|&d| eq[d] < eq[c]).map(combos).sum();
            t[c] = (below + combos(c) / 2.0) / 1326.0;
        }
        t
    })[cls]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::parse;

    fn h(s: &str) -> Vec<Card> {
        s.split_whitespace().map(|x| parse(x).unwrap()).collect()
    }

    #[test]
    fn known_preflop_equities() {
        let eq = preflop_equity();
        let aa = preflop_class(parse("As").unwrap(), parse("Ah").unwrap());
        let t72 = preflop_class(parse("7s").unwrap(), parse("2h").unwrap());
        assert!((eq[aa] - 0.852).abs() < 0.015, "{}", eq[aa]);
        assert!((eq[t72] - 0.346).abs() < 0.015, "{}", eq[t72]);
        assert!(preflop_percentile(aa) > 0.99);
    }

    #[test]
    fn river_nuts() {
        let b = h("As Ks Qs Js 2d");
        assert_eq!(river_hs([parse("Ts").unwrap(), parse("3c").unwrap()], &b), 1.0);
        assert!(river_hs([parse("3c").unwrap(), parse("4c").unwrap()], &b) < 0.2);
    }
}
