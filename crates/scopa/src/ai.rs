//! Giocatori artificiali di scopa: stili umani, ISMCTS e motore Monte Carlo (PIMC).

use crate::game::*;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// Un giocatore sceglie una mossa conoscendo solo la propria osservazione.
pub trait Player: Send {
    fn name(&self) -> String;
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Move;
}

// ---------------------------------------------------------------------------
// Valore delle carte per le regole pratiche (in centesimi di punto, circa).

/// Quanto vale prendere ogni carta e fare scopa, secondo uno stile di gioco.
#[derive(Clone, Copy, Debug)]
pub struct Weights {
    pub card: i32,
    pub denaro: i32,
    pub settebello: i32,
    pub seven: i32,
    pub six: i32,
    pub ace: i32,
    pub scopa: i32,
    /// Costo di lasciare in tavola una carta senza prendere: valore della carta diviso questo numero.
    pub discard_div: i32,
}

impl Default for Weights {
    fn default() -> Self {
        Weights { card: 10, denaro: 20, settebello: 100, seven: 35, six: 15, ace: 8, scopa: 100, discard_div: 3 }
    }
}

impl Weights {
    #[inline]
    pub fn worth(&self, c: Card) -> i32 {
        let mut w = self.card;
        if suit(c) == 0 {
            w += self.denaro;
        }
        if c == SETTEBELLO {
            w += self.settebello;
        }
        w + match rank(c) {
            6 => self.seven,
            5 => self.six,
            0 => self.ace,
            _ => 0,
        }
    }

    /// Pesi adattati alla smazzata in corso: carte e denari non valgono più niente quando il
    /// punto è già deciso (21 carte o 6 denari a qualcuno), i sette e i sei contano meno
    /// quando la primiera è già praticamente assegnata.
    pub fn in_context(&self, captured: [CardSet; 2]) -> Weights {
        let mut w = *self;
        let cards = [captured[0].len(), captured[1].len()];
        if cards[0] >= 21 || cards[1] >= 21 {
            w.card = 1;
        }
        let den = [(captured[0].0 & 0x3FF).count_ones(), (captured[1].0 & 0x3FF).count_ones()];
        if den[0] >= 6 || den[1] >= 6 {
            w.denaro = 0;
        }
        let sevens = |s: CardSet| (0..4u8).filter(|&su| s.contains(make(su, 6))).count();
        let s = [sevens(captured[0]), sevens(captured[1])];
        if s[0] >= 3 || s[1] >= 3 {
            w.seven /= 3;
            w.six /= 3;
            w.ace /= 2;
        }
        w
    }

    fn set_worth(&self, s: CardSet) -> i32 {
        s.iter().map(|c| self.worth(c)).sum()
    }

    /// Guadagno immediato di una mossa sul tavolo `table`. `last` = ultima giocata della smazzata.
    #[inline]
    pub fn gain(&self, m: Move, table: CardSet, last: bool) -> i32 {
        if m.take.is_empty() {
            -self.worth(m.card) / self.discard_div
        } else {
            let scopa = if m.take == table && !last { self.scopa } else { 0 };
            self.set_worth(m.take) + self.worth(m.card) + scopa
        }
    }

    /// Miglior guadagno giocando `c` su `table`.
    fn best_gain_with(&self, table: CardSet, c: Card, last: bool) -> i32 {
        let mut best = i32::MIN;
        let any = for_each_capture(table, c, |take| best = best.max(self.gain(Move { card: c, take }, table, last)));
        if any {
            best
        } else {
            self.gain(Move { card: c, take: CardSet::EMPTY }, table, last)
        }
    }

    /// Miglior guadagno di chi ha in mano `hand` sul tavolo `table` (risposta a carte scoperte).
    pub fn best_reply(&self, hand: CardSet, table: CardSet, last: bool) -> i32 {
        hand.iter().map(|c| self.best_gain_with(table, c, last)).max().unwrap_or(0)
    }
}

/// Tavolo dopo la mossa.
#[inline]
pub fn table_after(table: CardSet, m: Move) -> CardSet {
    if m.take.is_empty() {
        table.union(single(m.card))
    } else {
        table.minus(m.take)
    }
}

/// Come stima il pericolo di ciò che lascia in tavola.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Risk {
    /// Non ci pensa.
    None,
    /// Regola da bar: evita di lasciare in tavola meno di 11 (scopa possibile), con questo costo.
    Naive(i32),
    /// Conta le carte uscite: stima la miglior risposta avversaria dalle carte ancora in gioco.
    Counted,
}

/// Probabilità che una mano di `h` carte, presa a caso tra `u`, contenga almeno una delle `k` carte indicate.
fn p_holds(u: u32, k: u32, h: u32) -> f64 {
    if k == 0 || h == 0 {
        return 0.0;
    }
    if u < h || k > u {
        return 1.0;
    }
    let mut none = 1.0;
    for i in 0..h {
        if u - i < k + 1 {
            return 1.0;
        }
        none *= (u - k - i) as f64 / (u - i) as f64;
    }
    1.0 - none
}

