//! Giocatore `rl:FILE[:TEMP]`: la rete auto-allenata in self-play (formato BRL1, vedi
//! `briscola_core::rl`), da sola, senza ricerca. TEMP 0 (predefinito) = sempre la carta
//! migliore secondo la rete; TEMP 1 = pesca come in allenamento.

use crate::agents::Agent;
use briscola_core::cards::Card;
use briscola_core::rl::RlNet;
use briscola_core::{Game, Observation};
use rand::rngs::SmallRng;
use rand::Rng;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub struct RlAgent {
    net: Arc<RlNet>,
    file: String,
    temp: f32,
    game: Option<Game>,
    hist: Vec<(u8, Card)>,
}

fn load(path: &str) -> Result<Arc<RlNet>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<RlNet>>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().unwrap();
    if let Some(n) = c.get(path) {
        return Ok(n.clone());
    }
    let b = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let n = Arc::new(RlNet::from_bytes(&b)?);
    c.insert(path.to_string(), n.clone());
    Ok(n)
}

/// Da `FILE[:TEMP]`.
pub fn make(rest: &str) -> Result<Box<dyn Agent>, String> {
    let (file, temp) = match rest.rsplit_once(':') {
        Some((f, t)) if t.parse::<f32>().is_ok() => (f, t.parse().unwrap()),
        _ => (rest, 0.0),
    };
    Ok(Box::new(RlAgent { net: load(file)?, file: file.to_string(), temp, game: None, hist: Vec::new() }))
}

impl Agent for RlAgent {
    fn name(&self) -> String {
        let f = std::path::Path::new(&self.file).file_name().map_or(self.file.clone(), |s| s.to_string_lossy().into());
        format!("Rete auto-allenata {f} (temperatura {})", self.temp)
    }
    fn start(&mut self, order: &[Card], first: u8) {
        self.game = Some(Game::from_order(order, first));
        self.hist.clear();
    }
    fn observe(&mut self, c: Card) {
        if let Some(g) = self.game.as_mut() {
            self.hist.push((g.to_move(), c));
            g.play(c);
        }
    }
    fn choose(&mut self, g: &Game, me: u8, rng: &mut SmallRng) -> Card {
        let obs = Observation::of(g, me);
        if obs.hand.len() == 1 {
            return obs.hand.iter().next().unwrap();
        }
        let p = self.net.policy(&obs, &self.hist, self.temp);
        let mut x: f32 = rng.gen();
        for &(c, w) in &p {
            if x < w {
                return c;
            }
            x -= w;
        }
        p.last().unwrap().0
    }
}

// ---------------------------------------------------------------------------
// Ricerca sopra la rete: `rlcerca:FILE:MONDI[:PROFONDITA]`.
//
// Per ogni mondo (mano avversaria pescata dalle mani pesate della deduzione, come Massima)
// si prova ogni carta della mano e si gioca il resto con la rete per ENTRAMBI i giocatori,
// ciascuno con ciò che vede lui (niente simulazioni a carte scoperte). Dopo PROFONDITA mosse
// (0 = fino in fondo) si usa la stima della rete. Vince la carta con l'esito medio migliore.

use briscola_core::infer::{Belief, Log};
use briscola_core::net::Net;
use briscola_core::rl::{canon_card, encode, RL_IN};

pub struct RlSearch {
    net: Arc<RlNet>,
    file: String,
    policy: Arc<Net>,
    worlds: usize,
    depth: usize,
    game: Option<Game>,
    hist: Vec<(u8, Card)>,
    log: Log,
    me: Option<u8>,
}

