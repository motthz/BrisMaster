//! Giocatori per il banco di prova, descritti da una stringa (SPEC).
//!
//! - `random`: scarta a caso
//! - `basic`: regole semplici (cala appena può, scarta la spaiata più alta)
//! - `engine`: il motore
//! - `noisy:EPS:SPEC`: SPEC che sbaglia una mossa su 1/EPS

use scala40_core::ai::{BasicPlayer, Defensive, EnginePlayer, Hoarder, Noisy, PairKeeper, Player, RandomPlayer, Weights};

pub fn make_agent(spec: &str) -> Result<Box<dyn Player>, String> {
    let parts: Vec<&str> = spec.split(':').collect();
    Ok(match parts[0] {
        "random" => Box::new(RandomPlayer),
        "basic" => Box::new(BasicPlayer),
        "pairs" => Box::new(PairKeeper),
        "defensive" => Box::new(Defensive),
        "hoarder" => Box::new(Hoarder { panic_at: parts.get(1).and_then(|x| x.parse().ok()).unwrap_or(5) }),
        "engine" => Box::new(EnginePlayer::new(weights(parts.get(1).copied().unwrap_or(""))?)),
        "noisy" => {
            let eps: f64 = parts.get(1).and_then(|x| x.parse().ok()).ok_or("noisy:EPS:SPEC")?;
            let inner = make_agent(&parts[2..].join(":"))?;
            Box::new(Noisy { eps, inner })
        }
        _ => return Err(format!("giocatore sconosciuto: {spec}")),
    })
}

/// I pesi tarabili del motore, per nome.
pub fn fields(w: &mut Weights) -> Vec<(&'static str, &mut f32)> {
    vec![
        ("single", &mut w.single),
        ("near", &mut w.single_near),
        ("pair", &mut w.pair),
        ("outs", &mut w.outs),
        ("cap", &mut w.outs_cap),
        ("joker", &mut w.joker),
        ("points", &mut w.points),
        ("threat", &mut w.threat),
        ("umeld", &mut w.unopened_meld),
        ("open", &mut w.open_value),
        ("attachable", &mut w.attachable),
        ("dattach", &mut w.danger_attach),
        ("dpartner", &mut w.danger_partner),
        ("safe", &mut w.safe),
    ]
}

/// Pesi del motore da una lista `nome=valore,...` (per le prove di taratura).
pub fn weights(list: &str) -> Result<Weights, String> {
    let mut w = Weights::default();
    for kv in list.split(',').filter(|x| !x.is_empty()) {
        let (k, v) = kv.split_once('=').ok_or_else(|| format!("peso senza valore: {kv}"))?;
        let v: f32 = v.parse().map_err(|_| format!("valore non valido: {kv}"))?;
        if k == "samples" {
            w.take_samples = v as u32;
            continue;
        }
        if k == "margin" {
            w.take_margin = v;
            continue;
        }
        let mut f = fields(&mut w);
        let slot = f.iter_mut().find(|(n, _)| *n == k).ok_or_else(|| format!("peso sconosciuto: {k}"))?;
        *slot.1 = v;
    }
    Ok(w)
}

/// I pesi come lista `nome=valore,...`.
pub fn weights_spec(w: &Weights) -> String {
    let mut w = *w;
    fields(&mut w).into_iter().map(|(n, v)| format!("{n}={}", *v)).collect::<Vec<_>>().join(",")
}
