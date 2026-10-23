//! Motore di scopa a due giocatori: regole, giocatori artificiali e partite a 11 punti.

pub mod ai;
pub mod game;
mod hand_dist;

pub use ai::{evaluate, EngineConfig, EnginePlayer, MoveEval, Player};
pub use game::{Game, HandScore, Move, Observation};

use rand::rngs::SmallRng;

/// Gioca una smazzata intera. `match_score` è il punteggio della partita prima della smazzata.
pub fn play_hand(mut g: Game, players: &mut [&mut dyn Player; 2], match_score: [u8; 2], rng: &mut SmallRng) -> HandScore {
    while !g.is_over() {
        let p = g.to_move;
        let obs = Observation::of(&g, p, match_score);
        let m = players[p as usize].choose(&obs, rng);
        assert!(g.is_legal(m), "mossa illegale da {}", players[p as usize].name());
        g.play(m);
    }
    g.score()
}