/// Da `FILE:MONDI[:PROFONDITA]`.
pub fn make_search(rest: &str, policy: Arc<Net>) -> Result<Box<dyn Agent>, String> {
    let num = |t: &str| t.parse::<usize>().ok();
    let p: Vec<&str> = rest.rsplitn(3, ':').collect();
    let (file, worlds, depth) = match p.as_slice() {
        [d, w, f] if num(d).is_some() && num(w).is_some() => (*f, num(w).unwrap(), num(d).unwrap()),
        _ => match rest.rsplit_once(':') {
            Some((f, w)) if num(w).is_some() => (f, num(w).unwrap(), 0),
            _ => return Err("rlcerca:FILE:MONDI[:PROFONDITA]".into()),
        },
    };
    Ok(Box::new(RlSearch {
        net: load(file)?,
        file: file.to_string(),
        policy,
        worlds,
        depth,
        game: None,
        hist: Vec::new(),
        log: Log::default(),
        me: None,
    }))
}

impl RlSearch {
    /// Esito per `me` giocando il resto di `g` con la rete da entrambe le parti.
    fn rollout(&self, mut g: Game, mut hist: Vec<(u8, Card)>, me: u8, x: &mut [u8; RL_IN]) -> f32 {
        let mut plies = 0;
        while !g.is_over() {
            let p = g.to_move();
            let o = Observation::of(&g, p);
            encode(&o, &hist, x);
            let out = self.net.forward(x);
            if self.depth > 0 && plies >= self.depth {
                return if p == me { out.value } else { -out.value };
            }
            let mut best = (f32::MIN, 0);
            for c in o.hand.iter() {
                let z = out.logits[canon_card(c, g.trump)];
                if z > best.0 {
                    best = (z, c);
                }
            }
            hist.push((p, best.1));
            g.play(best.1);
            plies += 1;
        }
        let d = g.points[me as usize] as f32 - g.points[1 - me as usize] as f32;
        d.signum() + 0.01 * d / 120.0
    }
}

impl Agent for RlSearch {
    fn name(&self) -> String {
        let f = std::path::Path::new(&self.file).file_name().map_or(self.file.clone(), |s| s.to_string_lossy().into());
        let d = if self.depth > 0 { format!(", stima dopo {} mosse", self.depth) } else { String::new() };
        format!("Rete auto-allenata {f} con ricerca ({} mondi{d})", self.worlds)
    }
    fn start(&mut self, order: &[Card], first: u8) {
        self.game = Some(Game::from_order(order, first));
        self.hist.clear();
        self.log = Log::default();
        self.me = None;
    }
    fn observe(&mut self, c: Card) {
        if let Some(g) = self.game.as_mut() {
            let me = self.me.unwrap_or(1 - g.to_move());
            self.log.before_play(g, me, c);
            self.hist.push((g.to_move(), c));
            g.play(c);
        }
    }
    fn choose(&mut self, g: &Game, me: u8, rng: &mut SmallRng) -> Card {
        self.me = Some(me);
        let obs = Observation::of(g, me);
        let cards: Vec<Card> = obs.hand.iter().collect();
        if cards.len() == 1 {
            return cards[0];
        }
        let belief = Belief::infer(&obs, &self.log.events, &self.policy, 1.0, 0.25, 1000, rng);
        let mut score = vec![0f32; cards.len()];
        let mut x = [0u8; RL_IN];
        for _ in 0..self.worlds {
            let w = if std::env::var_os("BRISCOLA_RL_BARO").is_some() { *g } else { obs.determinize_given(belief.sample(rng), rng) };
            for (k, &c) in cards.iter().enumerate() {
                let mut g2 = w;
                let mut h2 = self.hist.clone();
                h2.push((me, c));
                g2.play(c);
                score[k] += self.rollout(g2, h2, me, &mut x);
            }
        }
        // Prior della rete: log-probabilità della carta secondo la politica, con peso BETA.
        let beta: f32 = std::env::var("BRISCOLA_RL_BETA").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        if beta > 0.0 {
            let pol = self.net.policy(&obs, &self.hist, 1.0);
            for (k, (_, pr)) in pol.iter().enumerate() {
                score[k] = score[k] / self.worlds as f32 + beta * pr.max(1e-6).ln();
            }
        }
        let k = (0..cards.len()).max_by(|&a, &b| score[a].partial_cmp(&score[b]).unwrap()).unwrap();
        cards[k]
    }
}
