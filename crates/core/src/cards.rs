//! Carte del mazzo napoletano/piacentino da 40 carte.
//!
//! Una carta è un `u8` in 0..40: `seme = c / 10`, `valore = c % 10`
//! dove l'indice del valore è 0 = Asso, 1 = Due, 2 = Tre, ..., 6 = Sette,
//! 7 = Fante, 8 = Cavallo, 9 = Re.

pub type Card = u8;

pub const NUM_CARDS: usize = 40;
pub const SUIT_NAMES: [&str; 4] = ["Denari", "Coppe", "Spade", "Bastoni"];
pub const RANK_NAMES: [&str; 10] = [
    "Asso", "Due", "Tre", "Quattro", "Cinque", "Sei", "Sette", "Fante", "Cavallo", "Re",
];
pub const RANK_SHORT: [&str; 10] = ["A", "2", "3", "4", "5", "6", "7", "F", "C", "R"];

/// Punti di ogni valore: Asso 11, Tre 10, Re 4, Cavallo 3, Fante 2, il resto 0.
const POINTS: [u8; 10] = [11, 0, 10, 0, 0, 0, 0, 2, 3, 4];
/// Forza di presa: Asso > Tre > Re > Cavallo > Fante > 7 > 6 > 5 > 4 > 2.
const STRENGTH: [u8; 10] = [9, 0, 8, 1, 2, 3, 4, 5, 6, 7];

#[inline]
pub fn suit(c: Card) -> u8 {
    c / 10
}
#[inline]
pub fn rank(c: Card) -> u8 {
    c % 10
}
#[inline]
pub fn points(c: Card) -> u8 {
    POINTS[rank(c) as usize]
}
#[inline]
pub fn strength(c: Card) -> u8 {
    STRENGTH[rank(c) as usize]
}
#[inline]
pub fn make(suit: u8, rank: u8) -> Card {
    suit * 10 + rank
}

pub fn name(c: Card) -> String {
    format!("{} di {}", RANK_NAMES[rank(c) as usize], SUIT_NAMES[suit(c) as usize])
}

pub fn short(c: Card) -> String {
    format!("{}{}", RANK_SHORT[rank(c) as usize], &SUIT_NAMES[suit(c) as usize][..1])
}

/// Vero se `follow`, giocata dopo `lead`, vince la presa.
#[inline]
pub fn beats(follow: Card, lead: Card, trump: u8) -> bool {
    if suit(follow) == suit(lead) {
        strength(follow) > strength(lead)
    } else {
        suit(follow) == trump
    }
}

/// Insieme di carte come maschera di bit.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct CardSet(pub u64);

impl CardSet {
    pub const EMPTY: CardSet = CardSet(0);
    pub const FULL: CardSet = CardSet((1u64 << NUM_CARDS) - 1);

    #[inline]
    pub fn contains(self, c: Card) -> bool {
        self.0 & (1u64 << c) != 0
    }
    #[inline]
    pub fn insert(&mut self, c: Card) {
        self.0 |= 1u64 << c;
    }
    #[inline]
    pub fn remove(&mut self, c: Card) {
        self.0 &= !(1u64 << c);
    }
    #[inline]
    pub fn len(self) -> u32 {
        self.0.count_ones()
    }
    #[inline]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    #[inline]
    pub fn minus(self, o: CardSet) -> CardSet {
        CardSet(self.0 & !o.0)
    }
    #[inline]
    pub fn union(self, o: CardSet) -> CardSet {
        CardSet(self.0 | o.0)
    }
    pub fn iter(self) -> CardIter {
        CardIter(self.0)
    }
    pub fn to_vec(self) -> Vec<Card> {
        self.iter().collect()
    }
}

pub struct CardIter(u64);

impl Iterator for CardIter {
    type Item = Card;
    #[inline]
    fn next(&mut self) -> Option<Card> {
        if self.0 == 0 {
            None
        } else {
            let c = self.0.trailing_zeros() as Card;
            self.0 &= self.0 - 1;
            Some(c)
        }
    }
}

impl FromIterator<Card> for CardSet {
    fn from_iter<I: IntoIterator<Item = Card>>(it: I) -> Self {
        let mut s = CardSet::EMPTY;
        for c in it {
            s.insert(c);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_points_is_120() {
        let total: u32 = (0..40u8).map(|c| points(c) as u32).sum();
        assert_eq!(total, 120);
    }

    #[test]
    fn trick_rules() {
        let trump = 0; // Denari
        let tre_coppe = make(1, 2);
        let asso_coppe = make(1, 0);
        let re_coppe = make(1, 9);
        let due_denari = make(0, 1);
        let asso_spade = make(2, 0);
        assert!(beats(asso_coppe, tre_coppe, trump));
        assert!(!beats(re_coppe, tre_coppe, trump));
        assert!(beats(due_denari, asso_coppe, trump));
        assert!(!beats(asso_spade, re_coppe, trump));
    }
}
