//! Calcolo esatto del valore atteso (EV) di ogni azione, dato quello che resta nel sabot.
//!
//! Il banco gioca in modo meccanico, quindi la distribuzione del suo punteggio finale si
//! calcola esattamente ricorrendo su ogni carta che può pescare, togliendola dal sabot.
//! Il giocatore sceglie a ogni passo l'azione migliore tra stare e chiedere carta
//! (ricorsione con memoria). Gli EV sono in unità della puntata iniziale: -1 = puntata persa.
//!
//! Unica approssimazione: la divisione vale il doppio di una mano che parte dalla carta
//! divisa, senza ridividere e ignorando l'effetto di una mano sull'altra (le divisioni
//! successive le gioca comunque il simulatore). L'errore sul vantaggio del banco è di
//! qualche centesimo di punto percentuale.

use crate::cards::{Counts, Hand};
use crate::rules::Rules;
use crate::strategy::{Action, Legal};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Hasher veloce per chiavi intere (le chiavi qui non vengono dall'esterno).
#[derive(Default)]
pub(crate) struct FxHasher(u64);

impl Hasher for FxHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }
    fn write_u64(&mut self, x: u64) {
        self.0 = (self.0.rotate_left(5) ^ x).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    fn write_u128(&mut self, x: u128) {
        self.write_u64(x as u64);
        self.write_u64((x >> 64) as u64);
    }
}

type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

fn key(c: &Counts, h: Hand) -> u128 {
    let mut k = 0u128;
    for &x in &c.0 {
        k = (k << 8) | x as u128;
    }
    (k << 24) | ((h.hard as u128) << 16) | ((h.has_ace as u128) << 8) | h.cards as u128
}

/// Distribuzione del punteggio finale del banco: 17, 18, 19, 20, 21, sballato, blackjack.
pub type DealerDist = [f64; 7];
pub const BUST: usize = 5;
pub const BJ: usize = 6;

/// EV di ogni azione possibile in una situazione. `None` = azione non permessa.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActionEvs {
    pub stand: f64,
    pub hit: f64,
    pub double: Option<f64>,
    pub split: Option<f64>,
    pub surrender: Option<f64>,
}

impl ActionEvs {
    pub fn get(&self, a: Action) -> Option<f64> {
        match a {
            Action::Stand => Some(self.stand),
            Action::Hit => Some(self.hit),
            Action::Double => self.double,
            Action::Split => self.split,
            Action::Surrender => self.surrender,
        }
    }
    /// Azioni permesse, dalla migliore alla peggiore.
    pub fn ranked(&self) -> Vec<(Action, f64)> {
        let mut v: Vec<(Action, f64)> = Action::ALL.iter().filter_map(|&a| self.get(a).map(|e| (a, e))).collect();
        // A parità si preferisce l'azione più semplice (ordine di Action::ALL).
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        v
    }
    pub fn best(&self) -> (Action, f64) {
        self.ranked()[0]
    }
}

/// Situazione di una mano del giocatore, per sapere quali azioni sono permesse.
#[derive(Clone, Copy, Debug, Default)]
pub struct Situation {
    /// Mani già in gioco per questo giro (1 se non si è diviso).
    pub hands: u8,
    /// La mano viene da una divisione.
    pub from_split: bool,
}

/// Analizzatore per una carta scoperta del banco. Le memorie valgono per qualsiasi mano del
/// giocatore con quella carta scoperta, quindi conviene riusarlo.
pub struct Analyzer {
    pub rules: Rules,
    pub up: usize,
    /// Il giocatore sa già che il banco non ha blackjack (peek con A o 10 scoperto).
    pub no_bj: bool,
    dealer_memo: FxMap<u128, DealerDist>,
    play_memo: FxMap<u128, f64>,
}

impl Analyzer {
    pub fn new(rules: Rules, up: usize) -> Analyzer {
        Analyzer {
            rules,
            up,
            no_bj: rules.peek && (up == 0 || up == 9),
            dealer_memo: FxMap::default(),
            play_memo: FxMap::default(),
        }
    }

    fn dealer_stands(&self, h: Hand) -> bool {
        let t = h.total();
        t > 17 || (t == 17 && !(self.rules.hit_soft17 && h.is_soft()))
    }

    /// Distribuzione finale del banco; `c` sono le carte non viste (compresa quella coperta).
    pub fn dealer(&mut self, c: &Counts) -> DealerDist {
        let start = Hand::default().add(self.up);
        let k = key(c, start);
        if let Some(d) = self.dealer_memo.get(&k) {
            return *d;
        }
        let bj_card = match self.up {
            0 => Some(9),
            9 => Some(0),
            _ => None,
        };
        let mut norm = c.total() as f64;
        if self.no_bj {
            norm -= c.0[bj_card.unwrap()] as f64;
        }
        let mut out = [0.0; 7];
        for r in 0..10 {
            if c.0[r] == 0 || (self.no_bj && Some(r) == bj_card) {
                continue;
            }
            let p = c.0[r] as f64 / norm;
            let h = start.add(r);
            if h.is_blackjack() {
                out[BJ] += p;
            } else {
                let sub = self.dealer_from(&c.removed(r), h);
                for (o, s) in out.iter_mut().zip(sub) {
                    *o += p * s;
                }
            }
        }
        self.dealer_memo.insert(k, out);
        out
    }

