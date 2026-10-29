//! Chi siede al tavolo: i giocatori della versione attuale e quelli delle versioni rilasciate.
//!
//! SPEC: random | heuristic | aggressive | cautious | trumphappy | counter
//!       | followsuit | loadleader | notrump
//!       | engine:MONDI[:EPSILON[:SOGLIA[:PESO_PUNTI]]] | ismcts:ITERAZIONI
//!       | ismcts+:ITERAZIONI           (ISMCTS con le simulazioni del motore: a carte scoperte, finale esatto)
//!       | classic:MONDI                (motore della v0.1.1 compilato con la versione attuale)
//!       | eng:CHIAVE=VALORE,...        (varianti del motore attuale, vedi make_player)
//!       | noisy:ERRORE:SPEC            (es. noisy:0.25:heuristic)
//!       | net:ITERAZIONI[:PESO]        (ISMCTS potenziato con la rete di BRISCOLA_NET nelle foglie)
//!       | infer:ITERAZIONI:TEMP[:MANI[:MINIMO]][:net] (ISMCTS potenziato che deduce le carte avversarie
//!                                      con la rete di politica di BRISCOLA_POLICY)
//!       | peek:ITERAZIONI              (baro: ISMCTS potenziato che vede la mano avversaria)
//!       | vedetutto:ITERAZIONI         (super-baro: vede la mano avversaria e l'ordine del mazzo)
//!       | stile:NOME | persona:N       (giocatori umani da manopole, vedi persona.rs)
//!       | fretta:ERRORE:SPEC           (sbaglia sempre di più verso la fine, fino a ERRORE)
//!       | tilt:ERRORE:SPEC             (sbaglia con probabilità ERRORE quando è sotto di 10 punti)
//!       | camaleonte:N                 (cambia stile ogni 4 prese)
//!       | politica                     (rete di politica pura: gioca la carta più probabile)
//!       | misto:ITERAZIONI             (ISMCTS potenziato che sceglie a caso in proporzione alle visite)
//!       | vX.Y.Z/SPEC                  (versione rilasciata, es. v0.1.1/engine:1000)

use crate::opponents::*;
use crate::persona::{Chameleon, Clock, Persona, Tilt};
use briscola_core::ai::*;
use briscola_core::cards::Card;
use briscola_core::infer::{Belief, Exact, Log};
use briscola_core::net::Net;
use std::sync::{Arc, OnceLock};
use briscola_core::{Game, Observation};
use rand::rngs::SmallRng;

/// Un giocatore visto dal banco di prova. Le versioni precedenti hanno tipi propri,
/// quindi tengono una copia della partita nel proprio formato e ricevono ogni carta giocata.
pub trait Agent: Send {
    fn name(&self) -> String;
    /// Inizio partita: ordine del mazzo e chi apre (stesso formato in tutte le versioni).
    fn start(&mut self, _order: &[Card], _first: u8) {}
    fn choose(&mut self, g: &Game, me: u8, rng: &mut SmallRng) -> Card;
    /// Ogni carta giocata, da chiunque, nell'ordine.
    fn observe(&mut self, _c: Card) {}
}

struct Current(Box<dyn Player>, String);

impl Agent for Current {
    fn name(&self) -> String {
        format!("{}{}", self.0.name(), self.1)
    }
    fn choose(&mut self, g: &Game, me: u8, rng: &mut SmallRng) -> Card {
        self.0.choose(&Observation::of(g, me), rng)
    }
}

type NetResult = Result<Arc<Net>, String>;

fn load_net(var: &str) -> NetResult {
    let path = std::env::var(var).map_err(|_| format!("serve il file della rete in {var}"))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    Net::from_bytes(&bytes).map(Arc::new)
}

/// Rete di valutazione per i giocatori `net:`: dal file in BRISCOLA_NET, letta una volta sola.
fn value_net() -> NetResult {
    static NET: OnceLock<NetResult> = OnceLock::new();
    NET.get_or_init(|| load_net("BRISCOLA_NET")).clone()
}

/// Rete di politica per i giocatori `infer:`: dal file in BRISCOLA_POLICY.
fn policy_net() -> NetResult {
    static NET: OnceLock<NetResult> = OnceLock::new();
    NET.get_or_init(|| load_net("BRISCOLA_POLICY")).clone()
}

