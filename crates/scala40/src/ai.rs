//! Giocatori artificiali: il motore e gli stili di avversario per i test.
//!
//! Tutti i giocatori usano lo stesso schema di turno ([`plan`]): pescata, apertura o calata,
//! sostituzione dei jolly, attacchi, scarto. Cambiano le scelte: quando prendere dal pozzo,
//! quando calare, quale carta scartare.

use crate::cards::{self, is_joker, kind, Card, JOKER, NUM_KINDS};
use crate::game::{Action, Deal, OPEN_MIN};
use crate::meld::Meld;
use crate::solver::{self, Best, Counts, Group, Scorer, Shed};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::Rng;

/// Il resto del turno dopo la pescata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub actions: Vec<Action>,
    pub discard: Card,
}

pub trait Player: Send {
    fn name(&self) -> String;
    /// Pescata: `true` per prendere la carta in cima al pozzo (va usata subito).
    fn take_discard(&mut self, d: &Deal, me: u8, rng: &mut SmallRng) -> bool;
    /// Dopo la pescata: combinazioni, attacchi e scarto.
    fn play(&mut self, d: &Deal, me: u8, rng: &mut SmallRng) -> Turn;
}

/// La carta in cima al pozzo si può usare subito?
pub fn discard_usable(d: &Deal, me: u8) -> bool {
    if d.top_discard().is_none() || d.to_move != me {
        return false;
    }
    let mut g = d.clone();
    g.take_discard().unwrap();
    let mut p = Planner { g, me, actions: Vec::new() };
    p.use_taken();
    p.taken_in_hand().is_none()
}

/// Come calare: quali combinazioni tenere in mano e come scomporre la mano.
pub trait LayPolicy {
    /// Scomposizione da usare per scegliere le combinazioni da calare dopo l'apertura.
    fn decompose(&self, d: &Deal, me: u8, n: &Counts) -> Best;
    /// Calare adesso? (`closing`: calando tutto si chiude in questo turno.)
    fn lay_now(&self, _d: &Deal, _me: u8, _closing: bool) -> bool {
        true
    }
}

/// Toglie una carta dalle combinazioni da calare se calandole tutte la mano resterebbe vuota.
fn keep_one(mut melds: Vec<Vec<Card>>, hand_len: usize, opened: bool) -> Vec<Vec<Card>> {
    let total: usize = melds.iter().map(Vec::len).sum();
    if total < hand_len || melds.is_empty() {
        return melds;
    }
    // Da una combinazione di 4 o più carte si toglie un'estremità (il valore cala di poco).
    let mut best: Option<(usize, Vec<Card>, u32)> = None;
    for (i, m) in melds.iter().enumerate() {
        if m.len() < 4 {
            continue;
        }
        let meld = Meld::new(m, 0).unwrap();
        for cut in [0, meld.cards.len() - 1] {
            let mut rest = meld.cards.clone();
            rest.remove(cut);
            if let Some(r) = Meld::new(&rest, 0) {
                let lost = meld.value() - r.value();
                if best.as_ref().is_none_or(|b| lost < b.2) {
                    best = Some((i, rest, lost));
                }
            }
        }
    }
    match best {
        Some((i, rest, _)) => melds[i] = rest,
        None => {
            // Si tiene in mano la combinazione meno preziosa.
            let i = (0..melds.len()).min_by_key(|&i| Meld::new(&melds[i], 0).unwrap().value()).unwrap();
            melds.remove(i);
        }
    }
    let ok = opened || melds.iter().map(|m| Meld::new(m, 0).unwrap().value()).sum::<u32>() >= OPEN_MIN;
    if ok { melds } else { Vec::new() }
}

/// Esegue le azioni su una copia della smazzata e le registra.
struct Planner {
    g: Deal,
    me: u8,
    actions: Vec<Action>,
}

