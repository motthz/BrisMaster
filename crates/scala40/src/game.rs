//! Regole di una smazzata e di una partita a due giocatori.
//!
//! Regole usate (Scala 40 classica, due giocatori):
//! - 108 carte, 13 a testa; una carta scoperta inizia il pozzo, il resto è il tallone.
//! - Al proprio turno si pesca dal tallone oppure si prende la carta in cima al pozzo,
//!   che però va usata subito in una combinazione o attaccata.
//! - La prima volta che si cala (apertura) le combinazioni calate in quel turno devono
//!   valere almeno 40 punti (asso 11, o 1 in A-2-3; figure 10; il jolly vale la carta che sostituisce).
//! - Dopo l'apertura si può calare, attaccare carte a qualunque combinazione in tavola
//!   e sostituire un jolly in tavola con la carta che rappresenta, prendendolo in mano.
//! - Il turno finisce scartando una carta. Chi resta senza carte con lo scarto chiude la smazzata.
//! - L'avversario paga i punti delle carte che ha in mano (jolly 25, asso 11, figure 10),
//!   oppure 100 se non ha aperto. Arrivato a 101 punti si è eliminati: vince l'altro.

use crate::cards::{self, Card, NUM_CARDS, NUM_KINDS};
use crate::meld::Meld;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;

pub const HAND_SIZE: usize = 13;
pub const OPEN_MIN: u32 = 40;
pub const ELIMINATION: u32 = 101;
pub const UNOPENED_PENALTY: u32 = 100;
/// Oltre questo numero di turni la smazzata finisce senza vincitore (non succede quasi mai).
pub const MAX_TURNS: u32 = 400;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Draw,
    Play,
    Over,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    DrawStock(u8),
    TakeDiscard(u8, Card),
    Lay(u8, Vec<Vec<Card>>),
    Attach(u8, Card, usize),
    Swap(u8, Card, usize),
    Discard(u8, Card),
    Reshuffle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Cala queste combinazioni (insieme, per l'apertura da 40).
    Lay(Vec<Vec<Card>>),
    /// Attacca la carta alla combinazione in tavola con questo indice.
    Attach(Card, usize),
    /// Sostituisce il jolly della combinazione con la carta, e prende il jolly.
    Swap(Card, usize),
}

/// Una smazzata.
#[derive(Clone, Debug)]
pub struct Deal {
    pub hands: [Vec<Card>; 2],
    /// Tallone: si pesca dalla fine.
    pub stock: Vec<Card>,
    /// Pozzo: la cima è l'ultima carta.
    pub discard: Vec<Card>,
    pub melds: Vec<Meld>,
    pub opened: [bool; 2],
    pub to_move: u8,
    pub phase: Phase,
    /// Carta presa dal pozzo in questo turno, da usare prima di scartare.
    pub taken: Option<Card>,
    pub winner: Option<u8>,
    pub turns: u32,
    pub log: Vec<Event>,
    /// Azioni fatte nel turno dopo la pescata (per annullare la presa dal pozzo).
    actions_this_turn: u32,
    rng: SmallRng,
}

fn remove(hand: &mut Vec<Card>, c: Card) -> bool {
    match hand.iter().position(|&x| x == c) {
        Some(i) => {
            hand.remove(i);
            true
        }
        None => false,
    }
}

impl Deal {
    /// Smazzata da un mazzo già mescolato; `seed` decide i rimescolamenti del pozzo.
    pub fn from_order(order: &[Card], first: u8, seed: u64) -> Deal {
        assert_eq!(order.len(), NUM_CARDS);
        let mut hands = [Vec::new(), Vec::new()];
        hands[first as usize] = order[..HAND_SIZE].to_vec();
        hands[1 - first as usize] = order[HAND_SIZE..2 * HAND_SIZE].to_vec();
        let discard = vec![order[2 * HAND_SIZE]];
        let stock = order[2 * HAND_SIZE + 1..].iter().rev().copied().collect();
        Deal {
            hands,
            stock,
            discard,
            melds: Vec::new(),
            opened: [false; 2],
            to_move: first,
            phase: Phase::Draw,
            taken: None,
            winner: None,
            turns: 0,
            log: Vec::new(),
            actions_this_turn: 0,
            rng: SmallRng::seed_from_u64(seed),
        }
    }

