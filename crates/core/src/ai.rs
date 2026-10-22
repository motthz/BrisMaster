//! Giocatori artificiali: casuale, euristico e motore Monte Carlo.

use crate::cards::*;
use crate::game::{Game, Observation};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// Un giocatore sceglie una carta conoscendo solo la propria osservazione.
pub trait Player: Send {
    fn name(&self) -> String;
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card;
}

// ---------------------------------------------------------------------------
// Casuale

pub struct RandomPlayer;

impl Player for RandomPlayer {
    fn name(&self) -> String {
        "Casuale".into()
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        let moves = obs.hand.to_vec();
        moves[rng.gen_range(0..moves.len())]
    }
}

// ---------------------------------------------------------------------------
// Euristico ("goloso"): le regole pratiche di un giocatore da bar.

/// Costo di "buttare via" una carta: punti regalati + valore della briscola.
fn discard_cost(c: Card, trump: u8) -> i32 {
    let mut cost = points(c) as i32 * 4 + strength(c) as i32;
    if suit(c) == trump {
        cost += 30 + points(c) as i32 * 2;
    }
    cost
}

/// Mossa euristica. Usata come avversario di riferimento e nei rollout.
pub fn heuristic_move(hand: CardSet, table: Option<Card>, trump: u8) -> Card {
    match table {
        None => {
            // Apertura: la carta meno costosa da perdere.
            hand.iter().min_by_key(|&c| discard_cost(c, trump)).unwrap()
        }
        Some(lead) => {
            let lead_pts = points(lead);
            // Prendere di seme: sicuro, si gioca la vincente con più punti.
            let same_suit_win = hand
                .iter()
                .filter(|&c| suit(c) == suit(lead) && strength(c) > strength(lead))
                .max_by_key(|&c| (points(c), strength(c)));
            if suit(lead) != trump {
                if let Some(c) = same_suit_win {
                    return c;
                }
                // Prendere con la briscola più piccola se sul tavolo ci sono punti.
                if lead_pts >= 2 {
                    let trumps = hand.iter().filter(|&c| suit(c) == trump);
                    let cheapest = trumps.min_by_key(|&c| (points(c), strength(c)));
                    if let Some(t) = cheapest {
                        if lead_pts >= 10 || points(t) == 0 {
                            return t;
                        }
                    }
                }
            } else if let Some(c) = same_suit_win {
                if lead_pts + points(c) >= 10 {
                    return c;
                }
            }
            hand.iter().min_by_key(|&c| discard_cost(c, trump)).unwrap()
        }
    }
}

pub struct HeuristicPlayer;

impl Player for HeuristicPlayer {
    fn name(&self) -> String {
        "Euristico".into()
    }
    fn choose(&mut self, obs: &Observation, _rng: &mut SmallRng) -> Card {
        heuristic_move(obs.hand, obs.table, obs.trump)
    }
}

// ---------------------------------------------------------------------------
// Altri stili di gioco umani, usati come avversari nei test massivi.

fn cheapest(cards: impl Iterator<Item = Card>, trump: u8) -> Option<Card> {
    cards.min_by_key(|&c| discard_cost(c, trump))
}

fn winners(hand: CardSet, lead: Card, trump: u8) -> impl Iterator<Item = Card> {
    hand.iter().filter(move |&c| beats(c, lead, trump))
}

/// "Prendi-tutto": cerca sempre di aggiudicarsi la presa, apre con le carte forti.
pub struct AggressivePlayer;

impl Player for AggressivePlayer {
    fn name(&self) -> String {
        "Aggressivo".into()
    }
    fn choose(&mut self, obs: &Observation, _rng: &mut SmallRng) -> Card {
        let trump = obs.trump;
        match obs.table {
            Some(lead) => cheapest(winners(obs.hand, lead, trump), trump)
                .unwrap_or_else(|| cheapest(obs.hand.iter(), trump).unwrap()),
            None => obs
                .hand
                .iter()
                .filter(|&c| suit(c) != trump)
                .max_by_key(|&c| strength(c))
                .unwrap_or_else(|| cheapest(obs.hand.iter(), trump).unwrap()),
        }
    }
}

