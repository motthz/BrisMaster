//! Una mano di poker all'italiana a due giocatori, a limite fisso.
//!
//! Ognuno mette un cip di invito (1 fiche) e riceve 5 carte coperte. Primo giro di
//! puntate (puntata e rilanci da 2), cambio (si scartano da 0 a 5 carte e se ne pescano
//! altrettante), secondo giro di puntate (puntata e rilanci da 4), poi si confrontano
//! le mani. Al massimo 3 puntate per giro (puntata, rilancio, controrilancio). Il posto 0
//! parla e cambia per primo in entrambi i giri, il posto 1 (il mazziere) per secondo.

use crate::cards::{Card, NUM_CARDS};
use crate::draw::replace;
use crate::eval::evaluate;

pub const ANTE: i32 = 1;
pub const SMALL_BET: i32 = 2;
pub const BIG_BET: i32 = 4;
pub const CAP: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Fold,
    /// Parola (se non c'è niente da pagare) o vedo.
    Call,
    /// Punto (se non c'è niente da pagare) o rilancio.
    Raise,
    /// Cambio: maschera delle posizioni scartate nella mano ordinata. Nella storia
    /// vista dai giocatori resta solo il numero di carte cambiate (`Draw(k)`).
    Draw(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Bet1,
    Draw,
    Bet2,
    Over,
}

#[derive(Clone, Debug)]
pub struct Hand {
    pub deck: [Card; NUM_CARDS],
    pub hands: [[Card; 5]; 2],
    /// Mani prima del cambio.
    pub dealt: [[Card; 5]; 2],
    pub next: usize,
    pub phase: Phase,
    pub to_act: u8,
    pub contrib: [i32; 2],
    pub bets: u8,
    acted: u8,
    /// Storia pubblica: i cambi compaiono come `Draw(numero di carte)`.
    pub history: Vec<Action>,
    pub discards: [Option<u8>; 2],
    pub folded: Option<u8>,
}

impl Hand {
    /// Distribuisce dal mazzo `deck` (già mescolato): carte alternate, prima al posto 0.
    pub fn new(deck: [Card; NUM_CARDS]) -> Hand {
        let mut hands = [[0; 5]; 2];
        for i in 0..5 {
            hands[0][i] = deck[2 * i];
            hands[1][i] = deck[2 * i + 1];
        }
        hands[0].sort_unstable();
        hands[1].sort_unstable();
        Hand {
            deck,
            hands,
            dealt: hands,
            next: 10,
            phase: Phase::Bet1,
            to_act: 0,
            contrib: [ANTE; 2],
            bets: 0,
            acted: 0,
            history: Vec::new(),
            discards: [None; 2],
            folded: None,
        }
    }

    pub fn is_over(&self) -> bool {
        self.phase == Phase::Over
    }

    pub fn pot(&self) -> i32 {
        self.contrib[0] + self.contrib[1]
    }

    pub fn bet_size(&self) -> i32 {
        if self.phase == Phase::Bet1 { SMALL_BET } else { BIG_BET }
    }

    /// Fiche da mettere per vedere.
    pub fn to_call(&self) -> i32 {
        let p = self.to_act as usize;
        self.contrib[1 - p] - self.contrib[p]
    }

    /// Azioni di puntata possibili (nel cambio vale ogni maschera 0..32).
    pub fn legal(&self) -> Vec<Action> {
        match self.phase {
            Phase::Bet1 | Phase::Bet2 => {
                let mut v = Vec::with_capacity(3);
                if self.to_call() > 0 {
                    v.push(Action::Fold);
                }
                v.push(Action::Call);
                if self.bets < CAP {
                    v.push(Action::Raise);
                }
                v
            }
            Phase::Draw => (0..32).map(Action::Draw).collect(),
            Phase::Over => Vec::new(),
        }
    }

