//! Valore delle mani secondo le regole del poker all'italiana col mazzo da 32 carte.
//!
//! Dalla più bassa: carta alta, coppia, doppia coppia, tris, scala, full, colore, poker,
//! scala reale. Col mazzo corto il colore è più raro del full e lo batte. La scala più
//! bassa è A-7-8-9-10 (l'Asso fa da carta sotto il Sette). A parità di combinazione e di
//! valori vince chi ha il seme più alto nella carta più alta della combinazione
//! (cuori > quadri > fiori > picche), quindi non ci sono mai pareggi.

use crate::cards::{rank, suit, Card, ACE};

pub const CATEGORY_NAMES: [&str; 9] =
    ["Carta alta", "Coppia", "Doppia coppia", "Tris", "Scala", "Full", "Colore", "Poker", "Scala reale"];

pub const HIGH: u32 = 0;
pub const PAIR: u32 = 1;
pub const TWO_PAIR: u32 = 2;
pub const TRIPS: u32 = 3;
pub const STRAIGHT: u32 = 4;
pub const FULL: u32 = 5;
pub const FLUSH: u32 = 6;
pub const QUADS: u32 = 7;
pub const STRAIGHT_FLUSH: u32 = 8;

/// Valore di una mano: più alto è meglio, mai uguale per due mani diverse.
pub fn evaluate(h: &[Card; 5]) -> u32 {
    let mut count = [0u8; 8];
    let mut top_suit = [0u8; 8];
    for &c in h {
        let r = rank(c) as usize;
        count[r] += 1;
        top_suit[r] = top_suit[r].max(suit(c));
    }
    let flush = h.iter().all(|&c| suit(c) == suit(h[0]));
    // Gruppi ordinati per numerosità e poi per valore, dal più forte.
    let mut groups = [(0u8, 0u8); 5];
    let mut n = 0;
    for r in (0..8u8).rev() {
        if count[r as usize] > 0 {
            groups[n] = (count[r as usize], r);
            n += 1;
        }
    }
    groups[..n].sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let straight_high = if n == 5 {
        if groups[0].1 - groups[4].1 == 4 {
            Some(groups[0].1)
        } else if groups[0].1 == ACE && groups[1].1 == 3 {
            Some(3) // A-7-8-9-10
        } else {
            None
        }
    } else {
        None
    };
    let cat = match (straight_high, flush, groups[0].0, groups[1].0) {
        (Some(_), true, _, _) => STRAIGHT_FLUSH,
        (_, _, 4, _) => QUADS,
        (_, true, _, _) => FLUSH,
        (_, _, 3, 2) => FULL,
        (Some(_), _, _, _) => STRAIGHT,
        (_, _, 3, _) => TRIPS,
        (_, _, 2, 2) => TWO_PAIR,
        (_, _, 2, _) => PAIR,
        _ => HIGH,
    };
    let mut ranks = 0u32;
    let top;
    if let Some(hi) = straight_high {
        ranks = (hi as u32 + 1) << 16;
        top = hi;
    } else {
        for g in &groups[..n] {
            ranks = (ranks << 4) | (g.1 as u32 + 1);
        }
        ranks <<= 4 * (5 - n);
        top = groups[0].1;
    }
    (cat << 24) | (ranks << 4) | top_suit[top as usize] as u32
}

#[inline]
pub fn category(v: u32) -> u32 {
    v >> 24
}

pub fn describe(v: u32) -> &'static str {
    CATEGORY_NAMES[category(v) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{all_hands, parse};

    fn h(s: &str) -> [Card; 5] {
        let v: Vec<Card> = s.split_whitespace().map(|x| parse(x).unwrap()).collect();
        v.try_into().unwrap()
    }

    #[test]
    fn categories_and_order() {
        let sf = evaluate(&h("7c 8c 9c 10c Jc"));
        let quads = evaluate(&h("Ac Aq Af Ap 7c"));
        let flush = evaluate(&h("7c 9c Jc Qc Ac"));
        let full = evaluate(&h("Ac Aq Af Kp Kc"));
        let straight = evaluate(&h("10p Jc Qc Kc Ac"));
        let wheel = evaluate(&h("Ap 7c 8c 9c 10c"));
        let trips = evaluate(&h("7c 7q 7f Kp Ac"));
        assert_eq!(category(sf), STRAIGHT_FLUSH);
        assert_eq!(category(quads), QUADS);
        assert_eq!(category(flush), FLUSH);
        assert_eq!(category(full), FULL);
        assert_eq!(category(straight), STRAIGHT);
        assert_eq!(category(wheel), STRAIGHT);
        assert!(sf > quads && quads > flush && flush > full && full > straight && straight > trips);
        assert!(wheel < evaluate(&h("7p 8c 9c 10c Jc")), "A-7-8-9-10 è la scala più bassa");
    }

    #[test]
    fn suit_breaks_ties() {
        assert!(evaluate(&h("Kc Kp 8c 9f 10p")) > evaluate(&h("Kq Kf 8q 9p 10f")));
        assert!(evaluate(&h("7c 8q 9q 10q Jc")) > evaluate(&h("7p 8p 9p 10p Jq")));
        // kicker prima del seme
        assert!(evaluate(&h("Kp Kf Ap 8f 7f")) > evaluate(&h("Kc Kq Qc 8c 9q")));
    }

    #[test]
    fn no_ties_and_counts() {
        let all = all_hands();
        let mut vals: Vec<u32> = all.iter().map(evaluate).collect();
        let mut cats = [0usize; 9];
        for &v in &vals {
            cats[category(v) as usize] += 1;
        }
        // Conteggi esatti per il mazzo da 32 carte.
        assert_eq!(cats[STRAIGHT_FLUSH as usize], 20);
        assert_eq!(cats[QUADS as usize], 224);
        assert_eq!(cats[FLUSH as usize], 204);
        assert_eq!(cats[FULL as usize], 1344);
        assert_eq!(cats[STRAIGHT as usize], 5 * 1024 - 20);
        assert_eq!(cats[TRIPS as usize], 8 * 4 * 21 * 16);
        // Due mani con lo stesso valore hanno la stessa carta di testa: in una smazzata
        // le mani non hanno carte in comune, quindi non pareggiano mai.
        let mut by_val = std::collections::HashMap::new();
        for (hand, &v) in all.iter().zip(&vals) {
            by_val.entry(v).or_insert_with(Vec::new).push(*hand);
        }
        for group in by_val.values() {
            for a in group {
                for b in group {
                    assert!(a == b || a.iter().any(|c| b.contains(c)), "{a:?} {b:?}");
                }
            }
        }
        vals.sort_unstable();
    }
}
