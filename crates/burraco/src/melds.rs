//! Combinazioni: scale (stesso seme, valori consecutivi) e tris (stesso valore).
//!
//! Regole: almeno 3 carte, al massimo una matta (jolly o pinella) per combinazione.
//! La pinella dello stesso seme al posto del 2 conta come carta naturale.
//! Nelle scale l'asso sta sotto il 2 o sopra il Re (posizioni 1 e 14), mai in mezzo.
//! Quando si attacca la carta naturale al posto della matta, la matta si sposta in fondo alla scala.

use crate::cards::{self, kind, rank, suit, Kind, JOLLY};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Meld {
    /// Scala (true) o tris (false).
    pub seq: bool,
    /// Seme della scala.
    pub suit: u8,
    /// Valore del tris.
    pub rank: u8,
    /// Scala: posizioni occupate da carte naturali (bit 1..=14, 14 = asso sopra il Re).
    pub mask: u16,
    /// Tris: carte naturali per seme.
    pub suits: [u8; 4],
    /// Numero di carte naturali.
    pub nat: u8,
    pub wild: Option<Kind>,
    /// Scala: posizione che la matta sostituisce (0 se non c'è matta).
    pub wild_pos: u8,
}

/// Punti della carta naturale in una posizione di scala.
fn pos_value(p: u8) -> i32 {
    match p {
        1 | 14 => 15,
        2 => 20,
        3..=7 => 5,
        _ => 10,
    }
}

fn pos_kind(s: u8, p: u8) -> Kind {
    kind(s, if p == 14 { 1 } else { p })
}

fn bounds(mask: u16) -> (u8, u8) {
    (mask.trailing_zeros() as u8, 15 - mask.leading_zeros() as u8)
}

/// Dove va la matta in una scala con queste carte naturali; None se la scala non è valida.
fn arrange(mask: u16, wild: Option<Kind>, prev: u8) -> Option<u8> {
    if mask == 0 {
        return None;
    }
    let (lo, hi) = bounds(mask);
    let gaps = (hi - lo + 1) as u32 - mask.count_ones();
    match (gaps, wild) {
        (0, None) => Some(0),
        (0, Some(_)) => {
            if prev != 0 && (prev + 1 == lo || prev == hi + 1) && (1..=14).contains(&prev) {
                Some(prev)
            } else if hi < 14 {
                Some(hi + 1)
            } else if lo > 1 {
                Some(lo - 1)
            } else {
                None
            }
        }
        (1, Some(_)) => (lo..=hi).find(|&p| mask & (1 << p) == 0),
        _ => None,
    }
}

impl Meld {
    pub fn len(&self) -> usize {
        self.nat as usize + self.wild.is_some() as usize
    }

    pub fn is_burraco(&self) -> bool {
        self.len() >= 7
    }

    pub fn is_clean(&self) -> bool {
        self.wild.is_none()
    }

    /// Premio del burraco: 200 pulito, 100 sporco.
    pub fn bonus(&self) -> i32 {
        match (self.is_burraco(), self.is_clean()) {
            (false, _) => 0,
            (true, true) => 200,
            (true, false) => 100,
        }
    }

    /// Punti delle carte della combinazione.
    pub fn value(&self) -> i32 {
        let w = self.wild.map_or(0, cards::value);
        if self.seq {
            (1..=14).filter(|p| self.mask & (1 << p) != 0).map(pos_value).sum::<i32>() + w
        } else {
            self.nat as i32 * cards::value(kind(0, self.rank)) + w
        }
    }

    /// Estremi della scala, matta compresa.
    pub fn range(&self) -> (u8, u8) {
        let (mut lo, mut hi) = bounds(self.mask);
        if self.wild_pos != 0 {
            lo = lo.min(self.wild_pos);
            hi = hi.max(self.wild_pos);
        }
        (lo, hi)
    }

    /// Le carte in ordine di visualizzazione (la matta al suo posto nelle scale).
    pub fn cards(&self) -> Vec<Kind> {
        let mut v = Vec::with_capacity(self.len());
        if self.seq {
            let (lo, hi) = self.range();
            for p in lo..=hi {
                if self.mask & (1 << p) != 0 {
                    v.push(pos_kind(self.suit, p));
                } else if let Some(w) = self.wild {
                    v.push(w);
                }
            }
        } else {
            for s in 0..4 {
                for _ in 0..self.suits[s as usize] {
                    v.push(kind(s, self.rank));
                }
            }
            v.extend(self.wild);
        }
        v
    }

