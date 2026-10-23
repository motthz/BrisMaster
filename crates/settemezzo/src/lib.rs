//! Sette e mezzo contro il banco: regole, strategia ottimale esatta, modelli di banco e simulazioni.
//!
//! Il mazzo è lo stesso da 40 carte della briscola (`briscola_core::cards`).
//! Valori: Asso 1, dal Due al Sette il loro numero, Fante, Cavallo e Re mezzo punto.
//! Con la matta (Re di Denari) quella carta vale quello che conviene: ½ oppure da 1 a 7.
//!
//! I punteggi sono contati in mezzi punti (7½ = 15) per lavorare con numeri interi.

pub mod banco;
pub mod sim;
pub mod solver;

pub use banco::Banco;
pub use solver::{Advice, Dist, Solver};

use briscola_core::cards::{self, Card};

/// 7½ in mezzi punti.
pub const SETTE_E_MEZZO: u8 = 15;
/// Classe delle figure (½ punto). Le classi 1..=7 sono le carte dall'Asso al Sette.
pub const FIGURA: usize = 0;
/// Classe della matta.
pub const MATTA: usize = 8;
pub const CLASSES: usize = 9;
/// Il Re di Denari, la matta.
pub const MATTA_CARD: Card = 9;

/// Regole della variante giocata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// Il Re di Denari è la matta.
    pub matta: bool,
    /// Quante puntate paga il banco per un sette e mezzo reale (7½ con due carte).
    pub reale_paga: u8,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            matta: true,
            reale_paga: 2,
        }
    }
}

impl Rules {
    /// Classe di valore di una carta.
    pub fn class_of(&self, c: Card) -> usize {
        if self.matta && c == MATTA_CARD {
            return MATTA;
        }
        match cards::rank(c) {
            r @ 0..=6 => r as usize + 1,
            _ => FIGURA,
        }
    }

    /// Il mazzo intero diviso per classi.
    pub fn full_deck(&self) -> Deck {
        let mut d = Deck::default();
        for c in 0..40 {
            d.n[self.class_of(c)] += 1;
        }
        d
    }

    /// Le carte non viste: il mazzo intero meno quelle date.
    pub fn unseen(&self, seen: impl IntoIterator<Item = Card>) -> Deck {
        let mut d = self.full_deck();
        for c in seen {
            d.remove(self.class_of(c));
        }
        d
    }
}

/// Valore in mezzi punti di una classe (non della matta).
#[inline]
pub fn half_points(class: usize) -> u8 {
    if class == FIGURA {
        1
    } else {
        2 * class as u8
    }
}

/// Composizione di un insieme di carte per classi di valore.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct Deck {
    pub n: [u8; CLASSES],
}

impl Deck {
    pub fn total(&self) -> u32 {
        self.n.iter().map(|&x| x as u32).sum()
    }
    pub fn remove(&mut self, class: usize) {
        assert!(self.n[class] > 0, "carta già uscita");
        self.n[class] -= 1;
    }
    pub fn without(mut self, class: usize) -> Deck {
        self.remove(class);
        self
    }
    pub fn with(mut self, class: usize) -> Deck {
        self.n[class] += 1;
        self
    }
    /// 4 bit per classe: 36 bit.
    #[inline]
    pub fn pack(&self) -> u64 {
        self.n
            .iter()
            .enumerate()
            .fold(0, |k, (i, &x)| k | (x as u64) << (4 * i))
    }
}

/// Una mano riassunta: somma delle carte normali, presenza della matta, numero di carte.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Debug)]
pub struct Hand {
    /// Somma in mezzi punti delle carte diverse dalla matta.
    pub sum: u8,
    pub matta: bool,
    pub cards: u8,
}

impl Hand {
    pub const EMPTY: Hand = Hand {
        sum: 0,
        matta: false,
        cards: 0,
    };

    pub fn of(rules: &Rules, cs: impl IntoIterator<Item = Card>) -> Hand {
        cs.into_iter()
            .fold(Hand::EMPTY, |h, c| h.add(rules.class_of(c)))
    }