/// Stima della miglior risposta avversaria sul tavolo `t`, contando le carte non ancora viste.
fn counted_risk(w: &Weights, t: CardSet, unknown: CardSet, opp_len: u32, last: bool) -> f64 {
    if opp_len == 0 {
        return 0.0;
    }
    let u = unknown.len();
    let mut opts: Vec<(i32, f64)> = Vec::with_capacity(10);
    for r in 0..10u8 {
        let of_rank: Vec<Card> = (0..4u8).map(|s| make(s, r)).filter(|&c| unknown.contains(c)).collect();
        if of_rank.is_empty() {
            continue;
        }
        let g = of_rank.iter().map(|&c| w.best_gain_with(t, c, last)).max().unwrap();
        if g > 0 {
            opts.push((g, p_holds(u, of_rank.len() as u32, opp_len)));
        }
    }
    // Valore atteso della migliore tra le prese disponibili (eventi considerati indipendenti).
    opts.sort_by(|a, b| b.0.cmp(&a.0));
    let mut none_before = 1.0;
    let mut e = 0.0;
    for (g, p) in opts {
        e += g as f64 * p * none_before;
        none_before *= 1.0 - p;
    }
    e
}

/// Mossa secondo uno stile: guadagno immediato meno il pericolo stimato.
pub fn style_move(obs: &Observation, w: &Weights, risk: Risk, moves: &[Move]) -> Move {
    let last = obs.deck_len == 0 && obs.hand.len() == 1 && obs.opp_hand_len == 0;
    let reply_last = obs.deck_len == 0 && obs.hand.len() == 1 && obs.opp_hand_len == 1;
    let unknown = obs.unknown();
    let mut best = moves[0];
    let mut best_v = f64::MIN;
    for &m in moves {
        let t = table_after(obs.table, m);
        let mut v = w.gain(m, obs.table, last) as f64;
        if !t.is_empty() {
            match risk {
                Risk::None => {}
                Risk::Naive(cost) => {
                    if sum(t) <= 10 && obs.opp_hand_len > 0 {
                        v -= cost as f64;
                    }
                }
                Risk::Counted => v -= counted_risk(w, t, unknown, obs.opp_hand_len as u32, reply_last),
            }
        }
        // A parità, tiene le carte che valgono di più.
        v -= w.worth(m.card) as f64 * 1e-3;
        if v > best_v {
            best_v = v;
            best = m;
        }
    }
    best
}

/// Mossa a carte scoperte (nei mondi simulati): guadagno meno la miglior risposta avversaria esatta.
pub fn open_hand_move(g: &Game, w: &Weights, moves: &[Move]) -> Move {
    let me = g.to_move as usize;
    let opp = g.hands[1 - me];
    let last = g.last_play();
    let reply_last = g.deck_len == 0 && g.hands[me].len() == 1 && opp.len() == 1;
    let mut best = moves[0];
    let mut best_v = i32::MIN;
    for &m in moves {
        let t = table_after(g.table, m);
        let mut v = w.gain(m, g.table, last) * 4;
        if !opp.is_empty() {
            v -= w.best_reply(opp, t, reply_last) * 4;
        }
        v -= w.worth(m.card) / 8;
        if v > best_v {
            best_v = v;
            best = m;
        }
    }
    best
}

// ---------------------------------------------------------------------------
// Avversari: casuale e stili umani.

pub struct RandomPlayer;

