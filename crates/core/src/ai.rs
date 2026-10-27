//! Giocatori artificiali: casuale, euristico e motore Monte Carlo.

use crate::cards::*;
use crate::game::{Game, Observation};
use crate::net::Net;
use std::sync::Arc;
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
    /// Vittorie (pareggio = ½) e punti finali di chi ha giocato la carta.
    wins: f64,
    points: f64,
}

/// Risultato della ricerca per una carta giocabile adesso.
#[derive(Clone, Copy, Debug)]
pub struct RootChild {
    pub card: Card,
    pub visits: f64,
    /// Somme su tutte le visite: utilità, vittorie e punti finali.
    pub reward: f64,
    pub wins: f64,
    pub points: f64,
}

pub struct IsmctsPlayer {
    pub iterations: u32,
    /// Politica delle simulazioni: `None` = euristica con il 10% di mosse casuali (classica),
    /// altrimenti a carte scoperte senza mosse casuali.
    pub policy: Option<RolloutParams>,
    /// Con al più queste carte nel mazzo la simulazione si ferma e il resto si risolve esatto.
    pub rollout_solve: Option<u8>,
    /// Rete di valutazione: se c'è, le foglie con il mazzo ancora lungo si valutano con la
    /// rete invece che (o insieme) alla simulazione.
    pub value: Option<Arc<Net>>,
    /// Peso della rete nella valutazione delle foglie (1 = solo rete, 0.5 = media con la simulazione).
    pub value_mix: f64,
}

impl IsmctsPlayer {
    /// ISMCTS classico: simulazioni euristiche con mosse casuali fino alla fine.
    pub fn new(iterations: u32) -> Self {
        IsmctsPlayer { iterations, policy: None, rollout_solve: None, value: None, value_mix: 1.0 }
    }

    /// ISMCTS potenziato: le stesse simulazioni del motore principale (a carte scoperte,
    /// finale risolto con alfa-beta da 2 carte nel mazzo).
    pub fn strong(iterations: u32) -> Self {
        IsmctsPlayer {
            iterations,
            policy: Some(RolloutParams::default()),
            rollout_solve: Some(2),
            value: None,
            value_mix: 1.0,
        }
    }

    /// ISMCTS potenziato con la rete di valutazione nelle foglie.
    pub fn with_net(iterations: u32, net: Arc<Net>, mix: f64) -> Self {
        IsmctsPlayer { value: Some(net), value_mix: mix, ..IsmctsPlayer::strong(iterations) }
    }
}

impl Player for IsmctsPlayer {
    fn name(&self) -> String {
        if self.value.is_some() {
            format!("ISMCTS con rete ({} iterazioni)", self.iterations)
        } else if self.policy.is_some() {
            format!("ISMCTS potenziato ({} iterazioni)", self.iterations)
        } else {
            format!("ISMCTS ({} iterazioni)", self.iterations)
        }
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        if obs.hand.len() == 1 {
            return obs.hand.iter().next().unwrap();
        }
        let root = self.search(obs, rng);
        root.iter().max_by(|a, b| a.visits.partial_cmp(&b.visits).unwrap()).unwrap().card
    }
}

impl IsmctsPlayer {
    /// Ricerca dalla posizione: per ogni carta giocabile, visite e risultati.
    pub fn search(&self, obs: &Observation, rng: &mut SmallRng) -> Vec<RootChild> {
        self.search_with(obs, rng, |rng| obs.determinize(rng))
    }

