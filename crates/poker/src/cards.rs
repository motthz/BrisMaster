//! Carte francesi da 52, valutazione delle mani a 7 carte, classi delle mani iniziali.
//!
//! Una carta è un `u8` da 0 a 51: valore = c / 4 (0 = 2, ..., 12 = Asso), seme = c % 4.

pub type Card = u8;

pub const RANKS: &[u8; 13] = b"23456789TJQKA";
pub const SUITS: [char; 4] = ['♠', '♥', '♦', '♣'];
pub const SUIT_LETTERS: &[u8; 4] = b"shdc";

pub fn rank(c: Card) -> u8 {
    c / 4
}
pub fn suit(c: Card) -> u8 {
    c % 4
}
pub fn card(rank: u8, suit: u8) -> Card {
    rank * 4 + suit
}

/// Nome breve, es. "As", "Td", "7c".
pub fn name(c: Card) -> String {
    format!("{}{}", RANKS[rank(c) as usize] as char, SUIT_LETTERS[suit(c) as usize] as char)
}

/// Legge una carta tipo "As", "td", "10h", "7♣".
pub fn parse(text: &str) -> Option<Card> {
    let t = text.trim();
    let (r, s) = if let Some(rest) = t.strip_prefix("10") { ('T', rest) } else {
        let mut it = t.chars();
        (it.next()?.to_ascii_uppercase(), it.as_str())
    };
    let r = RANKS.iter().position(|&x| x as char == r)? as u8;
    let s = match s.trim() {
        "s" | "S" | "♠" | "p" | "P" => 0,
        "h" | "H" | "♥" | "c" | "C" => 1,
        "d" | "D" | "♦" | "q" | "Q" => 2,
        "f" | "F" | "♣" => 3,
        _ => return None,
    };
    Some(card(r, s))
}

/// Insieme di carte come bitmask a 64 bit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CardSet(pub u64);

impl CardSet {
    pub fn add(&mut self, c: Card) {
        self.0 |= 1 << c;
    }
    pub fn has(self, c: Card) -> bool {
        self.0 >> c & 1 == 1
    }
    pub fn of(cards: &[Card]) -> CardSet {
        let mut s = CardSet(0);
        for &c in cards {
            s.add(c);
        }
        s
    }
    pub fn len(self) -> u32 {
        self.0.count_ones()
    }
}

pub const CATEGORY_NAMES: [&str; 9] =
    ["Carta alta", "Coppia", "Doppia coppia", "Tris", "Scala", "Colore", "Full", "Poker", "Scala colore"];

/// Categoria (0 = carta alta, ..., 8 = scala colore) di un punteggio di `eval7`.
pub fn category(score: u32) -> usize {
    (score >> 20) as usize
}

fn top_bits(mut mask: u32, n: usize) -> u32 {
    // I primi `n` valori presenti in `mask`, dal più alto, impacchettati 4 bit ciascuno.
    let mut out = 0;
    let mut k = 0;
    while k < n {
        let r = 31 - mask.leading_zeros();
        out = out << 4 | r;
        mask &= !(1 << r);
        k += 1;
    }
    out << (4 * (5 - n))
}

fn straight_high(mask: u32) -> Option<u32> {
    // Valore più alto di una scala nei bit 0..12, con l'Asso anche basso (A-2-3-4-5).
    let m = mask << 1 | (mask >> 12 & 1);
    let s = m & m >> 1 & m >> 2 & m >> 3 & m >> 4;
    if s == 0 {
        None
    } else {
        // Il bit k di `s` indica una scala che va da k a k+4 nella maschera spostata.
        Some(31 - s.leading_zeros() + 3)
    }
}

/// Punteggio di una mano da 5 a 7 carte: più alto è meglio. Due mani pari hanno lo stesso punteggio.
pub fn eval(cards: &[Card]) -> u32 {
    let mut suits = [0u32; 4];
    let mut counts = [0u8; 13];
    let mut all = 0u32;
    for &c in cards {
        suits[suit(c) as usize] |= 1 << rank(c);
        counts[rank(c) as usize] += 1;
        all |= 1 << rank(c);
    }
    for &sm in &suits {
        if sm.count_ones() >= 5 {
            if let Some(h) = straight_high(sm) {
                return 8 << 20 | h << 16;
            }
            return 5 << 20 | top_bits(sm, 5);
        }
    }
    let (mut quads, mut trips, mut pairs) = (0u32, 0u32, 0u32);
    for r in 0..13 {
        match counts[r] {
            4 => quads |= 1 << r,
            3 => trips |= 1 << r,
            2 => pairs |= 1 << r,
            _ => {}
        }
    }
    if quads != 0 {
        let q = 31 - quads.leading_zeros();
        return 7 << 20 | q << 16 | top_bits(all & !(1 << q), 1) >> 4;
    }
    if trips != 0 {
        let t = 31 - trips.leading_zeros();
        let rest = (trips & !(1 << t)) | pairs;
        if rest != 0 {
            let p = 31 - rest.leading_zeros();
            return 6 << 20 | t << 16 | p << 12;
        }
    }
    if let Some(h) = straight_high(all) {
        return 4 << 20 | h << 16;
    }
    if trips != 0 {
        let t = 31 - trips.leading_zeros();
        return 3 << 20 | t << 16 | top_bits(all & !(1 << t), 2) >> 4;
    }
    if pairs.count_ones() >= 2 {
        let p1 = 31 - pairs.leading_zeros();
        let p2 = 31 - (pairs & !(1 << p1)).leading_zeros();
        return 2 << 20 | p1 << 16 | p2 << 12 | top_bits(all & !(1 << p1) & !(1 << p2), 1) >> 8;
    }
    if pairs != 0 {
        let p = 31 - pairs.leading_zeros();
        return 1 << 20 | p << 16 | top_bits(all & !(1 << p), 3) >> 4;
    }
    top_bits(all, 5)
}