impl Planner {
    fn hand(&self) -> &Vec<Card> {
        &self.g.hands[self.me as usize]
    }
    fn opened(&self) -> bool {
        self.g.opened[self.me as usize]
    }
    fn act(&mut self, a: Action) -> bool {
        if self.g.apply(&a).is_ok() {
            self.actions.push(a);
            true
        } else {
            false
        }
    }
    fn taken_in_hand(&self) -> Option<Card> {
        self.g.taken.filter(|t| self.hand().contains(t))
    }

    fn try_open(&mut self, lay: &dyn LayPolicy) -> bool {
        let hand = self.hand().clone();
        let (v, melds) = solver::best_opening(&hand, self.taken_in_hand());
        if v < OPEN_MIN {
            return false;
        }
        let closing = melds.iter().map(Vec::len).sum::<usize>() + 1 >= hand.len();
        if self.taken_in_hand().is_none() && !lay.lay_now(&self.g, self.me, closing) {
            return false;
        }
        let melds = keep_one(melds, hand.len(), false);
        if let Some(t) = self.taken_in_hand() {
            if !melds.iter().any(|m| m.contains(&t)) {
                return false;
            }
        }
        !melds.is_empty() && self.act(Action::Lay(melds))
    }

    fn lay_more(&mut self, lay: &dyn LayPolicy) -> bool {
        let hand = self.hand().clone();
        let n = cards::counts(&hand);
        let b = lay.decompose(&self.g, self.me, &n);
        let melds = solver::melds_as_cards(&hand, &b.groups);
        if melds.is_empty() {
            return false;
        }
        let closing = melds.iter().map(Vec::len).sum::<usize>() + 1 >= hand.len();
        if !lay.lay_now(&self.g, self.me, closing) {
            return false;
        }
        let melds = keep_one(melds, hand.len(), true);
        !melds.is_empty() && self.act(Action::Lay(melds))
    }

    /// La carta presa dal pozzo va usata: attacco o combinazione che la contiene.
    fn use_taken(&mut self) -> bool {
        let Some(t) = self.taken_in_hand() else { return false };
        if !self.opened() {
            return self.try_open(&AlwaysLay);
        }
        for i in 0..self.g.melds.len() {
            if self.g.melds[i].can_swap(t) && self.act(Action::Swap(t, i)) {
                return true;
            }
            if self.g.melds[i].can_attach(t) && self.act(Action::Attach(t, i)) {
                return true;
            }
        }
        let hand = self.hand().clone();
        let mut options = solver::melds_with(&cards::counts(&hand), kind(t));
        options.sort_by_key(|g| std::cmp::Reverse(g.len()));
        for g in options {
            let mut pool: Vec<Card> = hand.iter().copied().filter(|&c| c != t).collect();
            let mut meld = vec![t];
            let mut kinds = g.kinds();
            let pos = kinds.iter().position(|&k| k == kind(t)).unwrap();
            kinds.remove(pos);
            for k in kinds {
                let i = pool.iter().position(|&c| kind(c) == k).unwrap();
                meld.push(pool.swap_remove(i));
            }
            if self.act(Action::Lay(vec![meld])) {
                return true;
            }
        }
        false
    }

    fn swaps(&mut self) -> bool {
        if !self.opened() {
            return false;
        }
        let hand = self.hand().clone();
        for c in hand.into_iter().filter(|&c| !is_joker(c)) {
            for i in 0..self.g.melds.len() {
                if self.g.melds[i].can_swap(c) && self.act(Action::Swap(c, i)) {
                    return true;
                }
            }
        }
        false
    }

    fn attaches(&mut self) -> bool {
        if !self.opened() {
            return false;
        }
        let hand = self.hand().clone();
        for c in hand {
            // Il jolly si attacca solo se così si chiude.
            if is_joker(c) && self.hand().len() != 2 {
                continue;
            }
            if self.hand().len() <= 1 {
                return false;
            }
            for i in 0..self.g.melds.len() {
                if self.g.melds[i].can_attach(c) && self.act(Action::Attach(c, i)) {
                    return true;
                }
            }
        }
        false
    }
}