/// ISMCTS che deduce le carte avversarie da come ha giocato: tiene il registro della
/// partita (solo informazioni pubbliche e le proprie carte) e pesca i mondi dalle mani pesate.
struct Infer {
    search: IsmctsPlayer,
    policy: Arc<Net>,
    temp: f32,
    floor: f32,
    pool: usize,
    game: Option<Game>,
    log: Log,
    me: Option<u8>,
    /// Diagnosi: nelle situazioni scelte vede la mano avversaria (vedi `oracolo:`).
    oracle: Option<Moment>,
    /// Deduzione esatta (tutte le mani possibili) invece delle mani campionate (`esatta:`).
    exact: Option<Exact>,
}

/// Momento della partita in cui l'oracolo vede la mano avversaria.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Moment {
    /// Mazzo con 24 carte o più (le prime 5–6 prese).
    Inizio,
    /// Mazzo da 12 a 23 carte.
    Meta,
    /// Mazzo da 1 a 11 carte: il pre-finale.
    Prefinale,
    /// Quando apre la presa.
    Apre,
    /// Quando risponde.
    Risponde,
    /// Sempre (il baro).
    Sempre,
}

impl Moment {
    pub fn parse(s: &str) -> Option<Moment> {
        Some(match s {
            "inizio" => Moment::Inizio,
            "meta" => Moment::Meta,
            "prefinale" => Moment::Prefinale,
            "apre" => Moment::Apre,
            "risponde" => Moment::Risponde,
            "sempre" => Moment::Sempre,
            _ => return None,
        })
    }
    fn label(self) -> &'static str {
        match self {
            Moment::Inizio => "a inizio partita (mazzo da 24 carte in su)",
            Moment::Meta => "a metà partita (mazzo da 12 a 23 carte)",
            Moment::Prefinale => "nel pre-finale (mazzo da 1 a 11 carte)",
            Moment::Apre => "quando apre la presa",
            Moment::Risponde => "quando risponde",
            Moment::Sempre => "sempre",
        }
    }
    fn holds(self, obs: &Observation) -> bool {
        match self {
            Moment::Inizio => obs.deck_len >= 24,
            Moment::Meta => (12..24).contains(&obs.deck_len),
            Moment::Prefinale => (1..12).contains(&obs.deck_len),
            Moment::Apre => obs.table.is_none(),
            Moment::Risponde => obs.table.is_some(),
            Moment::Sempre => true,
        }
    }
}

impl Agent for Infer {
    fn name(&self) -> String {
        let floor = if self.floor > 0.0 { format!(", minimo {:.0}%", self.floor * 100.0) } else { String::new() };
        if self.exact.is_some() {
            let floor = if self.floor > 0.0 { format!(", minimo {:.0}%", self.floor * 100.0) } else { String::new() };
            return format!("{} · deduzione esatta (temperatura {}{floor})", self.search.name(), self.temp);
        }
        let oracle = self.oracle.map_or(String::new(), |m| format!(" · vede la mano avversaria {}", m.label()));
        format!("{} · deduce le carte avversarie (temperatura {}{floor}){oracle}", self.search.name(), self.temp)
    }
    fn start(&mut self, order: &[Card], first: u8) {
        self.game = Some(Game::from_order(order, first));
        self.log = Log::default();
        self.me = None;
        if self.exact.is_some() {
            self.exact = Some(Exact::default());
        }
    }
    fn choose(&mut self, g: &Game, me: u8, rng: &mut SmallRng) -> Card {
        self.me = Some(me);
        let obs = Observation::of(g, me);
        if obs.hand.len() == 1 {
            return obs.hand.iter().next().unwrap();
        }
        // A mazzo vuoto le carte avversarie sono note: non c'è niente da dedurre.
        let root = if obs.deck_len == 0 {
            self.search.search(&obs, rng)
        } else if self.oracle.is_some_and(|m| m.holds(&obs)) {
            let opp = g.hands[1 - me as usize];
            self.search.search_with(&obs, rng, |r| obs.determinize_given(opp, r))
        } else {
            let belief = match self.exact.as_mut() {
                Some(ex) => {
                    ex.update(&obs, &self.log.events, &self.policy, self.temp, self.floor);
                    ex.belief(&obs)
                }
                None => Belief::infer(&obs, &self.log.events, &self.policy, self.temp, self.floor, self.pool, rng),
            };
            self.search.search_with(&obs, rng, |r| obs.determinize_given(belief.sample(r), r))
        };
        root.iter().max_by(|a, b| a.visits.partial_cmp(&b.visits).unwrap()).unwrap().card
    }
    fn observe(&mut self, c: Card) {
        let g = self.game.as_mut().expect("partita non iniziata");
        // Il proprio posto si scopre alla prima mossa: se apre l'avversario, la sua prima carta
        // si registra lì, quando il posto è ancora ignoto.
        let me = self.me.unwrap_or(1 - g.to_move());
        self.log.before_play(g, me, c);
        g.play(c);
    }
}

