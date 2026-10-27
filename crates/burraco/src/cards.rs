//! Carte francesi del burraco: due mazzi da 52 più 4 jolly (108 carte).
//!
//! Le due copie di una stessa carta sono indistinguibili per il gioco, quindi si lavora
//! sui "tipi" di carta (0..53) e sulle quantità: `Cards` è un multinsieme.

/// Tipo di carta: `seme * 13 + (valore - 1)` per le carte normali, `JOLLY` per il jolly.
pub type Kind = u8;

pub const JOLLY: Kind = 52;
pub const NUM_KINDS: usize = 53;
pub const DECK_SIZE: usize = 108;

pub const SUIT_NAMES: [&str; 4] = ["cuori", "quadri", "fiori", "picche"];
pub const SUIT_LETTERS: [char; 4] = ['c', 'q', 'f', 'p'];
pub const RANK_LABELS: [&str; 14] = ["", "A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K"];

pub fn kind(suit: u8, rank: u8) -> Kind {
    suit * 13 + rank - 1
}

/// Seme (0 cuori, 1 quadri, 2 fiori, 3 picche). Non ha senso per il jolly.
pub fn suit(k: Kind) -> u8 {
    k / 13
}

/// Valore da 1 (Asso) a 13 (Re); 0 per il jolly.
pub fn rank(k: Kind) -> u8 {
    if k == JOLLY {
        0
    } else {
        k % 13 + 1
    }
}

/// Jolly o pinella (i 2): le matte.
pub fn is_wild(k: Kind) -> bool {
    k == JOLLY || rank(k) == 2
}

pub fn is_red(k: Kind) -> bool {
    k != JOLLY && suit(k) < 2
}

/// Punti della carta: jolly 30, pinella 20, asso 15, dall'8 al Re 10, dal 3 al 7 5.
pub fn value(k: Kind) -> i32 {
    match rank(k) {
        0 => 30,
        2 => 20,
        1 => 15,
        8..=13 => 10,
        _ => 5,
    }
}

/// Nome breve, per esempio "10♥" diventa "10 cuori".
pub fn name(k: Kind) -> String {
    if k == JOLLY {
        "Jolly".to_string()
    } else {
        format!("{} {}", RANK_LABELS[rank(k) as usize], SUIT_NAMES[suit(k) as usize])
    }
}

/// Codice da tastiera: "7c", "Kq", "Af", "10p", "jk" per il jolly.
pub fn code(k: Kind) -> String {
    if k == JOLLY {
        "jk".to_string()
    } else {
        format!("{}{}", RANK_LABELS[rank(k) as usize], SUIT_LETTERS[suit(k) as usize])
    }
}

/// Legge un codice come "7c", "kq", "10p", "jk" (o "*", "jolly").
pub fn parse(text: &str) -> Option<Kind> {
    let t = text.trim().to_lowercase();
    if t == "jk" || t == "*" || t == "jolly" || t == "j*" {
        return Some(JOLLY);
    }
    let s = t.chars().last()?;
    let suit = SUIT_LETTERS.iter().position(|&c| c == s)? as u8;
    let r = &t[..t.len() - 1];
    let rank = match r {
        "a" | "1" => 1,
        "j" | "11" => 11,
        "q" | "12" => 12,
        "k" | "13" => 13,
        _ => r.parse::<u8>().ok().filter(|v| (2..=10).contains(v))?,
    };
    Some(kind(suit, rank))
}

/// Multinsieme di carte: quante copie di ogni tipo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cards {
    pub n: [u8; NUM_KINDS],
    len: u8,
}

impl Default for Cards {
    fn default() -> Self {
        Cards::EMPTY
    }
}

impl Cards {
    pub const EMPTY: Cards = Cards { n: [0; NUM_KINDS], len: 0 };

    /// Le 108 carte dei due mazzi.
    pub fn full_deck() -> Cards {
        let mut c = Cards::EMPTY;
        for k in 0..52 {
            c.n[k] = 2;
        }
        c.n[JOLLY as usize] = 4;
        c.len = DECK_SIZE as u8;
        c
    }

    pub fn from_slice(ks: &[Kind]) -> Cards {
        let mut c = Cards::EMPTY;
        for &k in ks {
            c.add(k);
        }
        c
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn count(&self, k: Kind) -> u8 {
        self.n[k as usize]
    }
    pub fn has(&self, k: Kind) -> bool {
        self.n[k as usize] > 0
    }
    pub fn add(&mut self, k: Kind) {
        self.n[k as usize] += 1;
        self.len += 1;
    }
    pub fn add_n(&mut self, k: Kind, n: u8) {
        self.n[k as usize] += n;
        self.len += n;
    }
    /// Toglie una copia; falso se non c'era.
    pub fn remove(&mut self, k: Kind) -> bool {
        if self.n[k as usize] == 0 {
            return false;
        }
        self.n[k as usize] -= 1;
        self.len -= 1;
        true
    }
    pub fn contains_all(&self, ks: &[Kind]) -> bool {
        let mut c = *self;
        ks.iter().all(|&k| c.remove(k))
    }
    pub fn add_all(&mut self, o: &Cards) {
        for k in 0..NUM_KINDS {
            self.n[k] += o.n[k];
        }
        self.len += o.len;
    }
    /// Toglie le carte di `o` (fino a zero per tipo).
    pub fn minus(&self, o: &Cards) -> Cards {
        let mut c = Cards::EMPTY;
        for k in 0..NUM_KINDS {
            let v = self.n[k].saturating_sub(o.n[k]);
            c.n[k] = v;
            c.len += v;
        }
        c
    }
    /// Una carta per copia, in ordine di tipo.
    pub fn to_vec(&self) -> Vec<Kind> {
        let mut v = Vec::with_capacity(self.len());
        for k in 0..NUM_KINDS {
            for _ in 0..self.n[k] {
                v.push(k as Kind);
            }
        }
        v
    }
    /// I tipi presenti (senza ripetizioni).
    pub fn kinds(&self) -> impl Iterator<Item = Kind> + '_ {
        (0..NUM_KINDS as Kind).filter(move |&k| self.n[k as usize] > 0)
    }
    pub fn wilds(&self) -> u8 {
        self.n[JOLLY as usize] + (0..4).map(|s| self.n[kind(s, 2) as usize]).sum::<u8>()
    }
    /// Somma dei punti delle carte.
    pub fn value(&self) -> i32 {
        (0..NUM_KINDS).map(|k| self.n[k] as i32 * value(k as Kind)).sum()
    }
}

/// Ordine di visualizzazione di una mano: per seme e valore, matte in fondo.
pub fn display_order(k: Kind) -> u16 {
    if k == JOLLY {
        1000
    } else if rank(k) == 2 {
        900 + suit(k) as u16
    } else {
        let r = if rank(k) == 1 { 14 } else { rank(k) };
        suit(k) as u16 * 20 + r as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for k in 0..NUM_KINDS as Kind {
            assert_eq!(parse(&code(k)), Some(k), "{}", code(k));
        }
        assert_eq!(parse("10P"), Some(kind(3, 10)));
        assert_eq!(parse("xx"), None);
        assert_eq!(parse("1c"), Some(kind(0, 1)));
    }

    #[test]
    fn deck_values() {
        let d = Cards::full_deck();
        assert_eq!(d.len(), 108);
        assert_eq!(d.wilds(), 12);
        // 4 jolly da 30, 8 pinelle da 20, 8 assi da 15, 48 figure/8-9-10 da 10, 40 carte basse da 5.
        assert_eq!(d.value(), 4 * 30 + 8 * 20 + 8 * 15 + 48 * 10 + 40 * 5);
    }
}