    /// Posizione inserita a mano (assistente): la mano di `me`, il pozzo, le combinazioni in tavola,
    /// quante carte ha l'avversario. Le carte coperte (mano avversaria e tallone) sono riempite con
    /// carte qualunque tra quelle non visibili: il motore non le guarda.
    pub fn from_position(
        me: u8,
        hand: Vec<Card>,
        opp_hand_len: usize,
        discard: Vec<Card>,
        melds: Vec<Meld>,
        opened: [bool; 2],
        drawn: bool,
    ) -> Deal {
        let mut hidden: Vec<Card> = cards::full_deck();
        hidden.retain(|c| !hand.contains(c) && !discard.contains(c) && !melds.iter().any(|m| m.cards.contains(c)));
        let opp_len = opp_hand_len.min(hidden.len());
        let mut hands = [Vec::new(), Vec::new()];
        hands[1 - me as usize] = hidden.drain(..opp_len).collect();
        hands[me as usize] = hand;
        Deal {
            hands,
            stock: hidden,
            discard,
            melds,
            opened,
            to_move: me,
            phase: if drawn { Phase::Play } else { Phase::Draw },
            taken: None,
            winner: None,
            turns: 0,
            log: Vec::new(),
            actions_this_turn: 0,
            rng: SmallRng::seed_from_u64(0),
        }
    }

    pub fn new(rng: &mut impl rand::Rng, first: u8) -> Deal {
        let mut order = cards::full_deck();
        order.shuffle(rng);
        Deal::from_order(&order, first, rng.gen())
    }

    pub fn is_over(&self) -> bool {
        self.phase == Phase::Over
    }

    pub fn top_discard(&self) -> Option<Card> {
        self.discard.last().copied()
    }

    pub fn draw_stock(&mut self) -> Result<Card, String> {
        if self.phase != Phase::Draw {
            return Err("Non è il momento di pescare".into());
        }
        if self.stock.is_empty() {
            // Si rimescola il pozzo tranne la carta in cima.
            let top = self.discard.pop();
            self.stock = std::mem::take(&mut self.discard);
            self.stock.shuffle(&mut self.rng);
            self.discard.extend(top);
            self.log.push(Event::Reshuffle);
        }
        let Some(c) = self.stock.pop() else {
            self.end(None);
            return Err("Carte finite".into());
        };
        let p = self.to_move;
        self.hands[p as usize].push(c);
        self.phase = Phase::Play;
        self.actions_this_turn = 0;
        self.log.push(Event::DrawStock(p));
        Ok(c)
    }

    pub fn take_discard(&mut self) -> Result<Card, String> {
        if self.phase != Phase::Draw {
            return Err("Non è il momento di pescare".into());
        }
        let c = self.discard.pop().ok_or("Il pozzo è vuoto")?;
        let p = self.to_move;
        self.hands[p as usize].push(c);
        self.taken = Some(c);
        self.phase = Phase::Play;
        self.actions_this_turn = 0;
        self.log.push(Event::TakeDiscard(p, c));
        Ok(c)
    }

    /// Rimette nel pozzo la carta appena presa, se non si è ancora fatto niente.
    pub fn undo_take(&mut self) -> Result<(), String> {
        let c = self.taken.ok_or("Non hai preso dal pozzo")?;
        if self.actions_this_turn > 0 {
            return Err("Hai già calato o attaccato".into());
        }
        let p = self.to_move as usize;
        remove(&mut self.hands[p], c);
        self.discard.push(c);
        self.taken = None;
        self.phase = Phase::Draw;
        self.log.pop();
        Ok(())
    }

    fn check_play(&self) -> Result<(), String> {
        if self.phase != Phase::Play {
            return Err("Prima devi pescare".into());
        }
        Ok(())
    }