/// "Baro": ISMCTS potenziato che vede la mano dell'avversario (non l'ordine del mazzo).
/// Non è un avversario realistico: dice quanto vale sapere le carte avversarie.
struct Peek(IsmctsPlayer);

impl Agent for Peek {
    fn name(&self) -> String {
        format!("Baro: vede le carte avversarie, ISMCTS potenziato ({} iterazioni)", self.0.iterations)
    }
    fn choose(&mut self, g: &Game, me: u8, rng: &mut SmallRng) -> Card {
        let obs = Observation::of(g, me);
        if obs.hand.len() == 1 {
            return obs.hand.iter().next().unwrap();
        }
        let opp = g.hands[1 - me as usize];
        let root = self.0.search_with(&obs, rng, |r| obs.determinize_given(opp, r));
        root.iter().max_by(|a, b| a.visits.partial_cmp(&b.visits).unwrap()).unwrap().card
    }
}

/// "Super-baro": ISMCTS potenziato che vede tutto, mano avversaria e ordine del mazzo.
/// È il tetto assoluto: gioca a informazione completa.
struct SeeAll(IsmctsPlayer);

impl Agent for SeeAll {
    fn name(&self) -> String {
        format!("Super-baro: vede mano avversaria e mazzo, ISMCTS potenziato ({} iterazioni)", self.0.iterations)
    }
    fn choose(&mut self, g: &Game, me: u8, rng: &mut SmallRng) -> Card {
        let obs = Observation::of(g, me);
        if obs.hand.len() == 1 {
            return obs.hand.iter().next().unwrap();
        }
        let root = self.0.search_with(&obs, rng, |_| *g);
        root.iter().max_by(|a, b| a.visits.partial_cmp(&b.visits).unwrap()).unwrap().card
    }
}

/// Strategia mista: ISMCTS potenziato che sceglie la carta a caso in proporzione alle visite
/// al quadrato. Gioca quasi sempre bene ma non si fa prevedere.
struct Mixed(IsmctsPlayer);

impl Player for Mixed {
    fn name(&self) -> String {
        format!("Imprevedibile: ISMCTS potenziato ({} iterazioni) che sceglie a caso tra le mosse buone", self.0.iterations)
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        use rand::Rng;
        if obs.hand.len() == 1 {
            return obs.hand.iter().next().unwrap();
        }
        let root = self.0.search(obs, rng);
        let total: f64 = root.iter().map(|r| r.visits * r.visits).sum();
        let mut x = rng.gen::<f64>() * total;
        for r in &root {
            x -= r.visits * r.visits;
            if x <= 0.0 {
                return r.card;
            }
        }
        root.last().unwrap().card
    }
}

/// Rete di politica pura (quella inclusa nel programma): gioca la carta che la rete ritiene
/// più probabile per un giocatore forte. Niente ricerca: è un "istinto" allenato.
struct PolicyPlayer;

impl Player for PolicyPlayer {
    fn name(&self) -> String {
        "Rete di politica pura (istinto, senza ricerca)".into()
    }
    fn choose(&mut self, obs: &Observation, _rng: &mut SmallRng) -> Card {
        let mut p = [0f32; 3];
        let n = briscola_core::net::policy().policy(obs, 1.0, &mut p);
        let i = (0..n).max_by(|&a, &b| p[a].partial_cmp(&p[b]).unwrap()).unwrap();
        obs.hand.iter().nth(i).unwrap()
    }
}

