//! Mazzo di Scala 40: due mazzi francesi da 52 carte più 4 jolly, 108 carte.
//!
//! Una carta è un `u8`: 0..104 sono le carte naturali (copia * 52 + seme * 13 + valore - 1),
//! 104..108 i jolly. Il "tipo" di una carta ignora la copia: 0..52 per le naturali, 52 per il jolly.

pub type Card = u8;

pub const NUM_CARDS: usize = 108;
pub const JOKER: u8 = 52;
pub const NUM_KINDS: usize = 53;
pub const SUITS: [&str; 4] = ["cuori", "quadri", "fiori", "picche"];

pub fn is_joker(c: Card) -> bool {
    c >= 104
}

/// Tipo della carta: seme * 13 + valore - 1, oppure `JOKER`.
pub fn kind(c: Card) -> u8 {
    if is_joker(c) { JOKER } else { c % 52 }
}

pub fn suit(c: Card) -> u8 {
    (c % 52) / 13
}

/// Valore 1 (asso) ..= 13 (re).
pub fn rank(c: Card) -> u8 {
    c % 13 + 1
}

pub fn kind_of(suit: u8, rank: u8) -> u8 {
    suit * 13 + rank - 1
}

/// Punti di penalità di una carta rimasta in mano: jolly 25, asso 11, figure 10.
pub fn penalty(c: Card) -> u32 {
    kind_penalty(kind(c))
}

pub fn kind_penalty(k: u8) -> u32 {
    if k == JOKER {
        return 25;
    }
    match k % 13 + 1 {
        1 => 11,
        r if r >= 11 => 10,
        r => r as u32,
    }
}

/// Valore di una posizione in una scala (1 = asso basso, 14 = asso alto) per l'apertura.
pub fn pos_value(p: u8) -> u32 {
    match p {
        1 => 1,
        14 => 11,
        11..=13 => 10,
        p => p as u32,
    }
}

/// Valore di una carta di un tris o poker per l'apertura (asso 11, figure 10).
pub fn set_value(rank: u8) -> u32 {
    match rank {
        1 => 11,
        r if r >= 11 => 10,
        r => r as u32,
    }
}

pub fn rank_label(r: u8) -> &'static str {
    ["A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K", "A"][(r - 1) as usize]
}

/// Nome per esteso, es. "7 di cuori", "Jolly".
pub fn name(c: Card) -> String {
    if is_joker(c) {
        "Jolly".into()
    } else {
        format!("{} di {}", rank_label(rank(c)), SUITS[suit(c) as usize])
    }
}

/// Nome breve, es. "7 cuori", "Jolly".
pub fn short(c: Card) -> String {
    if is_joker(c) {
        "Jolly".into()
    } else {
        format!("{} {}", rank_label(rank(c)), SUITS[suit(c) as usize])
    }
}

/// Conteggio per tipo di un insieme di carte.
pub fn counts(cards: &[Card]) -> [u8; NUM_KINDS] {
    let mut n = [0u8; NUM_KINDS];
    for &c in cards {
        n[kind(c) as usize] += 1;
    }
    n
}

/// Una carta del tipo `k` presente in `cards`.
pub fn find_kind(cards: &[Card], k: u8) -> Option<Card> {
    cards.iter().copied().find(|&c| kind(c) == k)
}

/// Una copia del tipo `k` non presente in `used`, se c'è.
pub fn free_copy(k: u8, used: &[Card]) -> Option<Card> {
    let ids: Vec<Card> = if k == JOKER { (104..108).collect() } else { vec![k, k + 52] };
    ids.into_iter().find(|c| !used.contains(c))
}

/// Le due copie di ogni carta naturale e i quattro jolly.
pub fn full_deck() -> Vec<Card> {
    (0..NUM_CARDS as u8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_composition() {
        let n = counts(&full_deck());
        assert!(n[..52].iter().all(|&x| x == 2));
        assert_eq!(n[JOKER as usize], 4);
        assert_eq!(name(kind_of(0, 1)), "A di cuori");
        assert_eq!(penalty(105), 25);
        assert_eq!(penalty(kind_of(2, 12)), 10);
    }
}