    pub fn lay(&mut self, groups: &[Vec<Card>]) -> Result<(), String> {
        self.check_play()?;
        let p = self.to_move;
        let mut hand = self.hands[p as usize].clone();
        let mut melds = Vec::new();
        for g in groups {
            for &c in g {
                if !remove(&mut hand, c) {
                    return Err(format!("{} non è nella mano", cards::name(c)));
                }
            }
            melds.push(Meld::new(g, p).ok_or_else(|| format!("{} non è una combinazione valida", labels(g)))?);
        }
        if melds.is_empty() {
            return Err("Nessuna combinazione".into());
        }
        if hand.is_empty() {
            return Err("Devi tenere una carta da scartare".into());
        }
        let total: u32 = melds.iter().map(Meld::value).sum();
        if !self.opened[p as usize] && total < OPEN_MIN {
            return Err(format!("Per aprire servono almeno {OPEN_MIN} punti: queste combinazioni ne valgono {total}"));
        }
        self.opened[p as usize] = true;
        self.hands[p as usize] = hand;
        self.melds.extend(melds);
        self.actions_this_turn += 1;
        self.log.push(Event::Lay(p, groups.to_vec()));
        Ok(())
    }

    pub fn attach(&mut self, c: Card, meld: usize) -> Result<(), String> {
        self.check_play()?;
        let p = self.to_move as usize;
        if !self.opened[p] {
            return Err("Per attaccare devi prima aprire".into());
        }
        if !self.hands[p].contains(&c) {
            return Err(format!("{} non è nella mano", cards::name(c)));
        }
        if self.hands[p].len() == 1 {
            return Err("Devi tenere una carta da scartare".into());
        }
        let m = self.melds.get_mut(meld).ok_or("Combinazione inesistente")?;
        if !m.attach(c) {
            return Err(format!("{} non si attacca a {}", cards::name(c), m.label()));
        }
        remove(&mut self.hands[p], c);
        self.actions_this_turn += 1;
        self.log.push(Event::Attach(p as u8, c, meld));
        Ok(())
    }

    pub fn swap(&mut self, c: Card, meld: usize) -> Result<(), String> {
        self.check_play()?;
        let p = self.to_move as usize;
        if !self.opened[p] {
            return Err("Per prendere un jolly devi prima aprire".into());
        }
        if !self.hands[p].contains(&c) {
            return Err(format!("{} non è nella mano", cards::name(c)));
        }
        let m = self.melds.get_mut(meld).ok_or("Combinazione inesistente")?;
        if self.taken.is_some_and(|t| m.cards.contains(&t)) {
            return Err("Non puoi riprendere il jolly appena preso dal pozzo".into());
        }
        let j = m.swap(c).ok_or_else(|| format!("{} non sostituisce il jolly di {}", cards::name(c), m.label()))?;
        remove(&mut self.hands[p], c);
        self.hands[p].push(j);
        self.actions_this_turn += 1;
        self.log.push(Event::Swap(p as u8, c, meld));
        Ok(())
    }

    pub fn apply(&mut self, a: &Action) -> Result<(), String> {
        match a {
            Action::Lay(g) => self.lay(g),
            Action::Attach(c, m) => self.attach(*c, *m),
            Action::Swap(c, m) => self.swap(*c, *m),
        }
    }

    pub fn discard_card(&mut self, c: Card) -> Result<(), String> {
        self.check_play()?;
        let p = self.to_move as usize;
        if let Some(t) = self.taken {
            if self.hands[p].contains(&t) {
                return Err(format!("Devi usare {} preso dal pozzo prima di scartare", cards::name(t)));
            }
        }
        if !remove(&mut self.hands[p], c) {
            return Err(format!("{} non è nella mano", cards::name(c)));
        }
        self.discard.push(c);
        self.taken = None;
        self.log.push(Event::Discard(p as u8, c));
        self.turns += 1;
        if self.hands[p].is_empty() {
            self.end(Some(p as u8));
        } else if self.turns >= MAX_TURNS {
            self.end(None);
        } else {
            self.to_move = 1 - self.to_move;
            self.phase = Phase::Draw;
        }
        Ok(())
    }

    fn end(&mut self, winner: Option<u8>) {
        self.winner = winner;
        self.phase = Phase::Over;
    }

