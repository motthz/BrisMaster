//! Scomposizione ottima di una mano in combinazioni, coppie e carte spaiate.
//!
//! La ricerca prende sempre la carta di tipo più basso ancora libera e prova tutti i
//! gruppi che la contengono (le carte di tipo più basso sono già state assegnate), più
//! l'opzione di lasciarla spaiata. Il punteggio dei gruppi lo decide chi chiama, così la
//! stessa ricerca serve per l'apertura (massimo valore calato) e per la valutazione del motore.

use crate::cards::{kind_of, Card, JOKER, NUM_KINDS};
use crate::meld::Meld;

pub type Counts = [u8; NUM_KINDS];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// Tris o poker: valore, semi presenti (bit), jolly.
    Set { rank: u8, suits: u8, joker: bool },
    /// Scala: seme, prima posizione (1 = asso basso, 14 = asso alto), lunghezza, posizione del jolly.
    Run { suit: u8, low: u8, len: u8, joker: Option<u8> },
    /// Due carte dello stesso valore e semi diversi: manca una carta per il tris.
    PairSet { rank: u8, suits: u8 },
    /// Due carte dello stesso seme a distanza 1 o 2 (posizioni a < b).
    PairRun { suit: u8, a: u8, b: u8 },
    Single(u8),
}

fn pos_kind(suit: u8, p: u8) -> u8 {
    kind_of(suit, if p == 14 { 1 } else { p })
}

impl Group {
    pub fn is_meld(&self) -> bool {
        matches!(self, Group::Set { .. } | Group::Run { .. })
    }

    /// Tipi delle carte del gruppo (il jolly è `JOKER`), nelle scale in ordine.
    pub fn kinds(&self) -> Vec<u8> {
        match *self {
            Group::Set { rank, suits, joker } => {
                let mut v: Vec<u8> = (0..4).filter(|s| suits & (1 << s) != 0).map(|s| kind_of(s, rank)).collect();
                if joker {
                    v.push(JOKER);
                }
                v
            }
            Group::Run { suit, low, len, joker } => {
                (low..low + len).map(|p| if Some(p) == joker { JOKER } else { pos_kind(suit, p) }).collect()
            }
            Group::PairSet { rank, suits } => (0..4).filter(|s| suits & (1 << s) != 0).map(|s| kind_of(s, rank)).collect(),
            Group::PairRun { suit, a, b } => vec![pos_kind(suit, a), pos_kind(suit, b)],
            Group::Single(k) => vec![k],
        }
    }

    pub fn len(&self) -> usize {
        match *self {
            Group::Set { suits, joker, .. } => suits.count_ones() as usize + joker as usize,
            Group::Run { len, .. } => len as usize,
            Group::PairSet { .. } | Group::PairRun { .. } => 2,
            Group::Single(_) => 1,
        }
    }

    /// Valore per l'apertura (solo combinazioni).
    pub fn open_value(&self) -> u32 {
        match *self {
            Group::Set { rank, .. } => self.len() as u32 * crate::cards::set_value(rank),
            Group::Run { low, len, .. } => (low..low + len).map(crate::cards::pos_value).sum(),
            _ => 0,
        }
    }

    /// Le carte della mano che formano il gruppo (le toglie da `pool`).
    pub fn take_cards(&self, pool: &mut Vec<Card>) -> Vec<Card> {
        self.kinds()
            .into_iter()
            .map(|k| {
                let i = pool.iter().position(|&c| crate::cards::kind(c) == k).expect("carta del gruppo non in mano");
                pool.swap_remove(i)
            })
            .collect()
    }
}

/// Punteggio dei gruppi per la ricerca (si massimizza la somma).
pub trait Scorer {
    /// Considerare anche le coppie incomplete.
    const PARTIALS: bool;
    fn group(&self, g: &Group) -> f32;
    /// Jolly rimasto fuori da ogni combinazione.
    fn loose_joker(&self) -> f32;
}

#[derive(Clone, Debug, Default)]
pub struct Best {
    pub score: f32,
    pub groups: Vec<Group>,
    /// Jolly rimasti fuori dalle combinazioni.
    pub loose_jokers: u8,
}

struct Search<'a, S: Scorer> {
    n: Counts,
    s: &'a S,
    path: Vec<Group>,
    best: Best,
    leaves: u32,
}

/// Limite alle foglie esplorate: con mani normali non si raggiunge mai.
const MAX_LEAVES: u32 = 200_000;

fn have(n: &Counts, suit: u8, p: u8) -> bool {
    (1..=14).contains(&p) && n[pos_kind(suit, p) as usize] > 0
}