    #[inline]
    pub fn add(self, class: usize) -> Hand {
        if class == MATTA {
            Hand {
                matta: true,
                cards: self.cards + 1,
                ..self
            }
        } else {
            Hand {
                sum: self.sum + half_points(class),
                cards: self.cards + 1,
                ..self
            }
        }
    }

    /// Punteggio migliore in mezzi punti, `None` se la mano ha sballato.
    /// La matta prende il valore più alto che non fa sballare: ½ oppure un intero da 1 a 7.
    #[inline]
    pub fn total(self) -> Option<u8> {
        if !self.matta {
            return (self.sum <= SETTE_E_MEZZO).then_some(self.sum);
        }
        if self.sum >= SETTE_E_MEZZO {
            return None;
        }
        let room = SETTE_E_MEZZO - self.sum;
        let m = if room == 1 { 1 } else { (room & !1).min(14) };
        Some(self.sum + m)
    }

    #[inline]
    pub fn bust(self) -> bool {
        self.total().is_none()
    }

    /// Sette e mezzo reale: 7½ con due sole carte.
    #[inline]
    pub fn reale(self) -> bool {
        self.cards == 2 && self.total() == Some(SETTE_E_MEZZO)
    }

    /// Ordine per il confronto con il banco: il reale batte il 7½ normale.
    #[inline]
    pub fn rank(self) -> u8 {
        self.total().map_or(0, |t| 2 * t + self.reale() as u8)
    }

    /// 5 bit di somma, 1 di matta, 5 di carte: 11 bit.
    #[inline]
    pub fn pack(self) -> u64 {
        (self.sum.min(31) as u64) | (self.matta as u64) << 5 | (self.cards.min(31) as u64) << 6
    }
}

/// Punteggio leggibile: "5", "6½", "½".
pub fn fmt_points(half: u8) -> String {
    match (half / 2, half % 2) {
        (0, 1) => "½".into(),
        (n, 0) => n.to_string(),
        (n, _) => format!("{n}½"),
    }
}

/// Valore di una carta da mostrare all'utente.
pub fn card_value_label(rules: &Rules, c: Card) -> String {
    match rules.class_of(c) {
        MATTA => "matta".into(),
        k => fmt_points(half_points(k)),
    }
}

/// Hasher veloce per le tabelle di memoizzazione (chiavi già ben distribuite).
#[derive(Default, Clone, Copy)]
pub struct KeyHasher(u64);

impl std::hash::Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(8) ^ b as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }
    fn write_u64(&mut self, k: u64) {
        let x = (k ^ (k >> 31)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = x ^ (x >> 29);
    }
}

pub type Memo<V> = std::collections::HashMap<u64, V, std::hash::BuildHasherDefault<KeyHasher>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_composition() {
        let r = Rules::default();
        let d = r.full_deck();
        assert_eq!(d.total(), 40);
        assert_eq!(d.n[FIGURA], 11);
        assert_eq!(d.n[MATTA], 1);
        let r = Rules { matta: false, ..r };
        assert_eq!(r.full_deck().n[FIGURA], 12);
    }

    #[test]
    fn totals_and_matta() {
        let h = Hand::EMPTY.add(7).add(FIGURA);
        assert_eq!(h.total(), Some(15));
        assert!(h.reale());
        assert!(Hand::EMPTY.add(7).add(1).bust());
        // matta con un 5: diventa 2 (7), non 2½.
        assert_eq!(Hand::EMPTY.add(5).add(MATTA).total(), Some(14));
        // matta con un 5 e mezzo: diventa 2 → 7½.
        assert_eq!(Hand::EMPTY.add(5).add(FIGURA).add(MATTA).total(), Some(15));
        // matta da sola: 7.
        assert_eq!(Hand::EMPTY.add(MATTA).total(), Some(14));
        // matta con un 7: vale ½, sette e mezzo reale.
        assert!(Hand::EMPTY.add(7).add(MATTA).reale());
        // matta con 7½: sballa.
        assert!(Hand::EMPTY.add(7).add(FIGURA).add(MATTA).bust());
        assert_eq!(fmt_points(13), "6½");
        assert_eq!(fmt_points(1), "½");
        assert_eq!(fmt_points(8), "4");
    }
}