/// "Risparmiatore": tiene le briscole fino alla fine, prende solo quando costa poco.
pub struct CautiousPlayer;

impl Player for CautiousPlayer {
    fn name(&self) -> String {
        "Risparmiatore".into()
    }
    fn choose(&mut self, obs: &Observation, _rng: &mut SmallRng) -> Card {
        let trump = obs.trump;
        if let Some(lead) = obs.table {
            let same = obs
                .hand
                .iter()
                .filter(|&c| suit(c) == suit(lead) && strength(c) > strength(lead))
                .max_by_key(|&c| points(c));
            if let Some(c) = same {
                return c;
            }
            if points(lead) >= 10 && suit(lead) != trump {
                if let Some(t) = cheapest(obs.hand.iter().filter(|&c| suit(c) == trump), trump) {
                    return t;
                }
            }
        }
        cheapest(obs.hand.iter(), trump).unwrap()
    }
}

/// "Briscolone": usa le briscole appena ci sono punti in tavola e apre di briscola.
pub struct TrumpHappyPlayer;

impl Player for TrumpHappyPlayer {
    fn name(&self) -> String {
        "Briscolone".into()
    }
    fn choose(&mut self, obs: &Observation, _rng: &mut SmallRng) -> Card {
        let trump = obs.trump;
        let trumps = || obs.hand.iter().filter(move |&c| suit(c) == trump);
        match obs.table {
            Some(lead) => {
                if points(lead) > 0 {
                    if let Some(c) = winners(obs.hand, lead, trump).max_by_key(|&c| points(c)) {
                        return c;
                    }
                }
                cheapest(obs.hand.iter(), trump).unwrap()
            }
            None => {
                if trumps().count() >= 2 {
                    trumps().min_by_key(|&c| strength(c)).unwrap()
                } else {
                    cheapest(obs.hand.iter(), trump).unwrap()
                }
            }
        }
    }
}

/// Conta le carte: come l'euristico, ma se le carte che possono batterlo sono già uscite
/// apre con i carichi, e nel finale (mazzo vuoto) gioca a informazione completa.
pub struct CounterPlayer;

impl Player for CounterPlayer {
    fn name(&self) -> String {
        "Contacarte".into()
    }
    fn choose(&mut self, obs: &Observation, _rng: &mut SmallRng) -> Card {
        if obs.deck_len == 0 && obs.hand.len() > 1 {
            let mut rng = SmallRng::seed_from_u64(0);
            let world = obs.determinize(&mut rng);
            return best_by_solver(&world, obs.me);
        }
        let trump = obs.trump;
        if obs.table.is_none() {
            let mut gone = obs.played.union(obs.hand);
            if obs.deck_len > 0 {
                gone.insert(obs.trump_card); // nota, non può batterci a sorpresa
            }
            // Un carico non di briscola è sicuro se tutte le briscole e le carte più forti
            // dello stesso seme sono già uscite o in mano mia.
            let safe = obs.hand.iter().filter(|&c| {
                suit(c) != trump
                    && points(c) >= 10
                    && (0..10u8).all(|r| {
                        let t = make(trump, r);
                        let hi = make(suit(c), r);
                        gone.contains(t) && (strength(hi) <= strength(c) || gone.contains(hi))
                    })
            });
            if let Some(c) = safe.max_by_key(|&c| points(c)) {
                return c;
            }
        }
        heuristic_move(obs.hand, obs.table, trump)
    }
}

/// Miglior mossa a informazione perfetta (risolutore esatto).
fn best_by_solver(g: &Game, me: u8) -> Card {
    g.legal_moves()
        .iter()
        .max_by_key(|&c| {
            let mut n = *g;
            n.play(c);
            solve(&n, me, i32::MIN, i32::MAX)
        })
        .unwrap()
}

// ---------------------------------------------------------------------------
// ISMCTS (Information Set Monte Carlo Tree Search, osservatore singolo):
// algoritmo di famiglia diversa dal motore principale, usato come avversario forte.

struct Node {
    card: Card,
    mover: u8,
    children: Vec<usize>,
    visits: f64,
    avail: f64,
    reward: f64,
}

pub struct IsmctsPlayer {
    pub iterations: u32,
}