/// Gruppi che contengono una carta di tipo `k`, quando tutte le carte di `n` hanno tipo >= `k`
/// (oppure, per l'asso alto, sono più basse nella scala). Con `partials` anche le coppie.
fn groups_of(n: &mut Counts, k: usize, partials: bool) -> Vec<Group> {
    let mut out = Vec::new();
    let (suit, rank) = (k as u8 / 13, k as u8 % 13 + 1);
    let jokers = n[JOKER as usize] > 0;

    // Tris e poker con gli altri semi (tutti di tipo più alto).
    let others: Vec<u8> = (suit + 1..4).filter(|&s| n[kind_of(s, rank) as usize] > 0).collect();
    for mask in 1u8..(1 << others.len()) {
        let suits = others.iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).fold(1u8 << suit, |m, (_, &s)| m | 1 << s);
        let c = suits.count_ones();
        if c >= 3 {
            out.push(Group::Set { rank, suits, joker: false });
        }
        if jokers && (2..=3).contains(&c) {
            out.push(Group::Set { rank, suits, joker: true });
        }
        if partials && c == 2 {
            out.push(Group::PairSet { rank, suits });
        }
    }

    // Scale in cui questa è la carta naturale più bassa (o, per l'asso alto, la più alta).
    n[k] -= 1;
    let starts: &[(u8, i8)] = if rank == 1 { &[(1, 1), (14, -1)] } else { &[(rank, 1)] };
    for &(p0, dir) in starts {
        let mut misses = 0;
        let mut gap = None;
        let mut q = p0 as i8;
        loop {
            q += dir;
            if !(1..=14).contains(&q) || (p0 == 1 && q == 14) {
                break;
            }
            let q8 = q as u8;
            if !have(n, suit, q8) {
                misses += 1;
                gap = Some(q8);
                if misses > jokers as i32 {
                    break;
                }
                continue;
            }
            let (lo, hi) = if dir > 0 { (p0, q8) } else { (q8, p0) };
            let span = hi - lo + 1;
            if misses == 0 {
                if span >= 3 {
                    out.push(Group::Run { suit, low: lo, len: span, joker: None });
                }
                if jokers {
                    if hi < 14 {
                        out.push(Group::Run { suit, low: lo, len: span + 1, joker: Some(hi + 1) });
                    } else if lo > 1 {
                        out.push(Group::Run { suit, low: lo - 1, len: span + 1, joker: Some(lo - 1) });
                    }
                }
            } else {
                out.push(Group::Run { suit, low: lo, len: span, joker: gap });
            }
            if partials && ((span == 2 && misses == 0) || (span == 3 && misses == 1)) {
                out.push(Group::PairRun { suit, a: lo, b: hi });
            }
        }
    }
    n[k] += 1;
    out
}

impl<S: Scorer> Search<'_, S> {
    fn push(&mut self, g: Group, cur: f32, from: usize) {
        let ks = g.kinds();
        for &k in &ks {
            self.n[k as usize] -= 1;
        }
        self.path.push(g);
        let v = self.s.group(&g);
        self.go(from, cur + v);
        self.path.pop();
        for &k in &ks {
            self.n[k as usize] += 1;
        }
    }

    fn go(&mut self, from: usize, cur: f32) {
        if self.leaves > MAX_LEAVES {
            return;
        }
        let Some(k) = (from..52).find(|&k| self.n[k] > 0) else {
            self.leaves += 1;
            let j = self.n[JOKER as usize];
            let total = cur + j as f32 * self.s.loose_joker();
            if total > self.best.score {
                self.best = Best { score: total, groups: self.path.clone(), loose_jokers: j };
            }
            return;
        };
        for g in groups_of(&mut self.n, k, S::PARTIALS) {
            self.push(g, cur, k);
        }
        self.push(Group::Single(k as u8), cur, k);
    }
}

/// Miglior scomposizione della mano secondo il punteggio `s`.
pub fn best<S: Scorer>(n: &Counts, s: &S) -> Best {
    let mut search =
        Search { n: *n, s, path: Vec::new(), best: Best { score: f32::NEG_INFINITY, ..Default::default() }, leaves: 0 };
    search.go(0, 0.0);
    search.best
}

/// Punteggio per l'apertura: valore delle combinazioni calate.
pub struct OpenValue;

impl Scorer for OpenValue {
    const PARTIALS: bool = false;
    fn group(&self, g: &Group) -> f32 {
        g.open_value() as f32
    }
    fn loose_joker(&self) -> f32 {
        0.0
    }
}