    /// Punti di penalità della mano del giocatore se la smazzata finisse ora.
    pub fn hand_penalty(&self, p: u8) -> u32 {
        if !self.opened[p as usize] {
            UNOPENED_PENALTY
        } else {
            self.hands[p as usize].iter().map(|&c| cards::penalty(c)).sum()
        }
    }

    /// Punti presi da ciascun giocatore a fine smazzata.
    pub fn penalties(&self) -> [u32; 2] {
        let mut s = [0; 2];
        for p in 0..2u8 {
            if self.winner != Some(p) {
                s[p as usize] = self.hand_penalty(p);
            }
        }
        s
    }

    /// Carte non visibili al giocatore `p`, per tipo: mano avversaria e tallone.
    pub fn unseen(&self, p: u8) -> [u8; NUM_KINDS] {
        let mut n = cards::counts(&cards::full_deck());
        let visible = self.hands[p as usize].iter().chain(&self.discard).chain(self.melds.iter().flat_map(|m| &m.cards));
        for &c in visible {
            n[cards::kind(c) as usize] -= 1;
        }
        n
    }
}

fn labels(g: &[Card]) -> String {
    g.iter().map(|&c| cards::short(c)).collect::<Vec<_>>().join(" ")
}

/// Una partita: smazzate fino a quando un giocatore arriva a 101 punti.
#[derive(Clone, Debug, Default)]
pub struct Match {
    pub scores: [u32; 2],
    pub deals_played: u32,
}

impl Match {
    /// Il giocatore che comincia la smazzata numero `n` (si alterna).
    pub fn first_of(n: u32) -> u8 {
        (n % 2) as u8
    }

    pub fn record(&mut self, d: &Deal) {
        let p = d.penalties();
        self.scores[0] += p[0];
        self.scores[1] += p[1];
        self.deals_played += 1;
    }

    /// `Some(Some(p))` se ha vinto `p`, `Some(None)` per il pari (entrambi oltre 101 con gli stessi punti).
    pub fn result(&self) -> Option<Option<u8>> {
        let out = |p: usize| self.scores[p] >= ELIMINATION;
        match (out(0), out(1)) {
            (false, false) => None,
            (true, false) => Some(Some(1)),
            (false, true) => Some(Some(0)),
            _ => Some(match self.scores[0].cmp(&self.scores[1]) {
                std::cmp::Ordering::Less => Some(0),
                std::cmp::Ordering::Greater => Some(1),
                std::cmp::Ordering::Equal => None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::kind_of;

    #[test]
    fn opening_needs_forty_and_discard_must_be_used() {
        let mut order = cards::full_deck();
        // Mano del primo giocatore: K-Q-J di cuori e tre 9.
        let want = [kind_of(0, 13), kind_of(0, 12), kind_of(0, 11), kind_of(1, 9), kind_of(2, 9), kind_of(3, 9)];
        for (i, &w) in want.iter().enumerate() {
            let j = order.iter().position(|&c| c == w).unwrap();
            order.swap(i, j);
        }
        let mut d = Deal::from_order(&order, 0, 1);
        d.draw_stock().unwrap();
        assert!(d.lay(&[want[3..6].to_vec()]).is_err(), "27 punti non bastano");
        assert!(d.attach(want[0], 0).is_err());
        d.lay(&[want[..3].to_vec(), want[3..6].to_vec()]).unwrap();
        assert!(d.opened[0]);
        assert_eq!(d.hands[0].len(), 8);
        let c = d.hands[0][0];
        d.discard_card(c).unwrap();
        assert_eq!(d.to_move, 1);
        let t = d.take_discard().unwrap();
        assert_eq!(t, c);
        assert!(d.discard_card(d.hands[1][0]).is_err(), "la carta presa va usata");
        d.undo_take().unwrap();
        assert_eq!(d.top_discard(), Some(c));
    }

    #[test]
    fn match_ends_at_101() {
        let mut m = Match::default();
        m.scores = [100, 40];
        assert_eq!(m.result(), None);
        m.scores = [101, 40];
        assert_eq!(m.result(), Some(Some(1)));
    }
}
