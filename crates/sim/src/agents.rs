//! Chi siede al tavolo: i giocatori della versione attuale e quelli delle versioni rilasciate.
//!
//! SPEC: random | heuristic | aggressive | cautious | trumphappy | counter
//!       | followsuit | loadleader | notrump
//!       | engine:MONDI[:EPSILON[:SOGLIA[:PESO_PUNTI]]] | ismcts:ITERAZIONI
//!       | classic:MONDI                (motore della v0.1.1 compilato con la versione attuale)
//!       | eng:CHIAVE=VALORE,...        (varianti del motore attuale, vedi make_player)
//!       | noisy:ERRORE:SPEC            (es. noisy:0.25:heuristic)
//!       | vX.Y.Z/SPEC                  (versione rilasciata, es. v0.1.1/engine:1000)

use crate::opponents::*;
use briscola_core::ai::*;
use briscola_core::cards::Card;
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
        s if s.starts_with("ismcts:") => {
            Box::new(IsmctsPlayer { iterations: num(&s[7..], "numero di iterazioni")? as u32 })
        }
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
        // eng:mondi=1000,eps=0,solve=8,rsolve=2,pol=open,tb=8,tm=2,lb=0,os=4 (rsolve=no, pol=heuristic)
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
                    other => return Err(format!("opzione sconosciuta: {other}")),
                }
            }
            Box::new(EnginePlayer::new(cfg))
        }
        s if s.starts_with("noisy:") => {
            let (err, base) = s[6..].split_once(':').ok_or(format!("serve noisy:ERRORE:SPEC, non '{s}'"))?;
            Box::new(Noisy { base: make_player(base)?, error: num(err, "errore")? })
        }
        other => return Err(format!("giocatore sconosciuto: {other}")),
    })
}

/// Crea un giocatore da SPEC, anche di una versione rilasciata (`vX.Y.Z/SPEC`).
pub fn make_agent(spec: &str) -> Result<Box<dyn Agent>, String> {
    if let Some((ver, rest)) = spec.split_once('/') {
        return match ver {
            "v0.1.0" | "v0.1.1" => v0_1_1::make(rest),
            "v0.2.0" | "v0.2.1" => v0_2_0::make(rest),
            _ => Err(format!(
                "versione non disponibile: {ver} (disponibili: {})",
                RELEASED.join(", ")
            )),
        };
    }
    Ok(Box::new(Current(make_player(spec)?, variant_suffix(spec))))
}

/// Versioni rilasciate con cui si può confrontare il motore (la più recente per ultima).
/// v0.1.0 e v0.1.1 hanno lo stesso motore; dalla v0.2.0 il motore è quello forte (PR #4).
/// v0.2.1 ha lo stesso motore della v0.2.0 (aggiunge solo la registrazione delle partite).
pub const RELEASED: &[&str] = &["v0.1.1", "v0.2.0", "v0.2.1"];

/// Genera l'adattatore per una versione rilasciata del crate `briscola-core`.
/// Per aggiungere una versione: dipendenza in Cargo.toml, `legacy!`, voce in `make_agent` e `RELEASED`.
macro_rules! legacy {
    ($module:ident, $krate:ident, $tag:literal) => {
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
                    s if s.starts_with("ismcts:") => Box::new(IsmctsPlayer { iterations: n(&s[7..])? }),
                    _ => return Err(format!("{}: disponibili heuristic, engine:N, ismcts:N", $tag)),
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

legacy!(v0_1_1, core_v0_1_1, "v0.1.1");
legacy!(v0_2_0, core_v0_2_0, "v0.2.0");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_parse() {
        for s in ["heuristic", "noisy:0.2:engine:20", "v0.1.1/engine:20", "v0.2.0/engine:20", "v0.2.1/engine:20", "followsuit"] {
            assert!(make_agent(s).is_ok(), "{s}");
        }
        assert!(make_agent("v9.9.9/engine:20").is_err());
        assert!(make_agent("boh").is_err());
    }
}
