//! Motore di Scala 40: regole, combinazioni, giocatori artificiali.

pub mod ai;
pub mod cards;
pub mod game;
pub mod meld;
pub mod solver;

pub use cards::Card;
pub use game::{Action, Deal, Event, Match, Phase};
pub use meld::Meld;
