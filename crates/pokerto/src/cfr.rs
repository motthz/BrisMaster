//! Allenamento del motore: CFR Monte Carlo con campionamento esterno (MCCFR) sul gioco astratto.
//!
//! A ogni iterazione si mescola un mazzo vero; le mani, i cambi e lo showdown sono quelli
//! reali, solo le decisioni sono prese per secchi (vedi `abstraction`). Per il giocatore che
//! si aggiorna si esplorano tutte le azioni, per l'altro se ne estrae una. Regret matching+
//! con media pesata per iterazione: la strategia media converge verso un equilibrio del
//! gioco astratto, cioè una strategia che non perde contro nessun avversario che giochi
//! con la stessa astrazione.
//!
//! Con `frozen` l'avversario gioca sempre una strategia fissa e si allena solo l'altro:
//! si ottiene un giocatore che sfrutta al massimo quella strategia (risposta migliore
//! nell'astrazione), utile per misurare quanto è sfruttabile il motore.

use crate::abstraction::{key, post_bucket, pre_bucket, push, PRE_LEVELS};
use crate::cards::{all_hands, Card, NUM_CARDS};
use crate::draw::{percentile, replace, summary};
use crate::eval::evaluate;
use crate::game::{Action, ANTE, BIG_BET, CAP, SMALL_BET};
use crate::strategy::Strategy;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::Rng;
use rayon::prelude::*;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

