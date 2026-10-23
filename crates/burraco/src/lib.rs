//! Burraco a due giocatori: regole, giocatori artificiali e motore per i consigli.

pub mod ai;
pub mod arena;
pub mod cards;
pub mod game;
pub mod melds;

pub use cards::{Cards, Kind};
pub use game::{Action, Game, Phase};
pub use melds::Meld;