    /// Ricerca con un campionatore dei mondi a scelta (es. mani avversarie pesate).
    pub fn search_with(
        &self,
        obs: &Observation,
        rng: &mut SmallRng,
        mut world: impl FnMut(&mut SmallRng) -> Game,
    ) -> Vec<RootChild> {
        let mut solver = Solver::default();
        let mut nodes = vec![Node { card: 255, mover: 1 - obs.me, children: vec![], visits: 0.0, avail: 0.0, reward: 0.0, wins: 0.0, points: 0.0 }];
        for _ in 0..self.iterations {
            let mut g = world(rng);
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
                    nodes.push(Node { card: c, mover, children: vec![], visits: 0.0, avail: 1.0, reward: 0.0, wins: 0.0, points: 0.0 });
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
            // Esito per il giocatore 0: probabilità di vittoria e punti finali.
            let (w0, p0) = match self.policy {
                None => {
                    rollout(&mut g, 0.1, rng);
                    (result(g.points[0]), g.points[0] as f64)
                }
                Some(p) => {
                    let leaf = g;
                    let solve_at = self.rollout_solve.unwrap_or(0);
                    let net = self.value.as_ref().filter(|_| !leaf.is_over() && leaf.deck_len > solve_at);
                    let by_net = net.map(|net| {
                        let (w, pts) = net.eval(&leaf);
                        let (w, pts) = (w as f64, pts as f64);
                        if leaf.to_move() == 0 { (w, pts) } else { (1.0 - w, 120.0 - pts) }
                    });
                    let mut by_rollout = || {
                        rollout_until(&mut g, Some(&p), 0.0, self.rollout_solve, rng);
                        let p0 = if g.is_over() { g.points[0] } else { solver.solve(&g, 0) };
                        (result(p0), p0 as f64)
                    };
                    match by_net {
                        Some(v) if self.value_mix >= 1.0 => v,
                        Some((w, pts)) => {
                            let (rw, rp) = by_rollout();
                            let m = self.value_mix;
                            (m * w + (1.0 - m) * rw, m * pts + (1.0 - m) * rp)
                        }
                        None => by_rollout(),
                    }
                }
            };
            for &id in &path {
                let n = &mut nodes[id];
                let (w, pts) = if n.mover == 0 { (w0, p0) } else { (1.0 - w0, 120.0 - p0) };
                n.visits += 1.0;
                n.reward += w + pts / 1200.0;
                n.wins += w;
                n.points += pts;
            }
        }
        nodes[0]
            .children
            .iter()
            .map(|&k| {
                let n = &nodes[k];
                RootChild { card: n.card, visits: n.visits, reward: n.reward, wins: n.wins, points: n.points }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Politica di rollout "a carte scoperte": nei mondi campionati entrambe le mani sono
// note, quindi chi apre può prevedere la risposta e scegliere l'apertura migliore.

/// Costo di spendere una carta adesso invece di tenerla (vale solo per le briscole).
#[inline]
fn keep_cost(c: Card, trump: u8, p: &RolloutParams) -> i32 {
    if suit(c) == trump {
        p.trump_base + p.trump_mult * strength(c) as i32
    } else {
        0
    }
}

/// Valore della presa per chi risponde con `c` alla carta `lead`.
#[inline]
fn follow_value(c: Card, lead: Card, trump: u8, p: &RolloutParams) -> i32 {
    let pts = (points(lead) + points(c)) as i32 * 4;
    let swing = if beats(c, lead, trump) { pts + p.lead_bonus } else { -pts };
    swing - keep_cost(c, trump, p)
}

fn best_follow(hand: CardSet, lead: Card, trump: u8, p: &RolloutParams) -> Card {
    hand.iter()
        .max_by_key(|&c| (follow_value(c, lead, trump, p), std::cmp::Reverse(strength(c))))
        .unwrap()
}

/// Parametri della politica di rollout a carte scoperte.
#[derive(Clone, Copy, Debug)]
pub struct RolloutParams {
    pub trump_base: i32,
    pub trump_mult: i32,
    pub lead_bonus: i32,
    /// Quanto conta (in quarti) far spendere briscole all'avversario.
    pub opp_spend: i32,
}

impl Default for RolloutParams {
    fn default() -> Self {
        RolloutParams { trump_base: 8, trump_mult: 2, lead_bonus: 0, opp_spend: 4 }
    }
}

/// Mossa a carte scoperte: usa anche la mano avversaria del mondo simulato.
pub fn open_hand_move(g: &Game, p: &RolloutParams) -> Card {
    let me = g.to_move() as usize;
    let hand = g.hands[me];
    let trump = g.trump;
    match g.table {
        Some(lead) => best_follow(hand, lead, trump, p),
        None => {
            let opp = g.hands[1 - me];
            if opp.is_empty() {
                return hand.iter().next().unwrap();
            }
            hand.iter()
                .max_by_key(|&l| {
                    let r = best_follow(opp, l, trump, p);
                    let pts = (points(l) + points(r)) as i32 * 4;
                    let swing = if beats(r, l, trump) { -pts - p.lead_bonus } else { pts };
                    (
                        swing - keep_cost(l, trump, p) + keep_cost(r, trump, p) * p.opp_spend / 4,
                        std::cmp::Reverse(strength(l)),
                    )
                })
                .unwrap()
        }
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
    /// Thread usati per simulare i mondi. 1 = tutto sul thread chiamante, con risultati
    /// riproducibili dal seme; i test massivi lo lasciano a 1 perché parallelizzano le partite.
    pub threads: usize,
    /// Nei rollout, quando nel mazzo restano al più queste carte si smette di simulare
    /// e si risolve il resto della partita con alfa-beta (`None` = rollout fino alla fine).
    pub rollout_solve: Option<u8>,
    /// Politica dei rollout: `None` = euristica classica, altrimenti a carte scoperte.
    pub rollout_policy: Option<RolloutParams>,
    /// Se impostato, finché c'è il mazzo la mossa si sceglie con l'ISMCTS potenziato con
    /// queste iterazioni invece che con i mondi (`samples` vale solo a mazzo finito).
    pub ismcts: Option<u32>,
}

impl EngineConfig {
    /// Motore attuale: rollout a carte scoperte senza mosse casuali, finale esatto da 8
    /// carte nel mazzo e rollout che si fermano a 2 carte per risolvere il resto.
    pub fn strength(samples: u32) -> EngineConfig {
        EngineConfig {
            samples,
            solve_deck_len: 8,
            rollout_epsilon: 0.0,
            points_weight: 1.0 / 1200.0,
            rollout_solve: Some(2),
            rollout_policy: Some(RolloutParams::default()),
            threads: 1,
            ismcts: None,
        }
    }

    /// Motore della v0.1.1: rollout euristici con il 10% di mosse casuali, finale esatto
    /// da 4 carte. Resta come riferimento per i confronti.
    pub fn classic(samples: u32) -> EngineConfig {
        EngineConfig {
            samples,
            solve_deck_len: 4,
            rollout_epsilon: 0.1,
            points_weight: 1.0 / 1200.0,
            rollout_solve: None,
            rollout_policy: None,
            threads: 1,
            ismcts: None,
        }
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

fn rollout(g: &mut Game, eps: f64, rng: &mut SmallRng) {
    rollout_until(g, None, eps, None, rng);
}

/// Simula fino alla fine, o finché nel mazzo restano al più `deck_limit` carte, con la
/// politica euristica (`policy = None`) o a carte scoperte.
fn rollout_until(
    g: &mut Game,
    policy: Option<&RolloutParams>,
    eps: f64,
    deck_limit: Option<u8>,
    rng: &mut SmallRng,
) {
    while !g.is_over() && deck_limit.map_or(true, |l| g.deck_len > l) {
        let p = g.to_move() as usize;
        let hand = g.hands[p];
        let c = if rng.gen_bool(eps) {
            let n = hand.len();
            hand.iter().nth(rng.gen_range(0..n as usize)).unwrap()
        } else if let Some(params) = policy {
            open_hand_move(g, params)
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
    // Limiti certi: i punti già presi e quelli ancora in gioco.
    let lo = g.points[me as usize] as i32;
    let hi = 120 - g.points[1 - me as usize] as i32;
    if lo >= beta {
        return lo;
    }
    if hi <= alpha {
        return hi;
    }
    let p = g.to_move();
    let maximizing = p == me;
    // Ordinamento: prima le carte con più punti (tagli più frequenti).
    // Al più 3 carte in mano: si ordina su un array, senza allocare.
    let mut moves = [0 as Card; 3];
    let mut n = 0;
    for c in g.legal_moves().iter() {
        let key = points(c) * 4 + strength(c);
        let mut j = n;
        while j > 0 && points(moves[j - 1]) * 4 + strength(moves[j - 1]) < key {
            moves[j] = moves[j - 1];
            j -= 1;
        }
        moves[j] = c;
        n += 1;
    }
    let mut best = if maximizing { i32::MIN } else { i32::MAX };
    for &c in &moves[..n] {
        let mut next = *g;
        next.play(c);
        let v = solve(&next, me, alpha, beta);
        if maximizing {
            best = best.max(v);
            alpha = alpha.max(v);
        } else {
            best = best.min(v);
            beta = beta.min(v);
        }
        if alpha >= beta {
            break;
        }
    }
    best
}

/// Risolutore alfa-beta con tabella delle trasposizioni, per i molti mondi di una valutazione.
///
/// Calcola i punti che il giocatore 0 farà da qui alla fine (non dipendono dai punti già
/// presi), così le posizioni ritrovate per strade diverse, o in mondi diversi a mazzo vuoto,
/// si riprendono dalla tabella. Dà sempre lo stesso valore di `solve`.
pub struct Solver {
    tt: Vec<TtEntry>,
    /// Impronta del mazzo coperto del mondo corrente, per ogni lunghezza del mazzo.
    zdeck: [u64; NUM_CARDS + 1],
}

#[derive(Clone, Copy, Default)]
struct TtEntry {
    key: u64,
    lo: u8,
    hi: u8,
}

const TT_BITS: u32 = 16;

#[inline]
fn mix(mut x: u64) -> u64 {
    // splitmix64
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

impl Default for Solver {
    fn default() -> Self {
        Solver { tt: vec![TtEntry::default(); 1 << TT_BITS], zdeck: [0; NUM_CARDS + 1] }
    }
}

impl Solver {
    /// Punti finali di `me` giocando perfettamente da `g`.
    pub fn solve(&mut self, g: &Game, me: u8) -> u8 {
        // Impronta di ogni prefisso del mazzo: il mazzo coperto è deck[0..deck_len].
        let mut h = 0u64;
        for k in 0..g.deck_len as usize {
            h = mix(h ^ g.deck[k] as u64 ^ (k as u64) << 8);
            self.zdeck[k + 1] = h;
        }
        let rem = 120 - g.points[0] as i32 - g.points[1] as i32;
        let v0 = self.ab(g, -1, rem + 1);
        let mine = if me == 0 { v0 } else { rem - v0 };
        (g.points[me as usize] as i32 + mine) as u8
    }

    #[inline]
    fn key(&self, g: &Game) -> u64 {
        let small = g.leader as u64 | (g.table.map_or(63, |c| c as u64) << 1) | (g.deck_len as u64) << 8 | (g.trump as u64) << 16;
        mix(g.hands[0].0) ^ mix(g.hands[1].0 ^ 0x5555_0000_0000_0000) ^ mix(small ^ 0xABCD_0000) ^ self.zdeck[g.deck_len as usize]
    }

    /// Punti futuri del giocatore 0, con finestra (alpha, beta), fail-soft.
    fn ab(&mut self, g: &Game, mut alpha: i32, mut beta: i32) -> i32 {
        if g.is_over() {
            return 0;
        }
        let key = self.key(g);
        let slot = (key >> (64 - TT_BITS)) as usize;
        let e = self.tt[slot];
        let (mut lo, mut hi) = (0i32, 120 - g.points[0] as i32 - g.points[1] as i32);
        if e.key == key {
            lo = e.lo as i32;
            hi = e.hi as i32;
            if lo >= beta || lo == hi {
                return lo;
            }
            if hi <= alpha {
                return hi;
            }
            alpha = alpha.max(lo);
            beta = beta.min(hi);
        }
        let maximizing = g.to_move() == 0;
        let mut moves = [0 as Card; 3];
        let mut n = 0;
        for c in g.legal_moves().iter() {
            let key = points(c) * 4 + strength(c);
            let mut j = n;
            while j > 0 && points(moves[j - 1]) * 4 + strength(moves[j - 1]) < key {
                moves[j] = moves[j - 1];
                j -= 1;
            }
            moves[j] = c;
            n += 1;
        }
        let (a0, b0) = (alpha, beta);
        let mut best = if maximizing { i32::MIN } else { i32::MAX };
        for &c in &moves[..n] {
            let mut next = *g;
            let gained = next.play(c).map_or(0, |t| if t.winner == 0 { t.points as i32 } else { 0 });
            let v = gained + self.ab(&next, alpha - gained, beta - gained);
            if maximizing {
                best = best.max(v);
                alpha = alpha.max(v);
            } else {
                best = best.min(v);
                beta = beta.min(v);
            }
            if alpha >= beta {
                break;
            }
        }
        if best <= a0 {
            hi = hi.min(best);
        } else if best >= b0 {
            lo = lo.max(best);
        } else {
            lo = best;
            hi = best;
        }
        self.tt[slot] = TtEntry { key, lo: lo as u8, hi: hi as u8 };
        best
    }
}

/// Somme dei risultati per ogni mossa candidata (al più 3), su un gruppo di mondi.
#[derive(Clone, Copy, Default)]
struct Sums {
    util: [f64; 3],
    wins: [f64; 3],
    pts: [f64; 3],
}

impl Sums {
    fn add(&mut self, i: usize, final_pts: u8, cfg: &EngineConfig) {
        self.util[i] += result(final_pts) + final_pts as f64 * cfg.points_weight;
        self.wins[i] += result(final_pts);
        self.pts[i] += final_pts as f64;
    }

    fn merge(&mut self, o: &Sums) {
        for i in 0..3 {
            self.util[i] += o.util[i];
            self.wins[i] += o.wins[i];
            self.pts[i] += o.pts[i];
        }
    }
}

/// Identifica un mondo del finale: mano avversaria e ordine del mazzo coperto.
/// `None` se il mazzo è troppo lungo per stare nella chiave.
fn world_key(g: &Game, opp: u8) -> Option<u128> {
    if g.deck_len > 15 {
        return None;
    }
    let mut k = g.hands[opp as usize].0 as u128;
    for &c in &g.deck[1..g.deck_len.max(1) as usize] {
        k = k << 6 | c as u128;
    }
    Some(k)
}

/// Gioca `samples` mondi e somma i risultati di ogni mossa.
fn simulate(obs: &Observation, moves: &[Card], cfg: &EngineConfig, samples: u32, rng: &mut SmallRng) -> Sums {
    let me = obs.me;
    let exact = obs.deck_len <= cfg.solve_deck_len;
    let mut sums = Sums::default();
    // Nel finale i mondi distinti sono pochi (al più 120 con 4 carte nel mazzo) e il
    // risolutore è deterministico: ogni mondo già visto si riprende dalla memoria.
    let mut memo: std::collections::HashMap<u128, [u8; 3]> = Default::default();
    let mut solver = Solver::default();
    for _ in 0..samples {
        // Lo stesso mondo per tutte le mosse: confronto a parità di carte.
        let world = obs.determinize(rng);
        if exact {
            let mut solve_all = || {
                let mut finals = [0u8; 3];
                for (i, &c) in moves.iter().enumerate() {
                    let mut g = world;
                    g.play(c);
                    finals[i] = solver.solve(&g, me);
                }
                finals
            };
            let finals = match world_key(&world, 1 - me) {
                Some(k) => *memo.entry(k).or_insert_with(solve_all),
                None => solve_all(),
            };
            for i in 0..moves.len() {
                sums.add(i, finals[i], cfg);
            }
        } else {
            for (i, &c) in moves.iter().enumerate() {
                let mut g = world;
                g.play(c);
                let policy = cfg.rollout_policy.as_ref();
                rollout_until(&mut g, policy, cfg.rollout_epsilon, cfg.rollout_solve, rng);
                let final_pts =
                    if g.is_over() { g.points[me as usize] } else { solver.solve(&g, me) };
                sums.add(i, final_pts, cfg);
            }
        }
    }
    sums
}

/// Valuta tutte le mosse legali dal punto di vista di chi deve muovere.
pub fn evaluate(obs: &Observation, cfg: &EngineConfig, rng: &mut SmallRng) -> Vec<MoveEval> {
    if let Some(iterations) = cfg.ismcts {
        if obs.deck_len > 0 {
            return evaluate_ismcts(obs, iterations, rng);
        }
    }
    let moves = obs.hand.to_vec();
    // Se il mazzo è vuoto le mani sono note: basta un solo mondo.
    let samples = if obs.deck_len == 0 { 1 } else { cfg.samples.max(1) };
    // Con 4 carte o meno nel mazzo i mondi distinti sono pochi e la memoria li riprende:
    // il lavoro è già minimo e i thread non servono.
    let threads = if obs.deck_len <= 4 { 1 } else { (cfg.threads.max(1) as u32).min(samples / 64).max(1) };
    let sums = if threads == 1 {
        simulate(obs, &moves, cfg, samples, rng)
    } else {
        // Mondi divisi tra i thread, ognuno col proprio generatore derivato da `rng`.
        let seeds: Vec<u64> = (0..threads).map(|_| rng.gen()).collect();
        std::thread::scope(|s| {
            let handles: Vec<_> = seeds
                .iter()
                .enumerate()
                .map(|(t, &seed)| {
                    let share = samples / threads + u32::from((t as u32) < samples % threads);
                    let moves = &moves;
                    s.spawn(move || simulate(obs, moves, cfg, share, &mut SmallRng::seed_from_u64(seed)))
                })
                .collect();
            let mut total = Sums::default();
            for h in handles {
                total.merge(&h.join().expect("thread del motore"));
            }
            total
        })
    };
    let n = samples as f64;
    let mut out: Vec<(f64, MoveEval)> = moves
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            (sums.util[i] / n, MoveEval { card: c, win_prob: sums.wins[i] / n, exp_points: sums.pts[i] / n })
        })
        .collect();
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    out.into_iter().map(|(_, e)| e).collect()
}

/// Valutazione con l'ISMCTS potenziato, su un solo albero (un thread): è il giocatore
/// provato nei test massivi. Le mosse sono in ordine di visite: la prima è quella scelta.
pub fn evaluate_ismcts(obs: &Observation, iterations: u32, rng: &mut SmallRng) -> Vec<MoveEval> {
    let mut total = IsmctsPlayer::strong(iterations).search(obs, rng);
    total.sort_by(|a, b| b.visits.partial_cmp(&a.visits).unwrap());
    let mut out: Vec<MoveEval> = total
        .iter()
        .map(|c| MoveEval { card: c.card, win_prob: c.wins / c.visits, exp_points: c.points / c.visits })
        .collect();
    // Con pochissime iterazioni una carta può non essere mai stata provata: va in fondo.
    for c in obs.hand.iter().filter(|&c| !out.iter().any(|e| e.card == c)).collect::<Vec<_>>() {
        out.push(MoveEval { card: c, win_prob: 0.0, exp_points: 0.0 });
    }
    out
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

    /// Minimax senza tagli né ordinamento, come riferimento.
    fn minimax(g: &Game, me: u8) -> i32 {
        if g.is_over() {
            return g.points[me as usize] as i32;
        }
        let vals = g.legal_moves().iter().map(|c| {
            let mut n = *g;
            n.play(c);
            minimax(&n, me)
        });
        if g.to_move() == me { vals.max().unwrap() } else { vals.min().unwrap() }
    }

    #[test]
    fn solver_matches_minimax() {
        let mut rng = SmallRng::seed_from_u64(11);
        for _ in 0..300 {
            let mut g = Game::new(&mut rng, 0);
            let steps = rng.gen_range(28..38);
            for _ in 0..steps {
                let m = g.legal_moves().to_vec();
                g.play(m[rng.gen_range(0..m.len())]);
            }
            let me = rng.gen_range(0..2);
            assert_eq!(solve(&g, me, i32::MIN, i32::MAX), minimax(&g, me));
        }
    }

    #[test]
    fn table_solver_matches_plain_solver() {
        let mut rng = SmallRng::seed_from_u64(12);
        let mut solver = Solver::default();
        for _ in 0..400 {
            let mut g = Game::new(&mut rng, 0);
            let steps = rng.gen_range(16..38);
            for _ in 0..steps {
                let m = g.legal_moves().to_vec();
                g.play(m[rng.gen_range(0..m.len())]);
            }
            let me = rng.gen_range(0..2);
            assert_eq!(solver.solve(&g, me) as i32, solve(&g, me, i32::MIN, i32::MAX));
        }
    }

    #[test]
    fn open_hand_policy_basics() {
        let p = RolloutParams::default();
        let mut order: Vec<Card> = (0..40).collect();
        order.swap(0, 5); // briscola: Denari
        let mut g = Game::from_order(&order, 0);
        // Chi risponde a un Tre di Coppe con Due di Denari (briscola), Re e Quattro di Spade taglia.
        g.hands = [CardSet::EMPTY; 2];
        g.hands[0].insert(make(1, 2));
        for c in [make(0, 1), make(2, 9), make(2, 3)] {
            g.hands[1].insert(c);
        }
        g.play(make(1, 2));
        assert_eq!(open_hand_move(&g, &p), make(0, 1));
        // Chi apre non mette l'Asso di Coppe se l'avversario può tagliarlo: apre il liscio.
        let mut g = Game::from_order(&order, 0);
        g.hands = [CardSet::EMPTY; 2];
        for c in [make(1, 0), make(2, 3)] {
            g.hands[0].insert(c);
        }
        for c in [make(0, 1), make(3, 4)] {
            g.hands[1].insert(c);
        }
        assert_eq!(open_hand_move(&g, &p), make(2, 3));
    }

    #[test]
    fn ismcts_evaluation_is_complete() {
        let mut rng = SmallRng::seed_from_u64(6);
        let g = Game::new(&mut rng, 0);
        let obs = Observation::of(&g, 0);
        let mut cfg = EngineConfig::strength(100);
        cfg.ismcts = Some(4000);
        let evals = evaluate(&obs, &cfg, &mut rng);
        assert_eq!(evals.len(), 3);
        assert!(evals.iter().all(|e| (0.0..=1.0).contains(&e.win_prob) && e.exp_points <= 120.0));
    }

    #[test]
    fn threaded_evaluation_is_complete() {
        let mut rng = SmallRng::seed_from_u64(5);
        let g = Game::new(&mut rng, 0);
        let obs = Observation::of(&g, 0);
        let mut cfg = EngineConfig::strength(1000);
        cfg.threads = 4;
        let evals = evaluate(&obs, &cfg, &mut rng);
        assert_eq!(evals.len(), 3);
        assert!(evals.iter().all(|e| (0.0..=1.0).contains(&e.win_prob) && e.exp_points <= 120.0));
    }
}