/// Cala sempre appena può.
struct AlwaysLay;

impl LayPolicy for AlwaysLay {
    fn decompose(&self, _d: &Deal, _me: u8, n: &Counts) -> Best {
        solver::best(n, &Shed)
    }
}

/// Esegue il turno dopo la pescata: calate, jolly, attacchi; lo scarto lo sceglie `discard`
/// guardando la smazzata dopo le azioni.
pub fn plan(d: &Deal, me: u8, lay: &dyn LayPolicy, discard: impl FnOnce(&Deal, &[Card]) -> Card) -> Turn {
    let mut p = Planner { g: d.clone(), me, actions: Vec::new() };
    p.use_taken();
    for _ in 0..30 {
        let changed = if p.opened() { p.lay_more(lay) || p.swaps() || p.attaches() } else { p.try_open(lay) };
        if !changed {
            break;
        }
    }
    let hand = p.hand().clone();
    let c = if hand.len() == 1 { hand[0] } else { discard(&p.g, &hand) };
    Turn { actions: p.actions, discard: c }
}

/// La smazzata dopo il turno (per valutare le alternative).
pub fn after(d: &Deal, t: &Turn) -> Deal {
    let mut g = d.clone();
    for a in &t.actions {
        g.apply(a).unwrap();
    }
    g
}

// ---------------------------------------------------------------------------------------
// Avversari semplici
// ---------------------------------------------------------------------------------------

/// Carte spaiate (fuori da ogni combinazione) secondo la scomposizione che cala di più.
fn loose_cards(hand: &[Card]) -> Vec<Card> {
    let b = solver::best(&cards::counts(hand), &Shed);
    let mut pool = hand.to_vec();
    for m in solver::melds_as_cards(hand, &b.groups) {
        for c in m {
            let i = pool.iter().position(|&x| x == c).unwrap();
            pool.swap_remove(i);
        }
    }
    pool
}

/// Scarta a caso (mai un jolly, se può), non prende dal pozzo.
pub struct RandomPlayer;

impl Player for RandomPlayer {
    fn name(&self) -> String {
        "Carta a caso".into()
    }
    fn take_discard(&mut self, _: &Deal, _: u8, _: &mut SmallRng) -> bool {
        false
    }
    fn play(&mut self, d: &Deal, me: u8, rng: &mut SmallRng) -> Turn {
        plan(d, me, &AlwaysLay, |_, hand| {
            let nat: Vec<Card> = hand.iter().copied().filter(|&c| !is_joker(c)).collect();
            *nat.choose(rng).unwrap_or(&hand[0])
        })
    }
}

/// Giocatore di regole semplici: prende dal pozzo se può usarlo, cala appena può,
/// scarta la carta spaiata che vale più punti.
pub struct BasicPlayer;

pub fn highest_loose(hand: &[Card]) -> Card {
    let loose = loose_cards(hand);
    let pool = if loose.iter().any(|&c| !is_joker(c)) { loose } else { hand.to_vec() };
    *pool.iter().filter(|&&c| !is_joker(c)).max_by_key(|&&c| (cards::penalty(c), c)).unwrap_or(&pool[0])
}

impl Player for BasicPlayer {
    fn name(&self) -> String {
        "Regole semplici".into()
    }
    fn take_discard(&mut self, d: &Deal, me: u8, _: &mut SmallRng) -> bool {
        discard_usable(d, me)
    }
    fn play(&mut self, d: &Deal, me: u8, _: &mut SmallRng) -> Turn {
        plan(d, me, &AlwaysLay, |_, hand| highest_loose(hand))
    }
}

/// Punteggio "da giocatore di club": combinazioni, poi coppie, poi carte spaiate.
struct PairScore;

impl Scorer for PairScore {
    const PARTIALS: bool = true;
    fn group(&self, g: &Group) -> f32 {
        match g {
            Group::Set { .. } | Group::Run { .. } => 3.0 * g.len() as f32,
            Group::PairSet { .. } | Group::PairRun { .. } => 2.0,
            Group::Single(_) => 0.0,
        }
    }
    fn loose_joker(&self) -> f32 {
        3.0
    }
}

