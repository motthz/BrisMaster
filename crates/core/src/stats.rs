//! Statistiche per i test massivi.

/// Intervallo di Wilson al 95% per una proporzione `k/n`.
pub fn wilson95(k: f64, n: f64) -> (f64, f64) {
    if n == 0.0 {
        return (0.0, 1.0);
    }
    let z = 1.959964f64;
    let p = k / n;
    let denom = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denom;
    let half = z * ((p * (1.0 - p) / n) + z * z / (4.0 * n * n)).sqrt() / denom;
    ((center - half).max(0.0), (center + half).min(1.0))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Tally {
    pub wins: u64,
    pub draws: u64,
    pub losses: u64,
    pub points_for: u64,
}

impl Tally {
    pub fn add(&mut self, my_points: u8) {
        self.points_for += my_points as u64;
        if my_points > 60 {
            self.wins += 1;
        } else if my_points == 60 {
            self.draws += 1;
        } else {
            self.losses += 1;
        }
    }
    pub fn merge(&mut self, o: &Tally) {
        self.wins += o.wins;
        self.draws += o.draws;
        self.losses += o.losses;
        self.points_for += o.points_for;
    }
    pub fn games(&self) -> u64 {
        self.wins + self.draws + self.losses
    }
    /// Quota di partite vinte (pareggi esclusi dal numeratore).
    pub fn win_rate(&self) -> f64 {
        self.wins as f64 / self.games() as f64
    }
    /// Quota di partite vinte tra quelle decise (pareggi esclusi).
    pub fn decisive_win_rate(&self) -> f64 {
        self.wins as f64 / (self.wins + self.losses) as f64
    }
    pub fn avg_points(&self) -> f64 {
        self.points_for as f64 / self.games() as f64
    }
}
