//! Motore: il giocatore esperto più simulazioni Monte Carlo sulle due scelte decisive del turno,
//! pescare o raccogliere gli scarti e quale carta scartare.
//!
//! Per ogni scelta candidata si ridistribuiscono a caso le carte che il giocatore non vede
//! (mano avversaria tranne le carte note, tallone, pozzetti) e si gioca la smazzata fino in fondo
//! con lo stile esperto per entrambi. Vince la scelta con il risultato medio migliore.
//! Le calate seguono il pianificatore dell'esperto.

use crate::ai::{discard_cost, turn_plan, Heuristic, Player, Style};
use crate::cards::Kind;
use crate::game::{Action, Game, Phase};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    /// Distribuzioni simulate per ogni scelta candidata.
    pub samples: u32,
    /// Scarti candidati valutati con le simulazioni (i migliori secondo l'esperto).
    pub discards: usize,
    /// Peso dello scarto di punti rispetto alla vittoria (per 1000 punti).
    pub margin_w: f64,
    pub style: Style,
}

impl EngineConfig {
    pub fn new(samples: u32) -> EngineConfig {
        EngineConfig { samples, discards: 3, margin_w: 0.3, style: Style::EXPERT }
    }
}

/// Valutazione di una scelta: probabilità di vincere la smazzata e scarto medio di punti.
#[derive(Clone, Copy, Debug)]
pub struct ChoiceEval {
    pub action: Action2,
    pub win_prob: f64,
    pub margin: f64,
}

/// Scelta valutata (pesca o scarto).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action2 {
    Stock,
    Pile,
    Discard(Kind),
}

impl Action2 {
    pub fn action(self) -> Action {
        match self {
            Action2::Stock => Action::DrawStock,
            Action2::Pile => Action::TakePile,
            Action2::Discard(k) => Action::Discard(k),
        }
    }
}

/// Gioca la smazzata fino in fondo con lo stile dato per entrambi.
fn playout(mut g: Game, style: &Style, rng: &mut SmallRng) -> Game {
    let players = [Heuristic::new("", style.fast()), Heuristic::new("", style.fast())];
    while !g.is_over() {
        let p = g.to_move as usize;
        let a = players[p].decide(&g, rng);
        if g.apply(&a).is_err() {
            break;
        }
    }
    g
}

/// Le scelte candidate nella posizione attuale (vuoto se la scelta è obbligata).
pub fn candidates(g: &Game, cfg: &EngineConfig) -> Vec<Action2> {
    let me = g.to_move;
    match g.phase {
        // Una carta sola si raccoglie solo se si cala subito, come fa l'esperto.
        Phase::Draw if g.pile.len() >= 2 => vec![Action2::Stock, Action2::Pile],
        Phase::Play => {
            let plan = turn_plan(g, me, &cfg.style);
            if !plan.ops.is_empty() {
                return Vec::new();
            }
            let mut ks: Vec<Kind> = plan.hand.kinds().filter(|&k| g.can_discard(k)).collect();
            if ks.len() <= 1 {
                return Vec::new();
            }
            ks.sort_by(|&a, &b| {
                discard_cost(g, me, &plan, a, &cfg.style).total_cmp(&discard_cost(g, me, &plan, b, &cfg.style))
            });
            ks.truncate(cfg.discards);
            ks.into_iter().map(Action2::Discard).collect()
        }
        _ => Vec::new(),
    }
}

/// Valuta le scelte con le simulazioni, dalla migliore alla peggiore.
pub fn evaluate(g: &Game, cfg: &EngineConfig, rng: &mut SmallRng) -> Vec<ChoiceEval> {
    let me = g.to_move;
    let cands = candidates(g, cfg);
    let mut evals: Vec<ChoiceEval> =
        cands.iter().map(|&a| ChoiceEval { action: a, win_prob: 0.0, margin: 0.0 }).collect();
    for _ in 0..cfg.samples {
        let mut world = g.clone();
        world.determinize(me, rng);
        let seed: u64 = rng.gen();
        for e in evals.iter_mut() {
            let mut w = world.clone();
            if w.apply(&e.action.action()).is_err() {
                e.win_prob = f64::NEG_INFINITY;
                continue;
            }
            // Stessi numeri casuali per tutte le scelte: il confronto è più preciso.
            let mut r = SmallRng::seed_from_u64(seed);
            let end = playout(w, &cfg.style, &mut r);
            let d = end.score(me) - end.score(1 - me);
            e.win_prob += if d > 0 { 1.0 } else if d == 0 { 0.5 } else { 0.0 };
            e.margin += d as f64;
        }
    }
    let n = cfg.samples.max(1) as f64;
    for e in evals.iter_mut() {
        e.win_prob /= n;
        e.margin /= n;
    }
    let key = |e: &ChoiceEval| e.win_prob + cfg.margin_w * e.margin / 1000.0;
    evals.sort_by(|a, b| key(b).total_cmp(&key(a)));
    evals
}

pub struct Engine {
    pub cfg: EngineConfig,
    expert: Heuristic,
}

impl Engine {
    pub fn new(cfg: EngineConfig) -> Engine {
        Engine { expert: Heuristic::new("", cfg.style), cfg }
    }
}

impl Player for Engine {
    fn name(&self) -> String {
        format!("Motore ({} simulazioni)", self.cfg.samples)
    }

    fn act(&mut self, g: &Game, rng: &mut SmallRng) -> Action {
        if self.cfg.samples == 0 || candidates(g, &self.cfg).is_empty() {
            return self.expert.decide(g, rng);
        }
        evaluate(g, &self.cfg, rng)[0].action.action()
    }
}

