//! Allenamento del motore: CFR Monte Carlo con campionamento esterno (MCCFR) sul gioco astratto.
//!
//! A ogni iterazione si mescola un mazzo vero; le mani, i cambi e lo showdown sono quelli
//! reali, solo le decisioni sono prese per secchi (vedi `abstraction`). Per il giocatore che
//! si aggiorna si esplorano tutte le azioni, per l'altro se ne estrae una. Regret matching+
//! con media pesata per iterazione: la strategia media converge verso un equilibrio del
//! gioco astratto, cioè una strategia che non perde contro nessun avversario che giochi
//! con la stessa astrazione.
//!
//! L'albero delle puntate astratto si costruisce una volta sola e le tabelle sono vettori
//! densi indicizzati per nodo e secchio, condivisi tra i thread senza lucchetti (stile
//! "Hogwild": ogni tanto un aggiornamento concorrente si perde, il che non cambia la
//! convergenza). Così l'allenamento usa tutti i core.
//!
//! Oltre all'equilibrio il trainer allena anche risposte a un avversario dato (`Model`):
//! - con probabilità 1 l'avversario gioca sempre il modello: si ottiene il giocatore che
//!   lo sfrutta al massimo (risposta migliore nell'astrazione), utile per misurare quanto
//!   è sfruttabile una strategia;
//! - con probabilità `p < 1` l'avversario gioca il modello, altrimenti una strategia che si
//!   allena per battere noi ("restricted Nash response"): si ottiene una risposta che sfrutta
//!   il modello restando difficile da sfruttare se l'avversario vero è diverso.

use crate::abstraction::{key, push, Abstraction};
use crate::bots::Agent;
use crate::cards::{Card, NUM_CARDS};
use crate::draw::{percentile, replace, summary, DrawSummary};
use crate::eval::evaluate;
use crate::game::{Action, Observation, Phase, ANTE, BIG_BET, CAP, SMALL_BET};
use crate::strategy::Strategy;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

/// Tutto quello che serve di una smazzata: mani, secchi e valori finali per ogni coppia di cambi.
struct Deal {
    hands: [[Card; 5]; 2],
    summ: [DrawSummary; 2],
    pre: [u8; 2],
    post0: [[Card; 5]; 6],
    val0: [u32; 6],
    b0: [u8; 6],
    post1: [[[Card; 5]; 6]; 6],
    val1: [[u32; 6]; 6],
    b1: [[u8; 6]; 6],
}

impl Deal {
    fn new(deck: &[Card; NUM_CARDS], abs: &Abstraction) -> Deal {
        let mut h = [[0; 5]; 2];
        for i in 0..5 {
            h[0][i] = deck[2 * i];
            h[1][i] = deck[2 * i + 1];
        }
        h[0].sort_unstable();
        h[1].sort_unstable();
        let s = [summary(&h[0]), summary(&h[1])];
        let mut d = Deal {
            hands: h,
            summ: s,
            pre: [abs.pre(&s[0]), abs.pre(&s[1])],
            post0: [[0; 5]; 6],
            val0: [0; 6],
            b0: [0; 6],
            post1: [[[0; 5]; 6]; 6],
            val1: [[0; 6]; 6],
            b1: [[0; 6]; 6],
        };
        for k0 in 0..6 {
            let h0 = replace(&h[0], s[0].best[k0], &deck[10..10 + k0]);
            d.post0[k0] = h0;
            d.val0[k0] = evaluate(&h0);
            d.b0[k0] = abs.post(percentile(d.val0[k0]));
            for k1 in 0..6 {
                let h1 = replace(&h[1], s[1].best[k1], &deck[10 + k0..10 + k0 + k1]);
                d.post1[k0][k1] = h1;
                d.val1[k0][k1] = evaluate(&h1);
                d.b1[k0][k1] = abs.post(percentile(d.val1[k0][k1]));
            }
        }
        d
    }

    /// La mano di `p` nel nodo, e il suo valore se il cambio è già fatto.
    fn hand(&self, node: &TNode, p: usize) -> ([Card; 5], Option<u32>) {
        let [k0, k1] = node.k.map(|k| k as usize);
        match (node.phase, p) {
            (2, 0) => (self.post0[k0], Some(self.val0[k0])),
            (2, _) => (self.post1[k0][k1], Some(self.val1[k0][k1])),
            _ => (self.hands[p], None),
        }
    }
}

/// Stato astratto della mano, usato per costruire l'albero.
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

