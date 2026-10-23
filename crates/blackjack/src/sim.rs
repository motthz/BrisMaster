//! Simulatore: gioca mani vere da un sabot mescolato e misura il rendimento di una strategia.

use crate::cards::{Card, Counts, Hand, Shoe};
use crate::ev::{Analyzer, Situation};
use crate::rules::Rules;
use crate::strategy::{Action, Chart, Legal};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// Quello che il giocatore vede quando decide.
pub struct View<'a> {
    pub rules: &'a Rules,
    pub cards: &'a [usize],
    pub up: usize,
    pub sit: Situation,
    /// Carte non viste: il sabot più la carta coperta del banco.
    pub unseen: Counts,
    pub legal: Legal,
}

pub trait Strategy: Send {
    fn decide(&mut self, v: &View, rng: &mut SmallRng) -> Action;
    /// Puntata del prossimo giro, in unità; `unseen` è il sabot prima della distribuzione.
    fn bet(&mut self, _unseen: &Counts, _rules: &Rules) -> f64 {
        1.0
    }
}

/// Strategia di base: la tabella calcolata per le regole del tavolo.
pub struct Basic(pub Chart);

impl Strategy for Basic {
    fn decide(&mut self, v: &View, _: &mut SmallRng) -> Action {
        self.0.decide(v.cards, v.up, v.sit)
    }
}

/// Strategia di base con una frazione di decisioni casuali (giocatore distratto).
pub struct Sloppy(pub Chart, pub f64);

impl Strategy for Sloppy {
    fn decide(&mut self, v: &View, rng: &mut SmallRng) -> Action {
        if rng.gen_bool(self.1) {
            let legal: Vec<Action> = Action::ALL.into_iter().filter(|&a| v.legal.allows(a)).collect();
            legal[rng.gen_range(0..legal.len())]
        } else {
            self.0.decide(v.cards, v.up, v.sit)
        }
    }
}

/// Scelta ottima per la composizione esatta del sabot a ogni decisione (riferimento teorico,
/// irrealizzabile a mente: richiede di ricordare ogni carta uscita).
pub struct Optimal {
    analyzers: Vec<Analyzer>,
}

impl Optimal {
    pub fn new(rules: &Rules) -> Optimal {
        Optimal { analyzers: (0..10).map(|up| Analyzer::new(*rules, up)).collect() }
    }
}

impl Strategy for Optimal {
    fn decide(&mut self, v: &View, _: &mut SmallRng) -> Action {
        let a = &mut self.analyzers[v.up];
        if a.memo_len() > 3_000_000 {
            a.clear();
        }
        let e = a.analyze(&v.unseen, v.cards, v.sit);
        e.best().0
    }
}

/// Imita il banco: carta fino a 17, mai raddoppi, divisioni o resa.
pub struct MimicDealer;

impl Strategy for MimicDealer {
    fn decide(&mut self, v: &View, _: &mut SmallRng) -> Action {
        let h = Hand::of(v.cards);
        let t = h.total();
        if t < 17 || (t == 17 && h.is_soft() && v.rules.hit_soft17) {
            Action::Hit
        } else {
            Action::Stand
        }
    }
}

/// Non sballa mai: sta su ogni 12 duro o più, chiede carta solo quando non può sballare.
pub struct NeverBust;

impl Strategy for NeverBust {
    fn decide(&mut self, v: &View, _: &mut SmallRng) -> Action {
        let h = Hand::of(v.cards);
        if (h.is_soft() && h.total() < 18) || (!h.is_soft() && h.total() < 12) {
            Action::Hit
        } else {
            Action::Stand
        }
    }
}

/// Carta o stai a caso.
pub struct Random;

impl Strategy for Random {
    fn decide(&mut self, _: &View, rng: &mut SmallRng) -> Action {
        if rng.gen_bool(0.5) {
            Action::Hit
        } else {
            Action::Stand
        }
    }
}

/// Contatore Hi-Lo: gioca la strategia di base e punta di più quando il conteggio vero è alto
/// (2 unità per ogni punto di conteggio vero sopra 1, fino a `spread`).
/// Utile solo con un sabot vero: se si rimescola a ogni mano il conteggio resta sempre 0.
pub struct HiLo {
    pub chart: Chart,
    /// Puntata massima in unità (la minima è 1).
    pub spread: f64,
}