/// Carte da cui scegliere lo scarto: prima le spaiate, poi quelle delle coppie (mai i jolly).
fn discard_pool(hand: &[Card]) -> (Vec<Card>, Vec<Card>) {
    let b = solver::best(&cards::counts(hand), &PairScore);
    let mut singles = Vec::new();
    let mut pairs = Vec::new();
    let mut pool = hand.to_vec();
    for g in &b.groups {
        let cs = g.take_cards(&mut pool);
        match g {
            Group::Single(_) => singles.extend(cs),
            Group::PairSet { .. } | Group::PairRun { .. } => pairs.extend(cs),
            _ => {}
        }
    }
    (singles, pairs)
}

/// Tiene le coppie: scarta la spaiata più alta, poi la carta più alta delle coppie.
pub struct PairKeeper;

pub fn pair_keeper_discard(hand: &[Card], avoid: impl Fn(Card) -> bool) -> Card {
    let (singles, pairs) = discard_pool(hand);
    for pool in [&singles, &pairs] {
        let ok: Vec<Card> = pool.iter().copied().filter(|&c| !avoid(c)).collect();
        if let Some(&c) = ok.iter().max_by_key(|&&c| (cards::penalty(c), c)) {
            return c;
        }
    }
    for pool in [&singles, &pairs] {
        if let Some(&c) = pool.iter().max_by_key(|&&c| (cards::penalty(c), c)) {
            return c;
        }
    }
    highest_loose(hand)
}

impl Player for PairKeeper {
    fn name(&self) -> String {
        "Tiene le coppie".into()
    }
    fn take_discard(&mut self, d: &Deal, me: u8, _: &mut SmallRng) -> bool {
        discard_usable(d, me)
    }
    fn play(&mut self, d: &Deal, me: u8, _: &mut SmallRng) -> Turn {
        plan(d, me, &AlwaysLay, |_, hand| pair_keeper_discard(hand, |_| false))
    }
}

/// Non scarta carte che l'avversario può attaccare o usare con quelle che ha preso,
/// e preferisce scartare valori e semi che l'avversario ha già scartato.
pub struct Defensive;

impl Player for Defensive {
    fn name(&self) -> String {
        "Difensivo".into()
    }
    fn take_discard(&mut self, d: &Deal, me: u8, _: &mut SmallRng) -> bool {
        discard_usable(d, me)
    }
    fn play(&mut self, d: &Deal, me: u8, _: &mut SmallRng) -> Turn {
        let opp = 1 - me;
        plan(d, me, &AlwaysLay, |g, hand| {
            // Valori scartati dall'avversario: probabilmente non gli servono.
            let mut safe_rank = [false; 14];
            for e in &g.log {
                if let crate::game::Event::Discard(p, c) = *e {
                    if p == opp && !is_joker(c) {
                        safe_rank[cards::rank(c) as usize] = true;
                    }
                }
            }
            let risky = |c: Card| g.opened[opp as usize] && g.melds.iter().any(|m| m.can_attach(c) || m.can_swap(c));
            let (singles, pairs) = discard_pool(hand);
            for pool in [&singles, &pairs] {
                let ok: Vec<Card> = pool.iter().copied().filter(|&c| !is_joker(c) && !risky(c)).collect();
                if let Some(&c) = ok.iter().max_by_key(|&&c| (safe_rank[cards::rank(c) as usize], cards::penalty(c), c)) {
                    return c;
                }
            }
            pair_keeper_discard(hand, |_| false)
        })
    }
}

/// Tiene le combinazioni in mano e cala tutto insieme per chiudere ("chiusura in mano"),
/// a meno che l'avversario non stia per chiudere.
pub struct Hoarder {
    /// Cala comunque quando l'avversario ha al più queste carte.
    pub panic_at: usize,
}