/// Hasher veloce per chiavi `u64` già ben distribuite.
#[derive(Default)]
pub struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!()
    }
    fn write_u64(&mut self, k: u64) {
        self.0 = (k ^ (k >> 31)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

type Map<V> = HashMap<u64, V, BuildHasherDefault<KeyHasher>>;

#[derive(Clone, Copy, Default)]
struct Node {
    regret: [f32; 6],
    sum: [f32; 6],
}

/// Soglie dei livelli del secchio prima del cambio: quantili del percentile atteso
/// dopo il miglior cambio, su tutte le mani possibili.
pub fn pre_thresholds() -> Vec<f32> {
    let mut v: Vec<f32> = all_hands().par_iter().map(|h| summary(h).max_ev()).collect();
    v.sort_unstable_by(f32::total_cmp);
    (1..PRE_LEVELS).map(|i| v[i * v.len() / PRE_LEVELS]).collect()
}

/// Tutto quello che serve di una smazzata: secchi e valori finali per ogni coppia di cambi.
struct Deal {
    pre: [u8; 2],
    val0: [u32; 6],
    post0: [u8; 6],
    val1: [[u32; 6]; 6],
    post1: [[u8; 6]; 6],
}

impl Deal {
    fn new(deck: &[Card; NUM_CARDS], thresholds: &[f32]) -> Deal {
        let mut h = [[0; 5]; 2];
        for i in 0..5 {
            h[0][i] = deck[2 * i];
            h[1][i] = deck[2 * i + 1];
        }
        h[0].sort_unstable();
        h[1].sort_unstable();
        let s = [summary(&h[0]), summary(&h[1])];
        let mut d = Deal {
            pre: [pre_bucket(&s[0], thresholds), pre_bucket(&s[1], thresholds)],
            val0: [0; 6],
            post0: [0; 6],
            val1: [[0; 6]; 6],
            post1: [[0; 6]; 6],
        };
        for k0 in 0..6 {
            let v = evaluate(&replace(&h[0], s[0].best[k0], &deck[10..10 + k0]));
            d.val0[k0] = v;
            d.post0[k0] = post_bucket(percentile(v));
            for k1 in 0..6 {
                let v = evaluate(&replace(&h[1], s[1].best[k1], &deck[10 + k0..10 + k0 + k1]));
                d.val1[k0][k1] = v;
                d.post1[k0][k1] = post_bucket(percentile(v));
            }
        }
        d
    }
}

/// Stato astratto della mano durante l'esplorazione.
#[derive(Clone, Copy)]
struct St {
    /// 0 = primo giro, 1 = cambio, 2 = secondo giro.
    phase: u8,
    to_act: u8,
    contrib: [i32; 2],
    bets: u8,
    acted: u8,
    seq: u64,
    k: [u8; 2],
}

enum Child {
    Node(St),
    /// Chi ha lasciato e quanto aveva messo nel piatto.
    Fold(usize, i32),
    Showdown(St),
}

impl St {
    fn root() -> St {
        St { phase: 0, to_act: 0, contrib: [ANTE; 2], bets: 0, acted: 0, seq: 0, k: [0; 2] }
    }

    fn actions(&self) -> ([Action; 6], usize) {
        let mut a = [Action::Call; 6];
        if self.phase == 1 {
            for (k, x) in a.iter_mut().enumerate() {
                *x = Action::Draw(k as u8);
            }
            return (a, 6);
        }
        let p = self.to_act as usize;
        let mut n = 0;
        if self.contrib[1 - p] > self.contrib[p] {
            a[n] = Action::Fold;
            n += 1;
        }
        a[n] = Action::Call;
        n += 1;
        if self.bets < CAP {
            a[n] = Action::Raise;
            n += 1;
        }
        (a, n)
    }

    fn end_round(mut self) -> Child {
        self.bets = 0;
        self.acted = 0;
        self.to_act = 0;
        if self.phase == 0 {
            self.phase = 1;
            Child::Node(self)
        } else {
            Child::Showdown(self)
        }
    }

    fn child(&self, a: Action) -> Child {
        let p = self.to_act as usize;
        let mut s = *self;
        s.seq = push(s.seq, a);
        match a {
            Action::Fold => Child::Fold(p, s.contrib[p]),
            Action::Call => {
                let facing = s.contrib[1 - p] > s.contrib[p];
                s.contrib[p] = s.contrib[1 - p];
                s.acted += 1;
                if facing || s.acted >= 2 {
                    s.end_round()
                } else {
                    s.to_act = 1 - s.to_act;
                    Child::Node(s)
                }
            }
            Action::Raise => {
                s.contrib[p] = s.contrib[1 - p] + if s.phase == 0 { SMALL_BET } else { BIG_BET };
                s.bets += 1;
                s.acted += 1;
                s.to_act = 1 - s.to_act;
                Child::Node(s)
            }
            Action::Draw(k) => {
                s.k[p] = k;
                if p == 0 {
                    s.to_act = 1;
                } else {
                    s.phase = 2;
                    s.to_act = 0;
                }
                Child::Node(s)
            }
        }
    }
}

fn regret_matching(r: &[f32; 6], n: usize) -> [f32; 6] {
    let mut s = [0f32; 6];
    let tot: f32 = r[..n].iter().sum();
    for i in 0..n {
        s[i] = if tot > 0.0 { r[i] / tot } else { 1.0 / n as f32 };
    }
    s
}

fn sample(p: &[f32; 6], n: usize, rng: &mut SmallRng) -> usize {
    let mut x: f32 = rng.gen();
    for (i, &pi) in p[..n].iter().enumerate() {
        x -= pi;
        if x < 0.0 {
            return i;
        }
    }
    n - 1
}

pub struct Trainer {
    nodes: Map<Node>,
    pub thresholds: Vec<f32>,
    pub iterations: u64,
    /// Strategia fissa dell'avversario (allenamento di una risposta migliore).
    frozen: Option<Arc<Strategy>>,
}

impl Trainer {
    pub fn new(thresholds: Vec<f32>) -> Trainer {
        Trainer { nodes: Map::default(), thresholds, iterations: 0, frozen: None }
    }

    /// Allena un giocatore che sfrutta la strategia `opponent`.
    pub fn exploiter(opponent: Arc<Strategy>) -> Trainer {
        let thresholds = opponent.thresholds.clone();
        Trainer { nodes: Map::default(), thresholds, iterations: 0, frozen: Some(opponent) }
    }

    pub fn nodes(&self) -> usize {
        self.nodes.len()
    }

    pub fn run(&mut self, iterations: u64, rng: &mut SmallRng) {
        let mut deck: [Card; NUM_CARDS] = core::array::from_fn(|i| i as Card);
        for _ in 0..iterations {
            self.iterations += 1;
            deck.shuffle(rng);
            let d = Deal::new(&deck, &self.thresholds);
            for trav in 0..2 {
                self.walk(St::root(), trav, &d, rng);
            }
        }
    }

    fn bucket(&self, st: &St, d: &Deal) -> u8 {
        let p = st.to_act as usize;
        if st.phase < 2 {
            d.pre[p]
        } else if p == 0 {
            d.post0[st.k[0] as usize]
        } else {
            d.post1[st.k[0] as usize][st.k[1] as usize]
        }
    }

    fn value(&mut self, c: Child, trav: usize, d: &Deal, rng: &mut SmallRng) -> f32 {
        match c {
            Child::Node(s) => self.walk(s, trav, d, rng),
            Child::Fold(loser, amt) => {
                if loser == trav { -amt as f32 } else { amt as f32 }
            }
            Child::Showdown(s) => {
                let v0 = d.val0[s.k[0] as usize];
                let v1 = d.val1[s.k[0] as usize][s.k[1] as usize];
                let w = if v0 > v1 { 0 } else { 1 };
                let amt = s.contrib[1 - w] as f32;
                if w == trav { amt } else { -amt }
            }
        }
    }

    fn walk(&mut self, st: St, trav: usize, d: &Deal, rng: &mut SmallRng) -> f32 {
        let p = st.to_act as usize;
        let k = key(st.seq, self.bucket(&st, d));
        let (acts, n) = st.actions();
        let leaf = |this: &mut Trainer, a: Action, rng: &mut SmallRng| this.value(st.child(a), trav, d, rng);
        if p != trav {
            if let Some(f) = &self.frozen {
                let probs = f.get(k).copied();
                let i = match probs {
                    Some(pr) if pr[..n].iter().sum::<f32>() > 0.0 => {
                        let s: f32 = pr[..n].iter().sum();
                        let mut q = [0f32; 6];
                        for j in 0..n {
                            q[j] = pr[j] / s;
                        }
                        sample(&q, n, rng)
                    }
                    _ => rng.gen_range(0..n),
                };
                return leaf(self, acts[i], rng);
            }
            let t = self.iterations as f32;
            let node = self.nodes.entry(k).or_default();
            let sigma = regret_matching(&node.regret, n);
            for i in 0..n {
                node.sum[i] += t * sigma[i];
            }
            let i = sample(&sigma, n, rng);
            return leaf(self, acts[i], rng);
        }
        let sigma = regret_matching(&self.nodes.entry(k).or_default().regret, n);
        let mut u = [0f32; 6];
        let mut ev = 0f32;
        for i in 0..n {
            u[i] = leaf(self, acts[i], rng);
            ev += sigma[i] * u[i];
        }
        let node = self.nodes.get_mut(&k).unwrap();
        for i in 0..n {
            node.regret[i] = (node.regret[i] + u[i] - ev).max(0.0);
        }
        ev
    }

    /// La strategia media. Per un giocatore allenato contro una strategia fissa la media non
    /// ha senso (non c'è un equilibrio da raggiungere): si usa la strategia corrente, che
    /// converge verso la risposta migliore.
    pub fn strategy(&self) -> Strategy {
        let map = self
            .nodes
            .iter()
            .map(|(&k, node)| {
                let n = 6;
                let src = if self.frozen.is_some() { &node.regret } else { &node.sum };
                let tot: f32 = src[..n].iter().sum();
                let mut p = [0f32; 6];
                if tot > 0.0 {
                    for i in 0..n {
                        p[i] = src[i] / tot;
                    }
                } else {
                    // Mai visitato abbastanza: uniforme sulle azioni (il numero esatto lo
                    // ricava chi legge, normalizzando sulle azioni possibili).
                    p = [1.0; 6];
                }
                (k, p)
            })
            .collect();
        Strategy::from_map(self.thresholds.clone(), self.iterations, map)
    }
}