/// Le varianti del motore hanno lo stesso nome: si aggiungono i parametri diversi dal normale.
fn variant_suffix(spec: &str) -> String {
    let Some(rest) = spec.strip_prefix("engine:") else { return String::new() };
    let parts: Vec<&str> = rest.split(':').collect();
    let mut out = String::new();
    if let Some(e) = parts.get(1).and_then(|e| e.parse::<f64>().ok()) {
        out += &format!(" · simulazioni con {:.0}% di mosse casuali", e * 100.0);
    }
    if let Some(d) = parts.get(2) {
        out += &format!(" · finale esatto da {d} carte nel mazzo");
    }
    if parts.get(3).is_some() {
        out += &format!(" · peso dei punti {}", parts[3]);
    }
    out
}

fn policy(cfg: &mut EngineConfig) -> &mut RolloutParams {
    cfg.rollout_policy.get_or_insert_with(Default::default)
}

pub fn make_player(spec: &str) -> Result<Box<dyn Player>, String> {
    let num = |s: &str, what: &str| s.parse::<f64>().map_err(|_| format!("{what} non valido in '{spec}'"));
    Ok(match spec {
        "random" => Box::new(RandomPlayer),
        "heuristic" => Box::new(HeuristicPlayer),
        "aggressive" => Box::new(AggressivePlayer),
        "cautious" => Box::new(CautiousPlayer),
        "trumphappy" => Box::new(TrumpHappyPlayer),
        "counter" => Box::new(CounterPlayer),
        "followsuit" => Box::new(FollowSuitPlayer),
        "loadleader" => Box::new(LoadLeaderPlayer),
        "notrump" => Box::new(NoTrumpPlayer),
        s if s.starts_with("ismcts:") => Box::new(IsmctsPlayer::new(num(&s[7..], "numero di iterazioni")? as u32)),
        s if s.starts_with("ismcts+:") => Box::new(IsmctsPlayer::strong(num(&s[8..], "numero di iterazioni")? as u32)),
        s if s.starts_with("engine:") => {
            let parts: Vec<&str> = s[7..].split(':').collect();
            let mut cfg = EngineConfig::strength(num(parts[0], "numero di mondi")? as u32);
            if let Some(e) = parts.get(1) {
                cfg.rollout_epsilon = num(e, "epsilon")?;
            }
            if let Some(d) = parts.get(2) {
                cfg.solve_deck_len = num(d, "soglia del risolutore")? as u8;
            }
            if let Some(w) = parts.get(3) {
                cfg.points_weight = num(w, "peso dei punti")?;
            }
            Box::new(EnginePlayer::new(cfg))
        }
        s if s.starts_with("classic:") => {
            Box::new(EnginePlayer::new(EngineConfig::classic(num(&s[8..], "numero di mondi")? as u32)))
        }
        // Varianti per chiavi, a partire dal motore attuale:
        // eng:mondi=1000,eps=0,solve=8,rsolve=2,pol=open,tb=8,tm=2,lb=0,os=4,ismcts=N (rsolve=no, pol=heuristic)
        s if s.starts_with("eng:") => {
            let mut cfg = EngineConfig::strength(1000);
            for kv in s[4..].split(',').filter(|p| !p.is_empty()) {
                let (k, v) = kv.split_once('=').ok_or(format!("serve chiave=valore, non '{kv}'"))?;
                match k {
                    "mondi" => cfg.samples = num(v, "mondi")? as u32,
                    "eps" => cfg.rollout_epsilon = num(v, "eps")?,
                    "solve" => cfg.solve_deck_len = num(v, "solve")? as u8,
                    "peso" => cfg.points_weight = num(v, "peso")?,
                    "rsolve" => cfg.rollout_solve = if v == "no" { None } else { Some(num(v, "rsolve")? as u8) },
                    "pol" => cfg.rollout_policy = (v == "open").then(Default::default),
                    "tb" => policy(&mut cfg).trump_base = num(v, "tb")? as i32,
                    "tm" => policy(&mut cfg).trump_mult = num(v, "tm")? as i32,
                    "lb" => policy(&mut cfg).lead_bonus = num(v, "lb")? as i32,
                    "os" => policy(&mut cfg).opp_spend = num(v, "os")? as i32,
                    "ismcts" => cfg.ismcts = Some(num(v, "ismcts")? as u32),
                    other => return Err(format!("opzione sconosciuta: {other}")),
                }
            }
            Box::new(EnginePlayer::new(cfg))
        }
        s if s.starts_with("stile:") => Box::new(Persona::style(&s[6..]).ok_or(format!("stile sconosciuto: {}", &s[6..]))?),
        s if s.starts_with("persona:") => Box::new(Persona::numbered(num(&s[8..], "numero del personaggio")? as u64)),
        s if s.starts_with("fretta:") || s.starts_with("tilt:") => {
            let (kind, rest) = s.split_once(':').unwrap();
            let (err, base) = rest.split_once(':').ok_or(format!("serve {kind}:ERRORE:SPEC, non '{s}'"))?;
            let (base, error) = (make_player(base)?, num(err, "errore")?);
            if kind == "fretta" { Box::new(Clock { base, error }) } else { Box::new(Tilt { base, error }) }
        }
        s if s.starts_with("camaleonte:") => {
            let n = num(&s[11..], "numero")? as u64;
            let styles: Vec<Box<dyn Player>> = vec![
                Box::new(HeuristicPlayer),
                Box::new(AggressivePlayer),
                Box::new(CautiousPlayer),
                Box::new(TrumpHappyPlayer),
                Box::new(Persona::style("circolo").unwrap()),
                Box::new(Persona::numbered(n)),
                Box::new(IsmctsPlayer::strong(2000)),
            ];
            Box::new(Chameleon::new(n, styles))
        }
        "politica" => Box::new(PolicyPlayer),
        s if s.starts_with("noisy:") => {
            let (err, base) = s[6..].split_once(':').ok_or(format!("serve noisy:ERRORE:SPEC, non '{s}'"))?;
            Box::new(Noisy { base: make_player(base)?, error: num(err, "errore")? })
        }
        other => return Err(format!("giocatore sconosciuto: {other}")),
    })
}