impl Strategy for HiLo {
    fn decide(&mut self, v: &View, _: &mut SmallRng) -> Action {
        self.chart.decide(v.cards, v.up, v.sit)
    }
    fn bet(&mut self, unseen: &Counts, rules: &Rules) -> f64 {
        let decks_left = (unseen.total() as f64 / 52.0).max(0.5);
        let tc = (unseen.hilo_running(rules.decks) as f64 / decks_left).floor();
        (2.0 * (tc - 1.0)).clamp(1.0, self.spread)
    }
}

/// Esito di un giro, in unità.
#[derive(Clone, Copy, Debug, Default)]
pub struct RoundResult {
    pub bet: f64,
    pub net: f64,
    /// Somma di tutte le puntate (raddoppi e divisioni compresi).
    pub wagered: f64,
}

struct PHand {
    cards: Vec<usize>,
    bet: f64,
    from_split: bool,
    surrendered: bool,
}

/// Gioca un giro completo.
pub fn play_round(rules: &Rules, shoe: &mut Shoe, s: &mut dyn Strategy, rng: &mut SmallRng) -> RoundResult {
    if rules.penetration <= 0.0 || shoe.needs_shuffle() {
        shoe.shuffle(rng);
    }
    let bet = s.bet(&shoe.remaining, rules);
    let draw = |shoe: &mut Shoe, rng: &mut SmallRng| -> usize { Card::idx(shoe.draw(rng)) };
    let p1 = draw(shoe, rng);
    let up = draw(shoe, rng);
    let p2 = draw(shoe, rng);
    let hole = draw(shoe, rng);
    let dealer_bj = Hand::of(&[up, hole]).is_blackjack();
    let player_bj = Hand::of(&[p1, p2]).is_blackjack();
    if dealer_bj && (rules.peek || player_bj) {
        let net = if player_bj { 0.0 } else { -bet };
        return RoundResult { bet, net, wagered: bet };
    }
    if player_bj {
        return RoundResult { bet, net: rules.blackjack_pays * bet, wagered: bet };
    }

    let mut hands = vec![PHand { cards: vec![p1, p2], bet, from_split: false, surrendered: false }];
    let mut i = 0;
    while i < hands.len() {
        loop {
            let sit = Situation { hands: hands.len() as u8, from_split: hands[i].from_split };
            let legal = Legal::of(rules, &hands[i].cards, sit);
            if !legal.hit && !legal.split {
                break;
            }
            let mut unseen = shoe.remaining;
            unseen.0[hole] += 1;
            let view = View { rules, cards: &hands[i].cards, up, sit, unseen, legal };
            let mut a = s.decide(&view, rng);
            if !legal.allows(a) {
                a = if legal.hit { Action::Hit } else { Action::Stand };
            }
            match a {
                Action::Stand => break,
                Action::Hit => {
                    let c = draw(shoe, rng);
                    hands[i].cards.push(c);
                    if Hand::of(&hands[i].cards).total() >= 21 {
                        break;
                    }
                }
                Action::Double => {
                    let c = draw(shoe, rng);
                    hands[i].cards.push(c);
                    hands[i].bet *= 2.0;
                    break;
                }
                Action::Split => {
                    let card = hands[i].cards.pop().unwrap();
                    hands[i].from_split = true;
                    let c = draw(shoe, rng);
                    hands[i].cards.push(c);
                    let c2 = draw(shoe, rng);
                    hands.push(PHand { cards: vec![card, c2], bet, from_split: true, surrendered: false });
                }
                Action::Surrender => {
                    hands[i].surrendered = true;
                    break;
                }
            }
        }
        i += 1;
    }

    let wagered: f64 = hands.iter().map(|h| h.bet).sum();
    let live = hands.iter().any(|h| !h.surrendered && !Hand::of(&h.cards).busted());
    let mut dealer = Hand::of(&[up, hole]);
    if live && !dealer_bj {
        loop {
            let t = dealer.total();
            if t > 17 || (t == 17 && !(rules.hit_soft17 && dealer.is_soft())) {
                break;
            }
            dealer = dealer.add(draw(shoe, rng));
        }
    }
    let mut net = 0.0;
    for h in &hands {
        let ph = Hand::of(&h.cards);
        net += if h.surrendered {
            if dealer_bj {
                -h.bet
            } else {
                -h.bet / 2.0
            }
        } else if ph.busted() || dealer_bj {
            -h.bet
        } else if dealer.busted() || ph.total() > dealer.total() {
            h.bet
        } else if ph.total() < dealer.total() {
            -h.bet
        } else {
            0.0
        };
    }
    RoundResult { bet, net, wagered }
}

