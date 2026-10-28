//! Il motore che si adatta all'avversario.
//!
//! Per ogni stile tipico (`Profile`) c'è una strategia allenata per sfruttarlo restando
//! prudente (`cfr::Trainer::restricted`) e i conteggi di come gioca quello stile. Mano dopo
//! mano il motore conta quello che fa l'avversario vero (`opponent::Stats`), dà a ogni stile
//! un peso secondo quanto spiega bene quello che si è visto, e consiglia la media delle
//! strategie pesata così. All'inizio quasi tutto il peso è sull'equilibrio; più mani si
//! vedono, più il consiglio si sposta verso la risposta allo stile riconosciuto.

use crate::bots::{Agent, RuleBot};
use crate::cards::Card;
use crate::engine::{bundled, concrete, pick};
use crate::game::{Action, Hand, Observation};
use crate::opponent::{Stats, BET_CTX, SD_BINS, SD_CTX};
use crate::strategy::Strategy;
use rand::rngs::SmallRng;
use std::sync::{Arc, OnceLock};

static BUNDLED: &[u8] = include_bytes!("../data/profiles.bin");
const MAGIC: &[u8; 6] = b"PKPR01";

/// Peso iniziale dell'equilibrio e quanto conta ogni osservazione (meno di 1: gli stili sono
/// solo un'approssimazione delle persone vere, non bisogna fidarsi troppo presto).
pub const PRIOR_EQ: f64 = 0.8;
pub const TEMPER: f64 = 0.5;
/// A ogni mano i conteggi vecchi pesano un po' meno: se l'avversario cambia stile, il motore
/// se ne accorge in qualche decina di mani invece di restare legato al passato.
pub const DECAY: f32 = 0.99;

#[derive(Clone)]
pub struct Profile {
    /// Nome dello stile (`RuleBot::by_name`), oppure `eq` per l'equilibrio.
    pub name: String,
    pub stats: Stats,
    /// La risposta allo stile; `None` per l'equilibrio (la strategia di base).
    pub strategy: Option<Arc<Strategy>>,
}

/// Descrizione breve dello stile e come lo si batte, per l'app.
pub fn describe(name: &str) -> (&'static str, &'static str) {
    match name {
        "eq" => ("gioca in modo equilibrato", "gioco normale"),
        "station" => ("vede quasi sempre", "niente bluff, punta per valore anche con mani medie"),
        "passive" => ("vede molto e punta solo con mani fortissime", "punta per valore; se punta lui, lascia le mani medie"),
        "rock" => ("gioca solo mani forti", "ruba i piatti piccoli; se rilancia, credigli"),
        "bar" => ("gioca a buon senso, bluffa di rado", "punta per valore; poco bluff"),
        "tag" => ("gioca poche mani ma con decisione", "difenditi di più, rilancia per valore"),
        "lag" => ("gioca tante mani e bluffa spesso", "vedi più spesso, lascia che bluffi"),
        "bluffer" => ("bluffa spesso, anche restando servito", "vedi fino in fondo con mani medie"),
        "maniac" => ("punta e rilancia sempre", "vedi con mani medie, rilancia quelle buone"),
        _ => ("stile sconosciuto", ""),
    }
}

fn put_f32s(out: &mut Vec<u8>, v: impl Iterator<Item = f32>) {
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
}

pub fn profiles_to_bytes(profiles: &[Profile]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&(profiles.len() as u32).to_le_bytes());
    for p in profiles {
        out.push(p.name.len() as u8);
        out.extend_from_slice(p.name.as_bytes());
        let s = &p.stats;
        put_f32s(&mut out, s.bet.iter().flatten().copied());
        put_f32s(&mut out, s.draw.iter().copied());
        put_f32s(&mut out, s.showdown.iter().flatten().copied());
        out.extend_from_slice(&s.hands.to_le_bytes());
        let strat = p.strategy.as_ref().map(|s| s.to_bytes()).unwrap_or_default();
        out.extend_from_slice(&(strat.len() as u32).to_le_bytes());
        out.extend_from_slice(&strat);
    }
    out
}

pub fn profiles_from_bytes(b: &[u8]) -> Option<Vec<Profile>> {
    if b.is_empty() {
        return Some(Vec::new());
    }
    let mut i = 0;
    let mut take = |n: usize| {
        let s = b.get(i..i + n);
        i += n;
        s
    };
    if take(6)? != MAGIC {
        return None;
    }
    let n = u32::from_le_bytes(take(4)?.try_into().ok()?);
    let mut v = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let len = take(1)?[0] as usize;
        let name = String::from_utf8(take(len)?.to_vec()).ok()?;
        let mut f = || Some(f32::from_le_bytes(take(4)?.try_into().ok()?));
        let mut stats = Stats::default();
        for x in stats.bet.iter_mut().flatten() {
            *x = f()?;
        }
        for x in stats.draw.iter_mut() {
            *x = f()?;
        }
        for x in stats.showdown.iter_mut().flatten() {
            *x = f()?;
        }
        stats.hands = u32::from_le_bytes(take(4)?.try_into().ok()?);
        let len = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
        let strategy = if len == 0 { None } else { Some(Arc::new(Strategy::from_bytes(take(len)?)?)) };
        v.push(Profile { name, stats, strategy });
    }
    debug_assert_eq!(BET_CTX * 3 + 6 + SD_CTX * SD_BINS, 69);
    Some(v)
}

/// Gli stili inclusi nel programma (`poker-it-sim profiles`).
pub fn bundled_profiles() -> Arc<Vec<Profile>> {
    static P: OnceLock<Arc<Vec<Profile>>> = OnceLock::new();
    P.get_or_init(|| Arc::new(profiles_from_bytes(BUNDLED).expect("profiles.bin non valido"))).clone()
}

