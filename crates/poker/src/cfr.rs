//! Allenamento del motore: CFR Monte Carlo con campionamento esterno sull'albero astratto.
//!
//! A ogni iterazione si distribuiscono carte vere; per il giocatore che si aggiorna si
//! esplorano tutte le azioni astratte, per l'altro se ne estrae una secondo la strategia
//! corrente. Regret matching+ con media pesata per iterazione. Più thread aggiornano le
//! stesse tabelle senza lock (gli aggiornamenti persi ogni tanto non spostano la convergenza).
//!
//! Con `exploiter` l'avversario gioca sempre una strategia fissa: si ottiene il giocatore
//! che la sfrutta di più (risposta migliore nell'astrazione), per misurare quanto è sfruttabile.

use crate::abstraction::{Kind, Tree};
use crate::buckets::{Bucketer, Quality, TRAIN};
use crate::cards::Card;
use crate::equity::Rest;
use crate::game::showdown_share;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, OnceLock};

/// L'albero astratto, uguale per tutti (costruito dalle regole).
pub fn tree() -> Arc<Tree> {
    static T: OnceLock<Arc<Tree>> = OnceLock::new();
    T.get_or_init(|| Arc::new(Tree::build())).clone()
}

struct Deal {
    hole: [[Card; 2]; 2],
    board: [Card; 5],
    /// Bucket per giocatore astratto e giro, calcolati solo se servono.
    buckets: [[u16; 4]; 2],
    share0: f32,
}

impl Deal {
    fn random(rng: &mut SmallRng) -> Deal {
        let mut rest = Rest::without(crate::cards::CardSet(0));
        let d = rest.draw(rng, 9);
        let hole = [[d[0], d[1]], [d[2], d[3]]];
        let board = [d[4], d[5], d[6], d[7], d[8]];
        Deal { hole, board, buckets: [[u16::MAX; 4]; 2], share0: showdown_share(hole[0], hole[1], &board) as f32 }
    }

    fn bucket(&mut self, p: usize, street: u8, b: &Bucketer, q: Quality, rng: &mut SmallRng) -> usize {
        let s = street as usize;
        if self.buckets[p][s] == u16::MAX {
            let n = [0, 3, 4, 5][s];
            self.buckets[p][s] = b.bucket(self.hole[p], &self.board[..n], q, rng);
        }
        self.buckets[p][s] as usize
    }
}

fn load(v: &[AtomicU32], i: usize) -> f32 {
    f32::from_bits(v[i].load(Relaxed))
}

fn store(v: &[AtomicU32], i: usize, x: f32) {
    v[i].store(x.to_bits(), Relaxed)
}

fn sample(p: &[f32], rng: &mut SmallRng) -> usize {
    let mut x: f32 = rng.gen();
    for (i, &pi) in p.iter().enumerate() {
        x -= pi;
        if x < 0.0 {
            return i;
        }
    }
    p.len() - 1
}

pub struct Trainer {
    tree: Arc<Tree>,
    pub bucketer: Bucketer,
    regret: Vec<AtomicU32>,
    sum: Vec<AtomicU32>,
    iterations: AtomicU64,
    frozen: Option<Arc<Strategy>>,
}

impl Trainer {
    pub fn new(bucketer: Bucketer) -> Trainer {
        let tree = tree();
        let n = tree.size;
        Trainer {
            tree,
            bucketer,
            regret: (0..n).map(|_| AtomicU32::new(0)).collect(),
            sum: (0..n).map(|_| AtomicU32::new(0)).collect(),
            iterations: AtomicU64::new(0),
            frozen: None,
        }
    }

    pub fn exploiter(opponent: Arc<Strategy>) -> Trainer {
        let mut t = Trainer::new(opponent.bucketer.clone());
        t.sum = Vec::new();
        t.frozen = Some(opponent);
        t
    }

    pub fn iterations(&self) -> u64 {
        self.iterations.load(Relaxed)
    }

