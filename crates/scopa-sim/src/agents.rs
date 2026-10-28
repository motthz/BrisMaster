//! Giocatori di scopa per il banco di prova, scelti con una stringa SPEC:
//!
//! - `random`, `greedy`, `heuristic`, `scope`, `primiera`, `denari`, `counter`: stili umani
//! - `noisy:ERRORE:SPEC`: SPEC che sbaglia (mossa a caso) con probabilità ERRORE
//! - `ismcts:N`: ricerca ad albero Monte Carlo con N iterazioni
//! - `engine:N`: il motore con N mondi; `engine:N:EPS` con mosse casuali nei rollout;
//!   `engine:N:EPS:diff` gioca per la differenza punti della smazzata invece che per la partita

use scopa_core::ai::*;

fn num(s: &str, what: &str) -> Result<f64, String> {
    s.parse().map_err(|_| format!("{what} non valido: '{s}'"))
}

pub fn make_player(spec: &str) -> Result<Box<dyn Player>, String> {
    Ok(match spec {
        "random" => Box::new(RandomPlayer),
        "greedy" => Box::new(StylePlayer::greedy()),
        "heuristic" => Box::new(StylePlayer::heuristic()),
        "scope" => Box::new(StylePlayer::scope_hunter()),
        "primiera" => Box::new(StylePlayer::primiera()),
        "denari" => Box::new(StylePlayer::denari()),
        "counter" => Box::new(StylePlayer::counter()),
        s if s.starts_with("ismcts:") => Box::new(IsmctsPlayer { iterations: num(&s[7..], "iterazioni")? as u32 }),
        s if s.starts_with("engine:") => {
            let parts: Vec<&str> = s[7..].split(':').collect();
            let mut cfg = EngineConfig::strength(num(parts[0], "numero di mondi")? as u32);
            if let Some(e) = parts.get(1) {
                cfg.rollout_epsilon = num(e, "epsilon")?;
            }
            if let Some(&m) = parts.get(2) {
                match m {
                    "diff" => cfg.match_aware = false,
                    "nosolve" => cfg.solve_endgame = false,
                    "greedy" => cfg.rollout = Rollout::Greedy,
                    "counted" => cfg.rollout = Rollout::Counted,
                    "diffgreedy" => (cfg.match_aware, cfg.rollout) = (false, Rollout::Greedy),
                    "diffcounted" => (cfg.match_aware, cfg.rollout) = (false, Rollout::Counted),
                    "infer" | "-" => {}
                    "ctx" => cfg.ctx_policy = true,
                    "ctxinfer" => cfg.ctx_policy = true,
                    _ => return Err(format!("variante sconosciuta: {m}")),
                }
                if m == "infer" || m == "ctxinfer" {
                    cfg.infer = Some(Infer::default());
                }
            }
            // Opzioni chiave=valore dopo la variante: beta=, ieps=, cand= (attivano la deduzione)
            for kv in parts.iter().skip(3) {
                let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                if k == "sd" {
                    cfg.solve_deck = num(v, "sd")? as u8;
                    continue;
                }
                let inf = cfg.infer.get_or_insert_with(Infer::default);
                match k {
                    "beta" => inf.beta = num(v, "beta")?,
                    "ieps" => inf.eps = num(v, "ieps")?,
                    "cand" => inf.candidates = num(v, "cand")? as u32,
                    _ => return Err(format!("opzione sconosciuta: {k}")),
                }
            }
            Box::new(EnginePlayer::new(cfg))
        }
        // Motore ad albero: tree:ITERAZIONI[,chiave=valore...] (c=esplorazione, pol=open|greedy|counted, eps=, diff, nosolve)
        s if s.starts_with("tree:") => {
            let mut parts = s[5..].split(',');
            let mut cfg = EngineConfig::strength(num(parts.next().unwrap(), "iterazioni")? as u32);
            cfg.tree = true;
            for kv in parts {
                let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                match k {
                    "c" => cfg.exploration = num(v, "esplorazione")?,
                    "eps" => cfg.rollout_epsilon = num(v, "eps")?,
                    "pol" => {
                        cfg.rollout = match v {
                            "open" => Rollout::OpenHand,
                            "greedy" => Rollout::Greedy,
                            "counted" => Rollout::Counted,
                            _ => return Err(format!("politica sconosciuta: {v}")),
                        }
                    }
                    "diff" => cfg.match_aware = false,
                    "nosolve" => cfg.solve_endgame = false,
                    "infer" => cfg.infer = Some(Infer::default()),
                    _ => return Err(format!("opzione sconosciuta: {k}")),
                }
            }
            Box::new(EnginePlayer::new(cfg))
        }
        s if s.starts_with("noisy:") => {
            let (err, base) = s[6..].split_once(':').ok_or(format!("serve noisy:ERRORE:SPEC, non '{s}'"))?;
            Box::new(NoisyPlayer { eps: num(err, "errore")?, inner: make_player(base)? })
        }
        other => return Err(format!("giocatore sconosciuto: {other}")),
    })
}

/// Nome leggibile, con le varianti del motore.
pub fn describe(spec: &str) -> String {
    match make_player(spec) {
        Ok(p) => {
            let mut n = p.name();
            if let Some(rest) = spec.strip_prefix("engine:") {
                let parts: Vec<&str> = rest.split(':').collect();
                if let Some(e) = parts.get(1).filter(|e| e.parse::<f64>().unwrap_or(0.0) > 0.0) {
                    n += &format!(" · rollout con {:.0}% di mosse casuali", e.parse::<f64>().unwrap_or(0.0) * 100.0);
                }
                match parts.get(2) {
                    Some(&"diff") => n += " · gioca per la differenza punti",
                    Some(&"nosolve") => n += " · senza finale esatto",
                    _ => {}
                }
            }
            n
        }
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_parse() {
        for s in ["random", "counter", "noisy:0.2:engine:20", "engine:50:0.1:diff", "ismcts:100"] {
            assert!(make_player(s).is_ok(), "{s}");
        }
        assert!(make_player("boh").is_err());
    }
}
