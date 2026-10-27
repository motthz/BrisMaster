//! Mani in duplicato e statistiche sulle coppie di mani.
//!
//! Ogni mazzo si gioca due volte con le stesse carte a posti invertiti: la fortuna delle
//! carte si compensa in gran parte. Le due mani dello stesso mazzo non sono indipendenti,
//! quindi l'intervallo di confidenza si calcola sulla media per mazzo.

use poker_it::adaptive::{profiles_from_bytes, Adaptive};
use poker_it::bots::{Agent, Noisy, RuleBot};
use poker_it::cards::{Card, NUM_CARDS};
use poker_it::abstraction::Abstraction;
use poker_it::cfr::{Model, Trainer};
use poker_it::engine::{bundled, Engine};
use poker_it::{Hand, Observation, Strategy};
use rand::Rng;
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
    let (kind, rest) = spec.split_once(':').unwrap();
    let (n, target) = rest.split_once(':').unwrap_or((rest, "engine"));
    let iters: u64 = n.parse().unwrap();
    let t = std::time::Instant::now();
    let tr = if kind == "cfr" {
        Trainer::new(Abstraction::standard())
    } else {
        let target = strategy_of(target).unwrap();
        let abs = target.abs.clone();
        Trainer::exploiter(Model::Strategy(target), abs)
    };
    tr.run(iters, 0xC0FFEE ^ iters);
    eprintln!("  {spec}: allenato in {:.0}s", t.elapsed().as_secs_f64());
    let s = Arc::new(tr.strategy());
    guard.insert(spec.to_string(), s.clone());
    s
}

fn profiles_file(path: &str) -> Result<Arc<Vec<poker_it::adaptive::Profile>>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Vec<poker_it::adaptive::Profile>>>>> = OnceLock::new();
    let mut guard = CACHE.get_or_init(Default::default).lock().unwrap();
    if let Some(s) = guard.get(path) {
        return Ok(s.clone());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let p = Arc::new(profiles_from_bytes(&bytes).ok_or_else(|| format!("{path}: stili non validi"))?);
    guard.insert(path.to_string(), p.clone());
    Ok(p)
}

/// Una strategia salvata su file (`poker-it-sim train --out`), letta una volta sola.
fn from_file(path: &str) -> Result<Arc<Strategy>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Strategy>>>> = OnceLock::new();
    let mut guard = CACHE.get_or_init(Default::default).lock().unwrap();
    if let Some(s) = guard.get(path) {
        return Ok(s.clone());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let s = Arc::new(Strategy::from_bytes(&bytes).ok_or_else(|| format!("{path}: strategia non valida"))?);
    guard.insert(path.to_string(), s.clone());
    Ok(s)
}

/// Un giocatore che cambia stile dopo `n` mani.
struct Switch {
    a: Box<dyn Agent>,
    b: Box<dyn Agent>,
    n: u32,
    played: u32,
}

impl Agent for Switch {
    fn name(&self) -> String {
        format!("switch:{}:{}>{}", self.n, self.a.name(), self.b.name())
    }

    fn act(&mut self, obs: &Observation, rng: &mut SmallRng) -> poker_it::Action {
        if self.played < self.n { self.a.act(obs, rng) } else { self.b.act(obs, rng) }
    }

    fn hand_over(&mut self, h: &Hand, seat: u8) {
        self.played += 1;
        self.a.hand_over(h, seat);
        self.b.hand_over(h, seat);
    }
}

/// Come `make_agent`, con un seme per i giocatori presi a caso: `pop` è una persona presa a
/// caso dalla popolazione, `pop:P` la stessa con una mossa a caso ogni tanto (fino a P),
/// `switch:N:A>B` gioca come A per N mani e poi come B.
pub fn make_agent_seeded(spec: &str, seed: u64) -> Result<Box<dyn Agent>, String> {
    let mut rng = SmallRng::seed_from_u64(seed);
    if spec == "pop" || spec.starts_with("pop:") {
        let bot = RuleBot::random_person(&mut rng);
        let max_noise: f32 = spec.strip_prefix("pop:").map_or(Ok(0.0), str::parse).map_err(|_| format!("rumore non valido: {spec}"))?;
        let p = max_noise * rng.gen::<f32>();
        return Ok(Box::new(Noisy { inner: Box::new(bot), p }));
    }
    if let Some(rest) = spec.strip_prefix("switch:") {
        let err = || "switch:N:A>B".to_string();
        let (n, ab) = rest.split_once(':').ok_or_else(err)?;
        let (a, b) = ab.split_once('>').ok_or_else(err)?;
        let n: u32 = n.parse().map_err(|_| format!("mani non valide: {n}"))?;
        return Ok(Box::new(Switch { a: make_agent_seeded(a, seed ^ 1)?, b: make_agent_seeded(b, seed ^ 2)?, n, played: 0 }));
    }
    make_agent(spec)
}

/// La strategia di un giocatore allenato: `engine`, `file:PERCORSO`, `profile:[FILE:]STILE`.
fn strategy_of(spec: &str) -> Result<Arc<Strategy>, String> {
    if spec == "engine" {
        return Ok(bundled());
    }
    if let Some(path) = spec.strip_prefix("file:") {
        return from_file(path);
    }
    if let Some(rest) = spec.strip_prefix("profile:") {
        let (p, name) = match rest.rsplit_once(':') {
            Some((path, name)) => (profiles_file(path)?, name),
            None => (poker_it::adaptive::bundled_profiles(), rest),
        };
        return p.iter().find(|p| p.name == name).and_then(|p| p.strategy.clone()).ok_or_else(|| format!("stile {name} non trovato"));
    }
    Err(format!("non è una strategia allenata: {spec}"))
}