struct HoardLay(usize);

impl LayPolicy for HoardLay {
    fn decompose(&self, _d: &Deal, _me: u8, n: &Counts) -> Best {
        solver::best(n, &Shed)
    }
    fn lay_now(&self, d: &Deal, me: u8, closing: bool) -> bool {
        closing || d.hands[1 - me as usize].len() <= self.0 || d.stock.len() < 10
    }
}

impl Player for Hoarder {
    fn name(&self) -> String {
        format!("Chiude in mano (cala prima solo se l'avversario ha {} carte o meno)", self.panic_at)
    }
    fn take_discard(&mut self, d: &Deal, me: u8, _: &mut SmallRng) -> bool {
        discard_usable(d, me)
    }
    fn play(&mut self, d: &Deal, me: u8, _: &mut SmallRng) -> Turn {
        plan(d, me, &HoardLay(self.panic_at), |_, hand| {
            // Le carte già in combinazione non si scartano.
            let b = solver::best(&cards::counts(hand), &Shed);
            let melded: Vec<Card> = solver::melds_as_cards(hand, &b.groups).concat();
            let mut rest = hand.to_vec();
            for c in melded {
                let i = rest.iter().position(|&x| x == c).unwrap();
                rest.swap_remove(i);
            }
            if rest.is_empty() {
                return pair_keeper_discard(hand, |_| false);
            }
            pair_keeper_discard(&rest, |_| false)
        })
    }
}

/// Avversario di un altro giocatore che fa una mossa a caso con probabilità `eps`
/// (pescata e scarto).
pub struct Noisy {
    pub eps: f64,
    pub inner: Box<dyn Player>,
}

impl Player for Noisy {
    fn name(&self) -> String {
        format!("{} che sbaglia il {:.0}% delle mosse", self.inner.name(), self.eps * 100.0)
    }
    fn take_discard(&mut self, d: &Deal, me: u8, rng: &mut SmallRng) -> bool {
        if rng.gen_bool(self.eps) {
            return rng.gen_bool(0.5) && discard_usable(d, me);
        }
        self.inner.take_discard(d, me, rng)
    }
    fn play(&mut self, d: &Deal, me: u8, rng: &mut SmallRng) -> Turn {
        let mut t = self.inner.play(d, me, rng);
        if rng.gen_bool(self.eps) {
            let g = after(d, &t);
            let hand = &g.hands[me as usize];
            if hand.len() > 1 {
                let nat: Vec<Card> = hand.iter().copied().filter(|&c| !is_joker(c) && Some(c) != g.taken).collect();
                if let Some(&c) = nat.choose(rng) {
                    t.discard = c;
                }
            }
        }
        t
    }
}

// ---------------------------------------------------------------------------------------
// Motore
// ---------------------------------------------------------------------------------------

/// Pesi della valutazione della mano. Una mano vale meno quanto più "costa": ogni carta
/// spaiata, ogni coppia da completare, i punti che si pagherebbero se l'avversario chiudesse.
#[derive(Clone, Copy, Debug)]
pub struct Weights {
    pub single: f32,
    pub single_near: f32,
    pub pair: f32,
    pub outs: f32,
    pub outs_cap: f32,
    pub joker: f32,
    pub points: f32,
    pub threat: f32,
    pub unopened_meld: f32,
    pub open_value: f32,
    pub attachable: f32,
    pub danger_attach: f32,
    pub danger_partner: f32,
    pub take_margin: f32,
    /// Pescate dal tallone simulate per decidere se prendere dal pozzo (0: si prende sempre).
    pub take_samples: u32,
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            single: 10.0,
            single_near: 0.6,
            pair: 10.0,
            outs: 0.45,
            outs_cap: 6.0,
            joker: 1.0,
            points: 0.2,
            threat: 0.8,
            unopened_meld: 1.4,
            open_value: 0.11,
            attachable: 3.0,
            danger_attach: 12.0,
            danger_partner: 0.0,
            take_margin: 0.0,
            take_samples: 0,
        }
    }
}

