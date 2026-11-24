//! HumanSim: initial interpretable *unfitted* baseline for simulated human opponents.
//!
//! This is NOT a validated model of human gameplay. Its probabilities and population
//! proportions must be calibrated using held-out recordings of human decisions.
//! It intentionally receives only `Observation`: no hidden opposing hand/deck.
use crate::ai::Player;
use crate::cards::{beats, points, rank, strength, suit, Card, CardSet, NUM_CARDS};
use crate::game::Observation;
use crate::style::{Knobs, STYLES};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// A fixed synthetic identity: the same id and knobs always produce the same
/// memory profile. Random choices during a game use the caller's seeded RNG.
pub struct HumanSimPlayer {
    pub id: u64,
    pub knobs: Knobs,
    remembered: CardSet,
    label: String,
}

impl HumanSimPlayer {
    pub fn new(id: u64, label: String, knobs: Knobs) -> Self {
        let mut rng = SmallRng::seed_from_u64(id ^ 0x4855_4d41_4e53_494d);
        let remembered: CardSet = (0..NUM_CARDS as u8)
            .filter(|_| rng.gen::<f64>() < knobs.memoria.clamp(0.0, 1.0))
            .collect();
        HumanSimPlayer { id, knobs, remembered, label }
    }

    /// Exploratory synthetic persona, not representative of population prevalence.
    pub fn numbered(id: u64) -> Self {
        Self::new(id, format!("HumanSim synthetic #{id}"), Knobs::random(id))
    }

    /// Reuses the existing 20 named profiles as explicit *unfitted* baselines.
    pub fn style(name: &str) -> Option<Self> {
        let (_, _, knobs) = STYLES.iter().find(|(n, _, _)| *n == name)?;
        let id = name.bytes().fold(0u64, |h, b| h.wrapping_mul(131).wrapping_add(b as u64));
        Some(Self::new(id, format!("HumanSim style {name}"), *knobs))
    }

    fn perceived_safe_load(&self, card: Card, obs: &Observation) -> bool {
        if suit(card) == obs.trump { return false; }
        let seen = CardSet(obs.played.0 & self.remembered.0).union(obs.hand);
        (0..NUM_CARDS as u8).all(|other| {
            if other == card || seen.contains(other) {
                return true;
            }
            // The exposed trump card is underneath the deck and is not an
            // immediately playable answer until drawn.
            if obs.deck_len > 0 && other == obs.trump_card {
                return true;
            }
            !beats(other, card, obs.trump)
        })
    }

    fn keep_value(&self, card: Card, obs: &Observation) -> f64 {
        let k = &self.knobs;
        let p = points(card) as f64;
        let s = strength(card) as f64;
        if suit(card) == obs.trump {
            let base = (0.5 + k.risparmio) * (7.0 + s)
                * if p >= 10.0 { 1.0 + k.tiene_assi } else { 1.0 };
            if obs.deck_len == 0 { base * 0.32 } else { base }
        } else {
            s * 0.24
        }
    }

    fn utility(&self, card: Card, obs: &Observation) -> f64 {
        let k = &self.knobs;
        let p = points(card) as f64;
        let keep = self.keep_value(card, obs);
        match obs.table {
            Some(lead) => {
                let prize = (points(lead) + points(card)) as f64;
                if beats(card, lead, obs.trump) {
                    prize * (0.5 + k.caccia) + 3.0 * k.aggressivita - keep
                } else {
                    -prize * (0.5 + k.protegge) - keep
                }
            }
            None if suit(card) == obs.trump => {
                4.0 * k.apre_briscola - 0.55 * p - keep
            }
            None => {
                let safe = p >= 10.0 && self.perceived_safe_load(card, obs);
                let open_load = (1.2 * k.apre_carichi - 0.5 - k.protegge) * p;
                open_load - keep + if safe { 9.0 * k.caccia } else { 0.0 }
            }
        }
    }

