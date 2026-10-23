//! Simulazione di mani vere: mazzo mescolato, carta coperta, richieste di carta, gioco del banco.
//! Serve a verificare con un metodo indipendente i valori esatti del risolutore.

use crate::banco::{pay, Banco, BancoBrain};
use crate::solver::Solver;
use crate::{Deck, Hand, Rules};
use briscola_core::cards::Card;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

/// Strategie del giocatore.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategia {
    /// Ottimale contro il banco che ha davvero davanti.
    Ottimale,
    /// Ottimale contro un altro modello di banco (per misurare cosa succede se lo si sbaglia).
    OttimaleContro(Banco),
    /// Chiede carta finché ha meno di `t` mezzi punti.
    Soglia(u8),
    /// Chiede carta a caso, metà delle volte.
    Casuale,
}

impl Strategia {
    pub fn label(self) -> String {
        match self {
            Strategia::Ottimale => "Ottimale (BrisMaster)".into(),
            Strategia::OttimaleContro(b) => {
                format!("Ottimale pensata per il {}", b.label().to_lowercase())
            }
            Strategia::Soglia(t) => format!("Chiede finché ha meno di {}", crate::fmt_points(t)),
            Strategia::Casuale => "A caso".into(),
        }
    }
}

/// Un giocatore pronto a giocare contro un certo banco.
pub struct Giocatore {
    pub strategia: Strategia,
    solver: Option<Solver>,
    rules: Rules,
}

impl Giocatore {
    pub fn new(strategia: Strategia, rules: Rules, banco: Banco) -> Self {
        let solver = match strategia {
            Strategia::Ottimale => Some(Solver::new(rules, banco)),
            Strategia::OttimaleContro(b) => Some(Solver::new(rules, b)),
            _ => None,
        };
        Giocatore { strategia, solver, rules }
    }