#[derive(Clone, Copy)]
enum Next {
    Node(u32),
    Fold { loser: u8, amt: i32 },
    Showdown { amt: i32 },
}

/// Un nodo di decisione dell'albero astratto.
struct TNode {
    seq: u64,
    phase: u8,
    to_act: u8,
    k: [u8; 2],
    contrib: [i32; 2],
    bets: u8,
    n: usize,
    next: [Next; 6],
    history: Vec<Action>,
    /// Prima casella del nodo nelle tabelle (secchi × 6 azioni).
    base: usize,
}

struct Tree {
    nodes: Vec<TNode>,
    slots: usize,
}

impl Tree {
    fn build(abs: &Abstraction) -> Tree {
        let mut t = Tree { nodes: Vec::new(), slots: 0 };
        t.add(St::root(), Vec::new(), abs);
        t
    }

    fn add(&mut self, s: St, history: Vec<Action>, abs: &Abstraction) -> u32 {
        let id = self.nodes.len();
        let buckets = if s.phase < 2 { abs.pre_buckets() } else { abs.post_levels };
        let (acts, n) = s.actions();
        self.nodes.push(TNode {
            seq: s.seq,
            phase: s.phase,
            to_act: s.to_act,
            k: s.k,
            contrib: s.contrib,
            bets: s.bets,
            n,
            next: [Next::Showdown { amt: 0 }; 6],
            history: history.clone(),
            base: self.slots,
        });
        self.slots += buckets * 6;
        for (i, &a) in acts[..n].iter().enumerate() {
            let mut h = history.clone();
            h.push(a);
            let nx = match s.child(a) {
                Child::Node(c) => Next::Node(self.add(c, h, abs)),
                Child::Fold(p, amt) => Next::Fold { loser: p as u8, amt },
                Child::Showdown(c) => Next::Showdown { amt: c.contrib[0] },
            };
            self.nodes[id].next[i] = nx;
        }
        id as u32
    }
}

impl TNode {
    fn action(&self, i: usize) -> Action {
        if self.phase == 1 {
            return Action::Draw(i as u8);
        }
        let facing = self.contrib[1 - self.to_act as usize] > self.contrib[self.to_act as usize];
        match (facing, i) {
            (true, 0) => Action::Fold,
            (true, 1) | (false, 0) => Action::Call,
            _ => Action::Raise,
        }
    }

    fn index_of(&self, a: Action) -> usize {
        match a {
            Action::Draw(mask) => mask.count_ones() as usize,
            _ => (0..self.n).find(|&i| self.action(i) == a).unwrap_or(self.n - 1),
        }
    }

    fn observation(&self, d: &Deal) -> Observation {
        let p = self.to_act as usize;
        let drawn = |q: usize| match self.phase {
            2 => Some(self.k[q]),
            1 if q == 0 && p == 1 => Some(self.k[0]),
            _ => None,
        };
        Observation {
            seat: p as u8,
            hand: d.hand(self, p).0,
            phase: [Phase::Bet1, Phase::Draw, Phase::Bet2][self.phase as usize],
            history: self.history.clone(),
            to_call: self.contrib[1 - p] - self.contrib[p],
            can_raise: self.bets < CAP,
            pot: self.contrib[0] + self.contrib[1],
            opp_drawn: drawn(1 - p),
            my_drawn: drawn(p),
        }
    }
}

/// Regret e somme della strategia, condivisi tra i thread.
struct Table {
    regret: Vec<AtomicU32>,
    sum: Vec<AtomicU64>,
}

