//! Banco di prova: migliaia di partite tra il motore e avversari di riferimento.
//!
//! Ogni mazzo viene giocato due volte a posti invertiti ("duplicato"),
//! così la fortuna delle carte si compensa e il confronto è più preciso.
//!
//! Uso:
//!   briscola-sim [--deals N] [--player SPEC] [--opponent SPEC] [--seed X]
//!   briscola-sim --panel [--deals N] [--player SPEC] [--out FILE]
//!
//! SPEC: random | heuristic | aggressive | cautious | trumphappy | counter
//!       | engine:MONDI[:EPSILON[:SOGLIA[:PESO_PUNTI]]] | ismcts:ITERAZIONI

use briscola_core::ai::*;
use briscola_core::stats::{wilson95, Tally};
use briscola_core::*;
use rand::rngs::SmallRng;
use rand::SeedableRng;
use rayon::prelude::*;
use std::fmt::Write as _;
use std::time::Instant;

fn make_player(spec: &str) -> Box<dyn Player> {
    match spec {
        "random" => Box::new(RandomPlayer),
        "heuristic" => Box::new(HeuristicPlayer),
        "aggressive" => Box::new(AggressivePlayer),
        "cautious" => Box::new(CautiousPlayer),
        "trumphappy" => Box::new(TrumpHappyPlayer),
        "counter" => Box::new(CounterPlayer),
        s if s.starts_with("ismcts:") => {
            Box::new(IsmctsPlayer { iterations: s[7..].parse().expect("iterazioni") })
        }
        s if s.starts_with("engine:") => {
            let parts: Vec<&str> = s[7..].split(':').collect();
            let mut cfg = EngineConfig::strength(parts[0].parse().expect("numero di mondi"));
            if let Some(e) = parts.get(1) {
                cfg.rollout_epsilon = e.parse().expect("epsilon");
            }
            if let Some(d) = parts.get(2) {
                cfg.solve_deck_len = d.parse().expect("soglia risolutore");
            }
            if let Some(w) = parts.get(3) {
                cfg.points_weight = w.parse().expect("peso dei punti");
            }
            Box::new(EnginePlayer::new(cfg))
        }
        other => panic!("giocatore sconosciuto: {other}"),
    }
}

fn run_match(player: &str, opponent: &str, deals: u64, seed: u64) -> Tally {
    (0..deals)
        .into_par_iter()
        .map(|d| {
            let mut tally = Tally::default();
            let mut deal_rng = SmallRng::seed_from_u64(seed ^ d.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let base = Game::new(&mut deal_rng, 0);
            // Stesso mazzo, posti scambiati: il giocatore siede prima al posto 0 e poi al posto 1.
            for seat in 0..2u8 {
                let mut me = make_player(player);
                let mut opp = make_player(opponent);
                let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(d * 2 + seat as u64));
                let pts = if seat == 0 {
                    play_game(base, &mut [me.as_mut(), opp.as_mut()], &mut rng)
                } else {
                    play_game(base, &mut [opp.as_mut(), me.as_mut()], &mut rng)
                };
                tally.add(pts[seat as usize]);
            }
            tally
        })
        .reduce(Tally::default, |mut a, b| {
            a.merge(&b);
            a
        })
}

fn describe(spec: &str) -> String {
    make_player(spec).name()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str, d: &str| -> String {
        args.iter().position(|a| a == k).map(|i| args[i + 1].clone()).unwrap_or(d.to_string())
    };
    let deals: u64 = get("--deals", "5000").parse().unwrap();
    let seed: u64 = get("--seed", "2026").parse().unwrap();
    let player = get("--player", "engine:1000");

    if args.iter().any(|a| a == "--panel") {
        let opponents = [
            "random", "heuristic", "aggressive", "cautious", "trumphappy", "counter",
            "ismcts:2000", "engine:150",
        ];
        let mut md = String::new();
        writeln!(md, "| Avversario | Partite | Vinte | Pari | Perse | % vittorie | IC 95% | Punti medi |").unwrap();
        writeln!(md, "|---|---:|---:|---:|---:|---:|---|---:|").unwrap();
        println!("{} contro il pannello di avversari, {} partite ciascuno", describe(&player), deals * 2);
        for opp in opponents {
            let start = Instant::now();
            let t = run_match(&player, opp, deals, seed);
            let (lo, hi) = wilson95(t.wins as f64, t.games() as f64);
            let row = format!(
                "| {} | {} | {} | {} | {} | **{:.1}%** | {:.1}% – {:.1}% | {:.1} |",
                describe(opp), t.games(), t.wins, t.draws, t.losses,
                t.win_rate() * 100.0, lo * 100.0, hi * 100.0, t.avg_points()
            );
            println!("{row}   ({:.0}s)", start.elapsed().as_secs_f64());
            writeln!(md, "{row}").unwrap();
        }
        if let Some(i) = args.iter().position(|a| a == "--out") {
            std::fs::write(&args[i + 1], md).expect("scrittura risultati");
        }
        return;
    }

    let opponent = get("--opponent", "heuristic");
    let start = Instant::now();
    let total = run_match(&player, &opponent, deals, seed);
    let n = total.games() as f64;
    let (lo, hi) = wilson95(total.wins as f64, n);
    let decided = (total.wins + total.losses) as f64;
    let (dlo, dhi) = wilson95(total.wins as f64, decided);
    println!("{} contro {}", describe(&player), describe(&opponent));
    println!("Partite: {} ({} mazzi in duplicato), seme {seed}", total.games(), deals);
    println!("Vittorie {} | Pareggi {} | Sconfitte {}", total.wins, total.draws, total.losses);
    println!(
        "Percentuale vittorie: {:.2}%  (IC 95%: {:.2}% - {:.2}%)",
        total.win_rate() * 100.0, lo * 100.0, hi * 100.0
    );
    println!(
        "Vittorie sulle partite decise: {:.2}%  (IC 95%: {:.2}% - {:.2}%)",
        total.decisive_win_rate() * 100.0, dlo * 100.0, dhi * 100.0
    );
    println!("Punti medi del giocatore: {:.2} / 120", total.avg_points());
    println!("Tempo: {:.1}s", start.elapsed().as_secs_f64());
}
