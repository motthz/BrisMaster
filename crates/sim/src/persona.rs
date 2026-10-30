//! Giocatori "umani" del banco di prova: i personaggi a manopole (in `briscola_core::style`)
//! e i loro difetti sotto pressione.
//!
//! - `stile:NOME`: uno dei 20 stili con nome di [`STYLES`];
//! - `persona:N`: il personaggio numero N, con le manopole estratte a caso dal seme N.

#[allow(unused_imports)]
pub use briscola_core::style::{Knobs, Persona, STYLES};
use briscola_core::ai::Player;
use briscola_core::cards::{Card, NUM_CARDS};
use briscola_core::Observation;
use rand::rngs::SmallRng;
use rand::Rng;

/// Persone vere sotto pressione: il giocatore `base` sbaglia sempre di più verso la fine
/// della partita (fino a `error` all'ultima presa).
pub struct Clock {
    pub base: Box<dyn Player>,
    pub error: f64,
}

impl Player for Clock {
    fn name(&self) -> String {
        format!("{} · il tempo stringe (fino a {:.0}% di errori)", self.base.name(), self.error * 100.0)
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        let progress = obs.played.len() as f64 / NUM_CARDS as f64;
        if obs.hand.len() > 1 && rng.gen_bool((self.error * progress).min(1.0)) {
            let h = obs.hand.to_vec();
            return h[rng.gen_range(0..h.len())];
        }
        self.base.choose(obs, rng)
    }
}

/// Tilt: il giocatore `base` sbaglia con probabilità `error` quando è sotto di 10 punti o più.
pub struct Tilt {
    pub base: Box<dyn Player>,
    pub error: f64,
}

impl Player for Tilt {
    fn name(&self) -> String {
        format!("{} · va in tilt quando è sotto ({:.0}% di errori)", self.base.name(), self.error * 100.0)
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        let me = obs.me as usize;
        let behind = obs.points[1 - me] as i32 - obs.points[me] as i32 >= 10;
        if behind && obs.hand.len() > 1 && rng.gen_bool(self.error) {
            let h = obs.hand.to_vec();
            return h[rng.gen_range(0..h.len())];
        }
        self.base.choose(obs, rng)
    }
}

/// Camaleonte: cambia stile ogni 4 prese, scegliendo a caso tra i giocatori dati.
/// Mette alla prova la deduzione delle carte, che non può contare su uno stile fisso.
pub struct Chameleon {
    pub n: u64,
    pub styles: Vec<Box<dyn Player>>,
    current: usize,
    last_switch: u32,
}

impl Chameleon {
    pub fn new(n: u64, styles: Vec<Box<dyn Player>>) -> Chameleon {
        Chameleon { n, styles, current: 0, last_switch: u32::MAX }
    }
}

impl Player for Chameleon {
    fn name(&self) -> String {
        format!("Camaleonte n. {}: cambia stile ogni 4 prese tra {} stili", self.n, self.styles.len())
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        let tricks = obs.played.len() / 2;
        let block = tricks / 4;
        if self.last_switch != block {
            self.last_switch = block;
            self.current = rng.gen_range(0..self.styles.len());
        }
        self.styles[self.current].choose(obs, rng)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use briscola_core::{play_game, Game, HeuristicPlayer};
    use rand::SeedableRng;

    #[test]
    fn personas_play_legal_and_differ() {
        let mut rng = SmallRng::seed_from_u64(9);
        assert_ne!(Knobs::random(1), Knobs::random(2));
        assert_eq!(Knobs::random(7), Knobs::random(7));
        let mut players: Vec<Box<dyn Player>> = STYLES.iter().map(|s| Box::new(Persona::style(s.0).unwrap()) as Box<dyn Player>).collect();
        players.extend((0..10).map(|n| Box::new(Persona::numbered(n)) as Box<dyn Player>));
        players.push(Box::new(Clock { base: Box::new(HeuristicPlayer), error: 0.5 }));
        players.push(Box::new(Tilt { base: Box::new(HeuristicPlayer), error: 0.5 }));
        players.push(Box::new(Chameleon::new(1, vec![Box::new(HeuristicPlayer), Box::new(Persona::numbered(3))])));
        for p in players.iter_mut() {
            for first in 0..2 {
                let g = Game::new(&mut rng, first);
                let mut h = HeuristicPlayer;
                let pts = play_game(g, &mut [p.as_mut(), &mut h], &mut rng);
                assert_eq!(pts[0] as u32 + pts[1] as u32, 120);
            }
        }
    }

    #[test]
    fn perfect_memory_style_beats_short_memory() {
        // Una sanità di base delle manopole: chi ricorda tutto batte chi dimentica.
        let mut rng = SmallRng::seed_from_u64(3);
        let (mut a, mut b) = (Persona::style("memoria-perfetta").unwrap(), Persona::style("domenica").unwrap());
        let mut total = 0u32;
        for i in 0..200 {
            let g = Game::new(&mut rng, (i % 2) as u8);
            total += play_game(g, &mut [&mut a, &mut b], &mut rng)[0] as u32;
        }
        assert!(total > 200 * 60, "media {}", total as f64 / 200.0);
    }
}
