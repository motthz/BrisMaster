//! Motore di scopa a due giocatori: regole, giocatori artificiali e partite a 11 punti.

pub mod ai;
pub mod game;
mod hand_dist;

pub use ai::{evaluate, EngineConfig, EnginePlayer, MoveEval, Player};
pub use game::{Game, HandScore, Move, Observation, OppPlay};

use rand::rngs::SmallRng;

/// Gioca una smazzata intera. `match_score` è il punteggio della partita prima della smazzata.
pub fn play_hand(
    mut g: Game,
    players: &mut [&mut dyn Player; 2],
    match_score: [u8; 2],
    rng: &mut SmallRng,
) -> HandScore {
    // Giocate di ciascuno nel giro in corso, viste dall'altro.
    let mut plays = [[OppPlay::default(); 2]; 2];
    let mut plays_len = [0u8; 2];
    while !g.is_over() {
        let p = g.to_move as usize;
        let mut obs = Observation::of(&g, p as u8, match_score);
        obs.opp_plays = plays[1 - p];
        obs.opp_plays_len = plays_len[1 - p];
        let m = players[p].choose(&obs, rng);
        assert!(g.is_legal(m), "mossa illegale da {}", players[p].name());
        if (plays_len[p] as usize) < 2 {
            plays[p][plays_len[p] as usize] = OppPlay {
                table: g.table,
                seen: g.captured[0].union(g.captured[1]),
                mv: m,
                my_len: g.hands[1 - p].len() as u8,
            };
            plays_len[p] += 1;
        }
        let deck = g.deck_len;
        g.play(m);
        if g.deck_len != deck {
            plays_len = [0, 0];
        }
    }
    g.score()
}