/// Punteggio per calare il più possibile: punti di penalità tolti dalla mano.
pub struct Shed;

impl Scorer for Shed {
    const PARTIALS: bool = false;
    fn group(&self, g: &Group) -> f32 {
        if g.is_meld() {
            g.kinds().iter().map(|&k| crate::cards::kind_penalty(k) as f32).sum::<f32>() + 0.01 * g.len() as f32
        } else {
            0.0
        }
    }
    fn loose_joker(&self) -> f32 {
        0.0
    }
}

/// Le combinazioni di una scomposizione come carte della mano.
pub fn melds_as_cards(hand: &[Card], groups: &[Group]) -> Vec<Vec<Card>> {
    let mut pool = hand.to_vec();
    groups.iter().filter(|g| g.is_meld()).map(|g| g.take_cards(&mut pool)).collect()
}

/// Tutte le combinazioni distinte formabili con la mano che contengono una carta di tipo `k`.
pub fn melds_with(n: &Counts, k: u8) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    for low in 0..52usize {
        if n[low] == 0 {
            continue;
        }
        let mut m = *n;
        for x in m.iter_mut().take(low) {
            *x = 0;
        }
        for g in groups_of(&mut m, low, false) {
            if g.kinds().contains(&k) && !out.contains(&g) {
                out.push(g);
            }
        }
    }
    out
}

/// Massimo valore di apertura con la mano, usando per forza la carta `must` (se data)
/// in una combinazione. Restituisce valore e combinazioni (come carte).
pub fn best_opening(hand: &[Card], must: Option<Card>) -> (u32, Vec<Vec<Card>>) {
    let n = crate::cards::counts(hand);
    let Some(m) = must else {
        let b = best(&n, &OpenValue);
        return (b.score as u32, melds_as_cards(hand, &b.groups));
    };
    let mk = crate::cards::kind(m);
    let mut top = (0, Vec::new());
    for g in melds_with(&n, mk) {
        let mut rest = n;
        for k in g.kinds() {
            rest[k as usize] -= 1;
        }
        let b = best(&rest, &OpenValue);
        let v = g.open_value() + b.score as u32;
        if v > top.0 || top.1.is_empty() {
            let mut groups = vec![g];
            groups.extend(b.groups);
            // La carta obbligata va messa proprio nella combinazione che la contiene.
            let mut pool: Vec<Card> = hand.to_vec();
            let mut first = Vec::new();
            for k in g.kinds() {
                let i = if k == mk {
                    pool.iter().position(|&c| c == m)
                } else {
                    pool.iter().position(|&c| crate::cards::kind(c) == k && c != m)
                }
                .unwrap();
                first.push(pool.swap_remove(i));
            }
            let mut melds = vec![first];
            melds.extend(melds_as_cards(&pool, &groups[1..]));
            top = (v, melds);
        }
    }
    top
}

/// Le combinazioni come `Meld` validi (controllo di coerenza).
pub fn to_melds(groups: &[Vec<Card>], owner: u8) -> Option<Vec<Meld>> {
    groups.iter().map(|g| Meld::new(g, owner)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{counts, kind_of as k};

    #[test]
    fn opening_value() {
        // 10-J-Q di picche (30) + tris di 5 (15) = 45; con il jolly si fa di più.
        let hand = vec![k(3, 10), k(3, 11), k(3, 12), k(0, 5), k(1, 5), k(2, 5), k(1, 2), 104];
        let (v, melds) = best_opening(&hand, None);
        assert_eq!(v, 55, "il jolly allunga la scala con il K");
        assert!(to_melds(&melds, 0).is_some());
        let (v, _) = best_opening(&hand[..7], None);
        assert_eq!(v, 45);
    }

    #[test]
    fn ace_high_and_low() {
        let hand = vec![k(0, 1), k(0, 12), k(0, 13), k(0, 2), k(0, 3), k(0, 1) + 52];
        let b = best(&counts(&hand), &OpenValue);
        assert_eq!(b.score as u32, 31 + 6, "{:?}", b.groups);
    }

    #[test]
    fn forced_card() {
        let hand = vec![k(1, 7), k(1, 8), k(1, 9), k(2, 9), k(3, 9), k(0, 9)];
        let (v, melds) = best_opening(&hand, Some(k(1, 9)));
        assert!(melds[0].contains(&k(1, 9)));
        assert!(v >= 36);
        assert_eq!(melds_with(&counts(&hand), k(1, 9) % 52).len(), 1 + 3 + 1);
    }
}
