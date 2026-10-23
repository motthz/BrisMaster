//! Banco di prova di Scala 40: migliaia di partite tra il motore e altri giocatori.

mod agents;
mod arena;

use std::time::Instant;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

/// Taratura a coordinate: ogni peso si moltiplica e divide per un fattore, si tiene la
/// variante che fa più punti contro il gruppo di avversari (stessi mazzi per tutte).
fn tune(args: &[String]) {
    let opponents: Vec<String> =
        arg(args, "--opponents").unwrap_or("basic,pairs,defensive,hoarder".into()).split(';').map(String::from).collect();
    let n: u64 = arg(args, "--matches").and_then(|x| x.parse().ok()).unwrap_or(300);
    let rounds: u32 = arg(args, "--rounds").and_then(|x| x.parse().ok()).unwrap_or(3);
    let mut seed: u64 = arg(args, "--seed").and_then(|x| x.parse().ok()).unwrap_or(100);
    let mut w = agents::weights(&arg(args, "--start").unwrap_or_default()).unwrap();
    let eval = |w: &scala40_core::ai::Weights, seed: u64| -> f64 {
        let spec = format!("engine:{}", agents::weights_spec(w));
        opponents.iter().map(|o| arena::run(&spec, o, n, seed).unwrap().score()).sum::<f64>() / opponents.len() as f64
    };
    let names: Vec<&str> = agents::fields(&mut w.clone()).into_iter().map(|(n, _)| n).collect();
    let mut factor = 1.6f32;
    for r in 0..rounds {
        let mut base = eval(&w, seed);
        println!("giro {r} (seme {seed}): {:.2}% {}", base * 100.0, agents::weights_spec(&w));
        for name in &names {
            for f in [factor, 1.0 / factor, 0.0] {
                let mut t = w;
                {
                    let mut fs = agents::fields(&mut t);
                    let slot = fs.iter_mut().find(|(n, _)| n == name).unwrap();
                    if f == 0.0 {
                        if *slot.1 == 0.0 {
                            continue;
                        }
                        *slot.1 = 0.0;
                    } else if *slot.1 == 0.0 {
                        *slot.1 = 0.5 * f;
                    } else {
                        *slot.1 *= f;
                    }
                }
                let v = eval(&t, seed);
                if v > base + 0.002 {
                    println!("  {name} x{f:.2}: {:.2}% (+{:.2})", v * 100.0, (v - base) * 100.0);
                    base = v;
                    w = t;
                    break;
                }
            }
        }
        factor = 1.0 + (factor - 1.0) * 0.7;
        seed += 1;
    }
    println!("finale: {}", agents::weights_spec(&w));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("tune") {
        tune(&args);
        return;
    }
    let player = arg(&args, "--player").unwrap_or("engine".into());
    let opponent = arg(&args, "--opponent").unwrap_or("basic".into());
    let n: u64 = arg(&args, "--matches").and_then(|x| x.parse().ok()).unwrap_or(200);
    let seed: u64 = arg(&args, "--seed").and_then(|x| x.parse().ok()).unwrap_or(1);
    let t = Instant::now();
    let s = arena::run(&player, &opponent, n, seed).unwrap();
    let (lo, hi) = s.score_ci95();
    println!(
        "{player} vs {opponent}: {:.1}% [{:.1}, {:.1}] partite {} (V{} P{} S{}), smazzate vinte {:.1}% su {}, penalità {:.1} vs {:.1}, chiuse senza aprire {:.1}% vs {:.1}%, {:.1}s",
        s.score() * 100.0,
        lo * 100.0,
        hi * 100.0,
        s.games(),
        s.wins,
        s.draws,
        s.losses,
        s.deal_rate() * 100.0,
        s.deals,
        s.pen_for as f64 / s.deals as f64,
        s.pen_against as f64 / s.deals as f64,
        s.unopened_for as f64 * 100.0 / s.deals as f64,
        s.unopened_against as f64 * 100.0 / s.deals as f64,
        t.elapsed().as_secs_f64()
    );
}
