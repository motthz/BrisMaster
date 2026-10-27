//! Il motore: gioca la strategia allenata con CFR sull'albero astratto.
//!
//! Le puntate vere non sono quasi mai esattamente quelle dell'albero: ogni rilancio si
//! traduce in uno dei rilanci astratti vicini con la traduzione pseudo-armonica (casuale,
//! con probabilità che dipendono da quanto è vicino a ciascuno), che è difficile da sfruttare
//! con misure intermedie. Le proprie azioni si ricordano così come sono state scelte.

use crate::abstraction::{AbsAction, Kind, Tree};
use crate::bots::RuleBot;
use crate::buckets::PLAY;
use crate::cards::Card;
use crate::cfr::{tree, Strategy};
use crate::game::Action;
use crate::{Agent, View};
use rand::rngs::SmallRng;
use rand::Rng;
use std::sync::{Arc, OnceLock};

static BUNDLED: &[u8] = include_bytes!("../data/holdem.bin");

/// La strategia allenata inclusa nel programma (`poker-sim train`).
pub fn bundled() -> Arc<Strategy> {
    static S: OnceLock<Arc<Strategy>> = OnceLock::new();
    S.get_or_init(|| Arc::new(Strategy::from_bytes(BUNDLED).expect("holdem.bin non valido"))).clone()
}

/// Probabilità di tradurre la frazione `x` nel rilancio astratto più piccolo `a` invece che in `b`.
pub fn pseudo_harmonic(a: f32, b: f32, x: f32) -> f32 {
    ((b - x) * (1.0 + a)) / ((b - a) * (1.0 + x))
}

pub struct Engine {
    strat: Arc<Strategy>,
    tree: Arc<Tree>,
    label: String,
    fallback: RuleBot,
    // Stato della mano in corso.
    hole: [Card; 2],
    node: u32,
    processed: usize,
    lost: bool,
    /// L'azione astratta appena scelta, da usare quando compare nella storia.
    pending: Option<usize>,
    bucket: Option<(u8, u16)>,
}

/// Consiglio per l'app: l'azione astratta, quella vera corrispondente e la probabilità.
#[derive(Clone, Debug)]
pub struct Advice {
    pub label: String,
    pub action: Action,
    pub prob: f32,
}

impl Engine {
    pub fn new(strat: Arc<Strategy>, label: &str) -> Engine {
        Engine {
            strat,
            tree: tree(),
            label: label.into(),
            fallback: RuleBot::by_name("equity").unwrap(),
            hole: [255, 255],
            node: 0,
            processed: 0,
            lost: false,
            pending: None,
            bucket: None,
        }
    }

    pub fn bundled() -> Engine {
        Engine::new(bundled(), "engine")
    }

