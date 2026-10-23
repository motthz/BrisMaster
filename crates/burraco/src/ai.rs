//! Giocatori artificiali: un pianificatore di combinazioni comune e stili di gioco diversi.
//!
//! Ogni turno il giocatore euristico:
//! 1. decide se pescare dal tallone o raccogliere gli scarti, confrontando il potenziale della mano;
//! 2. cala le combinazioni naturali e attacca le carte alle proprie combinazioni;
//! 3. usa le matte secondo il suo stile (subito, oppure solo per prendere il pozzetto, fare il burraco o chiudere);
//! 4. scarta la carta che costa meno: poco utile per sé, poco utile all'avversario.

use crate::cards::{self, kind, rank, suit, Cards, Kind};
use crate::game::{Action, Game, Phase};
use crate::melds::Meld;
use rand::rngs::SmallRng;
use rand::Rng;

pub trait Player: Send {
    fn name(&self) -> String;
    /// Un'azione del giocatore di turno (pesca, calata, attacco o scarto).
    fn act(&mut self, g: &Game, rng: &mut SmallRng) -> Action;
}

/// Gioca la smazzata fino in fondo e restituisce lo stato finale.
pub fn play_hand(mut g: Game, players: &mut [&mut dyn Player; 2], rng: &mut SmallRng) -> Game {
    let mut steps = 0;
    while !g.is_over() {
        let p = g.to_move as usize;
        let a = players[p].act(&g, rng);
        steps += 1;
        assert!(steps < 20_000, "{}: smazzata senza fine, ultima azione {a:?} in {g:?}", players[p].name());
        if let Err(e) = g.apply(&a) {
            panic!("{}: mossa non valida {a:?}: {e}", players[p].name());
        }
    }
    g
}

/// Un passo del piano di gioco del turno.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    New(Vec<Kind>),
    Add(usize, Vec<Kind>),
}

impl Op {
    pub fn cards(&self) -> &[Kind] {
        match self {
            Op::New(v) | Op::Add(_, v) => v,
        }
    }
    pub fn action(&self) -> Action {
        match self {
            Op::New(v) => Action::Meld(v.clone()),
            Op::Add(i, v) => Action::Attach(*i, v.clone()),
        }
    }
}

/// Le combinazioni che si possono fare con una mano.
#[derive(Clone, Debug)]
pub struct Plan {
    pub ops: Vec<Op>,
    /// Le carte che restano in mano.
    pub hand: Cards,
    /// Le proprie combinazioni dopo il piano.
    pub melds: Vec<Meld>,
}

impl Plan {
    fn start(hand: &Cards, melds: &[Meld]) -> Plan {
        Plan { ops: Vec::new(), hand: *hand, melds: melds.to_vec() }
    }

    fn apply(&mut self, op: Op) -> bool {
        let m = match &op {
            Op::New(v) => Meld::new(v),
            Op::Add(i, v) => self.melds[*i].with(v),
        };
        let Some(m) = m else { return false };
        let mut h = self.hand;
        if !op.cards().iter().all(|&k| h.remove(k)) {
            return false;
        }
        self.hand = h;
        match &op {
            Op::New(_) => self.melds.push(m),
            Op::Add(i, _) => self.melds[*i] = m,
        }
        self.ops.push(op);
        true
    }

    /// Punti in tavola (carte e burraco) delle combinazioni.
    pub fn table_points(&self) -> i32 {
        self.melds.iter().map(|m| m.value() + m.bonus()).sum()
    }

    /// Bontà del piano: punti calati più un piccolo premio per ogni carta calata.
    fn score(&self, base: &Cards) -> f32 {
        let melded = (base.len() - self.hand.len()) as f32;
        self.table_points() as f32 + 2.0 * melded + self.melds.iter().map(burraco_progress).sum::<f32>()
    }

    pub fn has_burraco(&self) -> bool {
        self.melds.iter().any(|m| m.is_burraco())
    }
}

/// Premio per le combinazioni vicine al burraco (più lunghe = più vicine ai 200 punti).
fn burraco_progress(m: &Meld) -> f32 {
    if m.is_burraco() {
        return 0.0;
    }
    let l = m.len() as f32;
    let clean = if m.is_clean() { 1.0 } else { 0.5 };
    clean
        * match m.len() {
            0..=3 => 0.0,
            _ => (l - 3.0) * 12.0,
        }
}