/// Contesto della valutazione: cosa sa il giocatore in questo momento.
pub struct Eval {
    w: Weights,
    unseen: Counts,
    opened: bool,
    /// Moltiplicatore del peso dei punti in mano (cresce quando l'avversario sta per chiudere).
    threat: f32,
    /// Tipi di carta che si attaccano a combinazioni in tavola.
    attachable: [bool; NUM_KINDS],
}

fn pos_kind(suit: u8, p: u8) -> Option<usize> {
    match p {
        1..=13 => Some(cards::kind_of(suit, p) as usize),
        14 => Some(cards::kind_of(suit, 1) as usize),
        _ => None,
    }
}

impl Eval {
    pub fn new(d: &Deal, me: u8, w: Weights) -> Eval {
        let opp = 1 - me as usize;
        let opp_len = d.hands[opp].len() as f32;
        let threat = 1.0 + w.threat * ((10.0 - opp_len).max(0.0)) * if d.opened[opp] { 1.0 } else { 0.4 };
        let mut attachable = [false; NUM_KINDS];
        for k in 0..52u8 {
            attachable[k as usize] = d.melds.iter().any(|m| m.can_attach(k) || m.can_swap(k));
        }
        Eval { w, unseen: d.unseen(me), opened: d.opened[me as usize], threat, attachable }
    }

    fn u(&self, k: Option<usize>) -> f32 {
        k.map_or(0.0, |k| self.unseen[k] as f32)
    }

    fn points_cost(&self, kinds: &[u8]) -> f32 {
        if self.opened {
            self.w.points * self.threat * kinds.iter().map(|&k| cards::kind_penalty(k) as f32).sum::<f32>()
        } else {
            0.0
        }
    }

    fn outs(&self, o: f32) -> f32 {
        self.w.outs * o.min(self.w.outs_cap)
    }

    /// Valore della mano (più alto è meglio).
    pub fn hand_value(&self, hand: &[Card]) -> f32 {
        solver::best(&cards::counts(hand), self).score
    }

    /// Quanto è pericoloso scartare `c`: l'avversario la può prendere e usare subito.
    pub fn danger(&self, d: &Deal, me: u8, c: Card) -> f32 {
        let opp = 1 - me as usize;
        let k = kind(c);
        if k == JOKER {
            return 100.0;
        }
        let mut x = 0.0;
        if d.opened[opp] && self.attachable[k as usize] {
            x += self.w.danger_attach;
        }
        // Carte che con questa formano una combinazione e potrebbero essere in mano all'avversario.
        let (s, r) = (cards::suit(c), cards::rank(c));
        let mut partners = 0.0;
        for t in 0..4 {
            if t != s {
                partners += self.unseen[cards::kind_of(t, r) as usize] as f32;
            }
        }
        let p = r as i32;
        for dp in [-2, -1, 1, 2] {
            partners += self.u(pos_kind(s, (p + dp).clamp(0, 15) as u8));
        }
        let total: f32 = self.unseen.iter().map(|&n| n as f32).sum::<f32>().max(1.0);
        let in_hand = d.hands[opp].len() as f32 / total;
        x + self.w.danger_partner * partners * in_hand * if d.opened[opp] { 1.0 } else { 0.5 }
    }
}

