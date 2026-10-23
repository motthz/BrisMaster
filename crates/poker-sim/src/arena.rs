//! Mani in duplicato e statistiche sulle coppie di mani.
//!
//! Ogni smazzata (carte coperte e board) si gioca due volte a posti invertiti: la fortuna
//! delle carte si compensa in gran parte. Quando i giocatori vanno all-in prima del river si
//! conta il valore atteso sulle carte mancanti invece della carta uscita: la media non cambia,
//! la varianza scende molto. L'intervallo di confidenza si calcola sulla media per smazzata.

use poker_core::bots::{Noisy, RuleBot};
use poker_core::buckets::Bucketer;
use poker_core::cfr::{Strategy, Trainer};
use poker_core::engine::{bundled, Engine};
use poker_core::game::BB;
use poker_core::{play_hand, Agent, Hand};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Debug, Default)]
pub struct MatchStats {
    pub deals: u64,
    pub hands: u64,
    pub won: u64,
    pub showdowns: u64,
    /// Somma e somma dei quadrati dei grandi bui medi per smazzata (valore atteso negli all-in).
    pub sum: f64,
    pub sum_sq: f64,
    /// Somma dei risultati veri, senza correzione degli all-in.
    pub raw: f64,
}

impl MatchStats {
    pub fn merge(mut self, o: MatchStats) -> MatchStats {
        self.deals += o.deals;
        self.hands += o.hands;
        self.won += o.won;
        self.showdowns += o.showdowns;
        self.sum += o.sum;
        self.sum_sq += o.sum_sq;
        self.raw += o.raw;
        self
    }

    /// Grandi bui vinti ogni 100 mani.
    pub fn bb100(&self) -> f64 {
        100.0 * self.sum / self.deals as f64
    }

    pub fn raw_bb100(&self) -> f64 {
        100.0 * self.raw / self.deals as f64
    }

    pub fn ci95(&self) -> (f64, f64) {
        let n = self.deals as f64;
        if n < 2.0 {
            return (f64::NEG_INFINITY, f64::INFINITY);
        }
        let m = self.sum / n;
        let var = (self.sum_sq - n * m * m) / (n - 1.0);
        let half = 1.959964 * (var.max(0.0) / n).sqrt();
        (100.0 * (m - half), 100.0 * (m + half))
    }

    pub fn summary(&self) -> String {
        let (lo, hi) = self.ci95();
        format!(
            "{:+.1} bb/100 (IC 95% {lo:+.1} .. {hi:+.1}; senza correzione all-in {:+.1}), {} smazzate, mani vinte {:.1}%",
            self.bb100(),
            self.raw_bb100(),
            self.deals,
            100.0 * self.won as f64 / self.hands as f64
        )
    }
}

pub fn threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// Strategie allenate al momento (`cfr:N`, `exploit:N`), condivise tra i thread.
fn trained(spec: &str) -> Arc<Strategy> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Strategy>>>> = OnceLock::new();
    let mut guard = CACHE.get_or_init(Default::default).lock().unwrap();
    if let Some(s) = guard.get(spec) {
        return s.clone();
    }
    let (kind, n) = spec.split_once(':').unwrap();
    let iters: u64 = n.parse().unwrap();
    let t = std::time::Instant::now();
    let tr = if kind == "cfr" { Trainer::new(bundled().bucketer.clone()) } else { Trainer::exploiter(bundled()) };
    tr.run(iters, threads(), 0xC0FFEE ^ iters);
    eprintln!("  {spec}: allenato in {:.0}s", t.elapsed().as_secs_f64());
    let s = Arc::new(tr.strategy());
    guard.insert(spec.to_string(), s.clone());
    s
}

/// Soglie dei bucket (per `train`): servono a tutti, si calcolano una volta.
pub fn fresh_bucketer() -> Bucketer {
    Bucketer::compute(200_000, 20261023)
}

pub fn make_agent(spec: &str) -> Result<Box<dyn Agent>, String> {
    if spec == "engine" {
        return Ok(Box::new(Engine::bundled()));
    }
    if let Some(rest) = spec.strip_prefix("noisy:") {
        let (p, inner) = rest.split_once(':').ok_or("noisy:P:SPEC")?;
        let p: f64 = p.parse().map_err(|_| format!("probabilità non valida: {p}"))?;
        return Ok(Box::new(Noisy { inner: make_agent(inner)?, p }));
    }
    if spec.starts_with("cfr:") || spec.starts_with("exploit:") {
        spec.split_once(':').unwrap().1.parse::<u64>().map_err(|_| format!("iterazioni non valide: {spec}"))?;
        return Ok(Box::new(Engine::new(trained(spec), spec)));
    }
    RuleBot::by_name(spec).map(|b| Box::new(b) as Box<dyn Agent>).ok_or_else(|| format!("giocatore sconosciuto: {spec}"))
}

pub fn run_deals(player: &str, opponent: &str, deals: std::ops::Range<u64>, seed: u64) -> Vec<MatchStats> {
    make_agent(player).unwrap();
    make_agent(opponent).unwrap();
    deals
        .into_par_iter()
        .map(|d| {
            let mut deal_rng = SmallRng::seed_from_u64(seed ^ d.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let base = Hand::deal(&mut deal_rng, (d % 2) as u8);
            let mut s = MatchStats { deals: 1, ..Default::default() };
            let (mut pair, mut raw) = (0.0, 0.0);
            for seat in 0..2usize {
                let mut me = make_agent(player).unwrap();
                let mut opp = make_agent(opponent).unwrap();
                let mut h = base.clone();
                let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(d * 2 + seat as u64));
                {
                    let mut agents: [&mut dyn Agent; 2] =
                        if seat == 0 { [me.as_mut(), opp.as_mut()] } else { [opp.as_mut(), me.as_mut()] };
                    play_hand(&mut h, &mut agents, &mut rng);
                }
                let r = h.result(seat);
                s.hands += 1;
                s.won += (r > 0.0) as u64;
                s.showdowns += h.folded.is_none() as u64;
                pair += h.result_ev(seat) / BB as f64 / 2.0;
                raw += r / BB as f64 / 2.0;
            }
            s.sum = pair;
            s.sum_sq = pair * pair;
            s.raw = raw;
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
        let s = run_match("station", "station", 300, 3).unwrap();
        assert!(s.bb100().abs() < 1e-9, "{}", s.bb100());
    }
}
