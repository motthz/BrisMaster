//! Motore di briscola: regole, giocatori artificiali e strumenti di simulazione.

pub mod ai;
pub mod cards;
pub mod game;
pub mod stats;

pub use ai::{evaluate, EngineConfig, EnginePlayer, HeuristicPlayer, MoveEval, Player, RandomPlayer};
pub use cards::{Card, CardSet};
pub use game::{Game, Observation, Trick};

use rand::rngs::SmallRng;

/// Gioca una partita intera tra due giocatori a partire da uno stato dato.
/// Restituisce i punti finali `[giocatore 0, giocatore 1]`.
pub fn play_game(mut g: Game, players: &mut [&mut dyn Player; 2], rng: &mut SmallRng) -> [u8; 2] {
    while !g.is_over() {
        let p = g.to_move();
        let obs = Observation::of(&g, p);
        let c = players[p as usize].choose(&obs, rng);
        assert!(obs.hand.contains(c), "mossa illegale da {}", players[p as usize].name());
        g.play(c);
    }
    g.points
}