/// Statistiche di una simulazione.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub hands: u64,
    pub net: f64,
    pub net_sq: f64,
    pub bet: f64,
    pub wagered: f64,
    pub won: u64,
    pub pushed: u64,
    pub lost: u64,
    /// Sessioni di `SESSION` mani: chiuse in attivo, in pari, totali.
    pub sessions_up: u64,
    pub sessions_even: u64,
    pub sessions: u64,
}

/// Mani per sessione nelle statistiche delle sessioni.
pub const SESSION: u64 = 100;

impl Stats {
    pub fn record(&mut self, r: &RoundResult) {
        self.hands += 1;
        self.net += r.net;
        self.net_sq += r.net * r.net;
        self.bet += r.bet;
        self.wagered += r.wagered;
        match r.net.partial_cmp(&0.0).unwrap() {
            std::cmp::Ordering::Greater => self.won += 1,
            std::cmp::Ordering::Equal => self.pushed += 1,
            std::cmp::Ordering::Less => self.lost += 1,
        }
    }
    pub fn end_session(&mut self, net: f64) {
        self.sessions += 1;
        if net > 1e-9 {
            self.sessions_up += 1;
        } else if net.abs() <= 1e-9 {
            self.sessions_even += 1;
        }
    }
    pub fn merge(&mut self, o: &Stats) {
        self.hands += o.hands;
        self.net += o.net;
        self.net_sq += o.net_sq;
        self.bet += o.bet;
        self.wagered += o.wagered;
        self.won += o.won;
        self.pushed += o.pushed;
        self.lost += o.lost;
        self.sessions_up += o.sessions_up;
        self.sessions_even += o.sessions_even;
        self.sessions += o.sessions;
    }
    /// Rendimento medio per mano, in frazione della puntata minima (1 unità).
    pub fn ev(&self) -> f64 {
        self.net / self.hands as f64
    }
    pub fn sd(&self) -> f64 {
        let n = self.hands as f64;
        (self.net_sq / n - self.ev().powi(2)).max(0.0).sqrt()
    }
    /// Semiampiezza dell'intervallo di confidenza al 95% del rendimento per mano.
    pub fn ci95(&self) -> f64 {
        1.96 * self.sd() / (self.hands as f64).sqrt()
    }
    /// Rendimento per unità puntata in media all'inizio di ogni giro (conta per chi varia la puntata).
    pub fn ev_per_bet(&self) -> f64 {
        self.net / self.bet
    }
    pub fn pct(&self, x: u64) -> f64 {
        100.0 * x as f64 / self.hands as f64
    }
}

/// Simula `hands` giri su più thread. `make` crea la strategia per ogni thread.
pub fn simulate(
    rules: &Rules,
    hands: u64,
    seed: u64,
    threads: usize,
    make: &(dyn Fn() -> Box<dyn Strategy> + Sync),
) -> Stats {
    let threads = threads.max(1) as u64;
    // Ogni thread gioca un numero intero di sessioni.
    let sessions = hands.div_ceil(SESSION);
    std::thread::scope(|sc| {
        let jobs: Vec<_> = (0..threads)
            .map(|t| {
                let n_sess = sessions / threads + u64::from(t < sessions % threads);
                sc.spawn(move || {
                    let mut rng = SmallRng::seed_from_u64(seed.wrapping_mul(0x9e37_79b9).wrapping_add(t));
                    let mut shoe = Shoe::new(rules.decks, rules.penetration, &mut rng);
                    let mut s = make();
                    let mut st = Stats::default();
                    for _ in 0..n_sess {
                        let mut session = 0.0;
                        for _ in 0..SESSION {
                            let r = play_round(rules, &mut shoe, s.as_mut(), &mut rng);
                            st.record(&r);
                            session += r.net;
                        }
                        st.end_session(session);
                    }
                    st
                })
            })
            .collect();
        let mut total = Stats::default();
        for j in jobs {
            total.merge(&j.join().unwrap());
        }
        total
    })
}