    /// Nuova combinazione da queste carte, se valida.
    pub fn new(ks: &[Kind]) -> Option<Meld> {
        if ks.len() < 3 {
            return None;
        }
        let core: Vec<Kind> = ks.iter().copied().filter(|&k| !cards::is_wild(k)).collect();
        let first = *core.first()?;
        if core.iter().all(|&k| rank(k) == rank(first)) && core.len() >= 2 {
            let empty = Meld {
                seq: false,
                suit: 0,
                rank: rank(first),
                mask: 0,
                suits: [0; 4],
                nat: 0,
                wild: None,
                wild_pos: 0,
            };
            return empty.with(ks);
        }
        let empty =
            Meld { seq: true, suit: suit(first), rank: 0, mask: 0, suits: [0; 4], nat: 0, wild: None, wild_pos: 0 };
        empty.with(ks).filter(|m| m.len() >= 3)
    }

    /// La combinazione con queste carte in più, se valida.
    pub fn with(&self, ks: &[Kind]) -> Option<Meld> {
        if self.seq {
            self.seq_with(ks)
        } else {
            self.group_with(ks)
        }
    }

    /// Le carte naturali che si possono attaccare (al più 4).
    pub fn natural_targets(&self) -> impl Iterator<Item = Kind> {
        let mut t = [JOLLY; 4];
        if self.seq {
            let (lo, hi) = self.range();
            let cand = [lo.wrapping_sub(1), hi + 1, if self.wild_pos != 0 { self.wild_pos } else { 0 }];
            for (i, p) in cand.into_iter().enumerate() {
                if (1..=14).contains(&p) && p != 2 && self.mask & (1 << p) == 0 {
                    t[i] = pos_kind(self.suit, p);
                }
            }
        } else {
            for s in 0..4 {
                t[s as usize] = kind(s, self.rank);
            }
        }
        t.into_iter().filter(|&k| k != JOLLY)
    }

    /// La combinazione con una carta naturale in più (veloce, senza allocazioni).
    /// Per le pinelle e il jolly si usa `with`.
    pub fn with_natural(&self, k: Kind) -> Option<Meld> {
        debug_assert!(!cards::is_wild(k));
        if !self.seq {
            if rank(k) != self.rank {
                return None;
            }
            let mut m = *self;
            m.suits[suit(k) as usize] += 1;
            m.nat += 1;
            return Some(m);
        }
        if suit(k) != self.suit {
            return None;
        }
        let try_pos = |p: u8| -> Option<Meld> {
            let b = 1u16 << p;
            if self.mask & b != 0 {
                return None;
            }
            let mask = self.mask | b;
            let wp = arrange(mask, self.wild, self.wild_pos)?;
            Some(Meld { mask, nat: self.nat + 1, wild_pos: wp, ..*self })
        };
        if rank(k) == 1 {
            try_pos(1).or_else(|| try_pos(14))
        } else {
            try_pos(rank(k))
        }
    }

    fn group_with(&self, ks: &[Kind]) -> Option<Meld> {
        let mut m = *self;
        for &k in ks {
            if cards::is_wild(k) {
                if m.wild.is_some() {
                    return None;
                }
                m.wild = Some(k);
            } else {
                if rank(k) != m.rank {
                    return None;
                }
                m.suits[suit(k) as usize] += 1;
                m.nat += 1;
            }
        }
        (m.nat >= 2 && m.len() >= 3).then_some(m)
    }

