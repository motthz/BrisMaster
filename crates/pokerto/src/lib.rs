//! Poker all'italiana a due giocatori: regole, valutazione delle mani, analisi del cambio,
//! motore allenato con CFR e giocatori artificiali per i test.

pub mod abstraction;
pub mod bots;
pub mod cards;
pub mod cfr;
pub mod draw;
pub mod engine;
pub mod eval;
pub mod game;
pub mod strategy;

pub use bots::{Agent, RuleBot};
pub use engine::Engine;
pub use game::{Action, Hand, Observation, Phase};
pub use strategy::Strategy;
