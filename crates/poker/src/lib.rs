//! Texas Hold'em No Limit testa a testa: regole, valutazione delle mani, motore (CFR su astrazione) e bot.

pub mod abstraction;
pub mod bots;
pub mod buckets;
pub mod cards;
pub mod cfr;
pub mod engine;
pub mod equity;
pub mod game;

pub use game::{Action, Hand};

use rand::rngs::SmallRng;

/// Quello che un giocatore vede della mano: le sue carte, il board scoperto e le azioni.
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub me: u8,
    pub hole: [cards::Card; 2],
    pub board: &'a [cards::Card],
    pub button: u8,
    pub street: u8,
    pub stack: [u32; 2],
    pub committed: [u32; 2],
    pub street_bet: [u32; 2],
    pub to_call: u32,
    pub raise_range: Option<(u32, u32)>,
    pub raises_this_street: u8,
    pub log: &'a [game::Logged],
}

impl<'a> View<'a> {
    pub fn of(h: &'a Hand, me: u8) -> View<'a> {
        View {
            me,
            hole: h.hole[me as usize],
            board: h.visible_board(),
            button: h.button,
            street: h.street,
            stack: h.stack,
            committed: h.committed,
            street_bet: h.street_bet,
            to_call: if h.to_act == me { h.to_call() } else { 0 },
            raise_range: if h.to_act == me { h.raise_range() } else { None },
            raises_this_street: h.raises_this_street(),
            log: &h.log,
        }
    }
    pub fn pot(&self) -> u32 {
        self.committed[0] + self.committed[1]
    }
    pub fn in_position(&self) -> bool {
        // Dopo il flop il bottone parla per ultimo.
        self.me == self.button
    }
    /// Puntata più alta del giro (quella da pareggiare).
    pub fn current_bet(&self) -> u32 {
        self.street_bet[0].max(self.street_bet[1])
    }
    /// Rilancio di `frac` volte il piatto (dopo aver pagato), legalizzato.
    pub fn pot_raise(&self, frac: f64) -> Option<Action> {
        let (lo, hi) = self.raise_range?;
        let t = self.current_bet() as f64 + frac * (self.pot() + self.to_call) as f64;
        Some(Action::RaiseTo((t.round() as u32).clamp(lo, hi)))
    }
}

/// Un giocatore di poker: riceve ciò che vede e sceglie un'azione (che verrà resa legale).
pub trait Agent: Send {
    fn name(&self) -> String;
    fn act(&mut self, v: &View, rng: &mut SmallRng) -> Action;
}

/// Gioca una mano fino alla fine tra due agenti (`agents[i]` siede al posto i).
pub fn play_hand(h: &mut Hand, agents: &mut [&mut dyn Agent; 2], rng: &mut SmallRng) {
    while !h.over {
        let p = h.to_act;
        let a = agents[p as usize].act(&View::of(h, p), rng);
        h.apply(a);
    }
}
