//! Banco di prova del Texas Hold'em: allenamento del motore e migliaia di mani in duplicato.
//!
//! Uso:
//!   poker-sim train [--iters N] [--seed X] [--out FILE]
//!   poker-sim match --player SPEC --opponent SPEC [--deals N] [--seed X]
//!   poker-sim report [--deals N] [--self-deals N] [--out FILE]
//!   poker-sim calibrate [--deals N]
//!
//! SPEC: engine, uno stile a regole (vedi `bots::STYLES`), noisy:P:SPEC, cfr:N (motore
//! allenato al momento con N iterazioni), exploit:N (allenato con N iterazioni per
//! sfruttare il motore, conoscendone la strategia).

mod arena;

use arena::{run_match, threads, MatchStats};
use poker_core::bots;
use poker_core::cfr::Trainer;
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

fn num(x: f64) -> String {
    format!("{x:+.1}").replace('.', ",")
}

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0).replace('.', ",")
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str).unwrap_or("") {
        "train" => train(&args),
        "match" => {
            let player: String = arg(&args, "--player", "engine".to_string());
            let opponent: String = arg(&args, "--opponent", "tag".to_string());
            let deals: u64 = arg(&args, "--deals", 8000);
            let seed: u64 = arg(&args, "--seed", 1);
            let t = Instant::now();
            match run_match(&player, &opponent, deals, seed) {
                Ok(s) => {
                    println!("{player} contro {opponent}: {} ({:.0}s)", s.summary(), t.elapsed().as_secs_f64());
                }
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        "report" => return report(&args),
        "trace" => {
            use poker_core::{Agent, Hand, View};
            use rand::SeedableRng;
            let opponent: String = arg(&args, "--opponent", "maniac".to_string());
            let n: u64 = arg(&args, "--hands", 5);
            let mut rng = rand::rngs::SmallRng::seed_from_u64(5);
            for i in 0..n {
                let mut h = Hand::deal(&mut rng, (i % 2) as u8);
                let mut eng = poker_core::engine::Engine::bundled();
                let mut opp = arena::make_agent(&opponent).unwrap();
                println!("--- mano {i}: bottone {} motore {:?} avversario {:?} board {:?}", h.button,
                    h.hole[0].map(poker_core::cards::name), h.hole[1].map(poker_core::cards::name), h.board.map(poker_core::cards::name));
                while !h.over {
                    let p = h.to_act;
                    let v = View::of(&h, p);
                    let a = if p == 0 {
                        let adv = eng.advice(&v, &mut rng.clone());
                        let a = eng.act(&v, &mut rng);
                        println!("  motore [{}] to_call {} -> {:?}   {:?}", v.street, v.to_call, a,
                            adv.map(|x| x.iter().map(|e| format!("{}={:.2}", e.label, e.prob)).collect::<Vec<_>>()));
                        a
                    } else {
                        let a = opp.act(&v, &mut rng);
                        println!("  avv    [{}] to_call {} -> {:?}", v.street, v.to_call, a);
                        a
                    };
                    h.apply(a);
                }
                println!("  risultato motore {}", h.result(0));
            }
        }
        "calibrate" => {
            let deals: u64 = arg(&args, "--deals", 16000);
            for (n, s) in self_play(deals, 7) {
                let (lo, hi) = s.ci95();
                println!("{n:>6} smazzate: {} bb/100, IC 95% {} .. {}", num(s.bb100()), num(lo), num(hi));
            }
        }
        _ => {
            eprintln!("comandi: train, match, report, calibrate (vedi l'inizio di main.rs)");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn train(args: &[String]) {
    let iters: u64 = arg(args, "--iters", 100_000_000);
    let seed: u64 = arg(args, "--seed", 20261023);
    let out: String = arg(args, "--out", "crates/poker/data/holdem.bin".to_string());
    let t = Instant::now();
    let b = arena::fresh_bucketer();
    eprintln!("soglie dei bucket: {:.0}s", t.elapsed().as_secs_f64());
    let tr = Trainer::new(b);
    let chunk = (iters / 20).max(1);
    while tr.iterations() < iters {
        tr.run(chunk.min(iters - tr.iterations()), threads(), seed ^ tr.iterations());
        let bytes = tr.strategy().to_bytes();
        std::fs::write(&out, &bytes).expect("scrittura strategia");
        eprintln!("{} iterazioni, {:.0}s (salvata in {out})", tr.iterations(), t.elapsed().as_secs_f64());
    }
}

fn self_play(deals: u64, seed: u64) -> Vec<(u64, MatchStats)> {
    let per = arena::run_deals("engine", "engine", 0..deals, seed);
    [1000, 2000, 4000, 8000, 16000, 32000]
        .into_iter()
        .filter(|&n| n <= deals)
        .map(|n| (n, per[..n as usize].iter().copied().fold(MatchStats::default(), MatchStats::merge)))
        .collect()
}

/// Avversari del rapporto: gli stili a regole, i giocatori distratti, il motore allenato meno,
/// l'avversario allenato apposta per sfruttarlo.
fn panel() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = bots::STYLES.iter().map(|(s, d)| (s.to_string(), d.to_string())).collect();
    v.push(("noisy:0.3:tag".into(), "Principiante: stretto-aggressivo che una volta su tre fa una mossa a caso".into()));
    v.push(("noisy:0.1:engine".into(), "Lo stesso motore, distratto: una mossa su dieci a caso".into()));
    v.push(("cfr:2000000".into(), "Motore allenato con 2 milioni di iterazioni".into()));
    v.push(("cfr:10000000".into(), "Motore allenato con 10 milioni di iterazioni".into()));
    v.push(("exploit:20000000".into(), "Allenato con 20 milioni di iterazioni per battere proprio questo motore, conoscendone la strategia".into()));
    v
}

fn report(args: &[String]) -> ExitCode {
    let deals: u64 = arg(args, "--deals", 8000);
    let self_deals: u64 = arg(args, "--self-deals", 16000);
    let out: String = arg(args, "--out", "bench/HOLDEM.md".to_string());
    let seed: u64 = arg(args, "--seed", 20261023);
    let t = Instant::now();
    let mut md = String::new();
    writeln!(md, "# Texas Hold'em: rapporto del motore\n").unwrap();
    writeln!(
        md,
        "No Limit testa a testa, bui 50/100, stack di 100 grandi bui ricaricati a ogni mano. \
         Ogni smazzata si gioca due volte a posti invertiti (duplicato); negli all-in prima del river \
         si conta il valore atteso sulle carte mancanti. Risultato in grandi bui ogni 100 mani (bb/100) \
         per il motore: 0 è la parità. Intervallo di confidenza al 95% sulle medie per smazzata.\n"
    )
    .unwrap();
    writeln!(md, "{deals} smazzate per avversario ({} mani), seme {seed}.\n", deals * 2).unwrap();
    writeln!(md, "| Avversario | bb/100 | IC 95% | Senza correzione all-in | Mani vinte | Showdown | |").unwrap();
    writeln!(md, "|---|---:|---|---:|---:|---:|---|").unwrap();
    for (spec, desc) in panel() {
        let s = match run_match("engine", &spec, deals, seed) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        };
        let (lo, hi) = s.ci95();
        let verdict = if lo > 0.0 { "vince" } else if hi < 0.0 { "perde" } else { "pari" };
        writeln!(
            md,
            "| {desc} (`{spec}`) | **{}** | {} .. {} | {} | {} | {} | {verdict} |",
            num(s.bb100()),
            num(lo),
            num(hi),
            num(s.raw_bb100()),
            pct(s.won as f64 / s.hands as f64),
            pct(s.showdowns as f64 / s.hands as f64),
        )
        .unwrap();
        eprintln!("{spec}: {} ({:.0}s)", s.summary(), t.elapsed().as_secs_f64());
    }
    writeln!(md, "\n## Contro se stesso\n").unwrap();
    writeln!(
        md,
        "Il motore contro una copia di se stesso deve dare 0 entro l'intervallo: verifica che il \
         numero di smazzate basti a distinguere un vantaggio vero dal caso.\n"
    )
    .unwrap();
    writeln!(md, "| Smazzate | bb/100 | IC 95% |").unwrap();
    writeln!(md, "|---:|---:|---|").unwrap();
    for (n, s) in self_play(self_deals, seed ^ 0xABCD) {
        let (lo, hi) = s.ci95();
        writeln!(md, "| {n} | {} | {} .. {} |", num(s.bb100()), num(lo), num(hi)).unwrap();
    }
    writeln!(md, "\nTempo totale: {:.0} s.", t.elapsed().as_secs_f64()).unwrap();
    std::fs::write(&out, md).expect("scrittura rapporto");
    eprintln!("rapporto in {out}");
    ExitCode::SUCCESS
}
