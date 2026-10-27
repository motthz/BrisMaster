//! Genera i dati di allenamento della rete di valutazione: partite tra motori (self-play),
//! e per ogni posizione con il mazzo ancora lungo l'esito finale per chi doveva muovere.
//!
//! Uso:
//!   selfplay --out CARTELLA [--games N] [--player SPEC] [--seed X] [--chunk N] [--net FILE]
//!
//! SPEC: ismcts+:ITERAZIONI | engine:MONDI | net:ITERAZIONI (ISMCTS con la rete di --net).
//! I dati vanno a blocchi di `--chunk` partite (file `blocco_INIZIO.bin`): i blocchi già
//! scritti si saltano, così una generazione interrotta riparte da dove era arrivata.
//! Ogni record: posizione (`Position::BYTES`), esito in mezzi punti (0, 1, 2), punti finali.
//! Accanto (`politica_INIZIO.pol`) le decisioni per la rete di politica: ciò che vedeva chi
//! muoveva (`Position::of_obs`) e la carta scelta (indice con i semi rinumerati).

use briscola_core::ai::*;
use briscola_core::net::{canon_index, Net, Position};
use briscola_core::*;
use rand::rngs::SmallRng;
use rand::SeedableRng;
use rayon::prelude::*;
use std::sync::Arc;

pub const RECORD: usize = Position::BYTES + 2;

fn make(spec: &str, net: &Option<Arc<Net>>) -> Box<dyn Player> {
    let (kind, n) = spec.split_once(':').expect("SPEC: tipo:numero");
    let n: u32 = n.parse().expect("numero nella SPEC");
    match kind {
        "ismcts+" => Box::new(IsmctsPlayer::strong(n)),
        "engine" => Box::new(EnginePlayer::new(EngineConfig::strength(n))),
        "net" => Box::new(IsmctsPlayer::with_net(n, net.clone().expect("serve --net"), 1.0)),
        _ => panic!("giocatore sconosciuto: {spec}"),
    }
}

fn game_records(spec: &str, net: &Option<Arc<Net>>, seed: u64) -> (Vec<u8>, Vec<u8>) {
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut players = [make(spec, net), make(spec, net)];
    let mut g = Game::new(&mut rng, (seed % 2) as u8);
    let mut seen: Vec<(Position, u8)> = Vec::new();
    let mut decisions = Vec::new();
    while !g.is_over() {
        let p = g.to_move();
        if g.deck_len > 2 {
            seen.push((Position::of(&g), p));
        }
        let obs = Observation::of(&g, p);
        let c = players[p as usize].choose(&obs, &mut rng);
        if obs.hand.len() > 1 {
            Position::of_obs(&obs).write(&mut decisions);
            decisions.push(canon_index(c, g.trump) as u8);
        }
        g.play(c);
    }
    let mut out = Vec::with_capacity(seen.len() * RECORD);
    for (pos, p) in seen {
        pos.write(&mut out);
        let pts = g.points[p as usize];
        out.push(if pts > 60 { 2 } else if pts == 60 { 1 } else { 0 });
        out.push(pts);
    }
    (out, decisions)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str, d: &str| -> String {
        args.iter().position(|a| a == k).map(|i| args[i + 1].clone()).unwrap_or(d.to_string())
    };
    let out = get("--out", "");
    assert!(!out.is_empty(), "serve --out CARTELLA");
    let games: u64 = get("--games", "10000").parse().unwrap();
    let chunk: u64 = get("--chunk", "500").parse().unwrap();
    let seed: u64 = get("--seed", "1").parse().unwrap();
    let spec = get("--player", "ismcts+:2000");
    let net = args.iter().position(|a| a == "--net").map(|i| {
        Arc::new(Net::from_bytes(&std::fs::read(&args[i + 1]).expect("lettura rete")).expect("rete"))
    });
    std::fs::create_dir_all(&out).unwrap();
    let start = std::time::Instant::now();
    for from in (0..games).step_by(chunk as usize) {
        let path = std::path::Path::new(&out).join(format!("blocco_{from:08}.bin"));
        if path.exists() {
            continue;
        }
        let to = (from + chunk).min(games);
        let data: Vec<(Vec<u8>, Vec<u8>)> =
            (from..to).into_par_iter().map(|i| game_records(&spec, &net, seed.wrapping_mul(1_000_003) + i)).collect();
        let (values, decisions): (Vec<Vec<u8>>, Vec<Vec<u8>>) = data.into_iter().unzip();
        std::fs::write(std::path::Path::new(&out).join(format!("politica_{from:08}.pol")), decisions.concat()).unwrap();
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, values.concat()).unwrap();
        std::fs::rename(&tmp, &path).unwrap();
        eprintln!("{to}/{games} partite ({:.0}s)", start.elapsed().as_secs_f64());
    }
}
