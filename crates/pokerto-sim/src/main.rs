//! Banco di prova del poker all'italiana: allenamento del motore e migliaia di mani in duplicato.
//!
//! Uso:
//!   pokerto-sim train [--iters N] [--seed X] [--out FILE]
//!   pokerto-sim match --player SPEC --opponent SPEC [--deals N] [--seed X]
//!   pokerto-sim report [--deals N] [--self-deals N] [--out FILE]
//!   pokerto-sim calibrate [--deals N]
//!   pokerto-sim quantiles
//!
//! SPEC: engine, uno stile a regole (random, station, maniac, rock, bar, tag, lag, passive,
//! bluffer), noisy:P:SPEC, cfr:N (motore allenato al momento con N iterazioni),
//! exploit:N (allenato con N iterazioni per sfruttare il motore).

mod arena;

use arena::{run_match, MatchStats};
use pokerto::cfr::{pre_thresholds, Trainer};
use pokerto::{bots, cards, draw};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use rayon::prelude::*;
use std::fmt::Write as _;
use std::process::ExitCode;
use std::time::Instant;

fn arg<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(|v| v.parse().unwrap_or_else(|_| panic!("valore non valido per {name}: {v}")))
        .unwrap_or(default)
}

fn fmt_num(x: f64) -> String {
    format!("{x:+.3}").replace('.', ",")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    match cmd {
        "quantiles" => quantiles(),
        "train" => train(&args),
        "match" => {
            let player: String = arg(&args, "--player", "engine".to_string());
            let opponent: String = arg(&args, "--opponent", "tag".to_string());
            let deals: u64 = arg(&args, "--deals", 8000);
            let seed: u64 = arg(&args, "--seed", 1);
            let t = Instant::now();
            match run_match(&player, &opponent, deals, seed) {
                Ok(s) => {
                    println!("{player} contro {opponent}: {}", s.summary());
                    println!("({:.1}s)", t.elapsed().as_secs_f64());
                }
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        "report" => return report(&args),
        "calibrate" => calibrate(&args),
        _ => {
            eprintln!("comandi: train, match, report, calibrate, quantiles (vedi l'inizio di main.rs)");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn quantiles() {
    let mut v: Vec<f32> = cards::all_hands().par_iter().map(|h| draw::summary(h).max_ev()).collect();
    v.sort_unstable_by(f32::total_cmp);
    let q: Vec<String> = (0..=100).map(|i| format!("{:.4}", v[(i * (v.len() - 1)) / 100])).collect();
    println!("const MAX_EV_QUANTILES: [f32; 101] = [{}];", q.join(", "));
}

fn train(args: &[String]) {
    let iters: u64 = arg(args, "--iters", 20_000_000);
    let seed: u64 = arg(args, "--seed", 20261023);
    let out: String = arg(args, "--out", "crates/pokerto/data/strategy.bin".to_string());
    let t = Instant::now();
    let thresholds = pre_thresholds();
    eprintln!("analisi del cambio di tutte le mani: {:.1}s", t.elapsed().as_secs_f64());
    let mut tr = Trainer::new(thresholds);
    let mut rng = SmallRng::seed_from_u64(seed);
    let chunk = (iters / 20).max(1);
    while tr.iterations < iters {
        tr.run(chunk.min(iters - tr.iterations), &mut rng);
        eprintln!("{} iterazioni, {} situazioni, {:.0}s", tr.iterations, tr.nodes(), t.elapsed().as_secs_f64());
    }
    let bytes = tr.strategy().to_bytes();
    std::fs::write(&out, &bytes).expect("scrittura strategia");
    eprintln!("salvata in {out} ({} KB)", bytes.len() / 1024);
}

fn calibrate(args: &[String]) {
    let deals: u64 = arg(args, "--deals", 8000);
    let per = arena::run_deals("engine", "engine", 0..deals, 7);
    for n in [1000, 2000, 4000, 8000, 16000].into_iter().filter(|&n| n <= deals) {
        let s = per[..n as usize].iter().copied().fold(MatchStats::default(), MatchStats::merge);
        let (lo, hi) = s.ci95();
        println!("{n:>6} mazzi: {} fiche/mano, IC 95% {} .. {}", fmt_num(s.mean()), fmt_num(lo), fmt_num(hi));
    }
}

/// Avversari del rapporto: gli stili a regole, i giocatori distratti, il motore allenato meno,
/// l'avversario allenato apposta per sfruttarlo.
fn panel() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = bots::STYLES.iter().map(|(s, d)| (s.to_string(), d.to_string())).collect();
    v.push(("noisy:0.3:bar".into(), "Principiante: giocatore da bar che una volta su tre fa una mossa a caso".into()));
    v.push(("noisy:0.1:engine".into(), "Lo stesso motore, distratto: una mossa su dieci a caso".into()));
    v.push(("cfr:200000".into(), "Motore allenato con 200.000 iterazioni (un centesimo)".into()));
    v.push(("cfr:2000000".into(), "Motore allenato con 2 milioni di iterazioni (un decimo)".into()));
    v.push(("exploit:4000000".into(), "Allenato con 4 milioni di iterazioni per battere proprio questo motore, conoscendone la strategia".into()));
    v
}

fn report(args: &[String]) -> ExitCode {
    let deals: u64 = arg(args, "--deals", 8000);
    let self_deals: u64 = arg(args, "--self-deals", 16000);
    let out: String = arg(args, "--out", "bench/POKERTO.md".to_string());
    let seed: u64 = arg(args, "--seed", 20261023);
    let t = Instant::now();
    let mut md = String::new();
    writeln!(md, "# Poker all'italiana: rapporto del motore\n").unwrap();
    writeln!(
        md,
        "Testa a testa, limite fisso: invito 1 fiche, puntate da 2 prima del cambio e da 4 dopo, \
         al massimo 3 puntate per giro. Mazzo da 32 carte. Ogni mazzo si gioca due volte a posti \
         invertiti (duplicato) e il risultato è la media delle due mani, in fiche per mano: \
         0 è la parità. L'intervallo di confidenza al 95% è calcolato sulle medie per mazzo.\n"
    )
    .unwrap();
    writeln!(md, "{deals} mazzi per avversario ({} mani), seme {seed}.\n", deals * 2).unwrap();
    writeln!(md, "| Avversario | Fiche per mano | IC 95% | Mani vinte | Piatti a showdown | |").unwrap();
    writeln!(md, "|---|---:|---|---:|---:|---|").unwrap();
    let mut worst = f64::INFINITY;
    for (spec, desc) in panel() {
        let s = match run_match("engine", &spec, deals, seed) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        };
        let (lo, hi) = s.ci95();
        worst = worst.min(lo);
        let verdict = if lo > 0.0 { "vince" } else if hi < 0.0 { "perde" } else { "pari" };
        writeln!(
            md,
            "| {desc} (`{spec}`) | **{}** | {} .. {} | {} | {} | {verdict} |",
            fmt_num(s.mean()),
            fmt_num(lo),
            fmt_num(hi),
            pct(s.won as f64 / s.hands as f64),
            pct(s.showdowns as f64 / s.hands as f64),
        )
        .unwrap();
        eprintln!("{spec}: {} ({:.0}s)", s.summary(), t.elapsed().as_secs_f64());
    }
    writeln!(md, "\n## Contro se stesso\n").unwrap();
    writeln!(
        md,
        "Il motore contro una copia di se stesso deve dare 0 entro l'intervallo: serve a verificare \
         che il numero di mazzi basti a distinguere un vantaggio vero dal caso.\n"
    )
    .unwrap();
    let per = arena::run_deals("engine", "engine", 0..self_deals, seed ^ 0xABCD);
    writeln!(md, "| Mazzi | Fiche per mano | IC 95% |").unwrap();
    writeln!(md, "|---:|---:|---|").unwrap();
    for n in [1000, 2000, 4000, 8000, 16000].into_iter().filter(|&n| n <= self_deals) {
        let s = per[..n as usize].iter().copied().fold(MatchStats::default(), MatchStats::merge);
        let (lo, hi) = s.ci95();
        writeln!(md, "| {n} | {} | {} .. {} |", fmt_num(s.mean()), fmt_num(lo), fmt_num(hi)).unwrap();
    }
    writeln!(md, "\nTempo totale: {:.0} s.", t.elapsed().as_secs_f64()).unwrap();
    std::fs::write(&out, md).expect("scrittura rapporto");
    eprintln!("rapporto in {out}; caso peggiore (IC basso) {worst:+.3}");
    ExitCode::SUCCESS
}

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0).replace('.', ",")
}
