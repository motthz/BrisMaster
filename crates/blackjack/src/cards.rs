//! Carte francesi, composizione del sabot e sabot da cui si distribuisce.

use rand::Rng;

/// Carta francese 0..52: `rank` 0 = asso, 1..9 = 2..10, 10..12 = J, Q, K; `suit` 0..4 = ♠ ♥ ♦ ♣.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Card(pub u8);

impl Card {
    pub fn new(rank: u8, suit: u8) -> Card {
        Card(suit * 13 + rank)
    }
    pub fn rank(self) -> u8 {
        self.0 % 13
    }
    pub fn suit(self) -> u8 {
        self.0 / 13
    }
    /// Indice del valore: 0 = asso, 1..8 = 2..9, 9 = dieci e figure.
    pub fn idx(self) -> usize {
        (self.rank() as usize).min(9)
    }
    pub fn rank_label(self) -> &'static str {
        ["A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K"][self.rank() as usize]
    }
    pub fn suit_symbol(self) -> &'static str {
        ["♠", "♥", "♦", "♣"][self.suit() as usize]
    }
    pub fn is_red(self) -> bool {
        matches!(self.suit(), 1 | 2)
    }
}

/// Etichetta di un indice di valore (0 = A, 9 = 10).
pub fn idx_label(i: usize) -> &'static str {
    ["A", "2", "3", "4", "5", "6", "7", "8", "9", "10"][i]
}

/// Valore in punti di un indice: l'asso vale 1 (l'eventuale +10 lo aggiunge `Hand`).
pub fn idx_value(i: usize) -> u8 {
    i as u8 + 1
}

/// Composizione del sabot: quante carte restano di ogni valore.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Counts(pub [u8; 10]);

impl Counts {
    pub fn full(decks: u8) -> Counts {
        let mut c = [4 * decks; 10];
        c[9] = 16 * decks;
        Counts(c)
    }
    pub fn total(&self) -> u32 {
        self.0.iter().map(|&x| x as u32).sum()
    }
    pub fn remove(&mut self, i: usize) {
        debug_assert!(self.0[i] > 0);
        self.0[i] -= 1;
    }
    pub fn removed(mut self, i: usize) -> Counts {
        self.remove(i);
        self
    }
    /// Conteggio Hi-Lo delle carte uscite rispetto a un sabot pieno da `decks` mazzi.
    pub fn hilo_running(&self, decks: u8) -> i32 {
        let full = Counts::full(decks);
        let gone = |i: usize| (full.0[i] - self.0[i]) as i32;
        (1..=5).map(gone).sum::<i32>() - gone(0) - gone(9)
    }
}

/// Somma di una mano con la gestione dell'asso morbido.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub struct Hand {
    /// Somma con tutti gli assi a 1.
    pub hard: u8,
    pub has_ace: bool,
    pub cards: u8,
}

impl Hand {
    pub fn add(mut self, i: usize) -> Hand {
        self.hard += idx_value(i);
        self.has_ace |= i == 0;
        self.cards += 1;
        self
    }
    pub fn of(idx: &[usize]) -> Hand {
        idx.iter().fold(Hand::default(), |h, &i| h.add(i))
    }
    pub fn total(self) -> u8 {
        if self.is_soft() {
            self.hard + 10
        } else {
            self.hard
        }
    }
    /// Mano morbida: un asso conta 11 senza sballare.
    pub fn is_soft(self) -> bool {
        self.has_ace && self.hard <= 11
    }
    pub fn busted(self) -> bool {
        self.hard > 21
    }
    pub fn is_blackjack(self) -> bool {
        self.cards == 2 && self.total() == 21
    }
}

/// Sabot da cui si distribuisce, con la carta di taglio.
pub struct Shoe {
    pub decks: u8,
    cards: Vec<Card>,
    pos: usize,
    /// Si rimescola quando restano meno di queste carte.
    cut: usize,
    /// Composizione delle carte non ancora distribuite.
    pub remaining: Counts,
}

impl Shoe {
    /// `penetration` è la frazione del sabot giocata prima di rimescolare; 0 = rimescola a ogni mano.
    pub fn new(decks: u8, penetration: f64, rng: &mut impl Rng) -> Shoe {
        let cards: Vec<Card> = (0..decks).flat_map(|_| (0..52).map(Card)).collect();
        let n = cards.len();
        let cut = if penetration <= 0.0 { n } else { ((1.0 - penetration) * n as f64).round() as usize };
        let mut s = Shoe { decks, cards, pos: 0, cut, remaining: Counts::full(decks) };
        s.shuffle(rng);
        s
    }
    /// Rimescola. Il mescolamento vero avviene pescando: ogni carta è scelta a caso tra quelle
    /// rimaste (Fisher-Yates un passo alla volta), così rimescolare a ogni mano costa niente.
    /// Le carte ripartono dall'ordine di fabbrica: con lo stesso generatore casuale si
    /// ottengono le stesse carte (serve ai confronti appaiati tra strategie).
    pub fn shuffle(&mut self, _rng: &mut impl Rng) {
        for (i, c) in self.cards.iter_mut().enumerate() {
            *c = Card((i % 52) as u8);
        }
        self.pos = 0;
        self.remaining = Counts::full(self.decks);
    }
    pub fn len(&self) -> usize {
        self.cards.len()
    }
    pub fn left(&self) -> usize {
        self.cards.len() - self.pos
    }
    /// Da chiamare prima di ogni mano: rimescola se si è passata la carta di taglio.
    pub fn needs_shuffle(&self) -> bool {
        self.left() <= self.cut.max(20)
    }
    pub fn draw(&mut self, rng: &mut impl Rng) -> Card {
        if self.pos >= self.cards.len() {
            // Sabot esaurito a metà mano (solo con penetrazione estrema): si rimescola.
            self.shuffle(rng);
        }
        let j = rng.gen_range(self.pos..self.cards.len());
        self.cards.swap(self.pos, j);
        let c = self.cards[self.pos];
        self.pos += 1;
        self.remaining.remove(c.idx());
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_totals() {
        assert_eq!(Hand::of(&[0, 9]).total(), 21);
        assert!(Hand::of(&[0, 9]).is_blackjack());
        assert_eq!(Hand::of(&[0, 0]).total(), 12);
        assert_eq!(Hand::of(&[0, 5, 9]).total(), 17);
        assert!(!Hand::of(&[0, 5, 9]).is_soft());
        assert!(Hand::of(&[9, 9, 1]).busted());
    }

    #[test]
    fn full_counts() {
        assert_eq!(Counts::full(6).total(), 312);
        assert_eq!(Counts::full(6).hilo_running(6), 0);
    }
}