impl Table {
    fn new(slots: usize) -> Table {
        Table {
            regret: (0..slots).map(|_| AtomicU32::new(0)).collect(),
            sum: (0..slots).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    fn regrets(&self, at: usize, n: usize) -> [f32; 6] {
        let mut r = [0f32; 6];
        for i in 0..n {
            r[i] = f32::from_bits(self.regret[at + i].load(Relaxed));
        }
        r
    }

    fn sums(&self, at: usize, n: usize) -> [f64; 6] {
        let mut r = [0f64; 6];
        for i in 0..n {
            r[i] = f64::from_bits(self.sum[at + i].load(Relaxed));
        }
        r
    }

    fn sigma(&self, at: usize, n: usize) -> [f32; 6] {
        regret_matching(&self.regrets(at, n), n)
    }

    fn update(&self, at: usize, n: usize, u: &[f32; 6], ev: f32, sigma: &[f32; 6], w: f64) {
        for i in 0..n {
            let r = f32::from_bits(self.regret[at + i].load(Relaxed));
            self.regret[at + i].store((r + u[i] - ev).max(0.0).to_bits(), Relaxed);
            if w > 0.0 && sigma[i] > 0.0 {
                let s = f64::from_bits(self.sum[at + i].load(Relaxed));
                self.sum[at + i].store((s + w * sigma[i] as f64).to_bits(), Relaxed);
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

/// L'avversario contro cui allenare una risposta.
#[derive(Clone)]
pub enum Model {
    /// Una strategia allenata (anche con un'altra astrazione).
    Strategy(Arc<Strategy>),
    /// Un giocatore qualsiasi, chiamato sulle carte vere: ogni thread ne crea uno.
    Agent(Arc<dyn Fn() -> Box<dyn Agent> + Send + Sync>),
}

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Us,
    Them,
    Model,
}

struct Worker {
    rng: SmallRng,
    agent: Option<Box<dyn Agent>>,
    /// Peso della media in questa iterazione.
    t: f64,
}

pub struct Trainer {
    pub abs: Abstraction,
    tree: Tree,
    ours: Table,
    theirs: Option<Table>,
    model: Option<Model>,
    /// Probabilità che l'avversario giochi il modello.
    p: f32,
    iterations: AtomicU64,
}

impl Trainer {
    /// Allenamento dell'equilibrio (il motore contro se stesso).
    pub fn new(abs: Abstraction) -> Trainer {
        let tree = Tree::build(&abs);
        let ours = Table::new(tree.slots);
        Trainer { abs, tree, ours, theirs: None, model: None, p: 0.0, iterations: AtomicU64::new(0) }
    }

    /// Risposta migliore a un modello fisso.
    pub fn exploiter(model: Model, abs: Abstraction) -> Trainer {
        Trainer::restricted(model, 1.0, abs)
    }

    /// Risposta che sfrutta `model` restando prudente: l'avversario gioca il modello con
    /// probabilità `p`, altrimenti la strategia che ci batte meglio.
    pub fn restricted(model: Model, p: f32, abs: Abstraction) -> Trainer {
        let mut t = Trainer::new(abs);
        if p < 1.0 {
            t.theirs = Some(Table::new(t.tree.slots));
        }
        t.model = Some(model);
        t.p = p;
        t
    }

    pub fn iterations(&self) -> u64 {
        self.iterations.load(Relaxed)
    }

    /// Caselle (secchi × azioni) dell'albero astratto.
    pub fn slots(&self) -> usize {
        self.tree.slots
    }

    /// Esegue `iters` iterazioni divise su tutti i thread di rayon.
    pub fn run(&self, iters: u64, seed: u64) {
        let threads = rayon::current_num_threads() as u64;
        let start = self.iterations();
        (0..threads).into_par_iter().for_each(|i| {
            let n = iters / threads + (i < iters % threads) as u64;
            let s = seed ^ start.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (i + 1).wrapping_mul(0xD1B5_4A32_D192_ED03);
            self.work(n, s);
        });
    }

    fn work(&self, iters: u64, seed: u64) {
        let agent = match &self.model {
            Some(Model::Agent(f)) => Some(f()),
            _ => None,
        };
        let mut w = Worker { rng: SmallRng::seed_from_u64(seed), agent, t: 0.0 };
        let mut deck: [Card; NUM_CARDS] = core::array::from_fn(|i| i as Card);
        for _ in 0..iters {
            w.t = (self.iterations.fetch_add(1, Relaxed) + 1) as f64;
            deck.shuffle(&mut w.rng);
            let d = Deal::new(&deck, &self.abs);
            if self.model.is_none() {
                for trav in 0..2 {
                    self.walk(&mut w, 0, trav, [Role::Us; 2], &d, 1.0);
                }
                continue;
            }
            let fixed = self.p >= 1.0 || w.rng.gen::<f32>() < self.p;
            for us in 0..2 {
                let mut roles = [Role::Us; 2];
                roles[1 - us] = if fixed { Role::Model } else { Role::Them };
                self.walk(&mut w, 0, us, roles, &d, 1.0);
                if !fixed {
                    self.walk(&mut w, 0, 1 - us, roles, &d, 1.0);
                }
            }
        }
    }

    fn table(&self, r: Role) -> &Table {
        match r {
            Role::Them => self.theirs.as_ref().unwrap(),
            _ => &self.ours,
        }
    }

    fn slot(&self, node: &TNode, d: &Deal) -> usize {
        let p = node.to_act as usize;
        let b = match node.phase {
            0 | 1 => d.pre[p],
            _ if p == 0 => d.b0[node.k[0] as usize],
            _ => d.b1[node.k[0] as usize][node.k[1] as usize],
        };
        node.base + b as usize * 6
    }

    /// L'azione del modello nel nodo.
    fn model_action(&self, w: &mut Worker, node: &TNode, d: &Deal) -> usize {
        match self.model.as_ref().unwrap() {
            Model::Strategy(s) => {
                let p = node.to_act as usize;
                let b = match d.hand(node, p).1 {
                    Some(v) => s.abs.post(percentile(v)),
                    None => s.abs.pre(&d.summ[p]),
                };
                match s.get(key(node.seq, b)) {
                    Some(pr) if pr[..node.n].iter().sum::<f32>() > 0.0 => {
                        let tot: f32 = pr[..node.n].iter().sum();
                        let mut q = [0f32; 6];
                        for j in 0..node.n {
                            q[j] = pr[j] / tot;
                        }
                        sample(&q, node.n, &mut w.rng)
                    }
                    _ => w.rng.gen_range(0..node.n),
                }
            }
            Model::Agent(_) => {
                let obs = node.observation(d);
                let a = w.agent.as_mut().unwrap().act(&obs, &mut w.rng);
                node.index_of(a)
            }
        }
    }

    fn value(&self, w: &mut Worker, nx: Next, trav: usize, roles: [Role; 2], d: &Deal, reach: f32) -> f32 {
        match nx {
            Next::Node(i) => self.walk(w, i, trav, roles, d, reach),
            Next::Fold { loser, amt } => {
                if loser as usize == trav { -amt as f32 } else { amt as f32 }
            }
            Next::Showdown { .. } => unreachable!("lo showdown si valuta nel nodo padre"),
        }
    }

    fn walk(&self, w: &mut Worker, id: u32, trav: usize, roles: [Role; 2], d: &Deal, reach: f32) -> f32 {
        let node = &self.tree.nodes[id as usize];
        let p = node.to_act as usize;
        let n = node.n;
        let leaf = |this: &Trainer, w: &mut Worker, i: usize, reach: f32| match node.next[i] {
            Next::Showdown { amt } => {
                let [k0, k1] = node.k.map(|k| k as usize);
                let win = if d.val0[k0] > d.val1[k0][k1] { 0 } else { 1 };
                if win == trav { amt as f32 } else { -amt as f32 }
            }
            nx => this.value(w, nx, trav, roles, d, reach),
        };
        if p != trav {
            let i = match roles[p] {
                Role::Model => self.model_action(w, node, d),
                r => {
                    let sigma = self.table(r).sigma(self.slot(node, d), n);
                    sample(&sigma, n, &mut w.rng)
                }
            };
            return leaf(self, w, i, reach);
        }
        let table = self.table(roles[p]);
        let at = self.slot(node, d);
        let sigma = table.sigma(at, n);
        let mut u = [0f32; 6];
        let mut ev = 0f32;
        for i in 0..n {
            u[i] = leaf(self, w, i, reach * sigma[i]);
            ev += sigma[i] * u[i];
        }
        table.update(at, n, &u, ev, &sigma, w.t * reach as f64);
        ev
    }

    /// La strategia da giocare. Contro un modello fisso si usa la strategia corrente, che con
    /// regret matching+ converge verso la risposta migliore; altrimenti la strategia media.
    /// Le situazioni che la strategia stessa non raggiunge mai (ma che un giocatore vero può
    /// creare non seguendo il consiglio) usano la strategia corrente.
    pub fn strategy(&self) -> Strategy {
        let current = self.model.is_some() && self.p >= 1.0;
        let mut map = HashMap::new();
        for node in &self.tree.nodes {
            let buckets = if node.phase < 2 { self.abs.pre_buckets() } else { self.abs.post_levels };
            for b in 0..buckets {
                let at = node.base + b * 6;
                let r = self.ours.regrets(at, node.n);
                let s = self.ours.sums(at, node.n);
                let rt: f32 = r.iter().sum();
                let st: f64 = s.iter().sum();
                let mut p = [0f32; 6];
                if st > 0.0 && !current {
                    for i in 0..node.n {
                        p[i] = (s[i] / st) as f32;
                    }
                } else if rt > 0.0 {
                    p = regret_matching(&r, node.n);
                } else if st > 0.0 {
                    p[..node.n].fill(1.0 / node.n as f32);
                } else {
                    continue;
                }
                map.insert(key(node.seq, b as u8), p);
            }
        }
        Strategy::from_map(self.abs.clone(), self.iterations(), map)
    }
}
