//! Banco di prova del poker all'italiana: allenamento del motore e migliaia di mani in duplicato.
//!
//! Uso:
//!   poker-it-sim train [--iters N] [--seed X] [--out FILE] [--pre LIVELLI] [--post LIVELLI]
//!   poker-it-sim match --player SPEC --opponent SPEC [--deals N] [--seed X]
//!   poker-it-sim report [--deals N] [--self-deals N] [--sessions-100 N] [--sessions-500 N] [--solo-base] [--out FILE]
//!   poker-it-sim calibrate [--deals N]
//!   poker-it-sim sessions --player SPEC --opponent SPEC [--sessions N] [--len MANI] [--seed X]
//!   poker-it-sim profiles [--iters N] [--p P] [--stats-hands N] [--styles a,b,..] [--from FILE] [--out FILE]
//!   poker-it-sim reference
//!
//! SPEC: engine, uno stile a regole (random, station, maniac, rock, bar, tag, lag, passive,
//! bluffer), noisy:P:SPEC, cfr:N (motore allenato al momento con N iterazioni),
//! exploit:N[:STRATEGIA] (allenato con N iterazioni per sfruttare il motore, o la strategia
//! indicata), file:PERCORSO (strategia salvata), profile:FILE:STILE (risposta a uno stile),
//! adaptive (il motore che si adatta), adaptive:FILE[:PRIORE:TEMPERATURA:MEMORIA]; per le sessioni anche
//! pop, pop:P (persona presa a caso) e switch:N:A>B (gioca come A per N mani, poi come B).

mod arena;

use arena::{run_match, MatchStats};
use poker_it::abstraction::Abstraction;
use poker_it::cfr::Trainer;
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
        "convert" => {
            // Riscrive strategia e stili inclusi nel formato più recente.
            let s = poker_it::engine::bundled();
            std::fs::write("crates/poker-it/data/strategy.bin", s.to_bytes()).expect("scrittura strategia");
            let p = poker_it::adaptive::bundled_profiles();
            std::fs::write("crates/poker-it/data/profiles.bin", poker_it::adaptive::profiles_to_bytes(&p)).expect("scrittura stili");
        }
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
        "profiles" => profiles(&args),
        "sessions" => {
            let player: String = arg(&args, "--player", "adaptive".to_string());
            let opponent: String = arg(&args, "--opponent", "pop".to_string());
            let n: u64 = arg(&args, "--sessions", 2000);
            let len: u64 = arg(&args, "--len", 200);
            let seed: u64 = arg(&args, "--seed", 1);
            let t = Instant::now();
            match arena::run_sessions(&player, &opponent, n, len, seed) {
                Ok(r) => {
                    let s = SessionSummary::of(&r, len);
                    println!("{player} contro {opponent}: {}", s.line());
                    println!("({:.1}s)", t.elapsed().as_secs_f64());
                }
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            }
        }
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
    let pre: usize = arg(args, "--pre", poker_it::abstraction::PRE_LEVELS);
    let post: usize = arg(args, "--post", poker_it::abstraction::POST_LEVELS);
    let abs = Abstraction::new(pre, post);
    eprintln!("analisi del cambio di tutte le mani: {:.1}s", t.elapsed().as_secs_f64());
    let tr = Trainer::new(abs);
    eprintln!("albero astratto: {} caselle", tr.slots());
    let chunk = (iters / 20).max(1);
    while tr.iterations() < iters {
        tr.run(chunk.min(iters - tr.iterations()), seed);
        eprintln!("{} iterazioni, {:.0}s", tr.iterations(), t.elapsed().as_secs_f64());
    }
    let bytes = tr.strategy().to_bytes();
    std::fs::write(&out, &bytes).expect("scrittura strategia");
    eprintln!("salvata in {out} ({} KB)", bytes.len() / 1024);
}

/// Riassunto di una serie di sessioni in coppia (vedi `arena::run_sessions`).
pub struct SessionSummary {
    /// Fiche per mano e intervallo di confidenza al 95% (sulle coppie di sessioni).
    pub mean: f64,
    pub lo: f64,
    pub hi: f64,
    /// Quota di sessioni chiuse in attivo, contando ogni sessione da sola.
    pub in_profit: f64,
}

impl SessionSummary {
    pub fn of(r: &[[f64; 2]], len: u64) -> SessionSummary {
        let per: Vec<f64> = r.iter().map(|p| (p[0] + p[1]) / (2.0 * len as f64)).collect();
        let n = per.len() as f64;
        let mean = per.iter().sum::<f64>() / n;
        let var = per.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
        let half = 1.959964 * (var / n).sqrt();
        let in_profit = r.iter().flatten().filter(|&&x| x > 0.0).count() as f64 / (2.0 * n);
        SessionSummary { mean, lo: mean - half, hi: mean + half, in_profit }
    }

    pub fn line(&self) -> String {
        format!(
            "{} fiche/mano (IC 95% {} .. {}), sessioni in attivo {}",
            fmt_num(self.mean),
            fmt_num(self.lo),
            fmt_num(self.hi),
            pct(self.in_profit)
        )
    }
}