    /// `iters` iterazioni divise su `threads` thread.
    pub fn run(&self, iters: u64, threads: usize, seed: u64) {
        let start = self.iterations();
        std::thread::scope(|s| {
            for th in 0..threads {
                s.spawn(move || {
                    let mut rng = SmallRng::seed_from_u64(seed ^ (start + th as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                    loop {
                        let t = self.iterations.fetch_add(1, Relaxed);
                        if t >= start + iters {
                            self.iterations.fetch_sub(1, Relaxed);
                            break;
                        }
                        let mut d = Deal::random(&mut rng);
                        for trav in 0..2 {
                            self.walk(0, trav, &mut d, &mut rng, (t + 1) as f32);
                        }
                    }
                });
            }
        });
    }

    fn current(&self, base: usize, n: usize, out: &mut [f32]) {
        let mut tot = 0.0;
        for i in 0..n {
            out[i] = load(&self.regret, base + i);
            tot += out[i];
        }
        for x in out[..n].iter_mut() {
            *x = if tot > 0.0 { *x / tot } else { 1.0 / n as f32 };
        }
    }

    fn walk(&self, id: u32, trav: usize, d: &mut Deal, rng: &mut SmallRng, t: f32) -> f32 {
        let node = &self.tree.nodes[id as usize];
        match node.kind {
            Kind::Fold(f) => {
                if f as usize == trav { -(node.committed[trav] as f32) } else { node.committed[1 - trav] as f32 }
            }
            Kind::Showdown => {
                let share = if trav == 0 { d.share0 } else { 1.0 - d.share0 };
                share * (node.committed[0] + node.committed[1]) as f32 - node.committed[trav] as f32
            }
            Kind::Decision => {
                let p = node.player as usize;
                let n = node.actions.len();
                let b = d.bucket(p, node.street, &self.bucketer, TRAIN, rng);
                let base = node.offset + b * n;
                let mut sigma = [0f32; 8];
                if p != trav {
                    match &self.frozen {
                        Some(f) => f.row(base, n, &mut sigma),
                        None => {
                            self.current(base, n, &mut sigma);
                            for i in 0..n {
                                store(&self.sum, base + i, load(&self.sum, base + i) + t * sigma[i]);
                            }
                        }
                    }
                    let a = sample(&sigma[..n], rng);
                    return self.walk(node.children[a], trav, d, rng, t);
                }
                self.current(base, n, &mut sigma);
                let mut u = [0f32; 8];
                let mut ev = 0.0;
                for i in 0..n {
                    u[i] = self.walk(node.children[i], trav, d, rng, t);
                    ev += sigma[i] * u[i];
                }
                for i in 0..n {
                    store(&self.regret, base + i, (load(&self.regret, base + i) + u[i] - ev).max(0.0));
                }
                ev
            }
        }
    }

    /// La strategia media; per un exploiter la strategia corrente (la risposta migliore).
    pub fn strategy(&self) -> Strategy {
        let src = if self.frozen.is_some() { &self.regret } else { &self.sum };
        let mut probs = vec![0u8; self.tree.size];
        for node in &self.tree.nodes {
            if node.kind != Kind::Decision {
                continue;
            }
            let n = node.actions.len();
            for b in 0..crate::abstraction::BUCKETS[node.street as usize] {
                let base = node.offset + b * n;
                let tot: f32 = (0..n).map(|i| load(src, base + i)).sum();
                for i in 0..n {
                    let p = if tot > 0.0 { load(src, base + i) / tot } else { 1.0 / n as f32 };
                    probs[base + i] = (p * 255.0).round() as u8;
                }
            }
        }
        Strategy { bucketer: self.bucketer.clone(), iterations: self.iterations(), probs }
    }
}

const MAGIC: &[u8; 6] = b"HOLD01";

/// Strategia media: probabilità (in 255esimi) di ogni azione astratta per nodo e bucket.
#[derive(Clone, Debug)]
pub struct Strategy {
    pub bucketer: Bucketer,
    pub iterations: u64,
    probs: Vec<u8>,
}

impl Strategy {
    /// Probabilità normalizzate della riga che parte da `base` con `n` azioni.
    pub fn row(&self, base: usize, n: usize, out: &mut [f32]) {
        let tot: u32 = self.probs[base..base + n].iter().map(|&x| x as u32).sum();
        for i in 0..n {
            out[i] = if tot > 0 { self.probs[base + i] as f32 / tot as f32 } else { 1.0 / n as f32 };
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.probs.len() + 1024);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.iterations.to_le_bytes());
        for c in &self.bucketer.cutoffs {
            out.extend_from_slice(&(c.len() as u32).to_le_bytes());
            for x in c {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        out.extend_from_slice(&(self.probs.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.probs);
        out
    }

    pub fn from_bytes(b: &[u8]) -> Option<Strategy> {
        let mut i = 0;
        let mut take = |n: usize| -> Option<&[u8]> {
            let s = b.get(i..i + n)?;
            i += n;
            Some(s)
        };
        if take(6)? != MAGIC {
            return None;
        }
        let iterations = u64::from_le_bytes(take(8)?.try_into().ok()?);
        let mut cutoffs: [Vec<f32>; 3] = Default::default();
        for c in cutoffs.iter_mut() {
            let n = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
            for _ in 0..n {
                c.push(f32::from_le_bytes(take(4)?.try_into().ok()?));
            }
        }
        let n = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
        let probs = take(n)?.to_vec();
        if n != tree().size {
            return None;
        }
        Some(Strategy { bucketer: Bucketer { cutoffs }, iterations, probs })
    }
}