    fn dealer_from(&mut self, c: &Counts, h: Hand) -> DealerDist {
        let mut out = [0.0; 7];
        if h.busted() {
            out[BUST] = 1.0;
            return out;
        }
        if self.dealer_stands(h) {
            out[(h.total() - 17) as usize] = 1.0;
            return out;
        }
        let k = key(c, h);
        if let Some(d) = self.dealer_memo.get(&k) {
            return *d;
        }
        let n = c.total() as f64;
        for r in 0..10 {
            if c.0[r] == 0 {
                continue;
            }
            let p = c.0[r] as f64 / n;
            let sub = self.dealer_from(&c.removed(r), h.add(r));
            for (o, s) in out.iter_mut().zip(sub) {
                *o += p * s;
            }
        }
        self.dealer_memo.insert(k, out);
        out
    }

    /// Probabilità che il banco abbia blackjack, vista dal giocatore.
    pub fn p_dealer_bj(&mut self, c: &Counts) -> f64 {
        self.dealer(c)[BJ]
    }

    /// EV dello stare con la mano `h` (non sballata).
    pub fn stand(&mut self, c: &Counts, h: Hand) -> f64 {
        let d = self.dealer(c);
        let t = h.total();
        let mut ev = d[BUST] - d[BJ];
        for (k, &p) in d[..5].iter().enumerate() {
            let dt = 17 + k as u8;
            ev += p * if t > dt {
                1.0
            } else if t < dt {
                -1.0
            } else {
                0.0
            };
        }
        ev
    }

    /// EV del chiedere una carta e poi continuare nel modo migliore (stare o chiedere ancora).
    pub fn hit(&mut self, c: &Counts, h: Hand) -> f64 {
        let n = c.total() as f64;
        let mut ev = 0.0;
        for r in 0..10 {
            if c.0[r] == 0 {
                continue;
            }
            let p = c.0[r] as f64 / n;
            let nh = h.add(r);
            ev += p * if nh.busted() { -1.0 } else { self.play(&c.removed(r), nh) };
        }
        ev
    }

    /// Valore della mano giocata al meglio tra stare e chiedere carta.
    fn play(&mut self, c: &Counts, h: Hand) -> f64 {
        let s = self.stand(c, h);
        if h.total() == 21 {
            return s;
        }
        let k = key(c, h);
        if let Some(&v) = self.play_memo.get(&k) {
            return v;
        }
        let v = s.max(self.hit(c, h));
        self.play_memo.insert(k, v);
        v
    }

    /// EV del raddoppio: una sola carta, puntata doppia.
    pub fn double(&mut self, c: &Counts, h: Hand) -> f64 {
        let n = c.total() as f64;
        let mut ev = 0.0;
        for r in 0..10 {
            if c.0[r] == 0 {
                continue;
            }
            let p = c.0[r] as f64 / n;
            let nh = h.add(r);
            ev += p * if nh.busted() { -1.0 } else { self.stand(&c.removed(r), nh) };
        }
        2.0 * ev
    }

    /// EV della divisione di una coppia di `pair`; `c` non contiene già le due carte divise.
    pub fn split(&mut self, c: &Counts, pair: usize) -> f64 {
        let n = c.total() as f64;
        let start = Hand::default().add(pair);
        let mut one = 0.0;
        for r in 0..10 {
            if c.0[r] == 0 {
                continue;
            }
            let p = c.0[r] as f64 / n;
            let h = start.add(r);
            let cr = c.removed(r);
            let v = if pair == 0 && self.rules.one_card_split_aces {
                self.stand(&cr, h)
            } else {
                let mut v = self.play(&cr, h);
                if self.rules.double_after_split && self.rules.double.allows(h.total(), h.is_soft()) {
                    v = v.max(self.double(&cr, h));
                }
                v
            };
            one += p * v;
        }
        2.0 * one
    }

    /// EV della resa: si perde mezza puntata, o tutta se il banco ha blackjack e non ha
    /// controllato prima.
    pub fn surrender(&mut self, c: &Counts) -> f64 {
        let pbj = self.p_dealer_bj(c);
        -0.5 * (1.0 - pbj) - pbj
    }