/// Il giocatore `infer:` da `rest` (quello che segue `infer:`).
fn make_infer(rest: &str, spec: &str) -> Result<Infer, String> {
    // infer:ITERAZIONI:TEMPERATURA[:MANI[:MINIMO]][:net]
    let mut parts: Vec<&str> = rest.split(':').collect();
    let with_net = parts.last() == Some(&"net");
    if with_net {
        parts.pop();
    }
    let num = |i: usize, d: f64| parts.get(i).map_or(Ok(d), |s| s.parse::<f64>().map_err(|_| format!("numero non valido in '{spec}'")));
    let n = num(0, 0.0)? as u32;
    let search = if with_net { IsmctsPlayer::with_net(n, value_net()?, 1.0) } else { IsmctsPlayer::strong(n) };
    Ok(Infer {
        search,
        policy: policy_net()?,
        temp: num(1, 1.0)? as f32,
        pool: num(2, 1000.0)? as usize,
        floor: num(3, 0.0)? as f32,
        game: None,
        log: Log::default(),
        me: None,
        oracle: None,
        exact: None,
    })
}

/// Crea un giocatore da SPEC, anche di una versione rilasciata (`vX.Y.Z/SPEC`).
pub fn make_agent(spec: &str) -> Result<Box<dyn Agent>, String> {
    if let Some((ver, rest)) = spec.split_once('/') {
        return match ver {
            "v0.1.0" | "v0.1.1" => v0_1_1::make(rest),
            "v0.2.0" | "v0.2.1" => v0_2_0::make(rest),
            "v0.3.0" | "v0.4.0" | "v0.4.1" | "v0.4.2" | "v0.4.3" | "v0.4.4" | "v0.4.5" | "v0.4.6" | "v0.4.7" => {
                v0_3_0::make(rest)
            }
            "v0.5.0" => v0_5_0::make(rest),
            _ => Err(format!(
                "versione non disponibile: {ver} (disponibili: {})",
                RELEASED.join(", ")
            )),
        };
    }
    if let Some(rest) = spec.strip_prefix("net:") {
        let mut parts = rest.split(':');
        let num = |s: Option<&str>, d: f64| s.map_or(Ok(d), |s| s.parse::<f64>().map_err(|_| format!("numero non valido in '{spec}'")));
        let n = num(parts.next(), 0.0)? as u32;
        let mix = num(parts.next(), 1.0)?;
        let suffix = if mix < 1.0 { format!(" · rete al {:.0}%", mix * 100.0) } else { String::new() };
        return Ok(Box::new(Current(Box::new(IsmctsPlayer::with_net(n, value_net()?, mix)), suffix)));
    }
    if let Some(rest) = spec.strip_prefix("oracolo:") {
        // oracolo:MOMENTO:SPEC_INFER — diagnosi: la deduzione, ma con la mano avversaria vista nel momento dato.
        let (m, inner) = rest.split_once(':').ok_or(format!("serve oracolo:MOMENTO:infer:..., non '{spec}'"))?;
        let moment = Moment::parse(m).ok_or(format!("momento sconosciuto: {m} (inizio, meta, prefinale, apre, risponde, sempre)"))?;
        let mut a = make_infer(inner.strip_prefix("infer:").ok_or(format!("dopo oracolo:{m}: serve infer:..., non '{inner}'"))?, spec)?;
        a.oracle = Some(moment);
        return Ok(Box::new(a));
    }
    if let Some(rest) = spec.strip_prefix("esatta:") {
        // esatta:ITERAZIONI:TEMPERATURA:MINIMO — come infer:, ma con tutte le mani possibili pesate in modo esatto.
        let p: Vec<&str> = rest.split(':').collect();
        let (n, t, f) = (p.first().copied().unwrap_or("0"), p.get(1).copied().unwrap_or("1"), p.get(2).copied().unwrap_or("0.25"));
        let mut a = make_infer(&format!("{n}:{t}:0:{f}"), spec)?;
        a.exact = Some(Exact::default());
        return Ok(Box::new(a));
    }
    if let Some(rest) = spec.strip_prefix("infer:") {
        return Ok(Box::new(make_infer(rest, spec)?));
    }
    if let Some(n) = spec.strip_prefix("vedetutto:") {
        let n = n.parse::<u32>().map_err(|_| format!("numero non valido in '{spec}'"))?;
        return Ok(Box::new(SeeAll(IsmctsPlayer::strong(n))));
    }
    if let Some(n) = spec.strip_prefix("misto:") {
        let n = n.parse::<u32>().map_err(|_| format!("numero non valido in '{spec}'"))?;
        return Ok(Box::new(Current(Box::new(Mixed(IsmctsPlayer::strong(n))), String::new())));
    }
    if let Some(n) = spec.strip_prefix("peek:") {
        let n = n.parse::<u32>().map_err(|_| format!("numero non valido in '{spec}'"))?;
        return Ok(Box::new(Peek(IsmctsPlayer::strong(n))));
    }
    Ok(Box::new(Current(make_player(spec)?, variant_suffix(spec))))
}