/// Posizione nella scala di una carta naturale (l'asso in alto, 14).
fn seq_kind(s: u8, p: u8) -> Kind {
    kind(s, if p == 14 { 1 } else { p })
}

/// Scale naturali di un seme (dal 3 all'asso alto), anche doppie con le seconde copie.
fn take_runs(c: &mut Cards, s: u8, out: &mut Vec<Vec<Kind>>) {
    loop {
        let mut found = false;
        let mut p = 3;
        while p <= 14 {
            let start = p;
            while p <= 14 && c.has(seq_kind(s, p)) {
                p += 1;
            }
            if p - start >= 3 {
                let run: Vec<Kind> = (start..p).map(|q| seq_kind(s, q)).collect();
                for &k in &run {
                    c.remove(k);
                }
                out.push(run);
                found = true;
            }
            p += 1;
        }
        if !found {
            break;
        }
    }
}

/// Tris naturali: tutte le copie di un valore presente almeno tre volte.
fn take_groups(c: &mut Cards, out: &mut Vec<Vec<Kind>>) {
    for r in [1u8, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13] {
        let total: u8 = (0..4).map(|s| c.count(kind(s, r))).sum();
        if total >= 3 {
            let mut g = Vec::new();
            for s in 0..4 {
                for _ in 0..c.count(kind(s, r)) {
                    g.push(kind(s, r));
                }
            }
            for &k in &g {
                c.remove(k);
            }
            out.push(g);
        }
    }
}

fn naturals(h: &Cards) -> impl Iterator<Item = Kind> + '_ {
    h.kinds().filter(|&k| !cards::is_wild(k))
}

/// Attacca alle combinazioni tutte le carte naturali possibili.
fn attach_naturals(plan: &mut Plan) {
    for i in 0..plan.melds.len() {
        'grow: loop {
            for k in plan.melds[i].natural_targets() {
                if plan.hand.has(k) {
                    if let Some(m) = plan.melds[i].with_natural(k) {
                        plan.melds[i] = m;
                        plan.hand.remove(k);
                        plan.ops.push(Op::Add(i, vec![k]));
                        continue 'grow;
                    }
                }
            }
            break;
        }
    }
}

fn new_naturals(plan: &mut Plan, groups_first: bool) {
    let mut c = plan.hand;
    let mut out = Vec::new();
    if groups_first {
        take_groups(&mut c, &mut out);
    }
    for s in 0..4 {
        take_runs(&mut c, s, &mut out);
    }
    if !groups_first {
        take_groups(&mut c, &mut out);
    }
    for m in out {
        plan.apply(Op::New(m));
    }
}

/// Il miglior piano con le sole carte naturali (le matte restano in mano).
pub fn natural_plan(hand: &Cards, melds: &[Meld]) -> Plan {
    let mut best: Option<(f32, Plan)> = None;
    for variant in 0..3 {
        let mut p = Plan::start(hand, melds);
        match variant {
            0 => {
                attach_naturals(&mut p);
                new_naturals(&mut p, false);
            }
            1 => new_naturals(&mut p, false),
            _ => new_naturals(&mut p, true),
        }
        attach_naturals(&mut p);
        let s = p.score(hand);
        if best.as_ref().is_none_or(|(b, _)| s > *b) {
            best = Some((s, p));
        }
    }
    best.unwrap().1
}

/// Quanto una carta rimasta in mano è vicina a fare combinazione con le altre.
fn closeness(h: &Cards, k: Kind) -> f32 {
    if cards::is_wild(k) {
        return 0.0;
    }
    let (s, r) = (suit(k), rank(k));
    let mut v = 0.0;
    let same: u8 = (0..4).map(|t| h.count(kind(t, r))).sum::<u8>() - 1;
    v += 5.0 * same.min(2) as f32;
    let positions: &[u8] = if r == 1 { &[1, 14] } else { std::slice::from_ref(&r) };
    for &p in positions {
        for d in [-2i8, -1, 1, 2] {
            let q = p as i8 + d;
            if !(3..=14).contains(&q) {
                continue;
            }
            if h.has(seq_kind(s, q as u8)) {
                v += if d.abs() == 1 { 6.0 } else { 3.0 };
            }
        }
    }
    v
}