impl Scorer for Eval {
    const PARTIALS: bool = true;
    fn group(&self, g: &Group) -> f32 {
        let w = &self.w;
        match *g {
            Group::Set { .. } | Group::Run { .. } => {
                if self.opened {
                    0.0
                } else {
                    -w.unopened_meld * g.len() as f32 + w.open_value * g.open_value() as f32
                }
            }
            Group::PairSet { rank, suits } => {
                let mut o = self.unseen[JOKER as usize] as f32;
                for s in 0..4 {
                    if suits & (1 << s) == 0 {
                        o += self.unseen[cards::kind_of(s, rank) as usize] as f32;
                    }
                }
                let v = if self.opened { 0.0 } else { w.open_value * 0.5 * 3.0 * cards::set_value(rank) as f32 };
                -w.pair + self.outs(o) + v - self.points_cost(&g.kinds())
            }
            Group::PairRun { suit, a, b } => {
                let mut o = self.unseen[JOKER as usize] as f32;
                if b - a == 1 {
                    o += self.u(pos_kind(suit, a.wrapping_sub(1))) + self.u(pos_kind(suit, b + 1));
                } else {
                    o += self.u(pos_kind(suit, a + 1));
                }
                let v = if self.opened {
                    0.0
                } else {
                    w.open_value * 0.5 * (a..=b).map(cards::pos_value).sum::<u32>() as f32 * 1.5
                };
                -w.pair + self.outs(o) + v - self.points_cost(&g.kinds())
            }
            Group::Single(k) => {
                if !self.opened && self.attachable[k as usize] {
                    return -w.attachable;
                }
                let (s, r) = (k / 13, k % 13 + 1);
                let mut near = 0.0;
                for t in 0..4 {
                    if t != s {
                        near += self.unseen[cards::kind_of(t, r) as usize] as f32;
                    }
                }
                for dp in [-2i32, -1, 1, 2] {
                    near += self.u(pos_kind(s, (r as i32 + dp).clamp(0, 15) as u8));
                }
                if r == 1 {
                    near += self.u(pos_kind(s, 13)) + self.u(pos_kind(s, 12));
                }
                -w.single + w.single_near * near.min(8.0) - self.points_cost(&[k])
            }
        }
    }
    fn loose_joker(&self) -> f32 {
        -self.w.joker - self.points_cost(&[JOKER])
    }
}

/// Scomposizione con la valutazione del motore (per decidere cosa calare).
struct EngineLay(Weights);

impl LayPolicy for EngineLay {
    fn decompose(&self, d: &Deal, me: u8, n: &Counts) -> Best {
        solver::best(n, &Eval::new(d, me, self.0))
    }
}

/// Il motore: valuta ogni scarto con la scomposizione della mano che resta (carte spaiate,
/// coppie e carte ancora in circolazione che le completano, punti a rischio) meno il rischio
/// di dare all'avversario una carta che usa subito. Per il pozzo confronta la presa con
/// la media di qualche pescata possibile dal tallone.
pub struct EnginePlayer {
    pub w: Weights,
}

impl EnginePlayer {
    pub fn new(w: Weights) -> Self {
        EnginePlayer { w }
    }

    /// Ogni scarto possibile (un tipo di carta per volta) con il valore della mano che resta,
    /// dal migliore al peggiore.
    pub fn discard_ranking(&self, g: &Deal, me: u8, hand: &[Card]) -> Vec<(Card, f32)> {
        let ev = Eval::new(g, me, self.w);
        let mut out: Vec<(Card, f32)> = Vec::new();
        let mut seen = [false; NUM_KINDS];
        for (i, &c) in hand.iter().enumerate() {
            if Some(c) == g.taken || seen[kind(c) as usize] {
                continue;
            }
            seen[kind(c) as usize] = true;
            let mut rest = hand.to_vec();
            rest.remove(i);
            out.push((c, ev.hand_value(&rest) - ev.danger(g, me, c)));
        }
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        out
    }

    /// Scarto migliore e valore della mano che resta.
    fn best_discard(&self, g: &Deal, me: u8, hand: &[Card]) -> (Card, f32) {
        self.discard_ranking(g, me, hand).first().copied().unwrap_or((hand[0], 0.0))
    }

    /// Consiglio per il turno dopo la pescata: azioni, scarto e classifica degli scarti.
    pub fn advise(&self, d: &Deal, me: u8) -> (Turn, Vec<(Card, f32)>) {
        let t = plan(d, me, &EngineLay(self.w), |g, hand| self.best_discard(g, me, hand).0);
        let g = after(d, &t);
        let hand = &g.hands[me as usize];
        let ranking = if hand.len() > 1 { self.discard_ranking(&g, me, hand) } else { vec![(t.discard, 0.0)] };
        (t, ranking)
    }