/// Versioni rilasciate con cui si può confrontare il motore (la più recente per ultima).
/// v0.1.0 e v0.1.1 hanno lo stesso motore; dalla v0.2.0 il motore è quello forte (PR #4).
/// v0.2.1 ha lo stesso motore della v0.2.0 (aggiunge solo la registrazione delle partite).
/// v0.3.0: finale risolto con tabella delle trasposizioni, ISMCTS potenziato nell'assistente.
/// Dalla v0.4.0 alla v0.4.7 il motore è quello della v0.3.0 (cambiano inserimento, lettura dello schermo e altri giochi).
/// v0.5.0: nell'assistente Massima deduce le carte avversarie (qui la deduzione è `infer:`; `v0.5.0/` ha i giocatori senza deduzione).
pub const RELEASED: &[&str] = &["v0.1.1", "v0.2.0", "v0.2.1", "v0.3.0", "v0.4.0", "v0.4.1", "v0.4.2", "v0.4.3", "v0.4.4", "v0.4.5", "v0.4.6", "v0.4.7", "v0.5.0"];

/// Genera l'adattatore per una versione rilasciata del crate `briscola-core`.
/// Per aggiungere una versione: dipendenza in Cargo.toml, `legacy!`, voce in `make_agent` e `RELEASED`.
/// Dopo il tag si elencano i giocatori ISMCTS della versione: prefisso della SPEC => costruttore dal numero di iterazioni.
macro_rules! legacy {
    ($module:ident, $krate:ident, $tag:literal, $($prefix:literal => $ismcts:expr),+ $(,)?) => {
        mod $module {
            use super::Agent;
            use briscola_core::cards::Card;
            use briscola_core::Game;
            use rand::rngs::SmallRng;
            use $krate as old;

            pub struct Legacy {
                player: Box<dyn old::ai::Player>,
                game: Option<old::Game>,
            }

            pub fn make(spec: &str) -> Result<Box<dyn Agent>, String> {
                use old::ai::*;
                let n = |s: &str| s.parse::<u32>().map_err(|_| format!("numero non valido in '{spec}'"));
                let player: Box<dyn Player> = match spec {
                    "heuristic" => Box::new(HeuristicPlayer),
                    s if s.starts_with("engine:") => Box::new(EnginePlayer::new(EngineConfig::strength(n(&s[7..])?))),
                    $(s if s.starts_with($prefix) => Box::new(($ismcts)(n(&s[$prefix.len()..])?)),)+
                    _ => {
                        let ismcts: &[&str] = &[$($prefix),+];
                        return Err(format!("{}: disponibili heuristic, engine:N, {}", $tag, ismcts.join("N, ") + "N"));
                    }
                };
                Ok(Box::new(Legacy { player, game: None }))
            }

            impl Agent for Legacy {
                fn name(&self) -> String {
                    format!("{} [{}]", self.player.name(), $tag)
                }
                fn start(&mut self, order: &[Card], first: u8) {
                    self.game = Some(old::Game::from_order(order, first));
                }
                fn choose(&mut self, g: &Game, me: u8, rng: &mut SmallRng) -> Card {
                    let mine = self.game.as_ref().expect("partita non iniziata");
                    debug_assert_eq!(mine.hands[me as usize].0, g.hands[me as usize].0, "copia della partita non allineata");
                    self.player.choose(&old::Observation::of(mine, me), rng)
                }
                fn observe(&mut self, c: Card) {
                    self.game.as_mut().expect("partita non iniziata").play(c);
                }
            }
        }
    };
}