    fn translate(&self, node: u32, a: Action, pot_before: u32, to_call: u32, bet_before: u32, rng: &mut SmallRng) -> Option<usize> {
        let n = &self.tree.nodes[node as usize];
        let call = n.actions.iter().position(|&x| x == AbsAction::Call)?;
        match a {
            Action::Fold => n.actions.iter().position(|&x| x == AbsAction::Fold),
            Action::Call => Some(call),
            Action::RaiseTo(t) => {
                let cur = bet_before + to_call;
                let x = (t.saturating_sub(cur)) as f32 / (pot_before + to_call) as f32;
                let raises: Vec<(usize, f32)> = n
                    .actions
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| matches!(a, AbsAction::Raise(_) | AbsAction::AllIn))
                    .map(|(i, _)| (i, n.fracs[i]))
                    .collect();
                if raises.is_empty() {
                    return Some(call);
                }
                if x <= raises[0].1 {
                    return Some(raises[0].0);
                }
                for w in raises.windows(2) {
                    let ((ia, a), (ib, b)) = (w[0], w[1]);
                    if x <= b {
                        return Some(if rng.gen::<f32>() < pseudo_harmonic(a, b, x) { ia } else { ib });
                    }
                }
                Some(raises.last().unwrap().0)
            }
        }
    }

    /// Porta il nodo astratto al punto della mano vera. `None` se la mano è uscita dall'albero.
    fn locate(&mut self, v: &View, rng: &mut SmallRng) -> Option<(u32, usize)> {
        if v.hole != self.hole || v.log.len() < self.processed {
            self.hole = v.hole;
            self.node = 0;
            self.processed = 0;
            self.lost = false;
            self.pending = None;
            self.bucket = None;
        }
        let me_abs = if v.me == v.button { 0 } else { 1 };
        while !self.lost && self.processed < v.log.len() {
            let l = v.log[self.processed];
            self.processed += 1;
            let n = &self.tree.nodes[self.node as usize];
            if n.kind != Kind::Decision {
                self.lost = true;
                break;
            }
            let mine = self.pending.take().filter(|&i| {
                l.seat == v.me
                    && matches!(
                        (n.actions[i], l.action),
                        (AbsAction::Fold, Action::Fold)
                            | (AbsAction::Call, Action::Call)
                            | (AbsAction::Raise(_) | AbsAction::AllIn, Action::RaiseTo(_))
                    )
            });
            let idx = if mine.is_some() {
                mine
            } else {
                self.translate(self.node, l.action, l.pot_before, l.to_call, l.bet_before, rng)
            };
            match idx {
                Some(i) => self.node = n.children[i],
                None => self.lost = true,
            }
        }
        self.pending = None;
        let n = &self.tree.nodes[self.node as usize];
        if self.lost || n.kind != Kind::Decision || n.player != me_abs || n.street != v.street {
            self.lost = true;
            return None;
        }
        let bucket = match self.bucket {
            Some((s, b)) if s == v.street => b,
            _ => {
                let b = self.strat.bucketer.bucket_det(v.hole, v.board, PLAY);
                self.bucket = Some((v.street, b));
                b
            }
        };
        Some((self.node, n.offset + bucket as usize * n.actions.len()))
    }

    fn concrete(v: &View, a: AbsAction) -> Action {
        match a {
            AbsAction::Fold if v.to_call == 0 => Action::Call,
            AbsAction::Fold => Action::Fold,
            AbsAction::Call => Action::Call,
            AbsAction::Raise(f) => v.pot_raise(f as f64).unwrap_or(Action::Call),
            AbsAction::AllIn => v.raise_range.map(|(_, hi)| Action::RaiseTo(hi)).unwrap_or(Action::Call),
        }
    }

    /// La strategia del motore nella situazione vera, se la mano è ancora dentro l'albero.
    pub fn advice(&mut self, v: &View, rng: &mut SmallRng) -> Option<Vec<Advice>> {
        let (node, base) = self.locate(v, rng)?;
        let n = &self.tree.nodes[node as usize];
        let mut p = [0f32; 8];
        self.strat.row(base, n.actions.len(), &mut p);
        Some(
            n.actions
                .iter()
                .zip(p)
                .map(|(&a, prob)| Advice { label: a.label(), action: Self::concrete(v, a), prob })
                .collect(),
        )
    }
}

impl Agent for Engine {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn act(&mut self, v: &View, rng: &mut SmallRng) -> Action {
        let Some((node, base)) = self.locate(v, rng) else {
            return self.fallback.act(v, rng);
        };
        let n = &self.tree.nodes[node as usize];
        let k = n.actions.len();
        let mut p = [0f32; 8];
        self.strat.row(base, k, &mut p);
        let mut x: f32 = rng.gen();
        let mut i = k - 1;
        for (j, &pj) in p[..k].iter().enumerate() {
            x -= pj;
            if x < 0.0 {
                i = j;
                break;
            }
        }
        // Un fold senza niente da pagare è un check: nell'albero è la stessa cosa.
        let i = if n.actions[i] == AbsAction::Fold && v.to_call == 0 { n.actions.iter().position(|&a| a == AbsAction::Call).unwrap() } else { i };
        self.pending = Some(i);
        Self::concrete(v, n.actions[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pseudo_harmonic_endpoints() {
        assert!((pseudo_harmonic(0.5, 1.0, 0.5) - 1.0).abs() < 1e-6);
        assert!(pseudo_harmonic(0.5, 1.0, 1.0).abs() < 1e-6);
        let mid = pseudo_harmonic(0.5, 1.0, 0.75);
        assert!(mid > 0.0 && mid < 1.0);
    }

    #[test]
    fn strategy_roundtrip() {
        let s = bundled();
        let back = Strategy::from_bytes(&s.to_bytes()).unwrap();
        assert_eq!(back.bucketer, s.bucketer);
        assert_eq!(back.iterations, s.iterations);
    }

    #[test]
    fn engine_follows_real_hands_and_plays_legal() {
        use crate::bots::RuleBot;
        use crate::play_hand;
        use rand::SeedableRng;
        let mut rng = SmallRng::seed_from_u64(11);
        let mut lost = 0;
        for i in 0..300 {
            let mut h = crate::Hand::deal(&mut rng, (i % 2) as u8);
            let mut e = Engine::bundled();
            let mut b = RuleBot::by_name("tag").unwrap();
            play_hand(&mut h, &mut [&mut e, &mut b], &mut rng);
            lost += e.lost as u32;
            assert!((h.result(0) + h.result(1)).abs() < 1e-9);
        }
        // Contro misure di puntata normali la mano resta quasi sempre dentro l'albero.
        assert!(lost < 15, "{lost} mani uscite dall'albero");
    }
}