impl Player for IsmctsPlayer {
    fn name(&self) -> String {
        format!("ISMCTS ({} iterazioni)", self.iterations)
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        if obs.hand.len() == 1 {
            return obs.hand.iter().next().unwrap();
        }
        let mut nodes = vec![Node { card: 255, mover: 1 - obs.me, children: vec![], visits: 0.0, avail: 0.0, reward: 0.0 }];
        for _ in 0..self.iterations {
            let mut g = obs.determinize(rng);
            let mut path = vec![0usize];
            let mut cur = 0usize;
            // Selezione ed espansione.
            loop {
                if g.is_over() {
                    break;
                }
                let legal = g.legal_moves();
                let kids: Vec<usize> =
                    nodes[cur].children.iter().copied().filter(|&k| legal.contains(nodes[k].card)).collect();
                let tried: CardSet = kids.iter().map(|&k| nodes[k].card).collect();
                let untried = legal.minus(tried);
                for &k in &kids {
                    nodes[k].avail += 1.0;
                }
                if !untried.is_empty() {
                    let v = untried.to_vec();
                    let c = v[rng.gen_range(0..v.len())];
                    let mover = g.to_move();
                    g.play(c);
                    nodes.push(Node { card: c, mover, children: vec![], visits: 0.0, avail: 1.0, reward: 0.0 });
                    let id = nodes.len() - 1;
                    nodes[cur].children.push(id);
                    path.push(id);
                    break;
                }
                let best = *kids
                    .iter()
                    .max_by(|&&a, &&b| {
                        let ucb = |k: usize| {
                            let n = &nodes[k];
                            n.reward / n.visits + 0.7 * (n.avail.ln() / n.visits).sqrt()
                        };
                        ucb(a).partial_cmp(&ucb(b)).unwrap()
                    })
                    .unwrap();
                g.play(nodes[best].card);
                path.push(best);
                cur = best;
            }
            rollout(&mut g, 0.1, rng);
            for &id in &path {
                let n = &mut nodes[id];
                n.visits += 1.0;
                n.reward += utility(g.points[n.mover as usize]);
            }
        }
        let root = &nodes[0];
        root.children.iter().max_by(|&&a, &&b| nodes[a].visits.partial_cmp(&nodes[b].visits).unwrap()).map(|&k| nodes[k].card).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Motore Monte Carlo determinizzato (PIMC) con risolutore esatto nel finale.

#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    /// Numero di mondi campionati per ogni decisione.
    pub samples: u32,
    /// Con al più queste carte nel mazzo si risolve ogni mondo con alfa-beta.
    pub solve_deck_len: u8,
    /// Probabilità di mossa casuale nei rollout (diversifica le simulazioni).
    pub rollout_epsilon: f64,
    /// Peso dei punti rispetto alla vittoria nella valutazione (1 = un punto vale una partita).
    pub points_weight: f64,
}

impl EngineConfig {
    pub fn strength(samples: u32) -> EngineConfig {
        EngineConfig { samples, solve_deck_len: 4, rollout_epsilon: 0.1, points_weight: 1.0 / 1200.0 }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig::strength(400)
    }
}

/// Valutazione di una mossa candidata.
#[derive(Clone, Copy, Debug)]
pub struct MoveEval {
    pub card: Card,
    /// Probabilità stimata di vincere la partita giocando questa carta (0..1, pareggio = 0.5).
    pub win_prob: f64,
    /// Punti finali attesi del giocatore.
    pub exp_points: f64,
}

#[inline]
fn result(pts: u8) -> f64 {
    if pts > 60 { 1.0 } else if pts == 60 { 0.5 } else { 0.0 }
}

/// Utilità usata dal motore: prima conta vincere, poi i punti.
#[inline]
fn utility(pts: u8) -> f64 {
    result(pts) + pts as f64 / 1200.0
}

fn rollout(g: &mut Game, eps: f64, rng: &mut SmallRng) {
    while !g.is_over() {
        let p = g.to_move() as usize;
        let hand = g.hands[p];
        let c = if rng.gen_bool(eps) {
            let n = hand.len();
            hand.iter().nth(rng.gen_range(0..n as usize)).unwrap()
        } else {
            heuristic_move(hand, g.table, g.trump)
        };
        g.play(c);
    }
}

/// Alfa-beta a informazione perfetta: restituisce i punti finali di `me`.
fn solve(g: &Game, me: u8, mut alpha: i32, mut beta: i32) -> i32 {
    if g.is_over() {
        return g.points[me as usize] as i32;
    }
    let p = g.to_move();
    let maximizing = p == me;
    // Ordinamento: prima le carte con più punti (tagli più frequenti).
    let mut moves: Vec<Card> = g.legal_moves().to_vec();
    moves.sort_by_key(|&c| std::cmp::Reverse(points(c) * 4 + strength(c)));
    if maximizing {
        let mut best = i32::MIN;
        for c in moves {
            let mut n = *g;
            n.play(c);
            let v = solve(&n, me, alpha, beta);
            best = best.max(v);
            alpha = alpha.max(v);
            if alpha >= beta {
                break;
            }
        }
        best
    } else {
        let mut best = i32::MAX;
        for c in moves {
            let mut n = *g;
            n.play(c);
            let v = solve(&n, me, alpha, beta);
            best = best.min(v);
            beta = beta.min(v);
            if alpha >= beta {
                break;
            }
        }
        best
    }
}

/// Valuta tutte le mosse legali dal punto di vista di chi deve muovere.
pub fn evaluate(obs: &Observation, cfg: &EngineConfig, rng: &mut SmallRng) -> Vec<MoveEval> {
    let moves = obs.hand.to_vec();
    let me = obs.me;
    let mut util = vec![0.0f64; moves.len()];
    let mut wins = vec![0.0f64; moves.len()];
    let mut pts = vec![0.0f64; moves.len()];
    // Se il mazzo è vuoto le mani sono note: basta un solo mondo.
    let samples = if obs.deck_len == 0 { 1 } else { cfg.samples.max(1) };
    let exact = obs.deck_len <= cfg.solve_deck_len;
    for _ in 0..samples {
        // Lo stesso mondo per tutte le mosse: confronto a parità di carte.
        let world = obs.determinize(rng);
        for (i, &c) in moves.iter().enumerate() {
            let mut g = world;
            g.play(c);
            let final_pts = if exact {
                solve(&g, me, i32::MIN, i32::MAX) as u8
            } else {
                rollout(&mut g, cfg.rollout_epsilon, rng);
                g.points[me as usize]
            };
            util[i] += result(final_pts) + final_pts as f64 * cfg.points_weight;
            wins[i] += result(final_pts);
            pts[i] += final_pts as f64;
        }
    }
    let n = samples as f64;
    let mut out: Vec<(f64, MoveEval)> = moves
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            (util[i] / n, MoveEval { card: c, win_prob: wins[i] / n, exp_points: pts[i] / n })
        })
        .collect();
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    out.into_iter().map(|(_, e)| e).collect()
}

