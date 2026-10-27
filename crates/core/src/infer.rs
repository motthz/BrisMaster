//! Deduzione delle carte avversarie da come ha giocato.
//!
//! Si campionano molte mani avversarie compatibili con quello che so; per ognuna si ricostruisce
//! a caso una storia delle pescate coerente (quali carte aveva in mano a ogni sua mossa) e si
//! pesa con la probabilità, secondo la rete di politica, che con quelle carte avrebbe giocato
//! proprio come ha giocato. La ricerca poi pesca i mondi da queste mani pesate invece che a caso.

use crate::cards::*;
use crate::game::{Game, Observation};
use crate::net::Net;
use rand::rngs::SmallRng;
use rand::Rng;

/// Ciò che si sa della partita, in ordine, dal punto di vista di chi deduce.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// L'avversario ha giocato `card`; `seen` è quello che vedevo io in quel momento.
    OppMove { seen: Observation, card: Card },
    /// L'avversario ha pescato una carta (`trump`: era la briscola scoperta, l'ultima del mazzo).
    OppDraw { trump: bool },
}

/// Registro degli eventi tenuto a partire da una copia della partita (per il banco di prova:
/// usa solo informazioni pubbliche e le mie carte).
#[derive(Clone, Default)]
pub struct Log {
    pub events: Vec<Event>,
}

impl Log {
    /// Da chiamare prima di giocare `c` in `g`, per ogni carta giocata da chiunque.
    pub fn before_play(&mut self, g: &Game, me: u8, c: Card) {
        let p = g.to_move();
        if p != me {
            self.events.push(Event::OppMove { seen: Observation::of(g, me), card: c });
        }
        // Se la carta chiude la presa, dopo si pesca: prima chi vince, poi l'altro.
        if let Some(lead) = g.table {
            if g.deck_len > 0 {
                let winner = if beats(c, lead, g.trump) { p } else { g.leader };
                let mut left = g.deck_len;
                for drawer in [winner, 1 - winner] {
                    if left == 0 {
                        break;
                    }
                    if drawer != me {
                        self.events.push(Event::OppDraw { trump: left == 1 });
                    }
                    left -= 1;
                }
            }
        }
    }
}

/// Mani avversarie pesate.
pub struct Belief {
    pub hands: Vec<CardSet>,
    /// Pesi cumulati (non normalizzati), per pescare in proporzione.
    cum: Vec<f64>,
    weights: Vec<f64>,
}

impl Belief {
    /// `samples` mani avversarie compatibili con `obs`, pesate con la politica `policy`
    /// con temperatura `temp` (0 = nessuna deduzione, 1 = la politica com'è).
    /// `floor` mescola ogni probabilità con la mossa a caso: (1 − floor)·p + floor/n, così una
    /// mossa che il modello non si aspetta (un avversario che gioca in modo diverso) non può
    /// escludere del tutto una mano.
    pub fn infer(obs: &Observation, log: &[Event], policy: &Net, temp: f32, floor: f32, samples: usize, rng: &mut SmallRng) -> Belief {
        let mut hands = Vec::with_capacity(samples);
        let mut weights = Vec::with_capacity(samples);
        let mut probs = [0f32; 3];
        for _ in 0..samples {
            let world = obs.determinize(rng);
            let now = world.hands[1 - obs.me as usize];
            let mut held = now;
            let mut logw = 0f64;
            // All'indietro: prima di ogni sua mossa aveva anche la carta giocata; prima di ogni
            // sua pescata non aveva una delle carte che ha adesso (a caso, tranne la briscola scoperta).
            for e in log.iter().rev() {
                match *e {
                    Event::OppDraw { trump } => {
                        let x = if trump {
                            obs.trump_card
                        } else {
                            let mut cands = held;
                            cands.remove(obs.trump_card);
                            cands.iter().nth(rng.gen_range(0..cands.len() as usize)).unwrap()
                        };
                        debug_assert!(held.contains(x));
                        held.remove(x);
                    }
                    Event::OppMove { seen, card } => {
                        held.insert(card);
                        if held.len() > 1 && temp > 0.0 {
                            let theirs = Observation {
                                me: 1 - seen.me,
                                hand: held,
                                opp_hand_len: seen.hand.len() as u8,
                                ..seen
                            };
                            let n = policy.policy(&theirs, temp, &mut probs);
                            let i = held.iter().position(|c| c == card).unwrap();
                            debug_assert!(i < n);
                            let p = (1.0 - floor) * probs[i] + floor / n as f32;
                            logw += (p.max(1e-6) as f64).ln();
                        }
                    }
                }
            }
            hands.push(now);
            weights.push(logw);
        }
        let max = weights.iter().copied().fold(f64::MIN, f64::max);
        let mut cum = Vec::with_capacity(samples);
        let mut acc = 0.0;
        for w in weights.iter_mut() {
            *w = (*w - max).exp();
            acc += *w;
            cum.push(acc);
        }
        Belief { hands, cum, weights }
    }

    /// Una mano avversaria pescata in proporzione al peso.
    pub fn sample(&self, rng: &mut SmallRng) -> CardSet {
        let x = rng.gen::<f64>() * self.cum.last().copied().unwrap_or(0.0);
        let i = self.cum.partition_point(|&c| c <= x).min(self.hands.len() - 1);
        self.hands[i]
    }

    /// Numero effettivo di mani (quanto la deduzione ha ristretto le possibilità).
    pub fn ess(&self) -> f64 {
        let s: f64 = self.weights.iter().sum();
        let s2: f64 = self.weights.iter().map(|w| w * w).sum();
        s * s / s2
    }

    /// Probabilità che l'avversario abbia in mano la carta `c`.
    pub fn prob(&self, c: Card) -> f64 {
        let s: f64 = self.weights.iter().sum();
        self.hands.iter().zip(&self.weights).filter(|(h, _)| h.contains(c)).map(|(_, w)| w).sum::<f64>() / s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{Net, POLICY_OUT};
    use rand::SeedableRng;

    /// La storia ricostruita è sempre coerente: a ogni mossa l'avversario aveva la carta che ha
    /// giocato e il numero giusto di carte, qualunque mano si campioni.
    #[test]
    fn reconstructed_histories_are_consistent() {
        let mut rng = SmallRng::seed_from_u64(9);
        let net = Net::random(3, POLICY_OUT);
        for _ in 0..100 {
            let first = rng.gen_range(0..2);
            let mut g = Game::new(&mut rng, first);
            let me = rng.gen_range(0..2);
            let mut log = Log::default();
            for _ in 0..rng.gen_range(1..39) {
                let m = g.legal_moves().to_vec();
                let c = m[rng.gen_range(0..m.len())];
                log.before_play(&g, me, c);
                g.play(c);
            }
            if g.is_over() {
                continue;
            }
            let obs = Observation::of(&g, me);
            let b = Belief::infer(&obs, &log.events, &net, 1.0, 0.1, 50, &mut rng);
            assert!(b.ess() >= 1.0 && b.ess() <= 50.0 + 1e-9);
            let h = b.sample(&mut rng);
            assert_eq!(h.len(), g.hands[1 - me as usize].len());
            let mut possible = obs.unknown();
            possible.insert(obs.trump_card);
            assert!(h.minus(possible).is_empty());
            // Senza deduzione tutti i pesi sono uguali.
            let flat = Belief::infer(&obs, &log.events, &net, 0.0, 0.0, 50, &mut rng);
            assert!((flat.ess() - 50.0).abs() < 1e-6);
        }
    }
}