/// Potenziale di una mano: combinazioni che si possono calare subito più le coppie promettenti.
pub fn potential(hand: &Cards, melds: &[Meld]) -> f32 {
    let p = natural_plan(hand, melds);
    let partial: f32 = naturals(&p.hand).map(|k| closeness(&p.hand, k) * p.hand.count(k) as f32).sum();
    p.score(hand) + 0.8 * partial + 22.0 * p.hand.wilds() as f32 - 0.25 * p.hand.value() as f32
}

/// La carta si attacca a una di queste combinazioni?
pub fn attaches(melds: &[Meld], k: Kind) -> bool {
    if cards::is_wild(k) {
        melds.iter().any(|m| m.with(&[k]).is_some())
    } else {
        melds.iter().any(|m| m.with_natural(k).is_some())
    }
}

/// Stile di gioco: i parametri del giocatore euristico.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// Guadagno di potenziale necessario per raccogliere gli scarti invece di pescare.
    pub pile_threshold: f32,
    /// Costo di ogni carta raccolta in più (rischio di restare con tante carte in mano).
    pub pile_card_cost: f32,
    /// Peso delle carte utili all'avversario che si tolgono dagli scarti raccogliendoli.
    pub deny_w: f32,
    /// Usa le matte appena servono a calare (true) o le tiene per i momenti decisivi.
    pub eager_wilds: bool,
    /// Cala le combinazioni appena può (false: le tiene in mano finché non serve).
    pub eager_melds: bool,
    /// Quanto evita di scartare carte utili all'avversario.
    pub care: f32,
    /// Quanto tiene le carte che stanno per fare combinazione.
    pub keep_w: f32,
    /// Quanto preferisce scartare carte di valore alto (meno punti in mano a fine smazzata).
    pub dump_w: f32,
    /// Chiude solo se in vantaggio di almeno tanti punti (stimati).
    pub close_margin: i32,
    /// Con il tallone sotto queste carte usa tutte le matte e chiude appena può.
    pub late_stock: usize,
    /// Probabilità di uno scarto a caso.
    pub noise: f32,
    /// Carte del tallone provate per stimare la pescata (0: stima fissa, più veloce).
    pub stock_samples: u8,
}

impl Style {
    /// Lo stile del motore: equilibrato, attento agli scarti, matte conservate per il burraco.
    pub const EXPERT: Style = Style {
        pile_threshold: -20.0,
        pile_card_cost: 1.0,
        deny_w: 0.5,
        eager_wilds: false,
        eager_melds: true,
        care: 1.0,
        keep_w: 1.0,
        dump_w: 0.3,
        close_margin: 0,
        late_stock: 8,
        noise: 0.0,
        stock_samples: 0,
    };

    /// Lo stesso stile, più veloce: per le simulazioni del motore.
    pub const fn fast(self) -> Style {
        Style { stock_samples: 0, ..self }
    }
}

/// Giocatore euristico con uno stile.
#[derive(Clone)]
pub struct Heuristic {
    pub label: String,
    pub style: Style,
}

impl Heuristic {
    pub fn new(label: &str, style: Style) -> Heuristic {
        Heuristic { label: label.to_string(), style }
    }
    pub fn expert() -> Heuristic {
        Heuristic::new("Esperto", Style::EXPERT)
    }
}

/// Stima dei punti dell'avversario se la smazzata finisse ora (la sua mano non si vede).
pub fn opp_estimate(g: &Game, me: u8) -> i32 {
    let o = 1 - me as usize;
    let table: i32 = g.melds[o].iter().map(|m| m.value() + m.bonus()).sum();
    let hand = 8 * g.hands[o].len() as i32;
    let pozzetto = if g.taken[o] { 0 } else { -100 - 8 * crate::game::POZZETTO_SIZE as i32 / 2 };
    table - hand + pozzetto
}

