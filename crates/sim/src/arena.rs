//! Partite in duplicato e statistiche sulle coppie di partite.
//!
//! Ogni mazzo viene giocato due volte a posti invertiti, così la fortuna delle carte
//! si compensa. Le due partite dello stesso mazzo non sono indipendenti: l'intervallo
//! di confidenza del punteggio si calcola sulla media per mazzo, che è il modo corretto
//! (e più stretto) di misurare la differenza di forza.

use crate::agents::{make_agent, Agent};
use briscola_core::cards::{Card, NUM_CARDS};
use briscola_core::stats::Tally;
use briscola_core::Game;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct MatchStats {
    pub tally: Tally,
    /// Vittorie del giocatore quando apre la prima presa (posto 0) e quando risponde (posto 1).
    pub seat_wins: [u64; 2],
    /// Numero di mazzi, somma e somma dei quadrati del punteggio per mazzo (0..1).
    pub deals: u64,
    pub sum: f64,
    pub sum_sq: f64,
}

impl MatchStats {
    pub fn merge(mut self, o: MatchStats) -> MatchStats {
        self.tally.merge(&o.tally);
        self.seat_wins[0] += o.seat_wins[0];
        self.seat_wins[1] += o.seat_wins[1];
        self.deals += o.deals;
        self.sum += o.sum;
        self.sum_sq += o.sum_sq;
        self
    }

    /// Punteggio medio: vittoria 1, pareggio 0,5, sconfitta 0. Il 50% è la parità.
    pub fn score(&self) -> f64 {
        self.sum / self.deals as f64
    }

    /// Intervallo di confidenza al 95% del punteggio, sulle medie per mazzo.
    pub fn score_ci95(&self) -> (f64, f64) {
        let n = self.deals as f64;
        if n < 2.0 {
            return (0.0, 1.0);
        }
        let mean = self.score();
        let var = (self.sum_sq - n * mean * mean) / (n - 1.0);
        let half = 1.959964 * (var.max(0.0) / n).sqrt();
        ((mean - half).max(0.0), (mean + half).min(1.0))
    }

    /// Differenza di forza in punti Elo corrispondente al punteggio.
    pub fn elo(&self) -> f64 {
        let s = self.score().clamp(1e-4, 1.0 - 1e-4);
        -400.0 * (1.0 / s - 1.0).log10()
    }
}

fn outcome(pts: u8) -> f64 {
    if pts > 60 { 1.0 } else if pts == 60 { 0.5 } else { 0.0 }
}

fn play(order: &[Card], agents: &mut [Box<dyn Agent>; 2], rng: &mut SmallRng) -> [u8; 2] {
    let mut g = Game::from_order(order, 0);
    for a in agents.iter_mut() {
        a.start(order, 0);
    }
    while !g.is_over() {
        let p = g.to_move();
        let c = agents[p as usize].choose(&g, p, rng);
        assert!(g.hands[p as usize].contains(c), "mossa illegale da {}", agents[p as usize].name());
        g.play(c);
        for a in agents.iter_mut() {
            a.observe(c);
        }
    }
    g.points
}

/// `deals` mazzi in duplicato tra `player` e `opponent`. Stesso seme, stessi mazzi:
/// due giocatori diversi contro lo stesso avversario vedono le stesse carte.
pub fn run_match(player: &str, opponent: &str, deals: u64, seed: u64) -> Result<MatchStats, String> {
    Ok(run_deals(player, opponent, deals, seed)?.into_iter().fold(MatchStats::default(), MatchStats::merge))
}

/// Come `run_match`, ma restituisce il risultato di ogni mazzo, in ordine.
pub fn run_deals(player: &str, opponent: &str, deals: u64, seed: u64) -> Result<Vec<MatchStats>, String> {
    make_agent(player)?;
    make_agent(opponent)?;
    Ok((0..deals)
        .into_par_iter()
        .map(|d| {
            let mut s = MatchStats { deals: 1, ..Default::default() };
            let mut deal_rng = SmallRng::seed_from_u64(seed ^ d.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut order: Vec<Card> = (0..NUM_CARDS as u8).collect();
            order.shuffle(&mut deal_rng);
            let mut pair = 0.0;
            for seat in 0..2usize {
                let me = make_agent(player).unwrap();
                let opp = make_agent(opponent).unwrap();
                let mut agents = if seat == 0 { [me, opp] } else { [opp, me] };
                let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(d * 2 + seat as u64));
                let pts = play(&order, &mut agents, &mut rng)[seat];
                s.tally.add(pts);
                if pts > 60 {
                    s.seat_wins[seat] += 1;
                }
                pair += outcome(pts) / 2.0;
            }
            s.sum = pair;
            s.sum_sq = pair * pair;
            s
        })
        .collect())
}

/// Statistiche cumulative sui primi `n` mazzi, per ogni `n` in `checkpoints`.
pub fn prefixes(per_deal: &[MatchStats], checkpoints: &[u64]) -> Vec<MatchStats> {
    checkpoints
        .iter()
        .map(|&n| per_deal[..(n as usize).min(per_deal.len())].iter().copied().fold(MatchStats::default(), MatchStats::merge))
        .collect()
}

pub fn describe(spec: &str) -> String {
    make_agent(spec).map(|a| a.name()).unwrap_or_else(|e| e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_cancels_luck_and_legacy_stays_in_sync() {
        // Due copie dello stesso giocatore deterministico, a posti invertiti sullo stesso mazzo,
        // fanno esattamente metà a testa: verifica il duplicato e la copia di partita delle versioni vecchie.
        let s = run_match("v0.1.1/heuristic", "v0.1.1/heuristic", 50, 1).unwrap();
        assert_eq!(s.tally.games(), 100);
        assert!((s.score() - 0.5).abs() < 1e-9, "punteggio {}", s.score());
        let s = run_match("engine:20", "v0.1.1/engine:20", 20, 1).unwrap();
        assert_eq!(s.tally.games(), 40);
    }

    #[test]
    fn stronger_player_scores_more() {
        let s = run_match("engine:30", "random", 40, 2).unwrap();
        let (lo, _) = s.score_ci95();
        assert!(lo > 0.6, "punteggio {} (IC basso {lo})", s.score());
    }
}