impl Player for RandomPlayer {
    fn name(&self) -> String {
        "Casuale".into()
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Move {
        let mut moves = Vec::new();
        obs.legal_moves(&mut moves);
        moves[rng.gen_range(0..moves.len())]
    }
}

/// Uno stile di gioco umano: pesi delle carte, stima del pericolo e, se `endgame`,
/// finale a mazzo vuoto giocato in modo perfetto (le carte avversarie sono note).
pub struct StylePlayer {
    pub label: &'static str,
    pub weights: Weights,
    pub risk: Risk,
    pub endgame: bool,
}

impl StylePlayer {
    /// "Goloso": prende sempre la presa più ricca, non guarda cosa lascia.
    pub fn greedy() -> Self {
        StylePlayer { label: "Goloso", weights: Weights::default(), risk: Risk::None, endgame: false }
    }
    /// Giocatore da bar: prende il più possibile e non lascia scope facili.
    pub fn heuristic() -> Self {
        StylePlayer { label: "Euristico", weights: Weights::default(), risk: Risk::Naive(70), endgame: false }
    }
    /// Cacciatore di scope: punta tutto sulle scope.
    pub fn scope_hunter() -> Self {
        let w = Weights { scopa: 300, ..Weights::default() };
        StylePlayer { label: "Cacciatore di scope", weights: w, risk: Risk::Naive(150), endgame: false }
    }
    /// Primierista: insegue i sette e i sei, non li butta mai.
    pub fn primiera() -> Self {
        let w = Weights { seven: 90, six: 45, ace: 20, ..Weights::default() };
        StylePlayer { label: "Primierista", weights: w, risk: Risk::Naive(70), endgame: false }
    }
    /// Denarista: raccoglie denari.
    pub fn denari() -> Self {
        let w = Weights { denaro: 60, ..Weights::default() };
        StylePlayer { label: "Denarista", weights: w, risk: Risk::Naive(70), endgame: false }
    }
    /// Contacarte: conta le carte uscite per stimare la risposta avversaria e gioca il finale perfetto.
    pub fn counter() -> Self {
        StylePlayer { label: "Contacarte", weights: Weights::default(), risk: Risk::Counted, endgame: true }
    }
}

impl Player for StylePlayer {
    fn name(&self) -> String {
        self.label.into()
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Move {
        let mut moves = Vec::new();
        obs.legal_moves(&mut moves);
        if moves.len() == 1 {
            return moves[0];
        }
        if self.endgame && obs.deck_len == 0 {
            let g = obs.determinize(rng);
            return best_by_solver(&g, obs.me, &moves);
        }
        style_move(obs, &self.weights, self.risk, &moves)
    }
}

/// Fa una mossa casuale con probabilità `eps`, altrimenti gioca come `inner`: errori umani.
pub struct NoisyPlayer {
    pub eps: f64,
    pub inner: Box<dyn Player>,
}

impl Player for NoisyPlayer {
    fn name(&self) -> String {
        format!("{} con {:.0}% di errori", self.inner.name(), self.eps * 100.0)
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Move {
        if rng.gen_bool(self.eps) {
            RandomPlayer.choose(obs, rng)
        } else {
            self.inner.choose(obs, rng)
        }
    }
}

// ---------------------------------------------------------------------------
// Risolutore esatto del finale (mazzo vuoto: al più 6 giocate, carte tutte note).

/// Differenza punti finale (me − avversario) con gioco perfetto da entrambe le parti.
pub fn solve(g: &Game, me: u8, alpha: i32, beta: i32) -> i32 {
    solve_line(g, me, alpha, beta, &mut Vec::new()).0
}

/// Come `solve`, ma restituisce anche i punti finali della variante giocata.
/// `bufs` sono vettori di appoggio per le mosse, uno per livello, riusati tra le chiamate.
pub fn solve_line(g: &Game, me: u8, mut alpha: i32, mut beta: i32, bufs: &mut Vec<Vec<Move>>) -> (i32, [u8; 2]) {
    if g.is_over() {
        let p = g.score().points;
        return (p[me as usize] as i32 - p[1 - me as usize] as i32, p);
    }
    let mut moves = bufs.pop().unwrap_or_else(|| Vec::with_capacity(16));
    g.legal_moves(&mut moves);
    let maximizing = g.to_move == me;
    let mut best = (if maximizing { i32::MIN } else { i32::MAX }, [0u8; 2]);
    for &m in &moves {
        let mut n = *g;
        n.play(m);
        let v = solve_line(&n, me, alpha, beta, bufs);
        if maximizing {
            if v.0 > best.0 {
                best = v;
            }
            alpha = alpha.max(v.0);
        } else {
            if v.0 < best.0 {
                best = v;
            }
            beta = beta.min(v.0);
        }
        if alpha >= beta {
            break;
        }
    }
    bufs.push(moves);
    best
}

fn best_by_solver(g: &Game, me: u8, moves: &[Move]) -> Move {
    let mut bufs = Vec::new();
    *moves
        .iter()
        .max_by_key(|&&m| {
            let mut n = *g;
            n.play(m);
            solve_line(&n, me, i32::MIN, i32::MAX, &mut bufs).0
        })
        .unwrap()
}

// ---------------------------------------------------------------------------
// Valore della partita: probabilità di vincerla dato il punteggio, per un motore che gioca
// per vincere la partita a 11 e non solo la smazzata.

/// Probabilità di vincere la partita per chi ha `a` punti contro `b`, prima di una nuova smazzata,
/// tra giocatori di pari forza. Tabella calcolata con `match_table`.
pub struct MatchTable {
    v: Vec<f64>,
}

const MT: usize = 32;

impl MatchTable {
    /// Calcola la tabella dalla distribuzione dei punti di una smazzata (`dist[i][j]` = probabilità
    /// che il primo faccia `i` punti e il secondo `j`), iterando fino a convergenza.
    pub fn from_distribution(dist: &[[f64; 16]; 16]) -> MatchTable {
        let mut v = vec![0.5; MT * MT];
        for _ in 0..200 {
            let mut nv = v.clone();
            for a in 0..MT {
                for b in 0..MT {
                    let mut e = 0.0;
                    for (i, row) in dist.iter().enumerate() {
                        for (j, &p) in row.iter().enumerate() {
                            if p == 0.0 {
                                continue;
                            }
                            e += p * Self::after(&v, a + i, b + j);
                        }
                    }
                    nv[a * MT + b] = e;
                }
            }
            v = nv;
        }
        MatchTable { v }
    }

