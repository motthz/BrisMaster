//! Il motore: gioca la strategia allenata con CFR (inclusa nel programma).

use crate::abstraction;
use crate::bots::{Agent, RuleBot};
use crate::draw::summary;
use crate::game::{Action, Observation, Phase};
use crate::strategy::Strategy;
use rand::rngs::SmallRng;
use rand::Rng;
use std::sync::{Arc, OnceLock};

static BUNDLED: &[u8] = include_bytes!("../data/strategy.bin");

/// La strategia allenata inclusa nel programma (`poker-it-sim train`).
pub fn bundled() -> Arc<Strategy> {
    static S: OnceLock<Arc<Strategy>> = OnceLock::new();
    S.get_or_init(|| Arc::new(Strategy::from_bytes(BUNDLED).expect("strategy.bin non valido"))).clone()
}

pub struct Engine {
    strat: Arc<Strategy>,
    label: String,
    fallback: RuleBot,
}

impl Engine {
    pub fn new(strat: Arc<Strategy>, label: &str) -> Engine {
        Engine { strat, label: label.into(), fallback: RuleBot::by_name("tag").unwrap() }
    }

    pub fn bundled() -> Engine {
        Engine::new(bundled(), "engine")
    }

    pub fn strategy(&self) -> &Strategy {
        &self.strat
    }

    /// Le azioni concrete possibili con la probabilità che il motore dà a ciascuna.
    /// Nel cambio le azioni sono i migliori scarti da 0 a 5 carte.
    pub fn advice(&self, obs: &Observation) -> Option<Vec<(Action, f32)>> {
        Some(concrete(obs, self.strat.probs(obs)?))
    }
}

/// Dalle probabilità delle azioni astratte alle azioni concrete.
pub fn concrete(obs: &Observation, probs: Vec<f32>) -> Vec<(Action, f32)> {
    let acts = abstraction::actions(obs);
    let d = (obs.phase == Phase::Draw).then(|| summary(&obs.hand));
    acts.into_iter()
        .zip(probs)
        .map(|(a, p)| match (a, &d) {
            (Action::Draw(k), Some(d)) => (Action::Draw(d.best[k as usize]), p),
            _ => (a, p),
        })
        .collect()
}

/// Estrae un'azione dal consiglio.
pub fn pick(adv: &[(Action, f32)], rng: &mut SmallRng) -> Action {
    let mut x: f32 = rng.gen();
    for &(a, p) in adv {
        x -= p;
        if x < 0.0 {
            return a;
        }
    }
    adv.last().unwrap().0
}

impl Agent for Engine {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn act(&mut self, obs: &Observation, rng: &mut SmallRng) -> Action {
        match self.advice(obs) {
            Some(adv) => pick(&adv, rng),
            None => self.fallback.act(obs, rng),
        }
    }
}
