//! Banco di prova del poker all'italiana: allenamento del motore e migliaia di mani in duplicato.
//!
//! Uso:
//!   poker-it-sim train [--iters N] [--seed X] [--out FILE]
//!   poker-it-sim match --player SPEC --opponent SPEC [--deals N] [--seed X]
//!   poker-it-sim report [--deals N] [--self-deals N] [--out FILE]
//!   poker-it-sim calibrate [--deals N]
//!   poker-it-sim reference
//!
//! SPEC: engine, uno stile a regole (random, station, maniac, rock, bar, tag, lag, passive,
//! bluffer), noisy:P:SPEC, cfr:N (motore allenato al momento con N iterazioni),
//! exploit:N (allenato con N iterazioni per sfruttare il motore).

mod arena;

use arena::{run_match, MatchStats};
use poker_it::cfr::{pre_thresholds, Trainer};
use poker_it::{bots, cards};
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
        "reference" => reference(&args),
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
            eprintln!("comandi: train, match, report, calibrate, reference (vedi l'inizio di main.rs)");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

/// Genera `crates/poker-it/src/reference.rs`: la distribuzione delle mani dopo il cambio e i
/// quantili delle mani di partenza. Due passate: la prima misura la forza tra le mani servite,
/// la seconda tra le mani dopo il cambio della prima passata.
fn reference(args: &[String]) {
    use poker_it::cards::{Card, NUM_CARDS};
    use poker_it::draw::{analyze_with, raw_percentile, replace, table_percentile};
    use poker_it::eval::evaluate;
    let out: String = arg(args, "--out", "crates/poker-it/src/reference.rs".to_string());
    let hands = cards::all_hands();
    let post_draw = |strength: &(dyn Fn(&[Card; 5]) -> f32 + Sync)| -> (Vec<u32>, Vec<f32>) {
        let per: Vec<(Vec<u32>, f32)> = hands
            .par_iter()
            .enumerate()
            .map(|(i, h)| {
                let d = analyze_with(h, strength);
                let mask = d.best[d.natural()];
                let k = mask.count_ones() as usize;
                let rest: Vec<Card> = (0..NUM_CARDS as u8).filter(|c| !h.contains(c)).collect();
                let mut rng = SmallRng::seed_from_u64(i as u64);
                let vals = (0..8)
                    .map(|_| {
                        let new: Vec<Card> = rand::seq::index::sample(&mut rng, rest.len(), k).into_iter().map(|j| rest[j]).collect();
                        evaluate(&replace(h, mask, &new))
                    })
                    .collect();
                (vals, d.max_ev())
            })
            .collect();
        let mut vals: Vec<u32> = per.iter().flat_map(|p| p.0.iter().copied()).collect();
        vals.sort_unstable();
        let table = (0..=1000).map(|i| vals[i * (vals.len() - 1) / 1000]).collect();
        (table, per.iter().map(|p| p.1).collect())
    };
    let t = Instant::now();
    let raw = |h: &[Card; 5]| raw_percentile(evaluate(h));
    let (t1, _) = post_draw(&raw);
    eprintln!("prima passata: {:.0}s", t.elapsed().as_secs_f64());
    let s1 = |h: &[Card; 5]| table_percentile(&t1, evaluate(h));
    let (t2, _) = post_draw(&s1);
    eprintln!("seconda passata: {:.0}s", t.elapsed().as_secs_f64());
    // Quantili delle mani di partenza con la misura finale.
    let s2 = |h: &[Card; 5]| table_percentile(&t2, evaluate(h));
    let mut ev: Vec<f32> = hands.par_iter().map(|h| analyze_with(h, &s2).max_ev()).collect();
    ev.sort_unstable_by(f32::total_cmp);
    let q: Vec<String> = (0..=100).map(|i| format!("{:.4}", ev[(i * (ev.len() - 1)) / 100])).collect();
    let diff = t1.iter().zip(&t2).filter(|(a, b)| a != b).count();
    eprintln!("quantili cambiati tra le due passate: {diff} su 1001");
    let mut src = String::from(
        "//! Tabelle generate con `poker-it-sim reference` (non modificare a mano).\n\n\
         /// Quantili (0, 0,1%, ..., 100%) del valore delle mani allo showdown dopo il cambio.\n\
         pub const POST_DRAW: &[u32] = &[",
    );
    src += &t2.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
    src += "];\n\n/// Quantili (0%, 1%, ..., 100%) del percentile atteso dopo il miglior cambio, sulle mani di partenza.\n\
            pub const START_QUANTILES: [f32; 101] = [";
    src += &q.join(", ");
    src += "];\n";
    std::fs::write(&out, src).expect("scrittura tabelle");
    eprintln!("scritto {out} ({:.0}s)", t.elapsed().as_secs_f64());
}

fn train(args: &[String]) {
    let iters: u64 = arg(args, "--iters", 20_000_000);
    let seed: u64 = arg(args, "--seed", 20261023);
    let out: String = arg(args, "--out", "crates/poker-it/data/strategy.bin".to_string());
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
    v.push(("exploit:20000000".into(), "Come sopra, con 20 milioni di iterazioni: l'avversario più forte possibile contro questo motore".into()));
    v
}

fn report(args: &[String]) -> ExitCode {
    let deals: u64 = arg(args, "--deals", 100_000);
    let self_deals: u64 = arg(args, "--self-deals", 100_000);
    let out: String = arg(args, "--out", "bench/POKER_IT.md".to_string());
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
    writeln!(
        md,
        "**Criterio.** Nella briscola si contano le partite vinte; nel poker conta quante fiche si \
         vincono in media per mano (1 fiche = un invito), perché si possono perdere tante mani piccole \
         e vincerne poche grandi. Il poker ha molta più varianza: con lo stesso criterio della briscola \
         (8.000 partite) l'intervallo sarebbe largo ±0,04 fiche, più dei vantaggi da misurare. Per questo \
         si giocano 100.000 mazzi per avversario (200.000 mani), che danno un intervallo di circa \
         ±0,01 fiche per mano; la tabella del motore contro se stesso, in fondo, mostra che a questo \
         numero di mazzi una copia identica dà 0 entro l'intervallo. **Vince** vuol dire che tutto \
         l'intervallo è sopra lo zero.\n"
    )
    .unwrap();
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
    for n in [1000, 4000, 16000, 50000, 100000, 200000].into_iter().filter(|&n| n <= self_deals) {
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