    fn after(v: &[f64], a: usize, b: usize) -> f64 {
        match match_winner([a.min(255) as u8, b.min(255) as u8]) {
            Some(0) => 1.0,
            Some(_) => 0.0,
            None => {
                // Pari sopra l'11: si gioca ancora, come da 10 a 10.
                let (a, b) = if a >= MT || b >= MT { (10, 10) } else { (a, b) };
                v[a * MT + b]
            }
        }
    }

    /// Probabilità di vincere la partita se la smazzata in corso finisce `a` a `b`.
    pub fn value(&self, a: u8, b: u8) -> f64 {
        Self::after(&self.v, a as usize, b as usize)
    }

    /// Tabella standard, dalla distribuzione dei punti di una smazzata tra due motori.
    pub fn standard() -> &'static MatchTable {
        static T: std::sync::OnceLock<MatchTable> = std::sync::OnceLock::new();
        T.get_or_init(|| MatchTable::from_distribution(&crate::hand_dist::HAND_DIST))
    }
}

// ---------------------------------------------------------------------------
// Motore Monte Carlo determinizzato (PIMC).

#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    /// Mondi campionati per ogni decisione.
    pub samples: u32,
    /// Probabilità di mossa casuale nei rollout.
    pub rollout_epsilon: f64,
    /// Pesi della politica dei rollout.
    pub policy: Weights,
    /// Come si gioca nei rollout.
    pub rollout: Rollout,
    /// Gioca per vincere la partita (probabilità di vittoria) invece che per la differenza punti.
    pub match_aware: bool,
    /// Nei rollout, a mazzo vuoto si risolve il finale in modo esatto.
    pub solve_endgame: bool,
    /// Thread per simulare i mondi (1 = riproducibile; i test massivi parallelizzano le partite).
    pub threads: usize,
    /// Ricerca ad albero (ISMCTS) invece di Monte Carlo piatto; `samples` = iterazioni.
    pub tree: bool,
    /// Costante di esplorazione dell'albero.
    pub exploration: f64,
    /// Deduzione delle carte avversarie dalle sue giocate nel giro (nessuna = mondi uniformi).
    pub infer: Option<Infer>,
    /// Nei rollout i pesi delle carte tengono conto dei punti già decisi nella smazzata.
    pub ctx_policy: bool,
    /// Nei rollout il finale esatto parte quando nel mazzo restano al più queste carte
    /// (0 = solo a mazzo vuoto; 6 = anche l'ultimo giro con le carte da pescare note nel mondo).
    pub solve_deck: u8,
}

/// Come si deducono le carte avversarie: si suppone che giochi come il Contacarte, ma non
/// sempre (scelta "morbida" con temperatura), e si pesano i mondi con la probabilità delle
/// giocate che ha fatto davvero.
#[derive(Clone, Copy, Debug)]
pub struct Infer {
    /// Quanto è deciso il modello: probabilità ∝ exp(beta × valore in centesimi di punto).
    pub beta: f64,
    /// Quota di giocate a caso nel modello (protegge dagli avversari che non giocano così).
    pub eps: f64,
    /// Mondi candidati per ogni mondo usato: se ne sceglie uno in proporzione al peso.
    pub candidates: u32,
}

impl Default for Infer {
    fn default() -> Self {
        Infer { beta: 0.03, eps: 0.1, candidates: 8 }
    }
}

/// Probabilità che l'avversario, con in mano ora `hand`, abbia fatto le giocate del giro.
pub fn play_likelihood(obs: &Observation, hand: CardSet, inf: &Infer, moves: &mut Vec<Move>) -> f64 {
    let w = Weights::default();
    let mut lik = 1.0;
    // La mano al momento della giocata i: quella di ora più le carte giocate da lì in poi.
    let mut h = hand;
    for p in obs.opp_plays().iter().rev() {
        h.insert(p.mv.card);
        moves.clear();
        moves_for(h, p.table, moves);
        if moves.len() < 2 {
            continue;
        }
        let unknown = CardSet::FULL.minus(h).minus(p.table).minus(p.seen);
        let mut top = f64::MIN;
        let mut chosen = f64::MIN;
        let mut vals = [0f64; 64];
        for (i, &m) in moves.iter().enumerate().take(64) {
            let t = table_after(p.table, m);
            let mut v = w.gain(m, p.table, false) as f64;
            if !t.is_empty() {
                v -= counted_risk(&w, t, unknown, p.my_len as u32, false);
            }
            v -= w.worth(m.card) as f64 * 1e-3;
            vals[i] = v;
            top = top.max(v);
            if m == p.mv {
                chosen = v;
            }
        }
        let n = moves.len().min(64);
        let z: f64 = vals[..n].iter().map(|v| (inf.beta * (v - top)).exp()).sum();
        let pc = if chosen == f64::MIN { 0.0 } else { (inf.beta * (chosen - top)).exp() / z };
        lik *= inf.eps / n as f64 + (1.0 - inf.eps) * pc;
    }
    lik
}

/// Un mondo compatibile con l'osservazione, scelto tenendo conto delle giocate avversarie.
fn sample_world(obs: &Observation, cfg: &EngineConfig, buf: &mut Vec<Move>, rng: &mut SmallRng) -> Game {
    let inf = match cfg.infer {
        Some(inf) if obs.opp_plays_len > 0 && obs.opp_hand_len + obs.opp_plays_len == 3 && obs.deck_len > 0 => inf,
        _ => return obs.determinize(rng),
    };
    let opp = 1 - obs.me as usize;
    let mut best = obs.determinize(rng);
    let mut total = play_likelihood(obs, best.hands[opp], &inf, buf);
    for _ in 1..inf.candidates {
        let g = obs.determinize(rng);
        let l = play_likelihood(obs, g.hands[opp], &inf, buf);
        total += l;
        // Campionamento proporzionale al peso, un candidato alla volta.
        if total > 0.0 && rng.gen::<f64>() * total < l {
            best = g;
        }
    }
    best
}

impl EngineConfig {
    pub fn strength(samples: u32) -> EngineConfig {
        EngineConfig {
            samples,
            rollout_epsilon: 0.0,
            policy: Weights::default(),
            rollout: Rollout::OpenHand,
            match_aware: true,
            solve_endgame: true,
            threads: 1,
            tree: false,
            exploration: 0.7,
            infer: None,
            ctx_policy: false,
            solve_deck: 0,
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig::strength(200)
    }
}

/// Politica dei rollout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rollout {
    /// Guadagno meno la miglior risposta avversaria, vedendo le carte avversarie del mondo simulato.
    OpenHand,
    /// Solo il guadagno immediato.
    Greedy,
    /// Come il Contacarte: risposta avversaria stimata dalle carte non viste da chi muove.
    Counted,
}

/// Valutazione di una mossa candidata.
#[derive(Clone, Copy, Debug)]
pub struct MoveEval {
    pub mv: Move,
    /// Differenza punti attesa nella smazzata (io − avversario).
    pub exp_diff: f64,
    /// Probabilità stimata di vincere la partita.
    pub win_prob: f64,
}

/// Simula fino a fine smazzata e restituisce i punti finali.
fn rollout(
    g: &mut Game,
    cfg: &EngineConfig,
    me: u8,
    moves: &mut Vec<Move>,
    bufs: &mut Vec<Vec<Move>>,
    rng: &mut SmallRng,
) -> [u8; 2] {
    while !g.is_over() {
        if cfg.solve_endgame && g.deck_len <= cfg.solve_deck && g.hands[0].len() + g.hands[1].len() > 1 {
            // Finale esatto: la variante migliore per entrambi (differenza punti).
            return solve_line(g, me, i32::MIN, i32::MAX, bufs).1;
        }
        g.legal_moves(moves);
        let m = if moves.len() == 1 {
            moves[0]
        } else if cfg.rollout_epsilon > 0.0 && rng.gen_bool(cfg.rollout_epsilon) {
            moves[rng.gen_range(0..moves.len())]
        } else {
            match cfg.rollout {
                Rollout::OpenHand if cfg.ctx_policy => open_hand_move(g, &cfg.policy.in_context(g.captured), moves),
                Rollout::OpenHand => open_hand_move(g, &cfg.policy, moves),
                Rollout::Greedy => {
                    let last = g.last_play();
                    *moves.iter().max_by_key(|&&m| cfg.policy.gain(m, g.table, last)).unwrap()
                }
                Rollout::Counted => {
                    let obs = Observation::of(g, g.to_move, [0, 0]);
                    style_move(&obs, &cfg.policy, Risk::Counted, moves)
                }
            }
        };
        g.play(m);
    }
    g.score().points
}

#[derive(Clone, Default)]
struct Sums {
    util: Vec<f64>,
    diff: Vec<f64>,
    win: Vec<f64>,
}

impl Sums {
    fn new(n: usize) -> Sums {
        Sums { util: vec![0.0; n], diff: vec![0.0; n], win: vec![0.0; n] }
    }
    fn merge(&mut self, o: &Sums) {
        for i in 0..self.util.len() {
            self.util[i] += o.util[i];
            self.diff[i] += o.diff[i];
            self.win[i] += o.win[i];
        }
    }
}

fn simulate(obs: &Observation, moves: &[Move], cfg: &EngineConfig, samples: u32, rng: &mut SmallRng) -> Sums {
    let me = obs.me as usize;
    let table = MatchTable::standard();
    let mut sums = Sums::new(moves.len());
    let mut buf = Vec::with_capacity(16);
    let mut bufs = Vec::new();
    for _ in 0..samples {
        // Lo stesso mondo per tutte le mosse: confronto a parità di carte.
        let world = sample_world(obs, cfg, &mut buf, rng);
        for (i, &m) in moves.iter().enumerate() {
            let mut g = world;
            g.play(m);
            let pts = rollout(&mut g, cfg, obs.me, &mut buf, &mut bufs, rng);
            let diff = pts[me] as f64 - pts[1 - me] as f64;
            let win = table.value(obs.match_score[me] + pts[me], obs.match_score[1 - me] + pts[1 - me]);
            sums.diff[i] += diff;
            sums.win[i] += win;
            // Anche giocando per la partita, la differenza punti fa da spareggio.
            sums.util[i] += if cfg.match_aware { win + diff * 1e-3 } else { diff };
        }
    }
    sums
}

/// Valuta tutte le mosse legali dal punto di vista di chi deve muovere, dalla migliore.
pub fn evaluate(obs: &Observation, cfg: &EngineConfig, rng: &mut SmallRng) -> Vec<MoveEval> {
    let mut moves = Vec::new();
    obs.legal_moves(&mut moves);
    if cfg.tree && obs.deck_len > 0 {
        return evaluate_tree(obs, &moves, cfg, rng);
    }
    // A mazzo vuoto la mano avversaria è nota: un mondo solo, risolto esattamente.
    let samples = if obs.deck_len == 0 { 1 } else { cfg.samples.max(1) };
    let threads = (cfg.threads.max(1) as u32).min(samples / 32).max(1);
    let sums = if threads == 1 {
        simulate(obs, &moves, cfg, samples, rng)
    } else {
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
            let mut total = Sums::new(moves.len());
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
        .map(|(i, &m)| (sums.util[i] / n, MoveEval { mv: m, exp_diff: sums.diff[i] / n, win_prob: sums.win[i] / n }))
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
        if self.cfg.tree {
            format!("Motore ad albero ({} iterazioni)", self.cfg.samples)
        } else {
            format!("Motore PIMC ({} mondi)", self.cfg.samples)
        }
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Move {
        let mut moves = Vec::new();
        obs.legal_moves(&mut moves);
        if moves.len() == 1 {
            return moves[0];
        }
        evaluate(obs, &self.cfg, rng)[0].mv
    }
}

// ---------------------------------------------------------------------------
// ISMCTS (Information Set Monte Carlo Tree Search): famiglia di algoritmi diversa dal motore.

struct Node {
    mv: Option<Move>,
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
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Move {
        let mut legal = Vec::new();
        obs.legal_moves(&mut legal);
        if legal.len() == 1 {
            return legal[0];
        }
        let greedy = Weights::default();
        let mut nodes =
            vec![Node { mv: None, mover: 1 - obs.me, children: vec![], visits: 0.0, avail: 0.0, reward: 0.0 }];
        let mut moves = Vec::new();
        for _ in 0..self.iterations {
            let mut g = obs.determinize(rng);
            let mut path = vec![0usize];
            let mut cur = 0usize;
            loop {
                if g.is_over() {
                    break;
                }
                g.legal_moves(&mut moves);
                let kids: Vec<usize> =
                    nodes[cur].children.iter().copied().filter(|&k| moves.contains(&nodes[k].mv.unwrap())).collect();
                for &k in &kids {
                    nodes[k].avail += 1.0;
                }
                let untried: Vec<Move> =
                    moves.iter().copied().filter(|m| !kids.iter().any(|&k| nodes[k].mv == Some(*m))).collect();
                if !untried.is_empty() {
                    let m = untried[rng.gen_range(0..untried.len())];
                    let mover = g.to_move;
                    g.play(m);
                    nodes.push(Node { mv: Some(m), mover, children: vec![], visits: 0.0, avail: 1.0, reward: 0.0 });
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
                g.play(nodes[best].mv.unwrap());
                path.push(best);
                cur = best;
            }
            // Rollout goloso con un po' di casualità.
            while !g.is_over() {
                g.legal_moves(&mut moves);
                let m = if rng.gen_bool(0.1) {
                    moves[rng.gen_range(0..moves.len())]
                } else {
                    let last = g.last_play();
                    *moves.iter().max_by_key(|&&m| greedy.gain(m, g.table, last)).unwrap()
                };
                g.play(m);
            }
            let pts = g.score().points;
            for &id in &path {
                let n = &mut nodes[id];
                let d = pts[n.mover as usize] as f64 - pts[1 - n.mover as usize] as f64;
                n.visits += 1.0;
                n.reward += (0.5 + d / 12.0).clamp(0.0, 1.0);
            }
        }
        let root = &nodes[0];
        root.children
            .iter()
            .max_by(|&&a, &&b| nodes[a].visits.partial_cmp(&nodes[b].visits).unwrap())
            .and_then(|&k| nodes[k].mv)
            .unwrap()
    }
}

// ---------------------------------------------------------------------------
// Motore ad albero: ISMCTS potenziato con la politica dei rollout del motore, il finale
// esatto a mazzo vuoto e il valore della partita come ricompensa.

/// Statistiche di una mossa alla radice.
#[derive(Clone, Copy, Default)]
struct RootStat {
    visits: f64,
    win: f64,
    diff: f64,
}

fn tree_search(
    obs: &Observation,
    cfg: &EngineConfig,
    legal: &[Move],
    iterations: u32,
    rng: &mut SmallRng,
) -> Vec<RootStat> {
    let table = MatchTable::standard();
    let reward = |pts: [u8; 2], p: u8| -> (f64, f64) {
        let (p, o) = (p as usize, 1 - p as usize);
        let d = pts[p] as f64 - pts[o] as f64;
        let w = table.value(obs.match_score[p] + pts[p], obs.match_score[o] + pts[o]);
        let r = if cfg.match_aware { w + d * 1e-3 } else { (0.5 + d / 12.0).clamp(0.0, 1.0) };
        (r, w)
    };
    let mut nodes = vec![Node { mv: None, mover: 1 - obs.me, children: vec![], visits: 0.0, avail: 0.0, reward: 0.0 }];
    // I figli della radice sono le mosse legali, in quest'ordine.
    for &m in legal {
        nodes.push(Node { mv: Some(m), mover: obs.me, children: vec![], visits: 0.0, avail: 0.0, reward: 0.0 });
        let id = nodes.len() - 1;
        nodes[0].children.push(id);
    }
    let mut root = vec![RootStat::default(); legal.len()];
    let mut moves = Vec::new();
    let mut bufs = Vec::new();
    for _ in 0..iterations {
        let mut g = sample_world(obs, cfg, &mut moves, rng);
        let mut path = vec![0usize];
        let mut cur = 0usize;
        loop {
            if g.is_over() || (cfg.solve_endgame && g.deck_len == 0 && cur != 0) {
                break;
            }
            g.legal_moves(&mut moves);
            let kids: Vec<usize> =
                nodes[cur].children.iter().copied().filter(|&k| moves.contains(&nodes[k].mv.unwrap())).collect();
            for &k in &kids {
                nodes[k].avail += 1.0;
            }
            if let Some(&k) = kids.iter().find(|&&k| nodes[k].visits == 0.0) {
                g.play(nodes[k].mv.unwrap());
                path.push(k);
                break;
            }
            let untried: Vec<Move> =
                moves.iter().copied().filter(|m| !kids.iter().any(|&k| nodes[k].mv == Some(*m))).collect();
            if !untried.is_empty() {
                let m = untried[rng.gen_range(0..untried.len())];
                let mover = g.to_move;
                g.play(m);
                nodes.push(Node { mv: Some(m), mover, children: vec![], visits: 0.0, avail: 1.0, reward: 0.0 });
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
                        n.reward / n.visits + cfg.exploration * (n.avail.ln() / n.visits).sqrt()
                    };
                    ucb(a).partial_cmp(&ucb(b)).unwrap()
                })
                .unwrap();
            g.play(nodes[best].mv.unwrap());
            path.push(best);
            cur = best;
        }
        let pts = rollout(&mut g, cfg, obs.me, &mut moves, &mut bufs, rng);
        for &id in &path {
            let n = &mut nodes[id];
            n.visits += 1.0;
            n.reward += reward(pts, n.mover).0;
        }
        let (_, w) = reward(pts, obs.me);
        let i = path[1] - 1;
        root[i].visits += 1.0;
        root[i].win += w;
        root[i].diff += pts[obs.me as usize] as f64 - pts[1 - obs.me as usize] as f64;
    }
    root
}

fn evaluate_tree(obs: &Observation, legal: &[Move], cfg: &EngineConfig, rng: &mut SmallRng) -> Vec<MoveEval> {
    let threads = (cfg.threads.max(1) as u32).min(cfg.samples / 200).max(1);
    let stats: Vec<RootStat> = if threads == 1 {
        tree_search(obs, cfg, legal, cfg.samples, rng)
    } else {
        // Un albero per thread; alla fine si sommano le visite delle mosse alla radice.
        let seeds: Vec<u64> = (0..threads).map(|_| rng.gen()).collect();
        std::thread::scope(|s| {
            let hs: Vec<_> = seeds
                .iter()
                .map(|&seed| {
                    s.spawn(move || {
                        tree_search(obs, cfg, legal, cfg.samples / threads, &mut SmallRng::seed_from_u64(seed))
                    })
                })
                .collect();
            let mut tot = vec![RootStat::default(); legal.len()];
            for h in hs {
                for (t, r) in tot.iter_mut().zip(h.join().expect("thread del motore")) {
                    t.visits += r.visits;
                    t.win += r.win;
                    t.diff += r.diff;
                }
            }
            tot
        })
    };
    let mut out: Vec<(f64, MoveEval)> = legal
        .iter()
        .zip(&stats)
        .map(|(&m, r)| {
            let n = r.visits.max(1.0);
            (r.visits, MoveEval { mv: m, exp_diff: r.diff / n, win_prob: r.win / n })
        })
        .collect();
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    out.into_iter().map(|(_, e)| e).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(cs: &[Card]) -> CardSet {
        cs.iter().copied().collect()
    }

    /// Minimax senza tagli, come riferimento.
    fn minimax(g: &Game, me: u8) -> i32 {
        if g.is_over() {
            let p = g.score().points;
            return p[me as usize] as i32 - p[1 - me as usize] as i32;
        }
        let mut moves = Vec::new();
        g.legal_moves(&mut moves);
        let vals = moves.iter().map(|&m| {
            let mut n = *g;
            n.play(m);
            minimax(&n, me)
        });
        if g.to_move == me {
            vals.max().unwrap()
        } else {
            vals.min().unwrap()
        }
    }

    #[test]
    fn solver_matches_minimax() {
        let mut rng = SmallRng::seed_from_u64(11);
        let mut moves = Vec::new();
        for _ in 0..300 {
            let mut g = Game::new(&mut rng, 0);
            for _ in 0..30 {
                g.legal_moves(&mut moves);
                g.play(moves[rng.gen_range(0..moves.len())]);
            }
            let me = rng.gen_range(0..2);
            assert_eq!(solve(&g, me, i32::MIN, i32::MAX), minimax(&g, me));
        }
    }

    #[test]
    fn takes_the_scopa() {
        // In tavola 3 + 4, in mano un 7 e un Re: il motore fa scopa col 7.
        let mut rng = SmallRng::seed_from_u64(3);
        let mut g = Game::new(&mut rng, 1);
        let hand = set(&[make(1, 6), make(2, 9), make(3, 1)]);
        let table = set(&[make(1, 2), make(2, 3)]);
        let rest: Vec<Card> = CardSet::FULL.minus(hand).minus(table).to_vec();
        g.hands = [hand, rest[..3].iter().copied().collect()];
        g.table = table;
        g.deck_len = 18;
        g.deck[..18].copy_from_slice(&rest[3..21]);
        g.captured = [CardSet::EMPTY, rest[21..].iter().copied().collect()];
        g.to_move = 0;
        let obs = Observation::of(&g, 0, [0, 0]);
        let evals = evaluate(&obs, &EngineConfig::strength(100), &mut rng);
        assert_eq!(evals[0].mv, Move { card: make(1, 6), take: table });
        let mut moves = Vec::new();
        obs.legal_moves(&mut moves);
        assert_eq!(style_move(&obs, &Weights::default(), Risk::Counted, &moves).card, make(1, 6));
    }

    #[test]
    fn inference_doubts_a_missed_scopa() {
        // In tavola un 3 e un 4: l'avversario ha lasciato in tavola un Re invece di fare scopa.
        // Una mano con un 7 (scopa lasciata) è molto meno probabile di una senza.
        let table = set(&[make(1, 2), make(2, 3)]);
        let king = make(3, 9);
        let mut obs = Observation::new_hand(0, 0, [0, 0]);
        obs.hand = set(&[make(0, 0), make(0, 1), make(0, 4)]);
        obs.table = table;
        obs.to_move = 1;
        obs.apply(Move { card: king, take: CardSet::EMPTY });
        assert_eq!(obs.opp_plays().len(), 1);
        let inf = Infer::default();
        let mut buf = Vec::new();
        let with_seven = play_likelihood(&obs, set(&[make(1, 6), make(2, 8)]), &inf, &mut buf);
        let without = play_likelihood(&obs, set(&[make(1, 1), make(2, 8)]), &inf, &mut buf);
        assert!(with_seven * 5.0 < without, "{with_seven} contro {without}");
    }

    #[test]
    fn match_table_is_sane() {
        let t = MatchTable::standard();
        assert!((t.value(0, 0) - 0.5).abs() < 0.05);
        assert!(t.value(10, 0) > 0.9);
        assert!(t.value(0, 10) < 0.1);
        assert_eq!(t.value(11, 3), 1.0);
        assert!(t.value(5, 3) > t.value(3, 5));
    }

    #[test]
    fn threaded_evaluation_is_complete() {
        let mut rng = SmallRng::seed_from_u64(5);
        let g = Game::new(&mut rng, 0);
        let obs = Observation::of(&g, 1, [0, 0]);
        let mut cfg = EngineConfig::strength(200);
        cfg.threads = 4;
        let evals = evaluate(&obs, &cfg, &mut rng);
        let mut moves = Vec::new();
        obs.legal_moves(&mut moves);
        assert_eq!(evals.len(), moves.len());
        assert!(evals.iter().all(|e| (0.0..=1.0).contains(&e.win_prob)));
    }
}