    /// EV di tutte le azioni permesse per la mano `cards` (indici di valore).
    /// `c` sono le carte non viste dal giocatore: il sabot più la carta coperta del banco.
    pub fn analyze(&mut self, c: &Counts, cards: &[usize], sit: Situation) -> ActionEvs {
        let h = Hand::of(cards);
        let stand = self.stand(c, h);
        let hit = if h.total() >= 21 { -1.0 } else { self.hit(c, h) };
        let legal = Legal::of(&self.rules, cards, sit);
        if !legal.hit {
            // Asso diviso: ha già ricevuto la sua unica carta.
            return ActionEvs { stand, hit: f64::NEG_INFINITY, double: None, split: None, surrender: None };
        }
        ActionEvs {
            stand,
            hit,
            double: legal.double.then(|| self.double(c, h)),
            split: legal.split.then(|| self.split(c, cards[0])),
            surrender: legal.surrender.then(|| self.surrender(c)),
        }
    }

    /// Libera le memorie (crescono con il numero di situazioni analizzate).
    pub fn clear(&mut self) {
        self.dealer_memo.clear();
        self.play_memo.clear();
    }

    pub fn memo_len(&self) -> usize {
        self.dealer_memo.len() + self.play_memo.len()
    }
}

/// Valore atteso esatto di un giro dal sabot `shoe` per chi gioca ogni prima mano nel modo
/// migliore per la sua composizione (le azioni dopo la prima: le migliori tra stare e carta).
/// È il riferimento teorico: nessuna strategia senza informazioni in più fa meglio.
pub fn round_ev(rules: &Rules, shoe: &Counts) -> f64 {
    let mut total = 0.0;
    let n0 = shoe.total() as f64;
    let threads: Vec<_> = (0..10)
        .map(|up| {
            let rules = *rules;
            let shoe = *shoe;
            std::thread::spawn(move || {
                let mut a = Analyzer::new(rules, up);
                let pu = shoe.0[up] as f64 / n0;
                if pu == 0.0 {
                    return 0.0;
                }
                let s1 = shoe.removed(up);
                let n1 = s1.total() as f64;
                let mut ev = 0.0;
                for p1 in 0..10 {
                    if s1.0[p1] == 0 {
                        continue;
                    }
                    let s2 = s1.removed(p1);
                    let n2 = s2.total() as f64;
                    for p2 in 0..10 {
                        if s2.0[p2] == 0 {
                            continue;
                        }
                        let pp = s1.0[p1] as f64 / n1 * s2.0[p2] as f64 / n2;
                        let c = s2.removed(p2);
                        ev += pp * deal_ev(&mut a, &c, [p1, p2]);
                    }
                }
                pu * ev
            })
        })
        .collect();
    for t in threads {
        total += t.join().unwrap();
    }
    total
}

/// EV di una mano appena distribuita, compresi blackjack e controllo del banco.
pub fn deal_ev(a: &mut Analyzer, c: &Counts, cards: [usize; 2]) -> f64 {
    let pay = a.rules.blackjack_pays;
    let bj_card = match a.up {
        0 => Some(9),
        9 => Some(0),
        _ => None,
    };
    let pbj = bj_card.map_or(0.0, |b| c.0[b] as f64 / c.total() as f64);
    let player_bj = Hand::of(&cards).is_blackjack();
    if player_bj {
        return pay * (1.0 - pbj);
    }
    let best = a.analyze(c, &cards, Situation { hands: 1, from_split: false }).best().1;
    if a.no_bj {
        -pbj + (1.0 - pbj) * best
    } else {
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dealer_dist_sums_to_one() {
        let r = Rules::preset(1);
        for up in 0..10 {
            let mut a = Analyzer::new(r, up);
            let c = Counts::full(8).removed(up);
            let d = a.dealer(&c);
            assert!((d.iter().sum::<f64>() - 1.0).abs() < 1e-9, "{up}: {d:?}");
        }
    }

    #[test]
    fn dealer_bust_rates() {
        // Probabilità note di sballare del banco (S17, 8 mazzi, dopo il controllo):
        // con il 6 scoperto circa 42%, con il 10 circa 21% e con l'asso circa 12%.
        let r = Rules::preset(1);
        let bust = |up: usize| Analyzer::new(r, up).dealer(&Counts::full(8).removed(up))[BUST];
        assert!((bust(5) - 0.42).abs() < 0.01, "{}", bust(5));
        assert!((bust(9) - 0.23).abs() < 0.02, "{}", bust(9));
        assert!((bust(0) - 0.17).abs() < 0.02, "{}", bust(0));
    }

    #[test]
    fn hard_16_vs_10_is_close_call() {
        let r = Rules { surrender: false, ..Rules::preset(1) };
        let mut a = Analyzer::new(r, 9);
        let c = Counts::full(8).removed(9).removed(9).removed(5);
        let e = a.analyze(&c, &[9, 5], Situation { hands: 1, from_split: false });
        // Stare e chiedere valgono entrambi circa -0.54.
        assert!((e.stand - e.hit).abs() < 0.02, "{e:?}");
    }
}