/// Confronto appaiato di due strategie sulle stesse carte (solo con rimescolamento a ogni
/// mano): ogni giro parte dallo stesso seme per entrambe, quindi la differenza di rendimento
/// si misura con un errore molto più piccolo che simulandole separatamente.
pub struct Paired {
    pub a: Stats,
    pub b: Stats,
    pub diff: f64,
    pub diff_sq: f64,
}

impl Paired {
    /// Differenza media per mano (b - a) e semiampiezza dell'IC al 95%.
    pub fn diff(&self) -> (f64, f64) {
        let n = self.a.hands as f64;
        let m = self.diff / n;
        let sd = (self.diff_sq / n - m * m).max(0.0).sqrt();
        (m, 1.96 * sd / n.sqrt())
    }
}

pub fn compare(
    rules: &Rules,
    hands: u64,
    seed: u64,
    threads: usize,
    make_a: &(dyn Fn() -> Box<dyn Strategy> + Sync),
    make_b: &(dyn Fn() -> Box<dyn Strategy> + Sync),
) -> Paired {
    assert!(rules.penetration <= 0.0, "il confronto appaiato richiede il rimescolamento a ogni mano");
    let threads = threads.max(1) as u64;
    std::thread::scope(|sc| {
        let jobs: Vec<_> = (0..threads)
            .map(|t| {
                let n = hands / threads + u64::from(t < hands % threads);
                sc.spawn(move || {
                    let mut rng = SmallRng::seed_from_u64(seed ^ (t << 40));
                    let mut shoe = Shoe::new(rules.decks, 0.0, &mut rng);
                    let (mut sa, mut sb) = (make_a(), make_b());
                    let mut p = Paired { a: Stats::default(), b: Stats::default(), diff: 0.0, diff_sq: 0.0 };
                    for _ in 0..n {
                        let round = SmallRng::seed_from_u64(rng.gen());
                        let ra = play_round(rules, &mut shoe, sa.as_mut(), &mut round.clone());
                        let rb = play_round(rules, &mut shoe, sb.as_mut(), &mut round.clone());
                        p.a.record(&ra);
                        p.b.record(&rb);
                        let d = rb.net - ra.net;
                        p.diff += d;
                        p.diff_sq += d * d;
                    }
                    p
                })
            })
            .collect();
        let mut tot = Paired { a: Stats::default(), b: Stats::default(), diff: 0.0, diff_sq: 0.0 };
        for j in jobs {
            let p = j.join().unwrap();
            tot.a.merge(&p.a);
            tot.b.merge(&p.b);
            tot.diff += p.diff;
            tot.diff_sq += p.diff_sq;
        }
        tot
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_strategy_matches_exact_ev() {
        // Il simulatore e il calcolo esatto devono concordare entro l'errore statistico.
        let rules = Rules::preset(1);
        let chart = Chart::compute(&rules);
        let exact = chart.round_ev;
        let st = simulate(&rules, 2_000_000, 7, 4, &|| Box::new(Basic(chart.clone())));
        assert!((st.ev() - exact).abs() < st.ci95() + 0.002, "sim {} ± {} vs esatto {exact}", st.ev(), st.ci95());
    }

    #[test]
    fn naive_strategies_lose_more() {
        let rules = Rules::preset(0);
        let chart = Chart::compute(&rules);
        let basic = simulate(&rules, 400_000, 3, 4, &|| Box::new(Basic(chart.clone())));
        let mimic = simulate(&rules, 400_000, 3, 4, &|| Box::new(MimicDealer));
        let random = simulate(&rules, 400_000, 3, 4, &|| Box::new(Random));
        assert!(basic.ev() > mimic.ev() + 0.02, "{} {}", basic.ev(), mimic.ev());
        assert!(mimic.ev() > random.ev(), "{} {}", mimic.ev(), random.ev());
    }
}