pub struct EnginePlayer {
    pub cfg: EngineConfig,
}

impl EnginePlayer {
    pub fn new(cfg: EngineConfig) -> Self {
        EnginePlayer { cfg }
    }
}

impl Player for EnginePlayer {
    fn name(&self) -> String {
        format!("Motore PIMC ({} mondi)", self.cfg.samples)
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        if obs.hand.len() == 1 {
            return obs.hand.iter().next().unwrap();
        }
        evaluate(obs, &self.cfg, rng)[0].card
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn engine_takes_free_points() {
        // Avversario apre col Tre di Coppe, io ho l'Asso di Coppe: devo prenderlo.
        let mut rng = SmallRng::seed_from_u64(3);
        let mut order: Vec<Card> = (0..40).collect();
        // Briscola: Denari (carta 0 in fondo al mazzo).
        order.swap(0, 5);
        let g0 = Game::from_order(&order, 0);
        let mut g = g0;
        let opp_lead = g.hands[0].iter().next().unwrap();
        g.play(opp_lead);
        let obs = Observation::of(&g, 1);
        let evals = evaluate(&obs, &EngineConfig::strength(100), &mut rng);
        assert_eq!(evals.len(), 3);
        assert!(evals[0].win_prob >= evals[2].win_prob);
    }
}