/// Il piano completo del turno: combinazioni naturali, uso delle matte, chiusura.
pub fn turn_plan(g: &Game, me: u8, style: &Style) -> Plan {
    let i = me as usize;
    let hand = g.hands[i];
    let melds = &g.melds[i];
    let taken = g.taken[i];
    let late = g.stock.len() <= style.late_stock;

    let mut plan = natural_plan(&hand, melds);
    if !style.eager_melds && !late {
        // Tiene in mano le combinazioni nuove finché non può prendere il pozzetto o chiudere.
        let near_out = plan.hand.len() <= 2 + plan.hand.wilds() as usize;
        if !near_out {
            let mut p = Plan::start(&hand, melds);
            attach_naturals(&mut p);
            plan = p;
        }
    }

    use_wilds(&mut plan, g, me, style, late);

    // Chiusura: con il pozzetto preso e un burraco si chiude scartando l'ultima carta.
    let want_close = |plan: &Plan| {
        let mine = plan.table_points() + 100;
        late || mine - opp_estimate(g, me) >= style.close_margin
    };
    if taken {
        loop {
            let n = plan.hand.len();
            let closable = plan.has_burraco() && n == 1 && !cards::is_wild(plan.hand.kinds().next().unwrap());
            let ok = n >= 2 || (closable && want_close(&plan));
            if ok || plan.ops.is_empty() {
                break;
            }
            // Toglie l'ultima calata per tenere carte in mano.
            plan = replay(&hand, melds, &plan.ops[..plan.ops.len() - 1]);
        }
    }
    plan
}

/// Rifà un piano con i soli primi passi.
fn replay(hand: &Cards, melds: &[Meld], ops: &[Op]) -> Plan {
    let mut p = Plan::start(hand, melds);
    for op in ops {
        p.apply(op.clone());
    }
    p
}

/// Possibili usi di una matta con le carte rimaste.
fn wild_options(plan: &Plan, w: Kind) -> Vec<(Op, usize)> {
    let mut out = Vec::new();
    let nat: Vec<Kind> = naturals(&plan.hand).collect();
    // Coppia + matta: nuova combinazione.
    for (a_i, &a) in nat.iter().enumerate() {
        for &b in &nat[a_i..] {
            if a == b && plan.hand.count(a) < 2 {
                continue;
            }
            let v = vec![a, b, w];
            if Meld::new(&v).is_some() {
                out.push((Op::New(v), 2));
            }
        }
    }
    for (i, m) in plan.melds.iter().enumerate() {
        if m.wild.is_some() {
            continue;
        }
        // Matta più una carta che resta staccata di uno.
        for &a in &nat {
            if m.with(&[w, a]).is_some() && m.with(&[a]).is_none() {
                out.push((Op::Add(i, vec![w, a]), 1));
            }
        }
        if m.with(&[w]).is_some() {
            out.push((Op::Add(i, vec![w]), 0));
        }
    }
    out
}

fn use_wilds(plan: &mut Plan, g: &Game, me: u8, style: &Style, late: bool) {
    let taken = g.taken[me as usize];
    let wilds = |p: &Plan| -> Vec<Kind> { p.hand.kinds().filter(|&k| cards::is_wild(k)).collect() };
    if wilds(plan).is_empty() {
        return;
    }
    // 1. Matte che fanno calare carte naturali.
    let mut trial = plan.clone();
    loop {
        let Some(&w) = wilds(&trial).first() else { break };
        let opts = wild_options(&trial, w);
        let best = opts.into_iter().filter(|(_, n)| *n > 0).max_by_key(|(op, n)| {
            let mut t = trial.clone();
            t.apply(op.clone());
            (*n, t.melds.iter().map(|m| m.len()).max().unwrap_or(0))
        });
        match best {
            Some((op, _)) if trial.apply(op.clone()) => attach_naturals(&mut trial),
            _ => break,
        }
    }
    // 2. Matte rimaste: per completare un burraco o per liberarsi della mano.
    let mut dumped = trial.clone();
    while let Some(&w) = wilds(&dumped).first() {
        let opts = wild_options(&dumped, w);
        let best = opts.into_iter().filter(|(op, _)| matches!(op, Op::Add(_, v) if v.len() == 1)).max_by_key(
            |(op, _)| match op {
                Op::Add(i, _) => dumped.melds[*i].len(),
                _ => 0,
            },
        );
        match best {
            Some((op, _)) if dumped.apply(op.clone()) => {}
            _ => break,
        }
    }

    let goes_out = |p: &Plan| p.hand.len() <= 1;
    let completes_burraco = |p: &Plan| p.has_burraco() && !plan.has_burraco();
    if style.eager_wilds || late {
        *plan = if late || goes_out(&dumped) { dumped } else { trial };
    } else if !taken {
        // Prima del pozzetto: le matte servono solo se permettono di prenderlo subito.
        if goes_out(&trial) {
            *plan = trial;
        } else if goes_out(&dumped) {
            *plan = dumped;
        }
    } else if completes_burraco(&trial) || (plan.has_burraco() && goes_out(&trial)) {
        *plan = trial;
    } else if completes_burraco(&dumped) || (plan.has_burraco() && goes_out(&dumped)) {
        *plan = dumped;
    }
}

