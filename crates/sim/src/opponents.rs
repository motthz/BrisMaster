//! Stili di gioco aggiuntivi per i test: errori tipici dei principianti e giocatori distratti.

use briscola_core::ai::{heuristic_move, Player};
use briscola_core::cards::*;
use briscola_core::Observation;
use rand::rngs::SmallRng;
use rand::Rng;

/// Costo di perdere una carta: punti regalati e valore della briscola.
fn discard_cost(c: Card, trump: u8) -> i32 {
    let mut cost = points(c) as i32 * 4 + strength(c) as i32;
    if suit(c) == trump {
        cost += 30 + points(c) as i32 * 2;
    }
    cost
}

fn cheapest(cards: impl Iterator<Item = Card>, trump: u8) -> Option<Card> {
    cards.min_by_key(|&c| discard_cost(c, trump))
}

/// Un altro giocatore che ogni tanto sbaglia: con probabilità `error` gioca una carta a caso.
pub struct Noisy {
    pub base: Box<dyn Player>,
    pub error: f64,
}

impl Player for Noisy {
    fn name(&self) -> String {
        format!("{} con {:.0}% di errori", self.base.name(), self.error * 100.0)
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        if rng.gen_bool(self.error) {
            let moves = obs.hand.to_vec();
            moves[rng.gen_range(0..moves.len())]
        } else {
            self.base.choose(obs, rng)
        }
    }
}

/// "Rispondi di seme": crede di dover rispondere al seme come in altri giochi.
/// Se ha carte del seme di apertura ne gioca una (la vincente più forte, o la più piccola).
pub struct FollowSuitPlayer;

impl Player for FollowSuitPlayer {
    fn name(&self) -> String {
        "Rispondi di seme".into()
    }
    fn choose(&mut self, obs: &Observation, _rng: &mut SmallRng) -> Card {
        let trump = obs.trump;
        if let Some(lead) = obs.table {
            let same = || obs.hand.iter().filter(move |&c| suit(c) == suit(lead));
            if same().next().is_some() {
                return same()
                    .filter(|&c| strength(c) > strength(lead))
                    .max_by_key(|&c| strength(c))
                    .unwrap_or_else(|| same().min_by_key(|&c| strength(c)).unwrap());
            }
            if points(lead) >= 10 {
                if let Some(t) = cheapest(obs.hand.iter().filter(|&c| suit(c) == trump), trump) {
                    return t;
                }
            }
        }
        cheapest(obs.hand.iter(), trump).unwrap()
    }
}

/// "Butta-carichi": apre con l'Asso o il Tre per fare punti subito, senza contare le briscole.
pub struct LoadLeaderPlayer;

impl Player for LoadLeaderPlayer {
    fn name(&self) -> String {
        "Butta-carichi".into()
    }
    fn choose(&mut self, obs: &Observation, _rng: &mut SmallRng) -> Card {
        if obs.table.is_none() {
            let load = obs.hand.iter().filter(|&c| suit(c) != obs.trump && points(c) >= 10);
            if let Some(c) = load.max_by_key(|&c| points(c)) {
                return c;
            }
        }
        heuristic_move(obs.hand, obs.table, obs.trump)
    }
}

/// "Mai briscola": non usa le briscole finché c'è il mazzo, a meno di non avere altro.
pub struct NoTrumpPlayer;

impl Player for NoTrumpPlayer {
    fn name(&self) -> String {
        "Mai briscola".into()
    }
    fn choose(&mut self, obs: &Observation, _rng: &mut SmallRng) -> Card {
        let plain: CardSet = obs.hand.iter().filter(|&c| suit(c) != obs.trump).collect();
        let hand = if obs.deck_len > 0 && !plain.is_empty() { plain } else { obs.hand };
        heuristic_move(hand, obs.table, obs.trump)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use briscola_core::{play_game, Game, HeuristicPlayer};
    use rand::SeedableRng;

    #[test]
    fn styles_play_legal_games() {
        let mut rng = SmallRng::seed_from_u64(5);
        let mut styles: Vec<Box<dyn Player>> = vec![
            Box::new(FollowSuitPlayer),
            Box::new(LoadLeaderPlayer),
            Box::new(NoTrumpPlayer),
            Box::new(Noisy { base: Box::new(HeuristicPlayer), error: 0.5 }),
        ];
        for s in styles.iter_mut() {
            for first in 0..2 {
                let g = Game::new(&mut rng, first);
                let mut h = HeuristicPlayer;
                let pts = play_game(g, &mut [s.as_mut(), &mut h], &mut rng);
                assert_eq!(pts[0] as u32 + pts[1] as u32, 120);
            }
        }
    }
}