    /// All legal moves receive a calibrated-ready probability, sorted by card id.
    /// This function is deterministic and does not look at Game's hidden fields.
    pub fn distribution(&self, obs: &Observation) -> Vec<(Card, f64)> {
        let cards = obs.hand.to_vec();
        if cards.is_empty() { return Vec::new(); }
        if cards.len() == 1 { return vec![(cards[0], 1.0)]; }

        let k = &self.knobs;
        let behind = obs.points[1 - obs.me as usize] as i16
            - obs.points[obs.me as usize] as i16 >= 10;
        let progress = obs.played.len() as f64 / NUM_CARDS as f64;
        // Warm start only: this is NOT fitted to human recordings.
        let lapse = (k.errori + if behind { 0.25 * k.tilt } else { 0.0 }
            + 0.25 * progress * k.fretta).clamp(0.0, 0.65);
        let expert = (k.memoria + k.finale) / 2.0;
        let temperature = if obs.deck_len == 0 {
            7.0 - 3.5 * expert
        } else {
            7.0 - 2.0 * expert
        };

        let scores: Vec<f64> = cards.iter().map(|&c| self.utility(c, obs) / temperature).collect();
        let max = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let exp: Vec<f64> = scores.iter().map(|&s| (s - max).exp()).collect();
        let normalizer: f64 = exp.iter().sum();
        let uniform = lapse / cards.len() as f64;
        cards.iter().zip(exp.iter()).map(|(&c, &w)| {
            (c, (1.0 - lapse) * w / normalizer + uniform)
        }).collect()
    }
}

impl Player for HumanSimPlayer {
    fn name(&self) -> String { self.label.clone() }

    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        let dist = self.distribution(obs);
        assert!(!dist.is_empty(), "HumanSim called without a legal move");
        let mut roll = rng.gen::<f64>();
        for &(card, weight) in &dist {
            if roll < weight { return card; }
            roll -= weight;
        }
        dist.last().unwrap().0 // floating-point rounding only
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::HeuristicPlayer;
    use crate::game::Game;
    use crate::play_game;

    #[test]
    fn distribution_is_legal_and_normalized() {
        let mut rng = SmallRng::seed_from_u64(31);
        for id in 0..40 {
            let mut game = Game::new(&mut rng, 0);
            let persona = HumanSimPlayer::numbered(id);
            while !game.is_over() {
                let obs = Observation::of(&game, game.to_move());
                let dist = persona.distribution(&obs);
                assert_eq!(dist.len(), obs.hand.len() as usize);
                assert!(dist.iter().all(|(c, p)| obs.hand.contains(*c) && p.is_finite() && *p >= 0.0));
                let sum: f64 = dist.iter().map(|(_, p)| p).sum();
                assert!((sum - 1.0).abs() < 1e-12);
                let moves = obs.hand.to_vec();
                let next = moves[rng.gen_range(0..moves.len())];
                game.play(next);
            }
        }
    }

    #[test]
    fn does_not_use_hidden_opponent_hand_or_future_deck() {
        let mut rng = SmallRng::seed_from_u64(45);
        let game = Game::new(&mut rng, 0);
        let mut altered = game;
        let their_card = altered.hands[1].iter().next().unwrap();
        let hidden_deck_card = altered.deck[1];
        altered.hands[1].remove(their_card);
        altered.hands[1].insert(hidden_deck_card);
        altered.deck[1] = their_card;
        let persona = HumanSimPlayer::numbered(4);
        assert_eq!(
            persona.distribution(&Observation::of(&game, 0)),
            persona.distribution(&Observation::of(&altered, 0))
        );
    }

    #[test]
    fn can_complete_games_with_120_points() {
        let mut rng = SmallRng::seed_from_u64(19);
        for id in 0..30 {
            let game = Game::new(&mut rng, (id % 2) as u8);
            let mut human = HumanSimPlayer::numbered(id);
            let mut reference = HeuristicPlayer;
            let pts = play_game(game, &mut [&mut human, &mut reference], &mut rng);
            assert_eq!(u16::from(pts[0]) + u16::from(pts[1]), 120);
        }
    }

    #[test]
    fn named_styles_exist() {
        for (name, _, _) in STYLES {
            assert!(HumanSimPlayer::style(name).is_some());
        }
    }
}