    pub fn apply(&mut self, a: Action) {
        let p = self.to_act as usize;
        match (self.phase, a) {
            (Phase::Bet1 | Phase::Bet2, Action::Fold) => {
                assert!(self.to_call() > 0, "lasciare senza puntata da pagare");
                self.history.push(a);
                self.folded = Some(p as u8);
                self.phase = Phase::Over;
            }
            (Phase::Bet1 | Phase::Bet2, Action::Call) => {
                let facing = self.to_call() > 0;
                self.contrib[p] = self.contrib[1 - p];
                self.history.push(a);
                self.acted += 1;
                if facing || self.acted >= 2 {
                    self.end_round();
                } else {
                    self.to_act = 1 - p as u8;
                }
            }
            (Phase::Bet1 | Phase::Bet2, Action::Raise) => {
                assert!(self.bets < CAP, "troppi rilanci");
                self.contrib[p] = self.contrib[1 - p] + self.bet_size();
                self.bets += 1;
                self.acted += 1;
                self.history.push(a);
                self.to_act = 1 - p as u8;
            }
            (Phase::Draw, Action::Draw(mask)) => {
                assert!(mask < 32);
                let k = mask.count_ones() as usize;
                let new = &self.deck[self.next..self.next + k];
                self.hands[p] = replace(&self.hands[p], mask, new);
                self.next += k;
                self.discards[p] = Some(mask);
                self.history.push(Action::Draw(k as u8));
                if p == 0 {
                    self.to_act = 1;
                } else {
                    self.phase = Phase::Bet2;
                    self.to_act = 0;
                    self.bets = 0;
                    self.acted = 0;
                }
            }
            _ => panic!("azione {a:?} non valida nella fase {:?}", self.phase),
        }
    }

    fn end_round(&mut self) {
        self.bets = 0;
        self.acted = 0;
        self.to_act = 0;
        self.phase = if self.phase == Phase::Bet1 { Phase::Draw } else { Phase::Over };
    }

    /// Chi vince il piatto (a mano finita).
    pub fn winner(&self) -> u8 {
        match self.folded {
            Some(p) => 1 - p,
            None => {
                if evaluate(&self.hands[0]) > evaluate(&self.hands[1]) { 0 } else { 1 }
            }
        }
    }

    /// Guadagno netto in fiche di ciascun posto (a mano finita).
    pub fn payoff(&self) -> [i32; 2] {
        let w = self.winner() as usize;
        let mut r = [0; 2];
        r[w] = self.contrib[1 - w];
        r[1 - w] = -self.contrib[1 - w];
        r
    }

    pub fn showdown(&self) -> bool {
        self.is_over() && self.folded.is_none()
    }
}

/// Quello che vede un giocatore quando deve decidere.
#[derive(Clone, Debug)]
pub struct Observation {
    pub seat: u8,
    pub hand: [Card; 5],
    pub phase: Phase,
    pub history: Vec<Action>,
    pub to_call: i32,
    pub can_raise: bool,
    pub pot: i32,
    /// Carte cambiate dall'avversario, se ha già cambiato.
    pub opp_drawn: Option<u8>,
    pub my_drawn: Option<u8>,
}

impl Observation {
    pub fn of(h: &Hand, seat: u8) -> Observation {
        let k = |p: usize| h.discards[p].map(|m| m.count_ones() as u8);
        Observation {
            seat,
            hand: h.hands[seat as usize],
            phase: h.phase,
            history: h.history.clone(),
            to_call: h.contrib[1 - seat as usize] - h.contrib[seat as usize],
            can_raise: h.bets < CAP,
            pot: h.pot(),
            opp_drawn: k(1 - seat as usize),
            my_drawn: k(seat as usize),
        }
    }

    /// Puntate fatte in questo giro (0 = nessuna).
    pub fn bets_this_round(&self) -> usize {
        let start = match self.phase {
            Phase::Bet2 => self.history.iter().rposition(|a| matches!(a, Action::Draw(_))).map_or(0, |i| i + 1),
            _ => 0,
        };
        self.history[start..].iter().filter(|a| **a == Action::Raise).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck() -> [Card; NUM_CARDS] {
        core::array::from_fn(|i| i as Card)
    }

    #[test]
    fn check_check_then_draw_then_showdown() {
        let mut h = Hand::new(deck());
        h.apply(Action::Call);
        assert_eq!(h.phase, Phase::Bet1);
        h.apply(Action::Call);
        assert_eq!(h.phase, Phase::Draw);
        h.apply(Action::Draw(0b11));
        assert_eq!(h.to_act, 1);
        h.apply(Action::Draw(0));
        assert_eq!(h.phase, Phase::Bet2);
        assert_eq!(h.history[2], Action::Draw(2));
        h.apply(Action::Raise);
        h.apply(Action::Raise);
        h.apply(Action::Raise);
        assert_eq!(h.legal(), vec![Action::Fold, Action::Call]);
        h.apply(Action::Call);
        assert!(h.showdown());
        assert_eq!(h.pot(), 2 + 2 * 12);
        let p = h.payoff();
        assert_eq!(p[0] + p[1], 0);
        assert_eq!(p[0].abs(), 13);
    }

    #[test]
    fn fold_gives_pot() {
        let mut h = Hand::new(deck());
        h.apply(Action::Raise);
        h.apply(Action::Fold);
        assert!(h.is_over());
        assert_eq!(h.payoff(), [1, -1]);
    }
}
