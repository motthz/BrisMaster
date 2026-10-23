//! Mazzo francese ridotto da 32 carte: dal 7 all'Asso, quattro semi.
//!
//! Una carta è un `u8` in 0..32: `valore = c / 4` (0 = Sette, ..., 7 = Asso),
//! `seme = c % 4` in ordine di forza crescente: picche, fiori, quadri, cuori
//! ("Come Quando Fuori Piove", al contrario).

pub type Card = u8;

pub const NUM_CARDS: usize = 32;
pub const RANKS: usize = 8;
pub const ACE: u8 = 7;
pub const RANK_SHORT: [&str; RANKS] = ["7", "8", "9", "10", "J", "Q", "K", "A"];
pub const RANK_NAMES: [&str; RANKS] = ["Sette", "Otto", "Nove", "Dieci", "Fante", "Donna", "Re", "Asso"];
pub const SUIT_NAMES: [&str; 4] = ["Picche", "Fiori", "Quadri", "Cuori"];
pub const SUIT_SYMBOLS: [&str; 4] = ["♠", "♣", "♦", "♥"];

#[inline]
pub fn rank(c: Card) -> u8 {
    c >> 2
}
#[inline]
pub fn suit(c: Card) -> u8 {
    c & 3
}
#[inline]
pub fn make(rank: u8, suit: u8) -> Card {
    rank * 4 + suit
}

pub fn name(c: Card) -> String {
    format!("{} di {}", RANK_NAMES[rank(c) as usize], SUIT_NAMES[suit(c) as usize])
}

pub fn short(c: Card) -> String {
    format!("{}{}", RANK_SHORT[rank(c) as usize], SUIT_SYMBOLS[suit(c) as usize])
}

pub fn hand_str(h: &[Card]) -> String {
    h.iter().map(|&c| short(c)).collect::<Vec<_>>().join(" ")
}

/// Legge una carta scritta come `10c`, `Aq`, `Jf`, `7p` (cuori, quadri, fiori, picche),
/// anche con le iniziali italiane delle figure (`F` fante, `D` donna, `R` re) o i simboli.
pub fn parse(s: &str) -> Option<Card> {
    let s = s.trim().to_lowercase();
    let suit_char = s.chars().last()?;
    let suit = match suit_char {
        'p' | '♠' | 's' => 0,
        'f' | '♣' => 1,
        'q' | '♦' | 'd' if s.len() > 1 => 2,
        'c' | '♥' | 'h' => 3,
        _ => return None,
    };
    let r = &s[..s.len() - suit_char.len_utf8()];
    let rank = match r {
        "7" => 0,
        "8" => 1,
        "9" => 2,
        "10" | "t" => 3,
        "j" | "f" => 4,
        "q" | "d" => 5,
        "k" | "r" => 6,
        "a" | "1" => 7,
        _ => return None,
    };
    Some(make(rank, suit))
}

/// Indice combinatorio (colex) di una mano di 5 carte ordinate: 0..C(32,5).
#[inline]
pub fn hand_index(h: &[Card; 5]) -> usize {
    let mut idx = 0;
    for (i, &c) in h.iter().enumerate() {
        idx += binom(c as usize, i + 1);
    }
    idx
}

pub const NUM_HANDS: usize = 201_376;

#[inline]
pub fn binom(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    let mut r = 1usize;
    for i in 0..k {
        r = r * (n - i) / (i + 1);
    }
    r
}

/// Tutte le mani di 5 carte, ordinate, nell'ordine di `hand_index`.
pub fn all_hands() -> Vec<[Card; 5]> {
    let mut v = Vec::with_capacity(NUM_HANDS);
    for e in 4..32u8 {
        for d in 3..e {
            for c in 2..d {
                for b in 1..c {
                    for a in 0..b {
                        v.push([a, b, c, d, e]);
                    }
                }
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_is_a_bijection() {
        let all = all_hands();
        assert_eq!(all.len(), NUM_HANDS);
        for (i, h) in all.iter().enumerate().step_by(97) {
            assert_eq!(hand_index(h), i);
        }
    }

    #[test]
    fn parse_names() {
        assert_eq!(parse("10c"), Some(make(3, 3)));
        assert_eq!(parse("Ap"), Some(make(ACE, 0)));
        assert_eq!(parse("Rq"), Some(make(6, 2)));
        assert_eq!(parse("Df"), Some(make(5, 1)));
        assert_eq!(parse("x"), None);
        for c in 0..32 {
            let s = format!("{}{}", RANK_SHORT[rank(c) as usize], ["p", "f", "q", "c"][suit(c) as usize]);
            assert_eq!(parse(&s), Some(c), "{s}");
        }
    }
}