    /// Valore della posizione dopo un turno giocato con la mano `d.hands[me]` già pescata.
    fn turn_value(&self, d: &Deal, me: u8) -> f32 {
        let t = plan(d, me, &EngineLay(self.w), |g, hand| self.best_discard(g, me, hand).0);
        let mut g = after(d, &t);
        let hand = g.hands[me as usize].clone();
        if hand.len() == 1 {
            return 1000.0;
        }
        let i = hand.iter().position(|&c| c == t.discard).unwrap();
        g.hands[me as usize].remove(i);
        let ev = Eval::new(&g, me, self.w);
        ev.hand_value(&g.hands[me as usize]) - ev.danger(&g, me, t.discard)
    }
}

impl Player for EnginePlayer {
    fn name(&self) -> String {
        "Motore".into()
    }

    fn take_discard(&mut self, d: &Deal, me: u8, rng: &mut SmallRng) -> bool {
        if !discard_usable(d, me) {
            return false;
        }
        // Senza campioni: si prende sempre la carta del pozzo se si può usare.
        if self.w.take_samples == 0 {
            return true;
        }
        let mut take = d.clone();
        take.take_discard().unwrap();
        let v_take = self.turn_value(&take, me);
        // Pescate possibili dal tallone: carte non viste, scelte a caso.
        let unseen = d.unseen(me);
        let pool: Vec<Card> = (0..NUM_KINDS)
            .flat_map(|k| std::iter::repeat_n(if k == JOKER as usize { 104 } else { k as u8 }, unseen[k] as usize))
            .collect();
        let n = self.w.take_samples.min(pool.len() as u32);
        let mut v_stock = 0.0;
        for &k in pool.choose_multiple(rng, n as usize) {
            let mut g = d.clone();
            // Una copia della carta che non sia già in mano.
            let hand = &g.hands[me as usize];
            let c = (0..4).map(|i| if k == 104 { 104 + i } else { k + 52 * (i % 2) }).find(|c| !hand.contains(c)).unwrap();
            g.hands[me as usize].push(c);
            g.phase = crate::game::Phase::Play;
            v_stock += self.turn_value(&g, me);
        }
        v_take > v_stock / n.max(1) as f32 + self.w.take_margin
    }

    fn play(&mut self, d: &Deal, me: u8, _: &mut SmallRng) -> Turn {
        plan(d, me, &EngineLay(self.w), |g, hand| self.best_discard(g, me, hand).0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn play_deal(players: &mut [Box<dyn Player>; 2], seed: u64) -> Deal {
        let mut rng = SmallRng::seed_from_u64(seed);
        let mut d = Deal::new(&mut rng, (seed % 2) as u8);
        while !d.is_over() {
            let p = d.to_move;
            if players[p as usize].take_discard(&d, p, &mut rng) {
                d.take_discard().unwrap();
            } else if d.draw_stock().is_err() {
                break;
            }
            let t = players[p as usize].play(&d, p, &mut rng);
            for a in &t.actions {
                d.apply(a).unwrap_or_else(|e| panic!("{}: {e}", players[p as usize].name()));
            }
            d.discard_card(t.discard).unwrap_or_else(|e| panic!("{}: {e}", players[p as usize].name()));
        }
        d
    }

    #[test]
    fn players_play_legal_deals() {
        for seed in 0..30 {
            let mut players: [Box<dyn Player>; 2] = [Box::new(EnginePlayer::new(Weights::default())), Box::new(BasicPlayer)];
            let d = play_deal(&mut players, seed);
            assert!(d.is_over());
            let mut players: [Box<dyn Player>; 2] = [Box::new(RandomPlayer), Box::new(BasicPlayer)];
            play_deal(&mut players, seed);
        }
    }
}
