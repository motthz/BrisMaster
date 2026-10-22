//! Misura i tempi di risposta del motore, fase per fase della partita.
//!
//! Uso:
//!   bench [--positions N] [--samples MONDI] [--threads T] [--solve SOGLIA] [--seed X]
//!
//! Senza `--solve` si usa la soglia del motore predefinito.
//!
//! Le posizioni si ottengono giocando partite casuali con l'euristico; per ogni
//! posizione si misura una chiamata a `evaluate` (la stessa usata dall'app per
//! i consigli e per la mossa del computer). L'impronta finale cambia se cambiano
//! le valutazioni: serve a controllare che un'ottimizzazione non alteri il gioco.

use briscola_core::ai::*;
use briscola_core::*;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str, d: &str| -> String {
        args.iter().position(|a| a == k).map(|i| args[i + 1].clone()).unwrap_or(d.to_string())
    };
    let positions: usize = get("--positions", "300").parse().unwrap();
    let samples: u32 = get("--samples", "1000").parse().unwrap();
    let threads: usize = get("--threads", "1").parse().unwrap();
    let seed: u64 = get("--seed", "7").parse().unwrap();

    // Posizioni di prova distribuite su tutta la partita.
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut obs_list = Vec::new();
    while obs_list.len() < positions {
        let first = rng.gen_range(0..2);
        let mut g = Game::new(&mut rng, first);
        let stop = rng.gen_range(0..38);
        for _ in 0..stop {
            let p = g.to_move() as usize;
            let c = heuristic_move(g.hands[p], g.table, g.trump);
            g.play(c);
        }
        if !g.is_over() && g.legal_moves().len() > 1 {
            obs_list.push(Observation::of(&g, g.to_move()));
        }
    }

    let mut cfg = EngineConfig::strength(samples);
    cfg.threads = threads;
    if let Some(i) = args.iter().position(|a| a == "--solve") {
        cfg.solve_deck_len = args[i + 1].parse().unwrap();
    }
    let solved = format!("mazzo 1-{} (alfa-beta)", cfg.solve_deck_len);
    let phases = ["rollout", solved.as_str(), "mazzo vuoto"];
    let mut times: [Vec<f64>; 3] = Default::default();
    let mut print: u64 = 0;
    let mut eval_rng = SmallRng::seed_from_u64(seed ^ 0xABCD);
    let total = Instant::now();
    for obs in &obs_list {
        let t = Instant::now();
        let ev = evaluate(obs, &cfg, &mut eval_rng);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let phase = if obs.deck_len > cfg.solve_deck_len { 0 } else if obs.deck_len > 0 { 1 } else { 2 };
        times[phase].push(ms);
        for e in &ev {
            print = print.wrapping_mul(1_000_003).wrapping_add(e.card as u64);
            print = print.wrapping_mul(1_000_003).wrapping_add((e.exp_points * 1000.0).round() as u64);
        }
    }
    println!("{} mondi, {} thread, {} posizioni", samples, threads, obs_list.len());
    println!("{:<24} {:>6} {:>10} {:>10} {:>10} {:>10}", "fase", "n", "media ms", "mediana", "p95", "max");
    for (name, v) in phases.iter().zip(times.iter_mut()) {
        if v.is_empty() {
            continue;
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let q = |f: f64| v[((v.len() - 1) as f64 * f).round() as usize];
        println!("{:<24} {:>6} {:>10.2} {:>10.2} {:>10.2} {:>10.2}", name, v.len(), mean, q(0.5), q(0.95), q(1.0));
    }
    println!("Tempo totale: {:.2}s", total.elapsed().as_secs_f64());
    println!("Impronta delle valutazioni: {print:016x}");
}