/// Pericolo di regalare la carta all'avversario.
pub fn danger(g: &Game, me: u8, k: Kind) -> f32 {
    let o = 1 - me as usize;
    let mut d = 0.0;
    if cards::is_wild(k) {
        d += 60.0;
    } else {
        if attaches(&g.melds[o], k) {
            d += 30.0;
        }
        let known = &g.known[o];
        if known.len() > 0 && closeness(&with(known, k), k) > 0.0 {
            d += 2.0 * closeness(&with(known, k), k);
        }
        if (0..4).any(|t| g.discarded[o].has(kind(t, rank(k)))) {
            d -= 4.0;
        }
        if g.discarded[o].has(k) {
            d -= 4.0;
        }
    }
    let threat = if g.taken[o] && g.has_burraco(o as u8) { 1.5 } else { 1.0 };
    d * (1.0 + g.pile.len() as f32 / 8.0) * threat
}

fn with(c: &Cards, k: Kind) -> Cards {
    let mut c = *c;
    c.add(k);
    c
}

/// Costo di scartare la carta: più è basso, meglio è.
pub fn discard_cost(g: &Game, me: u8, plan: &Plan, k: Kind, style: &Style) -> f32 {
    let keep = if cards::is_wild(k) {
        200.0
    } else {
        let mut v = closeness(&plan.hand, k);
        if attaches(&plan.melds, k) {
            v += 15.0;
        }
        v
    };
    let o = 1 - me as usize;
    let urgent = g.stock.len() < 15 || (g.taken[o] && g.has_burraco(o as u8));
    let dump = cards::value(k) as f32 * style.dump_w * if urgent { 1.0 } else { 0.3 };
    style.keep_w * keep + style.care * danger(g, me, k) - dump
}

/// Lo scarto migliore per la mano rimasta dopo il piano.
pub fn choose_discard(g: &Game, me: u8, plan: &Plan, style: &Style, rng: &mut SmallRng) -> Kind {
    let hand = &plan.hand;
    let mut cands: Vec<Kind> = hand.kinds().collect();
    if let Some(nd) = g.no_discard {
        if cands.iter().any(|&k| k != nd) {
            cands.retain(|&k| k != nd);
        }
    }
    if g.taken[me as usize] && hand.len() == 1 {
        return cands[0];
    }
    if style.noise > 0.0 && rng.gen::<f32>() < style.noise {
        // Anche il giocatore distratto non butta via una matta se ha altro.
        let non_wild: Vec<Kind> = cands.iter().copied().filter(|&k| !cards::is_wild(k)).collect();
        let pool = if non_wild.is_empty() { &cands } else { &non_wild };
        return pool[rng.gen_range(0..pool.len())];
    }
    *cands
        .iter()
        .min_by(|&&a, &&b| {
            discard_cost(g, me, plan, a, style).total_cmp(&discard_cost(g, me, plan, b, style)).then(a.cmp(&b))
        })
        .unwrap()
}

/// Guadagno medio di potenziale di una carta pescata dal tallone, quando non lo si stima.
const STOCK_GAIN: f32 = 6.0;

