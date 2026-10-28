//! Giocatori a regole per i test: stili tipici delle persone al tavolo.
//!
//! Prima del cambio decidono in base al quantile della mano (quante mani di partenza
//! sono peggiori, contando quello che si può pescare), dopo il cambio in base alla forza
//! della mano servita, corretta per quante carte ha cambiato l'avversario.

use crate::draw::{strength, summary};
use crate::reference::START_QUANTILES;
use crate::game::{Action, Hand, Observation, Phase};
use rand::rngs::SmallRng;
use rand::Rng;

pub trait Agent: Send {
    fn name(&self) -> String;
    fn act(&mut self, obs: &Observation, rng: &mut SmallRng) -> Action;
    /// Avviso a mano finita (`seat` è il proprio posto): serve a chi impara l'avversario.
    fn hand_over(&mut self, _h: &Hand, _seat: u8) {}
}

/// Quantile (0..1) di una mano di partenza: quante mani di partenza sono peggiori.
pub fn start_quantile(max_ev: f32) -> f32 {
    START_QUANTILES.partition_point(|&q| q < max_ev) as f32 / 100.0
}

#[derive(Clone, Debug)]
pub struct RuleBot {
    pub label: String,
    /// Primo giro, sul quantile della mano di partenza: punta, vede, rilancia.
    pub open: f32,
    pub call: f32,
    pub raise: f32,
    /// Secondo giro, sulla forza della mano servita.
    pub bet2: f32,
    pub call2: f32,
    pub raise2: f32,
    /// Probabilità di puntare o rilanciare comunque.
    pub bluff: f32,
    /// Probabilità di restare serviti con una mano debole, per bluffare.
    pub standpat_bluff: f32,
    pub never_fold: bool,
    pub always_raise: bool,
    pub random: bool,
}

impl RuleBot {
    fn base(label: &str) -> RuleBot {
        RuleBot {
            label: label.into(),
            open: 0.6,
            call: 0.5,
            raise: 0.85,
            bet2: 0.75,
            call2: 0.6,
            raise2: 0.92,
            bluff: 0.0,
            standpat_bluff: 0.0,
            never_fold: false,
            always_raise: false,
            random: false,
        }
    }

    /// Stili disponibili per nome (vedi `STYLES`).
    pub fn by_name(name: &str) -> Option<RuleBot> {
        let mut b = RuleBot::base(name);
        match name {
            "random" => b.random = true,
            "station" => b.never_fold = true,
            "maniac" => b.always_raise = true,
            "rock" => {
                (b.open, b.call, b.raise) = (0.85, 0.75, 0.93);
                (b.bet2, b.call2, b.raise2) = (0.9, 0.8, 0.97);
            }
            "bar" => {
                (b.open, b.call, b.raise) = (0.65, 0.55, 0.85);
                (b.bet2, b.call2, b.raise2) = (0.75, 0.6, 0.92);
                b.bluff = 0.05;
            }
            "tag" => {
                (b.open, b.call, b.raise) = (0.7, 0.62, 0.85);
                (b.bet2, b.call2, b.raise2) = (0.7, 0.65, 0.9);
                (b.bluff, b.standpat_bluff) = (0.1, 0.03);
            }
            "lag" => {
                (b.open, b.call, b.raise) = (0.45, 0.4, 0.7);
                (b.bet2, b.call2, b.raise2) = (0.55, 0.45, 0.85);
                (b.bluff, b.standpat_bluff) = (0.25, 0.08);
            }
            "passive" => {
                (b.open, b.call, b.raise) = (0.95, 0.3, 0.99);
                (b.bet2, b.call2, b.raise2) = (0.93, 0.35, 0.995);
            }
            "bluffer" => {
                (b.open, b.call, b.raise) = (0.6, 0.55, 0.85);
                (b.bet2, b.call2, b.raise2) = (0.7, 0.6, 0.9);
                (b.bluff, b.standpat_bluff) = (0.4, 0.2);
            }
            _ => return None,
        }
        Some(b)
    }

    /// Un giocatore "umano" preso a caso da una popolazione: soglie, bluff e testardaggine
    /// variano da persona a persona (per i test contro avversari mai visti).
    pub fn random_person(rng: &mut SmallRng) -> RuleBot {
        let mut b = RuleBot::base("pop");
        let mut u = |lo: f32, hi: f32| lo + (hi - lo) * rng.gen::<f32>();
        b.open = u(0.4, 0.92);
        b.call = u(0.25, 0.8);
        b.raise = u(b.open.max(b.call) + 0.05, 0.98);
        b.bet2 = u(0.45, 0.93);
        b.call2 = u(0.25, 0.8);
        b.raise2 = u(b.bet2.max(b.call2) + 0.03, 0.99);
        b.bluff = u(0.0, 0.3);
        b.standpat_bluff = u(0.0, 0.12);
        b.never_fold = u(0.0, 1.0) < 0.1;
        b
    }

