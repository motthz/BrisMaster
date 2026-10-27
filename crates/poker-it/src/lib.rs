//! Poker all'italiana a due giocatori: regole, valutazione delle mani, analisi del cambio,
//! motore allenato con CFR e giocatori artificiali per i test.

pub mod abstraction;
pub mod adaptive;
pub mod bots;
pub mod cards;
pub mod cfr;
pub mod draw;
pub mod engine;
pub mod eval;
pub mod game;
pub mod opponent;
pub mod reference;
pub mod strategy;

pub use adaptive::Adaptive;
pub use bots::{Agent, RuleBot};
pub use engine::Engine;
pub use game::{Action, Hand, Observation, Phase};
pub use strategy::Strategy;
