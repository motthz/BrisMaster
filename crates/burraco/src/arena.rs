//! Smazzate in duplicato e statistiche.
//!
//! Ogni mazzo si gioca due volte a posti invertiti: la fortuna delle carte si compensa.
//! L'intervallo di confidenza si calcola sulla media per mazzo (le due smazzate dello stesso
//! mazzo non sono indipendenti).

use crate::ai::{play_hand, Player};
use crate::game::Game;
use rand::rngs::SmallRng;
use rand::SeedableRng;
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct MatchStats {
    pub wins: u64,
    pub draws: u64,
    pub losses: u64,
    /// Somma dei punti del giocatore e dell'avversario.
    pub points_for: i64,
    pub points_against: i64,
    /// Smazzate chiuse dal giocatore e dall'avversario.
    pub closed: [u64; 2],
    /// Vittorie del giocatore quando inizia (posto 0) e quando è secondo (posto 1).
    pub seat_wins: [u64; 2],
    /// Turni giocati in tutto e smazzate finite per il limite di turni.
    pub turns: u64,
    pub timeouts: u64,
    pub deals: u64,
    pub sum: f64,
    pub sum_sq: f64,
}

impl MatchStats {
    pub fn merge(mut self, o: MatchStats) -> MatchStats {
        self.wins += o.wins;
        self.draws += o.draws;
        self.losses += o.losses;
        self.points_for += o.points_for;
        self.points_against += o.points_against;
        for i in 0..2 {
            self.closed[i] += o.closed[i];
            self.seat_wins[i] += o.seat_wins[i];
        }
        self.turns += o.turns;
        self.timeouts += o.timeouts;
        self.deals += o.deals;
        self.sum += o.sum;
        self.sum_sq += o.sum_sq;
        self
    }

    pub fn games(&self) -> u64 {
        self.wins + self.draws + self.losses
    }

    pub fn win_rate(&self) -> f64 {
        self.wins as f64 / self.games().max(1) as f64
    }

    /// Punteggio medio: vittoria 1, pareggio 0,5, sconfitta 0. Il 50% è la parità.
    pub fn score(&self) -> f64 {
        self.sum / self.deals.max(1) as f64
    }

    /// Intervallo di confidenza al 95% del punteggio, sulle medie per mazzo.
    pub fn score_ci95(&self) -> (f64, f64) {
        let n = self.deals as f64;
        if n < 2.0 {
            return (0.0, 1.0);
        }
        let m = self.sum / n;
        let var = (self.sum_sq / n - m * m).max(0.0) * n / (n - 1.0);
        let half = 1.959964 * (var / n).sqrt();
        ((m - half).max(0.0), (m + half).min(1.0))
    }

    /// Differenza di forza in punti Elo.
    pub fn elo(&self) -> f64 {
        let s = self.score().clamp(0.001, 0.999);
        -400.0 * (1.0 / s - 1.0).log10()
    }

    /// Differenza media di punti per smazzata (giocatore meno avversario).
    pub fn avg_margin(&self) -> f64 {
        (self.points_for - self.points_against) as f64 / self.games().max(1) as f64
    }

    pub fn avg_points(&self) -> f64 {
        self.points_for as f64 / self.games().max(1) as f64
    }

    /// Una riga di testo per mazzo, per la cache dei rapporti lunghi.
    pub fn to_line(&self) -> String {
        format!(
            "{} {} {} {} {} {} {} {} {} {}",
            self.wins,
            self.draws,
            self.losses,
            self.points_for,
            self.points_against,
            self.closed[0],
            self.closed[1],
            self.seat_wins[0],
            self.seat_wins[1],
            self.sum
        )
    }

    pub fn from_line(line: &str) -> Option<MatchStats> {
        let f: Vec<&str> = line.split(' ').collect();
        let u = |i: usize| f.get(i)?.parse::<u64>().ok();
        let s = |i: usize| f.get(i)?.parse::<i64>().ok();
        let sum: f64 = f.get(9)?.parse().ok()?;
        Some(MatchStats {
            wins: u(0)?,
            draws: u(1)?,
            losses: u(2)?,
            points_for: s(3)?,
            points_against: s(4)?,
            closed: [u(5)?, u(6)?],
            seat_wins: [u(7)?, u(8)?],
            deals: 1,
            sum,
            sum_sq: sum * sum,
            ..Default::default()
        })
    }
}

/// Il mazzo numero `i` della serie con questo seme: sempre lo stesso per tutti gli avversari.
pub fn deal_deck(seed: u64, i: u64) -> Vec<crate::Kind> {
    let mut rng = SmallRng::seed_from_u64(seed ^ (i.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
    Game::shuffled_deck(&mut rng)
}

/// Gioca un mazzo due volte a posti invertiti.
pub fn play_deal(
    seed: u64,
    i: u64,
    make_a: &(dyn Fn() -> Box<dyn Player> + Sync),
    make_b: &(dyn Fn() -> Box<dyn Player> + Sync),
) -> MatchStats {
    let deck = deal_deck(seed, i);
    let mut st = MatchStats { deals: 1, ..Default::default() };
    let mut deal_score = 0.0;
    for seat in 0..2u8 {
        let mut a = make_a();
        let mut b = make_b();
        let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(i * 2 + seat as u64 + 1));
        // Il giocatore A siede al posto `seat`; inizia sempre il posto 0.
        let g = Game::from_deck(&deck, 0);
        let end = if seat == 0 {
            play_hand(g, &mut [a.as_mut(), b.as_mut()], &mut rng)
        } else {
            play_hand(g, &mut [b.as_mut(), a.as_mut()], &mut rng)
        };
        let (pa, pb) = (end.score(seat), end.score(1 - seat));
        st.turns += end.turns as u64;
        if end.turns >= crate::game::MAX_TURNS {
            st.timeouts += 1;
        }
        st.points_for += pa as i64;
        st.points_against += pb as i64;
        match end.closed_by {
            Some(c) if c == seat => st.closed[0] += 1,
            Some(_) => st.closed[1] += 1,
            None => {}
        }
        if pa > pb {
            st.wins += 1;
            st.seat_wins[seat as usize] += 1;
            deal_score += 1.0;
        } else if pa == pb {
            st.draws += 1;
            deal_score += 0.5;
        } else {
            st.losses += 1;
        }
    }
    st.sum = deal_score / 2.0;
    st.sum_sq = st.sum * st.sum;
    st
}

/// Risultati mazzo per mazzo, in parallelo.
pub fn run_per_deal(
    seed: u64,
    from: u64,
    to: u64,
    make_a: &(dyn Fn() -> Box<dyn Player> + Sync),
    make_b: &(dyn Fn() -> Box<dyn Player> + Sync),
) -> Vec<MatchStats> {
    (from..to).into_par_iter().map(|i| play_deal(seed, i, make_a, make_b)).collect()
}

/// Gioca i mazzi `from..to` in parallelo.
pub fn run_deals(
    seed: u64,
    from: u64,
    to: u64,
    make_a: &(dyn Fn() -> Box<dyn Player> + Sync),
    make_b: &(dyn Fn() -> Box<dyn Player> + Sync),
) -> MatchStats {
    (from..to)
        .into_par_iter()
        .map(|i| play_deal(seed, i, make_a, make_b))
        .reduce(MatchStats::default, MatchStats::merge)
}