legacy!(v0_1_1, core_v0_1_1, "v0.1.1", "ismcts:" => |i| core_v0_1_1::ai::IsmctsPlayer { iterations: i });
legacy!(v0_2_0, core_v0_2_0, "v0.2.0", "ismcts:" => |i| core_v0_2_0::ai::IsmctsPlayer { iterations: i });
legacy!(
    v0_3_0,
    core_v0_3_0,
    "v0.3.0",
    "ismcts+:" => core_v0_3_0::ai::IsmctsPlayer::strong,
    "ismcts:" => core_v0_3_0::ai::IsmctsPlayer::new,
);
legacy!(
    v0_5_0,
    core_v0_5_0,
    "v0.5.0",
    "ismcts+:" => core_v0_5_0::ai::IsmctsPlayer::strong,
    "ismcts:" => core_v0_5_0::ai::IsmctsPlayer::new,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_parse() {
        for s in ["heuristic", "noisy:0.2:engine:20", "v0.1.1/engine:20", "v0.2.0/engine:20", "v0.2.1/engine:20", "v0.3.0/engine:20", "v0.3.0/ismcts+:20", "v0.3.0/ismcts:20", "v0.4.0/ismcts+:20", "v0.4.1/ismcts+:20", "v0.4.7/ismcts+:20", "v0.5.0/ismcts+:20", "v0.5.0/engine:20", "v0.2.0/ismcts:20", "followsuit", "peek:20", "vedetutto:20", "misto:20", "stile:nonno", "persona:7", "fretta:0.4:counter", "tilt:0.3:stile:circolo", "camaleonte:3", "politica"] {
            assert!(make_agent(s).is_ok(), "{s}");
        }
        assert!(make_agent("v9.9.9/engine:20").is_err());
        assert!(make_agent("oracolo:boh:infer:20:1").is_err());
        assert_eq!(Moment::parse("prefinale"), Some(Moment::Prefinale));
        assert!(make_agent("boh").is_err());
    }
}
