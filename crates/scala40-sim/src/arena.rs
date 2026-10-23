//! Partite in duplicato: ogni partita (fino a 101 punti) si gioca due volte a posti invertiti,
//! con gli stessi mazzi smazzata per smazzata, così la fortuna delle carte si compensa.

use crate::agents::make_agent;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rayon::prelude::*;
use scala40_core::ai::Player;
use scala40_core::cards;
use scala40_core::{Deal, Match};

#[derive(Clone, Copy, Debug, Default)]
pub struct MatchStats {
    pub wins: u64,
    pub draws: u64,
    pub losses: u64,
    /// Smazzate vinte (chiuse) e giocate dal giocatore.
    pub deals_won: u64,
    pub deals: u64,
    /// Punti di penalità presi e dati.
    pub pen_for: u64,
    pub pen_against: u64,
    /// Smazzate perse senza aver aperto (100 punti), dal giocatore e dall'avversario.
    pub unopened_for: u64,
    pub unopened_against: u64,
    /// Coppie di partite (stesso seme), somma e somma dei quadrati del punteggio per coppia.
    pub pairs: u64,
    pub sum: f64,
    pub sum_sq: f64,
}

impl MatchStats {
    pub fn merge(mut self, o: MatchStats) -> MatchStats {
        self.wins += o.wins;
        self.draws += o.draws;
        self.losses += o.losses;
        self.deals_won += o.deals_won;
        self.deals += o.deals;
        self.pen_for += o.pen_for;
        self.pen_against += o.pen_against;
        self.unopened_for += o.unopened_for;
        self.unopened_against += o.unopened_against;
        self.pairs += o.pairs;
        self.sum += o.sum;
        self.sum_sq += o.sum_sq;
        self
    }

    pub fn games(&self) -> u64 {
        self.wins + self.draws + self.losses
    }

    /// Punteggio medio: vittoria 1, pari 0,5, sconfitta 0.
    pub fn score(&self) -> f64 {
        self.sum / self.pairs as f64
    }

    /// Intervallo di confidenza al 95% del punteggio, sulle medie per coppia di partite.
    pub fn score_ci95(&self) -> (f64, f64) {
        let n = self.pairs as f64;
        if n < 2.0 {
            return (0.0, 1.0);
        }
        let mean = self.score();
        let var = (self.sum_sq - n * mean * mean) / (n - 1.0);
        let half = 1.959964 * (var.max(0.0) / n).sqrt();
        ((mean - half).max(0.0), (mean + half).min(1.0))
    }

    pub fn deal_rate(&self) -> f64 {
        self.deals_won as f64 / self.deals as f64
    }

    pub fn to_line(&self) -> String {
        format!(
            "{} {} {} {} {} {} {} {}",
            self.wins, self.draws, self.losses, self.deals_won, self.deals, self.pen_for, self.pen_against, self.sum
        )
    }

    pub fn from_line(line: &str) -> Option<MatchStats> {
        let f: Vec<&str> = line.split(' ').collect();
        let n = |i: usize| f.get(i)?.parse::<u64>().ok();
        let sum: f64 = f.get(7)?.parse().ok()?;
        Some(MatchStats {
            wins: n(0)?,
            draws: n(1)?,
            losses: n(2)?,
            deals_won: n(3)?,
            deals: n(4)?,
            pen_for: n(5)?,
            pen_against: n(6)?,
            unopened_for: 0,
            unopened_against: 0,
            pairs: 1,
            sum,
            sum_sq: sum * sum,
        })
    }
}

/// Gioca una smazzata fino alla fine; panico se un giocatore fa una mossa illegale.
pub fn play_deal(d: &mut Deal, agents: &mut [Box<dyn Player>; 2], rng: &mut SmallRng) {
    while !d.is_over() {
        let p = d.to_move;
        let a = &mut agents[p as usize];
        if a.take_discard(d, p, rng) {
            d.take_discard().unwrap();
        } else if d.draw_stock().is_err() {
            break;
        }
        let t = a.play(d, p, rng);
        for act in &t.actions {
            d.apply(act).unwrap_or_else(|e| panic!("mossa illegale di {}: {e}", a.name()));
        }
        d.discard_card(t.discard).unwrap_or_else(|e| panic!("scarto illegale di {}: {e}", a.name()));
    }
}

/// Mazzo della smazzata `h` della partita `m`: dipende solo dal seme, da `m` e da `h`.
fn deck(seed: u64, m: u64, h: u32) -> (Vec<u8>, u64) {
    let mut rng = SmallRng::seed_from_u64(seed ^ m.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (h as u64).wrapping_mul(0xD1B5_4A32_D192_ED03));
    let mut order = cards::full_deck();
    order.shuffle(&mut rng);
    (order, rand::Rng::gen(&mut rng))
}

/// Una partita completa; restituisce il risultato del posto 0 e le statistiche del posto `seat`.
fn play_match(seed: u64, m: u64, agents: &mut [Box<dyn Player>; 2], rng: &mut SmallRng, seat: usize, s: &mut MatchStats) -> f64 {
    let mut mt = Match::default();
    loop {
        if let Some(r) = mt.result() {
            return match r {
                Some(p) if p as usize == seat => 1.0,
                Some(_) => 0.0,
                None => 0.5,
            };
        }
        let (order, reshuffle) = deck(seed, m, mt.deals_played);
        let mut d = Deal::from_order(&order, Match::first_of(mt.deals_played), reshuffle);
        play_deal(&mut d, agents, rng);
        let pen = d.penalties();
        s.deals += 1;
        if d.winner == Some(seat as u8) {
            s.deals_won += 1;
        }
        s.pen_for += pen[seat] as u64;
        s.pen_against += pen[1 - seat] as u64;
        s.unopened_for += (pen[seat] == 100) as u64;
        s.unopened_against += (pen[1 - seat] == 100) as u64;
        mt.record(&d);
    }
}

/// `matches` coppie di partite in duplicato tra `player` e `opponent`.
pub fn run_matches(player: &str, opponent: &str, matches: std::ops::Range<u64>, seed: u64) -> Result<Vec<MatchStats>, String> {
    make_agent(player)?;
    make_agent(opponent)?;
    Ok(matches
        .into_par_iter()
        .map(|m| {
            let mut s = MatchStats { pairs: 1, ..Default::default() };
            let mut pair = 0.0;
            for seat in 0..2usize {
                let me = make_agent(player).unwrap();
                let opp = make_agent(opponent).unwrap();
                let mut agents = if seat == 0 { [me, opp] } else { [opp, me] };
                let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(m * 2 + seat as u64));
                let r = play_match(seed, m, &mut agents, &mut rng, seat, &mut s);
                match r {
                    x if x == 1.0 => s.wins += 1,
                    x if x == 0.0 => s.losses += 1,
                    _ => s.draws += 1,
                }
                pair += r / 2.0;
            }
            s.sum = pair;
            s.sum_sq = pair * pair;
            s
        })
        .collect())
}

pub fn run(player: &str, opponent: &str, matches: u64, seed: u64) -> Result<MatchStats, String> {
    Ok(run_matches(player, opponent, 0..matches, seed)?.into_iter().fold(MatchStats::default(), MatchStats::merge))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_is_symmetric_for_identical_players() {
        let s = run("basic", "basic", 20, 3).unwrap();
        assert_eq!(s.games(), 40);
        assert!((s.score() - 0.5).abs() < 1e-9, "punteggio {}", s.score());
    }
}
