//! Combinazioni: tris/poker (stesso valore, semi diversi) e scale (stesso seme, valori consecutivi).
//!
//! In una scala l'asso può stare in basso (A-2-3) o in alto (Q-K-A), mai in mezzo (K-A-2).
//! Le posizioni di una scala vanno da 1 (asso basso) a 14 (asso alto).
//! Ogni combinazione può contenere al massimo un jolly.

use crate::cards::{self, is_joker, rank, suit, Card};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeldKind {
    /// Tris o poker di questo valore.
    Set { rank: u8 },
    /// Scala di questo seme, dalla posizione `low`.
    Run { suit: u8, low: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meld {
    pub kind: MeldKind,
    /// Le carte; nelle scale in ordine di posizione, jolly compreso.
    pub cards: Vec<Card>,
    /// Chi l'ha calata.
    pub owner: u8,
}

/// Posizioni possibili di una carta naturale in una scala.
fn positions(c: Card) -> &'static [u8] {
    const P: [[u8; 2]; 14] =
        [[1, 14], [2, 0], [3, 0], [4, 0], [5, 0], [6, 0], [7, 0], [8, 0], [9, 0], [10, 0], [11, 0], [12, 0], [13, 0], [0, 0]];
    let r = rank(c) as usize;
    if r == 1 { &P[0][..] } else { &P[r - 1][..1] }
}

impl Meld {
    /// Combinazione valida con queste carte, se esiste. Se il jolly può stare a entrambi
    /// gli estremi di una scala lo mette in alto (vale di più per l'apertura).
    pub fn new(cards: &[Card], owner: u8) -> Option<Meld> {
        if cards.len() < 3 {
            return None;
        }
        let jokers = cards.iter().filter(|&&c| is_joker(c)).count();
        if jokers > 1 {
            return None;
        }
        let nat: Vec<Card> = cards.iter().copied().filter(|&c| !is_joker(c)).collect();
        let joker = cards.iter().copied().find(|&c| is_joker(c));
        // Tris o poker.
        if cards.len() <= 4 && nat.iter().all(|&c| rank(c) == rank(nat[0])) {
            let mut seen = 0u8;
            if nat.iter().all(|&c| {
                let b = 1 << suit(c);
                let fresh = seen & b == 0;
                seen |= b;
                fresh
            }) {
                return Some(Meld { kind: MeldKind::Set { rank: rank(nat[0]) }, cards: cards.to_vec(), owner });
            }
        }
        // Scala: stesso seme; si prova l'asso basso e poi l'asso alto.
        let s = suit(nat[0]);
        if nat.iter().any(|&c| suit(c) != s) {
            return None;
        }
        for ace_high in [false, true] {
            let mut pos: Vec<(u8, Card)> =
                nat.iter().map(|&c| (if rank(c) == 1 && ace_high { 14 } else { rank(c) }, c)).collect();
            pos.sort();
            if pos.windows(2).any(|w| w[0].0 == w[1].0) {
                continue;
            }
            let (lo, hi) = (pos[0].0, pos[pos.len() - 1].0);
            let gaps = (hi - lo + 1) as usize - pos.len();
            let (low, jpos) = match (gaps, joker) {
                (0, None) => (lo, None),
                (0, Some(_)) if hi < 14 => (lo, Some(hi + 1)),
                (0, Some(_)) if lo > 1 => (lo - 1, Some(lo - 1)),
                (1, Some(_)) => (lo, (lo..=hi).find(|p| pos.iter().all(|x| x.0 != *p))),
                _ => continue,
            };
            let mut out: Vec<Card> = pos.iter().map(|x| x.1).collect();
            if let (Some(j), Some(p)) = (joker, jpos) {
                out.insert((p - low) as usize, j);
            }
            return Some(Meld { kind: MeldKind::Run { suit: s, low }, cards: out, owner });
        }
        None
    }

    pub fn has_joker(&self) -> bool {
        self.cards.iter().any(|&c| is_joker(c))
    }

    /// Posizione più alta di una scala.
    fn high(&self) -> u8 {
        match self.kind {
            MeldKind::Run { low, .. } => low + self.cards.len() as u8 - 1,
            MeldKind::Set { .. } => 0,
        }
    }

    /// Punti della combinazione per l'apertura (il jolly vale la carta che sostituisce).
    pub fn value(&self) -> u32 {
        match self.kind {
            MeldKind::Set { rank } => self.cards.len() as u32 * cards::set_value(rank),
            MeldKind::Run { low, .. } => (low..low + self.cards.len() as u8).map(cards::pos_value).sum(),
        }
    }

    fn set_suits(&self) -> u8 {
        self.cards.iter().filter(|&&c| !is_joker(c)).fold(0, |m, &c| m | 1 << suit(c))
    }

    /// Dove si attaccherebbe la carta: `Some(true)` in fondo/in coda, `Some(false)` in testa.
    fn attach_side(&self, c: Card) -> Option<bool> {
        match self.kind {
            MeldKind::Set { rank: r } => {
                let ok = self.cards.len() < 4
                    && if is_joker(c) { !self.has_joker() } else { rank(c) == r && self.set_suits() & (1 << suit(c)) == 0 };
                ok.then_some(true)
            }
            MeldKind::Run { suit: s, low } => {
                let high = self.high();
                if is_joker(c) {
                    if self.has_joker() {
                        None
                    } else if high < 14 {
                        Some(true)
                    } else if low > 1 {
                        Some(false)
                    } else {
                        None
                    }
                } else if suit(c) != s {
                    None
                } else if positions(c).contains(&(high + 1)) {
                    Some(true)
                } else if low > 1 && positions(c).contains(&(low - 1)) {
                    Some(false)
                } else {
                    None
                }
            }
        }
    }

    pub fn can_attach(&self, c: Card) -> bool {
        self.attach_side(c).is_some()
    }

    pub fn attach(&mut self, c: Card) -> bool {
        match self.attach_side(c) {
            Some(true) => self.cards.push(c),
            Some(false) => {
                self.cards.insert(0, c);
                if let MeldKind::Run { low, .. } = &mut self.kind {
                    *low -= 1;
                }
            }
            None => return false,
        }
        true
    }

    /// Indice del jolly che la carta naturale `c` può sostituire.
    fn swap_index(&self, c: Card) -> Option<usize> {
        if is_joker(c) {
            return None;
        }
        let j = self.cards.iter().position(|&x| is_joker(x))?;
        match self.kind {
            MeldKind::Set { rank: r } => (rank(c) == r && self.set_suits() & (1 << suit(c)) == 0).then_some(j),
            MeldKind::Run { suit: s, low } => (suit(c) == s && positions(c).contains(&(low + j as u8))).then_some(j),
        }
    }

    pub fn can_swap(&self, c: Card) -> bool {
        self.swap_index(c).is_some()
    }

    /// Mette `c` al posto del jolly e restituisce il jolly.
    pub fn swap(&mut self, c: Card) -> Option<Card> {
        let j = self.swap_index(c)?;
        Some(std::mem::replace(&mut self.cards[j], c))
    }

    /// Descrizione breve, es. "5-6-Jolly-8 di cuori" o "tris di 7 (cuori, fiori, Jolly)".
    pub fn label(&self) -> String {
        match self.kind {
            MeldKind::Run { suit, .. } => format!(
                "{} di {}",
                self.cards
                    .iter()
                    .map(|&c| if is_joker(c) { "Jolly" } else { cards::rank_label(rank(c)) })
                    .collect::<Vec<_>>()
                    .join("-"),
                cards::SUITS[suit as usize]
            ),
            MeldKind::Set { rank: r } => format!(
                "{} di {} ({})",
                if self.cards.len() == 4 { "poker" } else { "tris" },
                cards::rank_label(r),
                self.cards
                    .iter()
                    .map(|&c| if is_joker(c) { "Jolly" } else { cards::SUITS[suit(c) as usize] })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::kind_of;

    fn c(s: u8, r: u8) -> Card {
        kind_of(s, r)
    }
    const J: Card = 104;

    #[test]
    fn sets_and_runs() {
        let m = Meld::new(&[c(0, 7), c(1, 7), c(2, 7)], 0).unwrap();
        assert_eq!(m.value(), 21);
        assert!(Meld::new(&[c(0, 7), c(0, 7) + 52, c(2, 7)], 0).is_none(), "stesso seme due volte");
        let r = Meld::new(&[c(3, 12), c(3, 13), c(3, 1)], 0).unwrap();
        assert_eq!(r.kind, MeldKind::Run { suit: 3, low: 12 });
        assert_eq!(r.value(), 31);
        let r = Meld::new(&[c(3, 1), c(3, 2), c(3, 3)], 0).unwrap();
        assert_eq!(r.value(), 6);
        assert!(Meld::new(&[c(3, 13), c(3, 1), c(3, 2)], 0).is_none(), "niente giro K-A-2");
        let r = Meld::new(&[c(1, 5), J, c(1, 7)], 0).unwrap();
        assert_eq!(r.cards, vec![c(1, 5), J, c(1, 7)]);
        assert_eq!(r.value(), 18);
        let r = Meld::new(&[c(1, 12), c(1, 13), J], 0).unwrap();
        assert_eq!(r.kind, MeldKind::Run { suit: 1, low: 12 });
        assert_eq!(r.value(), 31, "jolly come asso alto");
        assert!(Meld::new(&[c(1, 5), J, 105], 0).is_none(), "un solo jolly");
        assert!(Meld::new(&[c(1, 5), J, c(1, 8)], 0).is_none());
    }

    #[test]
    fn attach_and_swap() {
        let mut r = Meld::new(&[c(1, 5), J, c(1, 7)], 0).unwrap();
        assert!(r.can_attach(c(1, 8)));
        assert!(r.can_attach(c(1, 4)));
        assert!(!r.can_attach(c(1, 6)));
        assert!(!r.can_attach(105));
        assert_eq!(r.swap(c(1, 6)), Some(J));
        assert!(r.attach(c(1, 4)));
        assert_eq!(r.kind, MeldKind::Run { suit: 1, low: 4 });
        let mut s = Meld::new(&[c(0, 9), c(1, 9), J], 0).unwrap();
        assert!(s.can_attach(c(2, 9)));
        assert!(!s.can_attach(c(0, 9) + 52));
        assert!(s.can_swap(c(3, 9)));
        assert!(s.attach(c(2, 9)));
        assert!(!s.can_attach(c(3, 9)), "poker completo");
        let mut a = Meld::new(&[c(2, 11), c(2, 12), c(2, 13)], 0).unwrap();
        assert!(a.attach(c(2, 1)));
        assert_eq!(a.high(), 14);
        assert!(!a.can_attach(c(2, 2)));
    }
}