pub struct Adaptive {
    base: Arc<Strategy>,
    profiles: Arc<Vec<Profile>>,
    /// Quello che si è visto finora dell'avversario.
    pub seen: Stats,
    pub prior_eq: f64,
    pub temper: f64,
    pub decay: f32,
    label: String,
    fallback: RuleBot,
}

impl Adaptive {
    pub fn new(base: Arc<Strategy>, profiles: Arc<Vec<Profile>>, label: &str) -> Adaptive {
        Adaptive {
            base,
            profiles,
            seen: Stats::default(),
            prior_eq: PRIOR_EQ,
            temper: TEMPER,
            decay: DECAY,
            label: label.into(),
            fallback: RuleBot::by_name("tag").unwrap(),
        }
    }

    pub fn bundled() -> Adaptive {
        Adaptive::new(bundled(), bundled_profiles(), "adaptive")
    }

    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    /// Peso di ogni stile, nell'ordine di `profiles()`.
    pub fn weights(&self) -> Vec<f64> {
        let n = self.profiles.len();
        if n == 0 {
            return Vec::new();
        }
        let others = (n - self.profiles.iter().filter(|p| p.name == "eq").count()).max(1) as f64;
        let logw: Vec<f64> = self
            .profiles
            .iter()
            .map(|p| {
                let prior = if p.name == "eq" { self.prior_eq } else { (1.0 - self.prior_eq) / others };
                prior.ln() + self.temper * self.seen.log_likelihood(&p.stats)
            })
            .collect();
        let m = logw.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let e: Vec<f64> = logw.iter().map(|l| (l - m).exp()).collect();
        let tot: f64 = e.iter().sum();
        e.into_iter().map(|x| x / tot).collect()
    }

    /// Lo stile più probabile e il suo peso.
    pub fn best_guess(&self) -> Option<(&Profile, f64)> {
        let w = self.weights();
        let i = (0..w.len()).max_by(|&a, &b| w[a].total_cmp(&w[b]))?;
        Some((&self.profiles[i], w[i]))
    }

    pub fn advice(&self, obs: &Observation) -> Option<Vec<(Action, f32)>> {
        let w = self.weights();
        let base = self.base.probs(obs);
        let mut mix: Option<Vec<f32>> = None;
        let mut tot = 0.0;
        for (p, &wi) in self.profiles.iter().zip(&w) {
            if wi < 1e-4 {
                continue;
            }
            let probs = match &p.strategy {
                Some(s) => s.probs(obs).or_else(|| base.clone()),
                None => base.clone(),
            };
            if let Some(pr) = probs {
                let m = mix.get_or_insert_with(|| vec![0.0; pr.len()]);
                for (x, y) in m.iter_mut().zip(&pr) {
                    *x += wi as f32 * y;
                }
                tot += wi as f32;
            }
        }
        let probs = match mix {
            Some(m) if tot > 0.0 => m.into_iter().map(|x| x / tot).collect(),
            _ => base?,
        };
        Some(concrete(obs, probs))
    }

    /// Aggiunge una mano finita contro questo avversario: la storia pubblica, il suo posto e
    /// le carte che ha mostrato.
    pub fn learn(&mut self, history: &[Action], opp_seat: u8, shown: Option<&[Card; 5]>) {
        self.seen.fade(self.decay);
        self.seen.record(history, opp_seat, shown);
    }

    /// Come `learn`, da una mano giocata per intero (simulazioni).
    pub fn record(&mut self, h: &Hand, my_seat: u8) {
        let opp = 1 - my_seat;
        let shown = h.showdown().then(|| &h.hands[opp as usize]);
        self.learn(&h.history, opp, shown);
    }
}

impl Agent for Adaptive {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn act(&mut self, obs: &Observation, rng: &mut SmallRng) -> Action {
        match self.advice(obs) {
            Some(adv) => pick(&adv, rng),
            None => self.fallback.act(obs, rng),
        }
    }

    fn hand_over(&mut self, h: &Hand, seat: u8) {
        self.record(h, seat);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::NUM_CARDS;
    use rand::seq::SliceRandom;
    use rand::SeedableRng;

    fn learns(style: &str) -> (String, f64) {
        let mut me = Adaptive::bundled();
        let mut opp = RuleBot::by_name(style).unwrap();
        let mut rng = SmallRng::seed_from_u64(7);
        for i in 0..300 {
            let mut deck: [Card; NUM_CARDS] = core::array::from_fn(|c| c as Card);
            deck.shuffle(&mut rng);
            let mut h = Hand::new(deck);
            let my_seat = (i % 2) as u8;
            while !h.is_over() {
                let obs = Observation::of(&h, h.to_act);
                let a = if h.to_act == my_seat { me.act(&obs, &mut rng) } else { opp.act(&obs, &mut rng) };
                h.apply(a);
            }
            me.record(&h, my_seat);
        }
        let (p, w) = me.best_guess().unwrap();
        (p.name.clone(), w)
    }

    #[test]
    fn recognises_the_styles() {
        if bundled_profiles().is_empty() {
            return;
        }
        for style in ["station", "rock", "maniac"] {
            let (name, w) = learns(style);
            assert_eq!(name, style, "peso {w}");
            assert!(w > 0.5, "{style}: peso {w}");
        }
    }

    #[test]
    fn profiles_round_trip() {
        let p = bundled_profiles();
        let again = profiles_from_bytes(&profiles_to_bytes(&p)).unwrap();
        assert_eq!(again.len(), p.len());
        for (a, b) in p.iter().zip(&again) {
            assert_eq!((&a.name, &a.stats), (&b.name, &b.stats));
            assert_eq!(a.strategy.as_ref().map(|s| s.len()), b.strategy.as_ref().map(|s| s.len()));
        }
    }
}
