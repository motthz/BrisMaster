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
