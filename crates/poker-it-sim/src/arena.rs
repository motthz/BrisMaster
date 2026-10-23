//! Mani in duplicato e statistiche sulle coppie di mani.
//!
//! Ogni mazzo si gioca due volte con le stesse carte a posti invertiti: la fortuna delle
//! carte si compensa in gran parte. Le due mani dello stesso mazzo non sono indipendenti,
//! quindi l'intervallo di confidenza si calcola sulla media per mazzo.

use poker_it::bots::{Agent, Noisy, RuleBot};
use poker_it::cards::{Card, NUM_CARDS};
use poker_it::cfr::{pre_thresholds, Trainer};
use poker_it::engine::{bundled, Engine};
use poker_it::{Hand, Observation, Strategy};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Debug, Default)]
pub struct MatchStats {
    pub deals: u64,
    pub hands: u64,
    /// Mani in cui il giocatore ha vinto il piatto, e mani finite a showdown.
    pub won: u64,
    pub showdowns: u64,
    /// Somma e somma dei quadrati delle fiche medie per mazzo.
    pub sum: f64,
    pub sum_sq: f64,
}

impl MatchStats {
    pub fn merge(mut self, o: MatchStats) -> MatchStats {
        self.deals += o.deals;
        self.hands += o.hands;
        self.won += o.won;
        self.showdowns += o.showdowns;
        self.sum += o.sum;
        self.sum_sq += o.sum_sq;
        self
    }

    /// Fiche vinte in media per mano.
    pub fn mean(&self) -> f64 {
        self.sum / self.deals as f64
    }

    pub fn ci95(&self) -> (f64, f64) {
        let n = self.deals as f64;
        if n < 2.0 {
            return (f64::NEG_INFINITY, f64::INFINITY);
        }
        let m = self.mean();
        let var = (self.sum_sq - n * m * m) / (n - 1.0);
        let half = 1.959964 * (var.max(0.0) / n).sqrt();
        (m - half, m + half)
    }

    pub fn summary(&self) -> String {
        let (lo, hi) = self.ci95();
        format!(
            "{:+.3} fiche/mano (IC 95% {lo:+.3} .. {hi:+.3}), {} mazzi, mani vinte {:.1}%",
            self.mean(),
            self.deals,
            100.0 * self.won as f64 / self.hands as f64
        )
    }
}

/// Strategie allenate al momento, condivise tra i thread.
fn trained(spec: &str) -> Arc<Strategy> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Strategy>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut guard = cache.lock().unwrap();
    if let Some(s) = guard.get(spec) {
        return s.clone();
    }
    let (kind, n) = spec.split_once(':').unwrap();
    let iters: u64 = n.parse().unwrap();
    let mut rng = SmallRng::seed_from_u64(0xC0FFEE ^ iters);
    let t = std::time::Instant::now();
    let mut tr = if kind == "cfr" { Trainer::new(pre_thresholds()) } else { Trainer::exploiter(bundled()) };
    tr.run(iters, &mut rng);
    eprintln!("  {spec}: allenato in {:.0}s", t.elapsed().as_secs_f64());
    let s = Arc::new(tr.strategy());
    guard.insert(spec.to_string(), s.clone());
    s
}

pub fn make_agent(spec: &str) -> Result<Box<dyn Agent>, String> {
    if spec == "engine" {
        return Ok(Box::new(Engine::bundled()));
    }
    if let Some(rest) = spec.strip_prefix("noisy:") {
        let (p, inner) = rest.split_once(':').ok_or("noisy:P:SPEC")?;
        let p: f32 = p.parse().map_err(|_| format!("probabilità non valida: {p}"))?;
        return Ok(Box::new(Noisy { inner: make_agent(inner)?, p }));
    }
    if spec.starts_with("cfr:") || spec.starts_with("exploit:") {
        spec.split_once(':').unwrap().1.parse::<u64>().map_err(|_| format!("iterazioni non valide: {spec}"))?;
        return Ok(Box::new(Engine::new(trained(spec), spec)));
    }
    RuleBot::by_name(spec).map(|b| Box::new(b) as Box<dyn Agent>).ok_or_else(|| format!("giocatore sconosciuto: {spec}"))
}

/// Gioca una mano; restituisce il guadagno di ciascun posto e se si è arrivati allo showdown.
pub fn play(deck: [Card; NUM_CARDS], agents: &mut [Box<dyn Agent>; 2], rng: &mut SmallRng) -> ([i32; 2], bool) {
    let mut h = Hand::new(deck);
    while !h.is_over() {
        let p = h.to_act;
        let obs = Observation::of(&h, p);
        let a = agents[p as usize].act(&obs, rng);
        assert!(
            matches!(a, poker_it::Action::Draw(m) if h.phase == poker_it::Phase::Draw && m < 32) || h.legal().contains(&a),
            "azione illegale {a:?} da {}",
            agents[p as usize].name()
        );
        h.apply(a);
    }
    (h.payoff(), h.showdown())
}

pub fn run_deals(player: &str, opponent: &str, deals: std::ops::Range<u64>, seed: u64) -> Vec<MatchStats> {
    // Crea subito le strategie da allenare, una volta sola.
    make_agent(player).unwrap();
    make_agent(opponent).unwrap();
    deals
        .into_par_iter()
        .map(|d| {
            let mut deal_rng = SmallRng::seed_from_u64(seed ^ d.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut deck: [Card; NUM_CARDS] = core::array::from_fn(|i| i as Card);
            deck.shuffle(&mut deal_rng);
            let mut s = MatchStats { deals: 1, ..Default::default() };
            let mut pair = 0.0;
            for seat in 0..2usize {
                let me = make_agent(player).unwrap();
                let opp = make_agent(opponent).unwrap();
                let mut agents = if seat == 0 { [me, opp] } else { [opp, me] };
                let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(d * 2 + seat as u64));
                let (pay, sd) = play(deck, &mut agents, &mut rng);
                s.hands += 1;
                s.won += (pay[seat] > 0) as u64;
                s.showdowns += sd as u64;
                pair += pay[seat] as f64 / 2.0;
            }
            s.sum = pair;
            s.sum_sq = pair * pair;
            s
        })
        .collect()
}

pub fn run_match(player: &str, opponent: &str, deals: u64, seed: u64) -> Result<MatchStats, String> {
    make_agent(player)?;
    make_agent(opponent)?;
    Ok(run_deals(player, opponent, 0..deals, seed).into_iter().fold(MatchStats::default(), MatchStats::merge))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_is_fair_for_identical_deterministic_players() {
        let s = run_match("rock", "rock", 200, 3).unwrap();
        assert!(s.mean().abs() < 1e-9, "{}", s.mean());
    }
}
