//! Partite a 11 punti in duplicato e statistiche sulle coppie di partite.
//!
//! Ogni "mazzo" è in realtà una sequenza di mazzi, uno per smazzata, che dipende solo dal seme
//! e dal numero del mazzo. La si gioca due volte a posti invertiti: la fortuna delle carte
//! si compensa (in parte: le partite possono durare un numero diverso di smazzate).

use crate::agents::make_player;
use briscola_core::stats::wilson95;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rayon::prelude::*;
use scopa_core::game::{match_winner, Card, Game, NUM_CARDS};
use scopa_core::{play_hand, Player};

#[derive(Clone, Copy, Debug, Default)]
pub struct MatchStats {
    pub games: u64,
    pub wins: u64,
    /// Vittorie quando il giocatore gioca per primo nella prima smazzata (posto 0) e quando distribuisce.
    pub seat_wins: [u64; 2],
    pub hands: u64,
    /// Somma dei punti fatti e subiti nelle smazzate, per la media per smazzata.
    pub points_for: u64,
    pub points_against: u64,
    pub scope_for: u64,
    /// Numero di mazzi, somma e somma dei quadrati del punteggio per mazzo (0..1).
    pub deals: u64,
    pub sum: f64,
    pub sum_sq: f64,
}

impl MatchStats {
    pub fn merge(mut self, o: MatchStats) -> MatchStats {
        self.games += o.games;
        self.wins += o.wins;
        self.seat_wins[0] += o.seat_wins[0];
        self.seat_wins[1] += o.seat_wins[1];
        self.hands += o.hands;
        self.points_for += o.points_for;
        self.points_against += o.points_against;
        self.scope_for += o.scope_for;
        self.deals += o.deals;
        self.sum += o.sum;
        self.sum_sq += o.sum_sq;
        self
    }

    pub fn to_line(&self) -> String {
        format!(
            "{} {} {} {} {} {} {} {} {}",
            self.games, self.wins, self.seat_wins[0], self.seat_wins[1], self.hands, self.points_for,
            self.points_against, self.scope_for, self.sum
        )
    }

    pub fn from_line(line: &str) -> Option<MatchStats> {
        let f: Vec<&str> = line.split(' ').collect();
        let n = |i: usize| f.get(i)?.parse::<u64>().ok();
        let sum: f64 = f.get(8)?.parse().ok()?;
        Some(MatchStats {
            games: n(0)?,
            wins: n(1)?,
            seat_wins: [n(2)?, n(3)?],
            hands: n(4)?,
            points_for: n(5)?,
            points_against: n(6)?,
            scope_for: n(7)?,
            deals: 1,
            sum,
            sum_sq: sum * sum,
        })
    }

    /// Quota di partite vinte (nella scopa a 11 non ci sono pareggi).
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

    /// IC di Wilson sulle partite considerate indipendenti (per i confronti col posto).
    pub fn seat_ci(&self, seat: usize) -> (f64, f64) {
        wilson95(self.seat_wins[seat] as f64, self.deals as f64)
    }

    pub fn elo(&self) -> f64 {
        let s = self.score().clamp(1e-4, 1.0 - 1e-4);
        -400.0 * (1.0 / s - 1.0).log10()
    }

    /// Punti medi per smazzata: fatti e subiti.
    pub fn per_hand(&self) -> (f64, f64) {
        let h = self.hands.max(1) as f64;
        (self.points_for as f64 / h, self.points_against as f64 / h)
    }
}

/// Mazzo della smazzata `hand` del mazzo `deal`.
pub fn deck_order(seed: u64, deal: u64, hand: u64) -> Vec<Card> {
    let mut rng = SmallRng::seed_from_u64(seed ^ deal.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ hand.wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    let mut order: Vec<Card> = (0..NUM_CARDS as u8).collect();
    order.shuffle(&mut rng);
    order
}

/// Partita a 11 punti: nella prima smazzata distribuisce il posto 1 (gioca per primo il posto 0),
/// poi chi distribuisce si alterna.
pub fn play_match(players: &mut [Box<dyn Player>; 2], seed: u64, deal: u64, rng: &mut SmallRng) -> ([u8; 2], u64, [u64; 2]) {
    let mut score = [0u8; 2];
    let mut scope = [0u64; 2];
    let mut dealer = 1u8;
    let mut hand = 0u64;
    loop {
        let g = Game::from_order(&deck_order(seed, deal, hand), dealer);
        let [a, b] = players;
        let h = play_hand(g, &mut [a.as_mut(), b.as_mut()], score, rng);
        for p in 0..2 {
            score[p] += h.points[p];
            scope[p] += h.scope[p] as u64;
        }
        hand += 1;
        dealer = 1 - dealer;
        if match_winner(score).is_some() {
            return (score, hand, scope);
        }
        assert!(hand < 200, "partita senza fine");
    }
}

pub fn run_deals(player: &str, opponent: &str, deals: std::ops::Range<u64>, seed: u64) -> Result<Vec<MatchStats>, String> {
    make_player(player)?;
    make_player(opponent)?;
    Ok(deals
        .into_par_iter()
        .map(|d| {
            let mut s = MatchStats { deals: 1, ..Default::default() };
            let mut pair = 0.0;
            for seat in 0..2usize {
                let me = make_player(player).unwrap();
                let opp = make_player(opponent).unwrap();
                let mut players = if seat == 0 { [me, opp] } else { [opp, me] };
                let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(d * 2 + seat as u64));
                let (score, hands, scope) = play_match(&mut players, seed, d, &mut rng);
                let won = match_winner(score) == Some(seat as u8);
                s.games += 1;
                s.hands += hands;
                s.points_for += score[seat] as u64;
                s.points_against += score[1 - seat] as u64;
                s.scope_for += scope[seat];
                if won {
                    s.wins += 1;
                    s.seat_wins[seat] += 1;
                    pair += 0.5;
                }
            }
            s.sum = pair;
            s.sum_sq = pair * pair;
            s
        })
        .collect())
}

pub fn prefixes(per_deal: &[MatchStats], checkpoints: &[u64]) -> Vec<MatchStats> {
    checkpoints
        .iter()
        .map(|&n| per_deal[..(n as usize).min(per_deal.len())].iter().copied().fold(MatchStats::default(), MatchStats::merge))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_match(player: &str, opponent: &str, deals: u64, seed: u64) -> MatchStats {
        run_deals(player, opponent, 0..deals, seed).unwrap().into_iter().fold(MatchStats::default(), MatchStats::merge)
    }

    #[test]
    fn duplicate_is_fair() {
        // Due copie dello stesso giocatore deterministico fanno esattamente metà a testa.
        let s = run_match("heuristic", "heuristic", 50, 1);
        assert_eq!(s.games, 100);
        assert!((s.score() - 0.5).abs() < 1e-9, "punteggio {}", s.score());
    }

    #[test]
    fn stronger_player_scores_more() {
        let s = run_match("counter", "random", 60, 2);
        assert!(s.score_ci95().0 > 0.6, "punteggio {}", s.score());
    }
}