    pub fn random_action(obs: &Observation, rng: &mut SmallRng) -> Action {
        match obs.phase {
            Phase::Draw => {
                let k = rng.gen_range(0..6);
                Action::Draw(summary(&obs.hand).best[k])
            }
            _ => {
                let acts = crate::abstraction::actions(obs);
                acts[rng.gen_range(0..acts.len())]
            }
        }
    }

    /// Decisione di puntata con forza `s` e soglie già corrette.
    fn bet_decision(&self, obs: &Observation, s: f32, open: f32, call: f32, raise: f32, rng: &mut SmallRng) -> Action {
        let bluff = rng.gen::<f32>() < self.bluff;
        if obs.to_call == 0 {
            if obs.can_raise && (s >= open || bluff) { Action::Raise } else { Action::Call }
        } else {
            // Ogni puntata in più alza le soglie.
            let extra = obs.bets_this_round().saturating_sub(1) as f32 * 0.05;
            if obs.can_raise && (s >= raise + extra || (bluff && s >= call)) {
                Action::Raise
            } else if s >= call + extra || self.never_fold {
                Action::Call
            } else {
                Action::Fold
            }
        }
    }
}

impl Agent for RuleBot {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn act(&mut self, obs: &Observation, rng: &mut SmallRng) -> Action {
        if self.random {
            return RuleBot::random_action(obs, rng);
        }
        if self.always_raise && obs.phase != Phase::Draw {
            return if obs.can_raise { Action::Raise } else { Action::Call };
        }
        match obs.phase {
            Phase::Bet1 => {
                let q = start_quantile(summary(&obs.hand).max_ev());
                self.bet_decision(obs, q, self.open, self.call, self.raise, rng)
            }
            Phase::Draw => {
                let d = summary(&obs.hand);
                if d.max_ev() < 0.5 && rng.gen::<f32>() < self.standpat_bluff {
                    Action::Draw(0)
                } else {
                    Action::Draw(d.best[d.natural()])
                }
            }
            Phase::Bet2 => {
                let s = strength(&obs.hand);
                // Contro chi è rimasto servito servono mani migliori, contro chi ha cambiato
                // tre carte o più ne bastano di peggiori.
                let adj = |t: f32| match obs.opp_drawn {
                    Some(0) => t + (1.0 - t) * 0.5,
                    Some(k) if k >= 3 => t * 0.9,
                    _ => t,
                };
                self.bet_decision(obs, s, adj(self.bet2), adj(self.call2), adj(self.raise2), rng)
            }
            Phase::Over => unreachable!(),
        }
    }
}

/// Un giocatore distratto: con probabilità `p` fa una mossa a caso.
pub struct Noisy {
    pub inner: Box<dyn Agent>,
    pub p: f32,
}

impl Agent for Noisy {
    fn name(&self) -> String {
        format!("noisy:{}:{}", self.p, self.inner.name())
    }

    fn act(&mut self, obs: &Observation, rng: &mut SmallRng) -> Action {
        if rng.gen::<f32>() < self.p {
            RuleBot::random_action(obs, rng)
        } else {
            self.inner.act(obs, rng)
        }
    }

    fn hand_over(&mut self, h: &Hand, seat: u8) {
        self.inner.hand_over(h, seat);
    }
}

/// Gli stili a regole, con una descrizione per i rapporti.
pub const STYLES: &[(&str, &str)] = &[
    ("random", "Azioni a caso"),
    ("station", "Stazione di chiamata: vede sempre, non lascia e non rilancia mai"),
    ("maniac", "Maniaco: punta e rilancia sempre"),
    ("rock", "Roccia: gioca solo le mani molto forti"),
    ("bar", "Giocatore da bar: regole di buon senso, bluffa di rado"),
    ("tag", "Stretto-aggressivo: poche mani, giocate con decisione, qualche bluff"),
    ("lag", "Largo-aggressivo: molte mani, molti bluff"),
    ("passive", "Largo-passivo: vede molto, punta solo con mani fortissime"),
    ("bluffer", "Bluffatore: bluffa spesso e resta servito con mani deboli"),
];