pub fn make_agent(spec: &str) -> Result<Box<dyn Agent>, String> {
    if spec == "engine" {
        return Ok(Box::new(Engine::bundled()));
    }
    if spec == "adaptive" {
        return Ok(Box::new(Adaptive::bundled()));
    }
    // profile:FILE:STILE gioca sempre la risposta a quello stile.
    if spec.starts_with("profile:") {
        return Ok(Box::new(Engine::new(strategy_of(spec)?, spec)));
    }
    // adaptive:FILE[:PRIORE:TEMPERATURA:MEMORIA] con gli stili letti da un file.
    if let Some(rest) = spec.strip_prefix("adaptive:") {
        let mut parts = rest.split(':');
        let path = parts.next().unwrap();
        let mut a = Adaptive::new(bundled(), profiles_file(path)?, spec);
        if let Some(x) = parts.next() {
            a.prior_eq = x.parse().map_err(|_| format!("priore non valido: {x}"))?;
        }
        if let Some(x) = parts.next() {
            a.temper = x.parse().map_err(|_| format!("temperatura non valida: {x}"))?;
        }
        if let Some(x) = parts.next() {
            a.decay = x.parse().map_err(|_| format!("memoria non valida: {x}"))?;
        }
        return Ok(Box::new(a));
    }
    if let Some(rest) = spec.strip_prefix("noisy:") {
        let (p, inner) = rest.split_once(':').ok_or("noisy:P:SPEC")?;
        let p: f32 = p.parse().map_err(|_| format!("probabilità non valida: {p}"))?;
        return Ok(Box::new(Noisy { inner: make_agent(inner)?, p }));
    }
    if let Some(path) = spec.strip_prefix("file:") {
        return Ok(Box::new(Engine::new(from_file(path)?, spec)));
    }
    if spec.starts_with("cfr:") || spec.starts_with("exploit:") {
        let rest = spec.split_once(':').unwrap().1;
        let (n, target) = rest.split_once(':').unwrap_or((rest, "engine"));
        n.parse::<u64>().map_err(|_| format!("iterazioni non valide: {spec}"))?;
        strategy_of(target)?;
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
    for (seat, ag) in agents.iter_mut().enumerate() {
        ag.hand_over(&h, seat as u8);
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

/// Come gioca `style` contro il motore: i conteggi della lettura dell'avversario su `hands` mani.
pub fn style_stats(style: &str, hands: u64, seed: u64) -> Result<poker_it::opponent::Stats, String> {
    make_agent(style)?;
    let threads = rayon::current_num_threads() as u64;
    Ok((0..threads)
        .into_par_iter()
        .map(|t| {
            let mut stats = poker_it::opponent::Stats::default();
            let mut rng = SmallRng::seed_from_u64(seed ^ (t + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut agents: [Box<dyn Agent>; 2] = [make_agent("engine").unwrap(), make_agent(style).unwrap()];
            for i in 0..hands / threads {
                let mut deck: [Card; NUM_CARDS] = core::array::from_fn(|i| i as Card);
                deck.shuffle(&mut rng);
                let seat = (i % 2) as u8;
                agents.swap(0, 1);
                // Dopo lo scambio lo stile è al posto `seat`.
                let mut h = Hand::new(deck);
                while !h.is_over() {
                    let p = h.to_act;
                    let a = agents[p as usize].act(&Observation::of(&h, p), &mut rng);
                    h.apply(a);
                }
                stats.record(&h.history, seat, h.showdown().then(|| &h.hands[seat as usize]));
            }
            stats
        })
        .reduce(Default::default, |mut a, b| {
            a.merge(&b);
            a
        }))
}

/// Sessioni di `len` mani contro lo stesso avversario, con i giocatori che ricordano le mani
/// precedenti. Il mazziere si alterna a ogni mano. Ogni sessione si gioca due volte con gli
/// stessi mazzi a posti invertiti (e giocatori nuovi): per ogni coppia restituisce le fiche
/// vinte nelle due sessioni.
pub fn run_sessions(player: &str, opponent: &str, sessions: u64, len: u64, seed: u64) -> Result<Vec<[f64; 2]>, String> {
    make_agent_seeded(player, 0)?;
    make_agent_seeded(opponent, 0)?;
    Ok((0..sessions)
        .into_par_iter()
        .map(|s| {
            let sseed = seed ^ s.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            let mut out = [0.0; 2];
            for (mirror, total) in out.iter_mut().enumerate() {
                let mut me = make_agent_seeded(player, sseed ^ 0x51).unwrap();
                let mut opp = make_agent_seeded(opponent, sseed ^ 0x0FF).unwrap();
                let mut deal_rng = SmallRng::seed_from_u64(sseed);
                let mut rng = SmallRng::seed_from_u64(sseed ^ (mirror as u64 + 1) * 0xABCDEF);
                for i in 0..len {
                    let mut deck: [Card; NUM_CARDS] = core::array::from_fn(|i| i as Card);
                    deck.shuffle(&mut deal_rng);
                    let seat = ((i + mirror as u64) % 2) as usize;
                    let mut agents: [Box<dyn Agent>; 2] = if seat == 0 { [me, opp] } else { [opp, me] };
                    let (pay, _) = play(deck, &mut agents, &mut rng);
                    *total += pay[seat] as f64;
                    let [a, b] = agents;
                    (me, opp) = if seat == 0 { (a, b) } else { (b, a) };
                }
            }
            out
        })
        .collect())
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