    /// Probabilità di chiedere carta.
    pub fn hit_prob(&mut self, d: &Deck, hc: usize, up: Hand) -> f64 {
        match self.strategia {
            Strategia::Soglia(t) => crate::solver::soglia(t)(d, hc, up),
            Strategia::Casuale => 0.5,
            _ => {
                let rules = self.rules;
                if self.solver.as_mut().unwrap().advise(d, hc, up).should_hit(&rules) {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// Esito di una mano per il giocatore.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Esito {
    Vinta,
    VintaReale,
    Sballata,
    Persa,
}

/// Gioca una mano con il mazzo nell'ordine dato. Restituisce esito e vincita in puntate.
pub fn play_hand(
    rules: &Rules,
    order: &[Card],
    player: &mut Giocatore,
    brain: &mut BancoBrain,
    rng: &mut SmallRng,
) -> (Esito, f64) {
    let class = |c: Card| rules.class_of(c);
    let hc = class(order[0]);
    let mut b = Hand::EMPTY.add(class(order[1]));
    let mut next = 2;
    let mut up = Hand::EMPTY;
    // Il giocatore vede solo le sue carte.
    let mut d = rules.full_deck().without(hc);
    loop {
        let q = if next < order.len() { player.hit_prob(&d, hc, up) } else { 0.0 };
        if q <= 0.0 || (q < 1.0 && !rng.gen_bool(q)) {
            break;
        }
        let c = class(order[next]);
        next += 1;
        up = up.add(c);
        d.remove(c);
        if up.add(hc).bust() {
            return (Esito::Sballata, -1.0);
        }
    }
    let p = up.add(hc);
    // Il banco vede le sue carte e quelle scoperte del giocatore.
    let mut s = rules.full_deck();
    s.remove(class(order[1]));
    for &c in &order[2..next] {
        s.remove(class(c));
    }
    let win = |p: Hand| {
        if p.reale() && rules.reale_paga != 1 {
            (Esito::VintaReale, pay(rules, p))
        } else {
            (Esito::Vinta, 1.0)
        }
    };
    while next < order.len() && brain.hits(b, up, &s) {
        let c = class(order[next]);
        next += 1;
        b = b.add(c);
        s.remove(c);
        if b.bust() {
            return win(p);
        }
    }
    if b.rank() >= p.rank() {
        (Esito::Persa, -1.0)
    } else {
        win(p)
    }
}

/// Media e varianza in un passaggio (Welford).
#[derive(Clone, Copy, Default, Debug)]
pub struct Running {
    pub n: u64,
    pub mean: f64,
    m2: f64,
}

impl Running {
    pub fn add(&mut self, x: f64) {
        self.n += 1;
        let d = x - self.mean;
        self.mean += d / self.n as f64;
        self.m2 += d * (x - self.mean);
    }
    pub fn merge(&mut self, o: &Running) {
        if o.n == 0 {
            return;
        }
        let n = self.n + o.n;
        let d = o.mean - self.mean;
        self.mean += d * o.n as f64 / n as f64;
        self.m2 += o.m2 + d * d * self.n as f64 * o.n as f64 / n as f64;
        self.n = n;
    }
    /// Semiampiezza dell'intervallo di confidenza al 95% sulla media.
    pub fn half95(&self) -> f64 {
        if self.n < 2 {
            return f64::INFINITY;
        }
        1.959964 * (self.m2 / (self.n - 1) as f64 / self.n as f64).sqrt()
    }
}

/// Risultati simulati di una strategia, con il confronto appaiato con la strategia di riferimento.
#[derive(Clone, Copy, Default, Debug)]
pub struct SimStats {
    pub profit: Running,
    /// Differenza di vincita rispetto alla strategia di riferimento sulle stesse mani.
    pub diff: Running,
    pub counts: [u64; 4],
}

impl SimStats {
    pub fn merge(&mut self, o: &SimStats) {
        self.profit.merge(&o.profit);
        self.diff.merge(&o.diff);
        for i in 0..4 {
            self.counts[i] += o.counts[i];
        }
    }
}

/// Gioca `hands` mani per ogni strategia contro `banco`, usando gli stessi mazzi per tutte
/// (numeri casuali comuni). La prima strategia è il riferimento per le differenze.
pub fn simulate(rules: Rules, banco: Banco, strategie: &[Strategia], hands: u64, seed: u64) -> Vec<SimStats> {
    use rayon::prelude::*;
    const CHUNK: u64 = 10_000;
    let chunks = hands.div_ceil(CHUNK);
    (0..chunks)
        .into_par_iter()
        .map_init(
            || {
                let players: Vec<Giocatore> = strategie.iter().map(|&s| Giocatore::new(s, rules, banco)).collect();
                (players, BancoBrain::new(rules, banco))
            },
            |(players, brain), chunk| {
                let mut out = vec![SimStats::default(); strategie.len()];
                let n = CHUNK.min(hands - chunk * CHUNK);
                for i in 0..n {
                    let hand_seed = seed ^ (chunk * CHUNK + i).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                    let mut rng = SmallRng::seed_from_u64(hand_seed);
                    let mut order: Vec<Card> = (0..40).collect();
                    order.shuffle(&mut rng);
                    let mut first = 0.0;
                    for (k, p) in players.iter_mut().enumerate() {
                        let mut prng = SmallRng::seed_from_u64(hand_seed.rotate_left(17));
                        let (e, v) = play_hand(&rules, &order, p, brain, &mut prng);
                        if k == 0 {
                            first = v;
                        }
                        let st = &mut out[k];
                        st.profit.add(v);
                        st.diff.add(first - v);
                        st.counts[e as usize] += 1;
                    }
                }
                out
            },
        )
        .reduce(
            || vec![SimStats::default(); strategie.len()],
            |mut a, b| {
                for (x, y) in a.iter_mut().zip(&b) {
                    x.merge(y);
                }
                a
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulazione_coerente_con_il_calcolo_esatto() {
        let rules = Rules::default();
        for banco in [Banco::Soglia(11), Banco::Esperto] {
            let strategie = [Strategia::Ottimale, Strategia::Soglia(10), Strategia::Casuale];
            let sim = simulate(rules, banco, &strategie, 200_000, 7);
            let mut solver = Solver::new(rules, banco);
            let deck = rules.full_deck();
            for (s, st) in strategie.iter().zip(&sim) {
                let mut g = Giocatore::new(*s, rules, banco);
                let exact = solver.evaluate(&deck, &mut |d, hc, up| g.hit_prob(d, hc, up)).ev(&rules);
                // Margine largo (4 deviazioni standard) per non avere falsi allarmi.
                let tol = 2.0 * st.profit.half95();
                assert!((st.profit.mean - exact).abs() < tol, "{banco:?} {s:?}: sim {} esatto {exact}", st.profit.mean);
            }
        }
    }
}