/// Quanto conviene raccogliere gli scarti rispetto a pescare dal tallone (in punti di potenziale).
pub fn pile_gain(g: &Game, me: u8, style: &Style, rng: &mut SmallRng) -> f32 {
    let i = me as usize;
    let hand = g.hands[i];
    let base = potential(&hand, &g.melds[i]);
    let mut with_pile = hand;
    for &k in &g.pile {
        with_pile.add(k);
    }
    let n = style.stock_samples as usize;
    let stock = if n == 0 {
        STOCK_GAIN
    } else {
        let unseen = g.unseen(me).to_vec();
        (0..n)
            .map(|_| {
                let mut h = hand;
                h.add(unseen[rng.gen_range(0..unseen.len())]);
                potential(&h, &g.melds[i]) - base
            })
            .sum::<f32>()
            / n as f32
    };
    let gain = potential(&with_pile, &g.melds[i]) - base - stock;
    let deny: f32 =
        g.pile.iter().map(|&k| if cards::is_wild(k) || attaches(&g.melds[1 - i], k) { 20.0 } else { 0.0 }).sum();
    gain + style.deny_w * deny - style.pile_card_cost * (g.pile.len() as f32 - 1.0)
}

/// Raccoglie gli scarti? Una carta sola si prende solo se si cala subito
/// (altrimenti i due giocatori si passerebbero la stessa carta all'infinito).
pub fn wants_pile(g: &Game, me: u8, style: &Style, rng: &mut SmallRng) -> bool {
    match g.pile.len() {
        0 => false,
        1 => {
            let k = g.pile[0];
            let mut h = g.hands[me as usize];
            h.add(k);
            let p = natural_plan(&h, &g.melds[me as usize]);
            p.hand.count(k) < h.count(k)
        }
        _ => pile_gain(g, me, style, rng) >= style.pile_threshold,
    }
}

impl Heuristic {
    pub fn decide(&self, g: &Game, rng: &mut SmallRng) -> Action {
        let me = g.to_move;
        let style = &self.style;
        match g.phase {
            Phase::Draw => {
                if wants_pile(g, me, style, rng) {
                    Action::TakePile
                } else {
                    Action::DrawStock
                }
            }
            _ => {
                let plan = turn_plan(g, me, style);
                match plan.ops.first() {
                    Some(op) => op.action(),
                    None => Action::Discard(choose_discard(g, me, &plan, style, rng)),
                }
            }
        }
    }
}

impl Player for Heuristic {
    fn name(&self) -> String {
        self.label.clone()
    }
    fn act(&mut self, g: &Game, rng: &mut SmallRng) -> Action {
        self.decide(g, rng)
    }
}

/// Giocatore quasi casuale: cala quello che vede, pesca e scarta a caso.
pub struct RandomPlayer;

impl Player for RandomPlayer {
    fn name(&self) -> String {
        "Casuale".into()
    }
    fn act(&mut self, g: &Game, rng: &mut SmallRng) -> Action {
        let me = g.to_move;
        match g.phase {
            Phase::Draw => {
                if !g.pile.is_empty() && rng.gen_bool(0.3) {
                    Action::TakePile
                } else {
                    Action::DrawStock
                }
            }
            _ => {
                let style = Style { eager_wilds: true, noise: 1.0, close_margin: i32::MIN / 2, ..Style::EXPERT };
                let plan = turn_plan(g, me, &style);
                match plan.ops.first() {
                    Some(op) => op.action(),
                    None => Action::Discard(choose_discard(g, me, &plan, &style, rng)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::parse;
    use rand::SeedableRng;

    fn ks(s: &str) -> Vec<Kind> {
        s.split_whitespace().map(|c| parse(c).unwrap()).collect()
    }

    #[test]
    fn plans_naturals() {
        let h = Cards::from_slice(&ks("3c 4c 5c 6c 7p 7q 7f Kp Qp Ap 9f jk"));
        let p = natural_plan(&h, &[]);
        assert_eq!(p.melds.len(), 3);
        assert_eq!(p.hand, Cards::from_slice(&ks("9f jk")));
    }

    #[test]
    fn full_games_are_legal() {
        let mut rng = SmallRng::seed_from_u64(7);
        for n in 0..200 {
            let g = Game::new(&mut rng, (n % 2) as u8);
            let mut a = Heuristic::expert();
            let mut b = RandomPlayer;
            let end = play_hand(g, &mut [&mut a, &mut b], &mut rng);
            assert!(end.is_over());
            assert_eq!(end.total_cards(), 108);
        }
    }
}