    fn seq_with(&self, ks: &[Kind]) -> Option<Meld> {
        let s = self.suit;
        let mut mask = self.mask;
        let mut wilds: Vec<Kind> = self.wild.into_iter().collect();
        let mut aces = 0;
        let mut twos = 0;
        for &k in ks {
            if k == JOLLY || (rank(k) == 2 && suit(k) != s) {
                wilds.push(k);
            } else if suit(k) != s {
                return None;
            } else if rank(k) == 1 {
                aces += 1;
            } else if rank(k) == 2 {
                twos += 1;
            } else {
                let b = 1u16 << rank(k);
                if mask & b != 0 {
                    return None;
                }
                mask |= b;
            }
        }
        if aces > 2 || wilds.len() + twos > 2 + 1 {
            return None;
        }
        let ace_opts: &[u16] = match aces {
            0 => &[0],
            1 => &[1 << 1, 1 << 14],
            _ => &[(1 << 1) | (1 << 14)],
        };
        let mut best: Option<Meld> = None;
        for &a in ace_opts {
            if mask & a != 0 {
                continue;
            }
            // La pinella dello stesso seme: prima come 2 naturale, poi come matta.
            for natural_two in [true, false] {
                if twos == 0 && natural_two {
                    continue;
                }
                let mut mk = mask | a;
                let mut w = wilds.clone();
                let mut extra = twos;
                if natural_two {
                    if mk & (1 << 2) != 0 {
                        continue;
                    }
                    mk |= 1 << 2;
                    extra -= 1;
                }
                for _ in 0..extra {
                    w.push(kind(s, 2));
                }
                if w.len() > 1 {
                    continue;
                }
                let wild = w.first().copied();
                let prev = if wild == self.wild { self.wild_pos } else { 0 };
                let Some(mut wp) = arrange(mk, wild, prev) else { continue };
                let mut wild = wild;
                // Pinella dello stesso seme finita al posto del 2: diventa naturale.
                if wp == 2 && wild == Some(kind(s, 2)) {
                    mk |= 1 << 2;
                    wild = None;
                    wp = 0;
                }
                let m = Meld { mask: mk, nat: mk.count_ones() as u8, wild, wild_pos: wp, ..*self };
                if best.is_none_or(|b| m.is_clean() && !b.is_clean()) {
                    best = Some(m);
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::parse;

    fn ks(s: &str) -> Vec<Kind> {
        s.split_whitespace().map(|c| parse(c).unwrap()).collect()
    }

    #[test]
    fn sequences() {
        let m = Meld::new(&ks("5c 6c 7c")).unwrap();
        assert!(m.seq && m.is_clean() && m.len() == 3 && m.value() == 15);
        let m = Meld::new(&ks("5c jk 7c")).unwrap();
        assert_eq!(m.wild_pos, 6);
        assert_eq!(m.cards(), ks("5c jk 7c"));
        // Mettendo il 6 naturale la matta va in fondo.
        let m2 = m.with(&ks("6c")).unwrap();
        assert_eq!(m2.cards(), ks("5c 6c 7c jk"));
        assert!(Meld::new(&ks("5c 6q 7c")).is_none());
        assert!(Meld::new(&ks("5c jk 2q")).is_none());
        assert!(Meld::new(&ks("5c 8c 9c")).is_none());
        // Asso sopra il Re e sotto il 2.
        assert!(Meld::new(&ks("Qp Kp Ap")).is_some());
        assert!(Meld::new(&ks("Ap 3p 4p 2p")).unwrap().is_clean());
        assert!(Meld::new(&ks("Kp Ap 3p")).is_none());
        assert_eq!(Meld::new(&ks("Kp Ap 2p")).unwrap().wild_pos, 12);
        // Pinella dello stesso seme come 2 naturale.
        let m = Meld::new(&ks("2c 3c 4c")).unwrap();
        assert!(m.is_clean());
        assert_eq!(m.value(), 30);
        // ... oppure come matta.
        let m = Meld::new(&ks("2c 6c 7c")).unwrap();
        assert_eq!(m.wild, Some(parse("2c").unwrap()));
    }

    #[test]
    fn groups_and_burraco() {
        let m = Meld::new(&ks("7c 7c 7p")).unwrap();
        assert!(!m.seq && m.value() == 15);
        let m = m.with(&ks("2f")).unwrap();
        assert!(m.with(&ks("jk")).is_none());
        assert!(Meld::new(&ks("7c jk 2p")).is_none());
        let b = Meld::new(&ks("3q 4q 5q 6q 7q 8q 9q")).unwrap();
        assert_eq!(b.bonus(), 200);
        let d = Meld::new(&ks("3q 4q 5q jk 7q 8q 9q")).unwrap();
        assert_eq!(d.bonus(), 100);
        let full = Meld::new(&ks("Ac 3c 4c 5c 6c 7c 8c 9c 10c Jc Qc Kc jk")).unwrap();
        assert_eq!(full.len(), 13);
        assert!(full.with(&ks("2c")).is_some());
    }

    #[test]
    fn fast_natural_matches_general() {
        let melds = [
            Meld::new(&ks("5c jk 7c")).unwrap(),
            Meld::new(&ks("Qp Kp Ap")).unwrap(),
            Meld::new(&ks("3f 4f 5f")).unwrap(),
            Meld::new(&ks("9q 9q 9c 2f")).unwrap(),
            Meld::new(&ks("Ac 3c 4c 5c 6c 7c 8c 9c 10c Jc Qc Kc jk")).unwrap(),
        ];
        for m in melds {
            for k in 0..52u8 {
                if cards::is_wild(k) {
                    continue;
                }
                assert_eq!(m.with_natural(k), m.with(&[k]), "{:?} + {}", m.cards(), cards::name(k));
            }
        }
    }
}
