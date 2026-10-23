//! Regole del Texas Hold'em No Limit testa a testa (due giocatori).
//!
//! Chip: grande buio = 100, piccolo buio = 50, stack iniziale = 100 grandi bui.
//! Il bottone mette il piccolo buio, parla per primo prima del flop e per ultimo dopo.
//! Ogni mano parte con stack pieni (come ai tavoli cash con ricarica automatica):
//! il risultato si misura in grandi bui vinti per 100 mani (bb/100).

use crate::cards::{eval, Card};
use rand::seq::SliceRandom;
use rand::Rng;

pub const BB: u32 = 100;
pub const SB: u32 = 50;
pub const STACK: u32 = 100 * BB;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Fold,
    /// Check se non c'è niente da pagare, altrimenti call.
    Call,
    /// Bet o raise: porta la propria puntata in questo giro a questo totale.
    RaiseTo(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Logged {
    pub street: u8,
    pub seat: u8,
    pub action: Action,
    /// Piatto (tutte le puntate) prima dell'azione.
    pub pot_before: u32,
    /// Quanto doveva pagare chi agiva.
    pub to_call: u32,
    /// Quanto aveva già puntato in questo giro chi agiva.
    pub bet_before: u32,
}

pub const STREET_NAMES: [&str; 4] = ["Preflop", "Flop", "Turn", "River"];

#[derive(Clone, Debug)]
pub struct Hand {
    pub hole: [[Card; 2]; 2],
    /// Le 5 carte comuni, già estratte: si vedono solo quelle del giro corrente.
    pub board: [Card; 5],
    /// Il posto che ha il bottone (piccolo buio).
    pub button: u8,
    pub stack: [u32; 2],
    /// Chip messe nel piatto in tutta la mano.
    pub committed: [u32; 2],
    /// Chip messe nel giro corrente.
    pub street_bet: [u32; 2],
    pub street: u8,
    pub to_act: u8,
    /// Ultimo incremento di rilancio: il rilancio minimo deve essere almeno così.
    pub last_raise: u32,
    need: [bool; 2],
    pub folded: Option<u8>,
    pub over: bool,
    pub log: Vec<Logged>,
    raises_this_street: u8,
}

impl Hand {
    pub fn deal<R: Rng>(rng: &mut R, button: u8) -> Hand {
        let mut deck: Vec<Card> = (0..52).collect();
        // `partial_shuffle` restituisce le carte estratte a parte (non all'inizio del mazzo).
        let (d, _) = deck.partial_shuffle(rng, 9);
        Hand::with_cards([[d[0], d[1]], [d[2], d[3]]], [d[4], d[5], d[6], d[7], d[8]], button)
    }

    pub fn with_cards(hole: [[Card; 2]; 2], board: [Card; 5], button: u8) -> Hand {
        let bb = 1 - button;
        let mut h = Hand {
            hole,
            board,
            button,
            stack: [STACK; 2],
            committed: [0; 2],
            street_bet: [0; 2],
            street: 0,
            to_act: button,
            last_raise: BB,
            need: [true; 2],
            folded: None,
            over: false,
            log: Vec::new(),
            raises_this_street: 0,
        };
        h.put(button as usize, SB);
        h.put(bb as usize, BB);
        h
    }

    fn put(&mut self, p: usize, x: u32) {
        let x = x.min(self.stack[p]);
        self.stack[p] -= x;
        self.committed[p] += x;
        self.street_bet[p] += x;
    }

    pub fn pot(&self) -> u32 {
        self.committed[0] + self.committed[1]
    }

    pub fn to_call(&self) -> u32 {
        let p = self.to_act as usize;
        self.street_bet[1 - p].saturating_sub(self.street_bet[p]).min(self.stack[p])
    }

    /// Carte comuni visibili nel giro corrente.
    pub fn visible_board(&self) -> &[Card] {
        &self.board[..[0, 3, 4, 5][self.street as usize]]
    }

    pub fn raises_this_street(&self) -> u8 {
        self.raises_this_street
    }

    /// Intervallo del rilancio (totale del giro), se è permesso rilanciare.
    pub fn raise_range(&self) -> Option<(u32, u32)> {
        let p = self.to_act as usize;
        let o = 1 - p;
        let max = self.street_bet[p] + self.stack[p];
        let cur = self.street_bet[o];
        if self.stack[o] == 0 || max <= cur {
            return None;
        }
        let min = (cur + self.last_raise).min(max);
        Some((min, max))
    }

    /// Rende legale un'azione qualsiasi: fold senza puntata diventa check, rilanci fuori misura vengono limitati.
    pub fn legalize(&self, a: Action) -> Action {
        match a {
            Action::Fold if self.to_call() == 0 => Action::Call,
            Action::RaiseTo(x) => match self.raise_range() {
                Some((lo, hi)) => Action::RaiseTo(x.clamp(lo, hi)),
                None => Action::Call,
            },
            a => a,
        }
    }

    pub fn apply(&mut self, a: Action) {
        assert!(!self.over);
        let a = self.legalize(a);
        let p = self.to_act as usize;
        let o = 1 - p;
        self.log.push(Logged { street: self.street, seat: p as u8, action: a, pot_before: self.pot(), to_call: self.to_call(), bet_before: self.street_bet[p] });
        match a {
            Action::Fold => {
                self.folded = Some(p as u8);
                self.over = true;
                return;
            }
            Action::Call => {
                let x = self.to_call();
                self.put(p, x);
                self.need[p] = false;
            }
            Action::RaiseTo(t) => {
                let inc = t - self.street_bet[o];
                if inc >= self.last_raise {
                    self.last_raise = inc;
                }
                let x = t - self.street_bet[p];
                self.put(p, x);
                self.need[p] = false;
                self.need[o] = self.stack[o] > 0;
                self.raises_this_street += 1;
            }
        }
        if self.stack[p] == 0 {
            self.need[p] = false;
        }
        if self.need[o] {
            self.to_act = o as u8;
            return;
        }
        // Giro chiuso.
        if self.stack[0] == 0 || self.stack[1] == 0 || self.street == 3 {
            // Tutti all-in o river finito: si va allo showdown con il board completo.
            self.street = 3.max(self.street);
            self.over = true;
            return;
        }
        self.street += 1;
        self.street_bet = [0; 2];
        self.last_raise = BB;
        self.raises_this_street = 0;
        self.need = [true; 2];
        self.to_act = 1 - self.button;
    }

    /// Il giro in cui i giocatori sono andati all-in (per il calcolo del valore atteso), se c'è stato.
    pub fn allin_street(&self) -> Option<u8> {
        if self.folded.is_some() || !self.over {
            return None;
        }
        if self.stack[0] == 0 || self.stack[1] == 0 {
            self.log.last().map(|l| l.street)
        } else {
            None
        }
    }

    /// Chip vinte dal posto `p` (negative se perse), a mano finita.
    pub fn result(&self, p: usize) -> f64 {
        assert!(self.over);
        let o = 1 - p;
        if let Some(f) = self.folded {
            return if f as usize == p { -(self.committed[p] as f64) } else { self.committed[o] as f64 };
        }
        let mine = showdown_share(self.hole[p], self.hole[o], &self.board);
        mine * self.pot() as f64 - self.committed[p] as f64
    }

    /// Come `result`, ma se i giocatori sono andati all-in prima del river conta il valore atteso
    /// sulle carte ancora da scoprire, invece della carta uscita. Toglie fortuna senza cambiare la media.
    pub fn result_ev(&self, p: usize) -> f64 {
        match self.allin_street() {
            Some(s) if s < 3 => {
                let known = [0, 3, 4, 5][s as usize];
                let eq = exact_equity(self.hole[p], self.hole[1 - p], &self.board[..known]);
                eq * self.pot() as f64 - self.committed[p] as f64
            }
            _ => self.result(p),
        }
    }
}

/// Quota del piatto per `a` contro `b` con il board completo: 1, 0 o 0,5.
pub fn showdown_share(a: [Card; 2], b: [Card; 2], board: &[Card; 5]) -> f64 {
    let mut ca = [0u8; 7];
    let mut cb = [0u8; 7];
    ca[..5].copy_from_slice(board);
    cb[..5].copy_from_slice(board);
    ca[5..].copy_from_slice(&a);
    cb[5..].copy_from_slice(&b);
    let (x, y) = (eval(&ca), eval(&cb));
    if x > y {
        1.0
    } else if x == y {
        0.5
    } else {
        0.0
    }
}

/// Equità esatta di `a` contro `b`, enumerando tutte le carte comuni mancanti.
pub fn exact_equity(a: [Card; 2], b: [Card; 2], board: &[Card]) -> f64 {
    let mut used = crate::cards::CardSet::of(board);
    for &c in a.iter().chain(b.iter()) {
        used.add(c);
    }
    let rest: Vec<Card> = (0..52).filter(|&c| !used.has(c)).collect();
    let need = 5 - board.len();
    let mut full = [0u8; 5];
    full[..board.len()].copy_from_slice(board);
    let mut total = 0.0;
    let mut n = 0u32;
    let mut rec = |full: &mut [u8; 5]| {
        total += showdown_share(a, b, full);
        n += 1;
    };
    match need {
        0 => rec(&mut full),
        1 => {
            for &x in &rest {
                full[4] = x;
                rec(&mut full);
            }
        }
        2 => {
            for i in 0..rest.len() {
                for j in i + 1..rest.len() {
                    full[3] = rest[i];
                    full[4] = rest[j];
                    rec(&mut full);
                }
            }
        }
        _ => {
            // Preflop: 1,7 milioni di board, troppi; meglio un campione deterministico ampio.
            let mut k = 0usize;
            for i in 0..rest.len() {
                for j in i + 1..rest.len() {
                    for l in j + 1..rest.len() {
                        for m in l + 1..rest.len() {
                            for q in m + 1..rest.len() {
                                k += 1;
                                if k % 293 != 0 {
                                    continue;
                                }
                                full[..5].copy_from_slice(&[rest[i], rest[j], rest[l], rest[m], rest[q]]);
                                rec(&mut full);
                            }
                        }
                    }
                }
            }
        }
    }
    total / n as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::parse;
    use rand::SeedableRng;

    fn c(s: &str) -> Card {
        parse(s).unwrap()
    }

    #[test]
    fn blinds_and_order() {
        let mut rng = rand::rngs::SmallRng::seed_from_u64(3);
        let mut h = Hand::deal(&mut rng, 1);
        assert_eq!(h.to_act, 1);
        assert_eq!(h.pot(), 150);
        assert_eq!(h.to_call(), 50);
        assert_eq!(h.raise_range(), Some((200, STACK)));
        h.apply(Action::Call); // limp
        assert_eq!(h.to_act, 0); // il grande buio ha l'opzione
        h.apply(Action::Call); // check
        assert_eq!(h.street, 1);
        assert_eq!(h.to_act, 0); // dopo il flop parla per primo chi non ha il bottone
        h.apply(Action::RaiseTo(100));
        h.apply(Action::RaiseTo(300));
        assert_eq!(h.to_act, 0);
        assert_eq!(h.raise_range(), Some((500, STACK - 100)));
        h.apply(Action::Fold);
        assert!(h.over);
        assert_eq!(h.result(0), -200.0);
        assert_eq!(h.result(1), 200.0);
    }

    #[test]
    fn allin_preflop_goes_to_showdown() {
        let hole = [[c("As"), c("Ah")], [c("Kd"), c("Kc")]];
        let board = [c("2c"), c("7d"), c("9h"), c("Js"), c("3c")];
        let mut h = Hand::with_cards(hole, board, 0);
        h.apply(Action::RaiseTo(STACK));
        h.apply(Action::Call);
        assert!(h.over);
        assert_eq!(h.result(0), STACK as f64);
        assert_eq!(h.allin_street(), Some(0));
        let ev = h.result_ev(0);
        // AA contro KK ≈ 82%.
        assert!((ev / (2.0 * STACK as f64) + 0.5 - 0.82).abs() < 0.02, "{ev}");
        assert!((h.result_ev(0) + h.result_ev(1)).abs() < 1e-6);
    }

    #[test]
    fn deals_are_random() {
        let mut rng = rand::rngs::SmallRng::seed_from_u64(1);
        let mut seen = crate::cards::CardSet(0);
        for _ in 0..200 {
            let h = Hand::deal(&mut rng, 0);
            for &c in h.hole[0].iter().chain(&h.hole[1]).chain(&h.board) {
                seen.add(c);
            }
        }
        assert_eq!(seen.len(), 52);
    }

    #[test]
    fn checks_through_to_river() {
        let mut rng = rand::rngs::SmallRng::seed_from_u64(9);
        let mut h = Hand::deal(&mut rng, 0);
        h.apply(Action::Call);
        h.apply(Action::Call);
        for _ in 0..6 {
            h.apply(Action::Call);
        }
        assert!(h.over && h.folded.is_none());
        assert!((h.result(0) + h.result(1)).abs() < 1e-9);
    }

    #[test]
    fn short_allin_call() {
        let mut rng = rand::rngs::SmallRng::seed_from_u64(5);
        let mut h = Hand::deal(&mut rng, 0);
        h.apply(Action::RaiseTo(300));
        h.apply(Action::RaiseTo(STACK));
        assert_eq!(h.raise_range(), None);
        assert_eq!(h.legalize(Action::RaiseTo(99999)), Action::Call);
        h.apply(Action::Call);
        assert!(h.over);
        assert_eq!(h.pot(), 2 * STACK);
    }
}