/// Allena le risposte agli stili tipici e misura come gioca ciascuno: `data/profiles.bin`.
fn profiles(args: &[String]) {
    use poker_it::adaptive::{profiles_to_bytes, Profile};
    use poker_it::cfr::Model;
    use std::sync::Arc;
    let iters: u64 = arg(args, "--iters", 30_000_000);
    let p: f32 = arg(args, "--p", 0.55);
    let stats_hands: u64 = arg(args, "--stats-hands", 400_000);
    let styles: String = arg(args, "--styles", "station,passive,rock,bar,tag,lag,bluffer,maniac".to_string());
    let out: String = arg(args, "--out", "crates/poker-it/data/profiles.bin".to_string());
    let seed: u64 = arg(args, "--seed", 20261027);
    let t = Instant::now();
    let base = poker_it::engine::bundled();
    let eq = Profile { name: "eq".into(), stats: arena::style_stats("engine", stats_hands, seed).unwrap(), strategy: None };
    // Con --from si ripartono gli stili già allenati in quel file (si rimisura solo l'equilibrio
    // e si riallenano gli stili indicati con --styles).
    let from: String = arg(args, "--from", String::new());
    let (mut v, styles) = if from.is_empty() {
        (vec![eq], styles)
    } else {
        let old = poker_it::adaptive::profiles_from_bytes(&std::fs::read(&from).expect("lettura stili")).expect("stili non validi");
        let only: String = arg(args, "--styles", String::new());
        let mut v = vec![eq];
        v.extend(old.into_iter().filter(|p| p.name != "eq" && !only.split(',').any(|s| s == p.name)));
        (v, only)
    };
    for style in styles.split(',').filter(|s| !s.is_empty()) {
        let stats = arena::style_stats(style, stats_hands, seed).unwrap();
        let name = style.to_string();
        let factory = move || arena::make_agent(&name).unwrap();
        let tr = Trainer::restricted(Model::Agent(Arc::new(factory)), p, base.abs.clone());
        tr.run(iters, seed ^ v.len() as u64);
        let strategy = tr.strategy();
        eprintln!("{style}: {} situazioni, {:.0}s", strategy.len(), t.elapsed().as_secs_f64());
        v.push(Profile { name: style.to_string(), stats, strategy: Some(Arc::new(strategy)) });
    }
    let bytes = profiles_to_bytes(&v);
    std::fs::write(&out, &bytes).expect("scrittura stili");
    eprintln!("salvati in {out} ({} KB)", bytes.len() / 1024);
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
    if !args.iter().any(|a| a == "--solo-base") {
        if let Err(e) = adaptive_sections(args, &mut md) {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    }
    writeln!(md, "## Il motore di base: l'equilibrio\n").unwrap();
    writeln!(
        md,
        "Qui il motore non impara l'avversario: gioca sempre la strategia di equilibrio, che è il punto \
         di partenza del motore adattivo e quello che gioca finché non ha capito chi ha davanti.\n"
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

/// Avversari delle sessioni: gli stili per cui il motore ha una risposta, persone prese a caso
/// (mai viste in allenamento), chi cambia stile, il motore stesso.
fn session_panel() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = bots::STYLES
        .iter()
        .filter(|(s, _)| *s != "random")
        .map(|(s, d)| (s.to_string(), d.to_string()))
        .collect();
    v.push(("pop".into(), "Persone prese a caso: soglie, bluff e testardaggine scelti a caso per ogni sessione (stili mai visti in allenamento)".into()));
    v.push(("pop:0.3".into(), "Come sopra, ma distratte: fino a una mossa su tre a caso".into()));
    v.push(("switch:100:station>bluffer".into(), "Cambia stile a metà: vede sempre per 100 mani, poi bluffa spesso".into()));
    v.push(("engine".into(), "Il motore di equilibrio (un avversario quasi perfetto)".into()));
    v
}

fn adaptive_sections(args: &[String], md: &mut String) -> Result<(), String> {
    let n100: u64 = arg(args, "--sessions-100", 1000);
    let n500: u64 = arg(args, "--sessions-500", 300);
    let seed: u64 = arg(args, "--seed", 20261023);
    let t = Instant::now();
    writeln!(md, "## Il motore che si adatta: sessioni contro lo stesso avversario\n").unwrap();
    writeln!(
        md,
        "Il motore conta, mano dopo mano, cosa fa l'avversario (quanto lascia, quanto punta, quante carte \
         cambia, che mani mostra) e sposta il gioco verso la risposta allenata per lo stile che gli somiglia \
         di più. Per misurarlo si giocano **sessioni** di 100 e di 500 mani contro lo stesso avversario, \
         alternando il mazziere; ogni sessione si rigioca con gli stessi mazzi a posti invertiti.\n\n\
         **Criterio (il 60%).** Nel poker contare le mani vinte non ha senso: si perdono tante mani piccole \
         lasciando e se ne vincono poche grandi. Il 60% si legge come **almeno 6 sessioni su 10 chiuse in \
         attivo**. Più la sessione è lunga, meno pesa la fortuna delle carte.\n\n\
         {n100} coppie di sessioni da 100 mani e {n500} da 500 per avversario, seme {seed}. Le fiche per mano \
         (con l'intervallo al 95%) sono quelle delle sessioni da 500 mani.\n"
    )
    .unwrap();
    writeln!(
        md,
        "| Avversario | Adattivo: fiche/mano | IC 95% | In attivo, 100 mani | In attivo, 500 mani | Solo equilibrio: fiche/mano | In attivo, 100 mani | 500 mani | 60% |"
    )
    .unwrap();
    writeln!(md, "|---|---:|---|---:|---:|---:|---:|---:|---|").unwrap();
    for (spec, desc) in session_panel() {
        let a100 = SessionSummary::of(&arena::run_sessions("adaptive", &spec, n100, 100, seed)?, 100);
        let a500 = SessionSummary::of(&arena::run_sessions("adaptive", &spec, n500, 500, seed ^ 5)?, 500);
        let e100 = SessionSummary::of(&arena::run_sessions("engine", &spec, n100, 100, seed)?, 100);
        let e500 = SessionSummary::of(&arena::run_sessions("engine", &spec, n500, 500, seed ^ 5)?, 500);
        let verdict = if a100.in_profit >= 0.6 {
            "sì, già in 100 mani"
        } else if a500.in_profit >= 0.6 {
            "sì, in 500 mani"
        } else {
            "no"
        };
        writeln!(
            md,
            "| {desc} (`{spec}`) | **{}** | {} .. {} | {} | {} | {} | {} | {} | {verdict} |",
            fmt_num(a500.mean),
            fmt_num(a500.lo),
            fmt_num(a500.hi),
            pct(a100.in_profit),
            pct(a500.in_profit),
            fmt_num(e500.mean),
            pct(e100.in_profit),
            pct(e500.in_profit),
        )
        .unwrap();
        eprintln!("sessioni {spec}: {} ({:.0}s)", a500.line(), t.elapsed().as_secs_f64());
    }
    writeln!(
        md,
        "\nContro il motore di equilibrio il guadagno resta zero, come deve essere: contro chi gioca quasi \
         alla perfezione nessun algoritmo guadagna. Il guadagno viene dagli errori costanti degli avversari. \
         Attenzione: anche le \"persone prese a caso\" sono giocatori a regole, solo con parametri che il \
         motore non ha mai visto; le persone vere sbagliano in modi più vari, quindi i numeri reali saranno \
         più bassi di questi, soprattutto nelle prime mani contro un avversario nuovo.\n"
    )
    .unwrap();

    writeln!(md, "## Sicurezza: quanto si rischia contro chi studia il motore\n").unwrap();
    writeln!(
        md,
        "Ogni risposta a uno stile è allenata in modo prudente: durante l'allenamento l'avversario gioca \
         quello stile solo il 55% delle volte, e il resto delle volte gioca la strategia che batte meglio \
         la risposta stessa. La prima colonna dice quanto vincerebbe, per mano, un avversario allenato \
         (10 milioni di iterazioni) apposta contro la risposta a quello stile, se il motore la giocasse \
         sempre. La seconda è il caso più cattivo per il motore adattivo: un avversario che per 150 mani \
         imita lo stile, così il motore si sposta sulla risposta, e poi gioca proprio la strategia che \
         batte quella risposta (sessioni da 500 mani).\n"
    )
    .unwrap();
    writeln!(md, "| Stile | Risposta bloccata contro chi la studia | IC 95% | Adattivo contro chi imita e poi colpisce | IC 95% |").unwrap();
    writeln!(md, "|---|---:|---|---:|---|").unwrap();
    for p in poker_it::adaptive::bundled_profiles().iter().filter(|p| p.strategy.is_some()) {
        let target = format!("profile:{}", p.name);
        let ex = format!("exploit:10000000:{target}");
        let locked = run_match(&target, &ex, 50_000, seed)?;
        let (llo, lhi) = locked.ci95();
        let trap = SessionSummary::of(&arena::run_sessions("adaptive", &format!("switch:150:{}>{ex}", p.name), n500, 500, seed ^ 9)?, 500);
        writeln!(
            md,
            "| {} | {} | {} .. {} | {} | {} .. {} |",
            p.name,
            fmt_num(locked.mean()),
            fmt_num(llo),
            fmt_num(lhi),
            fmt_num(trap.mean),
            fmt_num(trap.lo),
            fmt_num(trap.hi)
        )
        .unwrap();
        eprintln!("sicurezza {}: {} / {} ({:.0}s)", p.name, locked.summary(), trap.line(), t.elapsed().as_secs_f64());
    }
    writeln!(md).unwrap();
    Ok(())
}

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0).replace('.', ",")
}
