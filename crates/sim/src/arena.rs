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

    /// Una riga di testo per mazzo, per salvare i risultati su file.
    pub fn to_line(&self) -> String {
        let t = &self.tally;
        format!("{} {} {} {} {} {} {}", t.wins, t.draws, t.losses, t.points_for, self.seat_wins[0], self.seat_wins[1], self.sum)
    }

    pub fn from_line(line: &str) -> Option<MatchStats> {
        let f: Vec<&str> = line.split(' ').collect();
        let n = |i: usize| f.get(i)?.parse::<u64>().ok();
        let sum: f64 = f.get(6)?.parse().ok()?;
        Some(MatchStats {
            tally: Tally { wins: n(0)?, draws: n(1)?, losses: n(2)?, points_for: n(3)? },
            seat_wins: [n(4)?, n(5)?],
            deals: 1,
            sum,
            sum_sq: sum * sum,
        })
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
#[cfg(test)]
pub fn run_match(player: &str, opponent: &str, deals: u64, seed: u64) -> Result<MatchStats, String> {
    Ok(run_deals(player, opponent, 0..deals, seed)?.into_iter().fold(MatchStats::default(), MatchStats::merge))
}

/// Come `run_match`, ma restituisce il risultato di ogni mazzo, in ordine.
/// Il mazzo `d` dipende solo dal seme e da `d`, quindi si può giocare un tratto alla volta.
pub fn run_deals(player: &str, opponent: &str, deals: std::ops::Range<u64>, seed: u64) -> Result<Vec<MatchStats>, String> {
    make_agent(player)?;
    make_agent(opponent)?;
    Ok(deals
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

/// Una partita vista dal giocatore: com'era la sua mano iniziale e come è finita.
#[derive(Clone, Copy, Debug)]
pub struct GameRec {
    /// 0 se ha aperto la prima presa, 1 se ha risposto.
    pub seat: u8,
    /// Briscole e punti nelle sue prime tre carte.
    pub trumps: u8,
    pub hand_points: u8,
    /// Punti finali del giocatore (l'avversario ne ha 120 meno questi).
    pub points: u8,
}

/// Come `run_deals` (stessi mazzi, stesse partite), ma restituisce ogni partita con la mano iniziale.
pub fn run_games(player: &str, opponent: &str, deals: std::ops::Range<u64>, seed: u64) -> Result<Vec<GameRec>, String> {
    make_agent(player)?;
    make_agent(opponent)?;
    Ok(deals
        .into_par_iter()
        .flat_map_iter(|d| {
            let mut deal_rng = SmallRng::seed_from_u64(seed ^ d.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut order: Vec<Card> = (0..NUM_CARDS as u8).collect();
            order.shuffle(&mut deal_rng);
            let start = Game::from_order(&order, 0);
            (0..2usize)
                .map(|seat| {
                    let me = make_agent(player).unwrap();
                    let opp = make_agent(opponent).unwrap();
                    let mut agents = if seat == 0 { [me, opp] } else { [opp, me] };
                    let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(d * 2 + seat as u64));
                    let hand = start.hands[seat];
                    GameRec {
                        seat: seat as u8,
                        trumps: hand.iter().filter(|&c| briscola_core::cards::suit(c) == start.trump).count() as u8,
                        hand_points: hand.iter().map(briscola_core::cards::points).sum(),
                        points: play(&order, &mut agents, &mut rng)[seat],
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect())
}

/// Dati per le reti (stesso formato di `selfplay`). Politica: per ogni decisione di entrambi
/// i giocatori con più di una carta in mano, ciò che vedeva chi muoveva e la carta scelta.
/// Valutazione onesta: per ogni decisione col mazzo ancora lungo, ciò che vedeva chi muoveva,
/// l'esito per lui in mezzi punti e i suoi punti finali.
/// Ogni partita ha un mazzo a caso e chi apre a turno.
pub fn policy_records(player: &str, opponent: &str, games: std::ops::Range<u64>, seed: u64) -> Result<(Vec<u8>, Vec<u8>), String> {
    use briscola_core::net::{canon_index, Position};
    use briscola_core::Observation;
    make_agent(player)?;
    make_agent(opponent)?;
    let parts: Vec<(Vec<u8>, Vec<u8>)> = games
        .into_par_iter()
        .map(|i| {
            let mut rng = SmallRng::seed_from_u64(seed ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut order: Vec<Card> = (0..NUM_CARDS as u8).collect();
            order.shuffle(&mut rng);
            let first = (i % 2) as u8;
            let mut agents = [make_agent(player).unwrap(), make_agent(opponent).unwrap()];
            let mut g = Game::from_order(&order, first);
            for a in agents.iter_mut() {
                a.start(&order, first);
            }
            let mut out = Vec::new();
            let mut seen: Vec<(Position, u8)> = Vec::new();
            while !g.is_over() {
                let p = g.to_move();
                let c = agents[p as usize].choose(&g, p, &mut rng);
                let obs = Observation::of(&g, p);
                if obs.hand.len() > 1 {
                    Position::of_obs(&obs).write(&mut out);
                    out.push(canon_index(c, g.trump) as u8);
                }
                if g.deck_len > 2 {
                    seen.push((Position::of_obs(&obs), p));
                }
                g.play(c);
                for a in agents.iter_mut() {
                    a.observe(c);
                }
            }
            let mut values = Vec::with_capacity(seen.len() * (Position::BYTES + 2));
            for (pos, p) in seen {
                pos.write(&mut values);
                let pts = g.points[p as usize];
                values.push(if pts > 60 { 2 } else if pts == 60 { 1 } else { 0 });
                values.push(pts);
            }
            (out, values)
        })
        .collect();
    let (pol, val): (Vec<Vec<u8>>, Vec<Vec<u8>>) = parts.into_iter().unzip();
    Ok((pol.concat(), val.concat()))
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