/// Le 169 classi di mani iniziali: coppie, combinazioni dello stesso seme e di semi diversi.
/// Indice: coppia r → r*13+r; suited con alto h e basso l → h*13+l; offsuit → l*13+h.
pub fn preflop_class(a: Card, b: Card) -> usize {
    let (ra, rb) = (rank(a) as usize, rank(b) as usize);
    let (h, l) = if ra >= rb { (ra, rb) } else { (rb, ra) };
    if h == l {
        h * 13 + h
    } else if suit(a) == suit(b) {
        h * 13 + l
    } else {
        l * 13 + h
    }
}

/// Nome di una classe: "AA", "AKs", "T9o".
pub fn class_name(cls: usize) -> String {
    let (i, j) = (cls / 13, cls % 13);
    let (h, l) = if i >= j { (i, j) } else { (j, i) };
    let mut s = format!("{}{}", RANKS[h] as char, RANKS[l] as char);
    if i > j {
        s.push('s');
    } else if i < j {
        s.push('o');
    }
    s
}

/// Nome della combinazione migliore, es. "Doppia coppia".
pub fn describe(cards: &[Card]) -> &'static str {
    CATEGORY_NAMES[category(eval(cards))]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(s: &str) -> Vec<Card> {
        s.split_whitespace().map(|x| parse(x).unwrap()).collect()
    }

    #[test]
    fn categories() {
        assert_eq!(category(eval(&h("As Ks Qs Js Ts 2d 3c"))), 8);
        assert_eq!(category(eval(&h("Ah 2h 3h 4h 5h Kd Kc"))), 8);
        assert_eq!(category(eval(&h("9c 9d 9h 9s 2d 3c 4h"))), 7);
        assert_eq!(category(eval(&h("9c 9d 9h 2s 2d 3c 4h"))), 6);
        assert_eq!(category(eval(&h("9c 9d 9h 2s 2d 2c 4h"))), 6);
        assert_eq!(category(eval(&h("2h 7h 9h Jh Kh Ad Ac"))), 5);
        assert_eq!(category(eval(&h("Ac 2d 3h 4s 5d Kc Kh"))), 4);
        assert_eq!(category(eval(&h("9c 9d 9h 2s 5d Kc 4h"))), 3);
        assert_eq!(category(eval(&h("9c 9d 2h 2s 5d Kc Kh"))), 2);
        assert_eq!(category(eval(&h("9c 9d 2h 3s 5d Kc 7h"))), 1);
        assert_eq!(category(eval(&h("9c Jd 2h 3s 5d Kc 7h"))), 0);
    }

    #[test]
    fn ordering() {
        // Scala alta batte scala con l'Asso basso; kicker; doppia coppia con kicker migliore.
        assert!(eval(&h("6c 2d 3h 4s 5d")) > eval(&h("Ac 2d 3h 4s 5d")));
        assert!(eval(&h("Ac Ad Kh 3s 5d")) > eval(&h("Ac Ad Qh 3s 5d")));
        assert!(eval(&h("Ac Ad Kh Ks 5d 4c 3c")) > eval(&h("Ac Ad Kh Ks 4d 3c 2c")));
        // La terza coppia non conta: AAKK5 in entrambi i casi.
        assert_eq!(eval(&h("Ac Ad Kh Ks 5d 4c 3c")), eval(&h("Ah As Kc Kd 5c 2c 2d")));
        assert!(eval(&h("Ah As Kc Kd Qc 2c 2d")) > eval(&h("Ah As Kc Kd 5c 2c 2d")));
        assert!(eval(&h("2h 2s 2c Kd Kc")) > eval(&h("Ah Ks Qc Jd Tc")));
        assert!(eval(&h("Ah 9h 7h 4h 2h")) > eval(&h("Ks Qs Js 9s 8s")));
    }

    #[test]
    fn all_hands_classes() {
        let mut seen = vec![0u32; 169];
        for a in 0..52 {
            for b in a + 1..52 {
                seen[preflop_class(a, b)] += 1;
            }
        }
        assert!(seen.iter().all(|&n| n == 6 || n == 4 || n == 12));
        assert_eq!(class_name(preflop_class(parse("As").unwrap(), parse("Ks").unwrap())), "AKs");
        assert_eq!(class_name(preflop_class(parse("2c").unwrap(), parse("7d").unwrap())), "72o");
    }

    #[test]
    fn category_frequencies_7_cards() {
        // Frequenze note su 7 carte (campione): carta alta ≈ 17,4%, coppia ≈ 43,8%, doppia coppia ≈ 23,5%.
        use rand::{seq::SliceRandom, SeedableRng};
        let mut rng = rand::rngs::SmallRng::seed_from_u64(1);
        let mut deck: Vec<Card> = (0..52).collect();
        let mut cnt = [0u32; 9];
        let n = 200_000;
        for _ in 0..n {
            deck.partial_shuffle(&mut rng, 7);
            cnt[category(eval(&deck[..7]))] += 1;
        }
        let f = |i: usize| cnt[i] as f64 / n as f64;
        assert!((f(0) - 0.174).abs() < 0.005);
        assert!((f(1) - 0.438).abs() < 0.005);
        assert!((f(2) - 0.235).abs() < 0.005);
        assert!((f(5) - 0.0303).abs() < 0.002);
        assert!((f(4) - 0.0462).abs() < 0.002);
    }
}
