//! Banco di prova del burraco: migliaia di smazzate in duplicato tra il motore e altri giocatori.
//!
//! Uso:
//!   burraco-sim match A B [--deals N] [--seed X]
//!   burraco-sim report [--player SPEC] [--deals N] [--seed X] [--out FILE]
//!
//! SPEC: random, beginner, classic, hoarder, closer, holder, noisy, expert, engine:N[:scarti[:peso]],
//! oppure h:chiave=valore,... (parametri dello stile esperto modificati, vedi `style`).
//! Con BURRACO_SIM_CACHE=cartella i risultati si salvano a blocchi e un rapporto interrotto riparte.

use burraco::ai::{Heuristic, Player, RandomPlayer, Style};
use burraco::arena::{run_per_deal, MatchStats};
use burraco::engine::{Engine, EngineConfig};
use std::fmt::Write as _;
use std::time::Instant;

/// Il motore provato nei rapporti.
const PLAYER: &str = "engine:16";

/// Gli avversari del rapporto: stili umani tipici e programmi forti.
const PANEL: &[(&str, &str)] = &[
    ("random", "Pesca e scarta a caso, cala quello che vede"),
    (
        "beginner",
        "Principiante: usa subito le matte, raccoglie gli scarti solo se servono subito, non guarda cosa regala",
    ),
    ("closer", "Chiude appena può, usa subito le matte"),
    ("hoarder", "Raccoglie sempre gli scarti e usa subito le matte"),
    ("holder", "Tiene in mano le combinazioni e le cala solo per andare al pozzetto"),
    ("classic", "Giocatore attento ma prudente: raccoglie gli scarti solo quando gli servono davvero"),
    ("noisy", "Esperto distratto: gioca come l'esperto, ma uno scarto su quattro è a caso"),
    ("expert", "Esperto: lo stile tarato con decine di migliaia di smazzate, usato dal motore nelle simulazioni"),
    ("engine:4", "Lo stesso motore con 4 simulazioni per scelta invece di 16"),
];

fn style(spec: &str) -> Style {
    let mut s = Style::EXPERT;
    for kv in spec.split(',').filter(|x| !x.is_empty()) {
        let (k, v) = kv.split_once('=').expect("chiave=valore");
        let f: f32 = v.parse().expect("numero");
        match k {
            "pt" => s.pile_threshold = f,
            "pc" => s.pile_card_cost = f,
            "deny" => s.deny_w = f,
            "ew" => s.eager_wilds = f > 0.0,
            "em" => s.eager_melds = f > 0.0,
            "care" => s.care = f,
            "keep" => s.keep_w = f,
            "dump" => s.dump_w = f,
            "cm" => s.close_margin = f as i32,
            "late" => s.late_stock = f as usize,
            "noise" => s.noise = f,
            "ss" => s.stock_samples = f as u8,
            _ => panic!("parametro sconosciuto: {k}"),
        }
    }
    s
}

fn named_style(name: &str) -> Option<Style> {
    let e = Style::EXPERT;
    Some(match name {
        "beginner" => Style {
            pile_threshold: 15.0,
            eager_wilds: true,
            care: 0.0,
            keep_w: 0.5,
            dump_w: 1.0,
            close_margin: -1000,
            ..e
        },
        "classic" => Style { pile_threshold: 8.0, stock_samples: 12, ..e },
        "hoarder" => Style { pile_threshold: -1000.0, eager_wilds: true, deny_w: 0.0, ..e },
        "closer" => Style { eager_wilds: true, close_margin: -1000, late_stock: 30, ..e },
        "holder" => Style { eager_melds: false, ..e },
        "noisy" => Style { noise: 0.25, ..e },
        "expert" => e,
        _ => return None,
    })
}

fn describe(spec: &str) -> String {
    match spec {
        "random" => "Casuale".into(),
        "beginner" => "Principiante".into(),
        "closer" => "Chiude subito".into(),
        "hoarder" => "Raccoglitore".into(),
        "holder" => "Trattenitore".into(),
        "classic" => "Prudente".into(),
        "noisy" => "Esperto distratto".into(),
        "expert" => "Esperto".into(),
        _ => match spec.strip_prefix("engine:") {
            Some(n) => format!("Motore ({} simulazioni)", n.split(':').next().unwrap()),
            None => spec.to_string(),
        },
    }
}

fn agent(spec: &str) -> Box<dyn Player> {
    if spec == "random" {
        return Box::new(RandomPlayer);
    }
    if let Some(s) = named_style(spec) {
        return Box::new(Heuristic::new(&describe(spec), s));
    }
    if let Some(rest) = spec.strip_prefix("engine:") {
        let f: Vec<&str> = rest.split(':').collect();
        let mut cfg = EngineConfig::new(f[0].parse().expect("simulazioni"));
        if let Some(d) = f.get(1) {
            cfg.discards = d.parse().expect("scarti");
        }
        if let Some(m) = f.get(2) {
            cfg.margin_w = m.parse().expect("peso");
        }
        return Box::new(Engine::new(cfg));
    }
    let s = spec.strip_prefix("h:").unwrap_or_else(|| panic!("giocatore sconosciuto: {spec}"));
    Box::new(Heuristic::new(spec, style(s)))
}

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0).replace('.', ",")
}

fn num(x: f64) -> String {
    format!("{x:.0}")
}

/// Risultati per mazzo, salvati a blocchi se è impostata BURRACO_SIM_CACHE.
fn deals_cached(a: &str, b: &str, deals: u64, seed: u64) -> Vec<MatchStats> {
    const CHUNK: u64 = 200;
    let play = |from: u64, to: u64| run_per_deal(seed, from, to, &|| agent(a), &|| agent(b));
    let Ok(dir) = std::env::var("BURRACO_SIM_CACHE") else { return play(0, deals) };
    let mut out = Vec::with_capacity(deals as usize);
    for start in (0..deals).step_by(CHUNK as usize) {
        let end = (start + CHUNK).min(deals);
        let key: String = format!("{a}_{b}_{seed}_{start}-{end}")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
            .collect();
        let path = std::path::Path::new(&dir).join(key);
        let cached: Option<Vec<MatchStats>> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| t.lines().map(MatchStats::from_line).collect::<Option<Vec<_>>>())
            .filter(|v| v.len() as u64 == end - start);
        let chunk = cached.unwrap_or_else(|| {
            let v = play(start, end);
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(&path, v.iter().map(|s| s.to_line() + "\n").collect::<String>());
            v
        });
        out.extend(chunk);
    }
    out
}

fn run(a: &str, b: &str, deals: u64, seed: u64) -> MatchStats {
    let t = Instant::now();
    let s = deals_cached(a, b, deals, seed).into_iter().fold(MatchStats::default(), MatchStats::merge);
    eprintln!("  {} contro {}: {}  ({:.0}s)", describe(a), describe(b), pct(s.score()), t.elapsed().as_secs_f64());
    s
}

fn row(name: &str, s: &MatchStats) -> String {
    let (lo, hi) = s.score_ci95();
    let g = s.games() as f64;
    format!(
        "| {name} | {} | {} / {} / {} | {} | **{}** | {} – {} | {:+.0} | {} | {:+.0} | {} / {} |",
        s.games(),
        s.wins,
        s.draws,
        s.losses,
        pct(s.win_rate()),
        pct(s.score()),
        pct(lo),
        pct(hi),
        s.elo(),
        num(s.avg_points()),
        s.avg_margin(),
        pct(s.closed[0] as f64 / g),
        pct(s.closed[1] as f64 / g),
    )
}

fn header(md: &mut String) {
    writeln!(md, "| Avversario | Smazzate | V / P / S | % vittorie | Punteggio | IC 95% | Elo | Punti medi | Scarto medio | Chiude lui / l'avversario |").unwrap();
    writeln!(md, "|---|---:|---:|---:|---:|---|---:|---:|---:|---:|").unwrap();
}

fn verdict(s: &MatchStats) -> &'static str {
    let (lo, hi) = s.score_ci95();
    if lo > 0.55 {
        "✅ sopra il 55%"
    } else if hi >= 0.55 {
        "≈ al limite"
    } else {
        "❌ sotto il 55%"
    }
}

fn commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "?".into())
}

fn report(player: &str, deals: u64, seed: u64, out: &str) {
    let mut results = Vec::new();
    for &(spec, desc) in PANEL {
        results.push((spec, desc, run(player, spec, deals, seed)));
    }
    let selfplay = run(player, player, deals, seed);

    let mut md = String::new();
    writeln!(md, "# Burraco: rapporto dei test massivi\n").unwrap();
    writeln!(
        md,
        "Motore provato: **{}** (`{player}`), commit `{}`. Ogni riga: {deals} mazzi giocati due volte a posti invertiti ({} smazzate), seme `{seed}`. Gli stessi mazzi per tutti gli avversari.\n",
        describe(player),
        commit(),
        deals * 2
    )
    .unwrap();
    writeln!(md, "Regole: burraco a due giocatori, 11 carte, due pozzetti, chiusura con almeno un burraco e scarto finale. Una partita del test è una **smazzata**: vince chi fa più punti nella smazzata (carte calate, burraco, chiusura, meno le carte in mano e il pozzetto non preso).\n").unwrap();

    let above = results.iter().filter(|(_, _, s)| s.score_ci95().0 > 0.55).count();
    writeln!(md, "## Riepilogo: obiettivo 55% contro ogni avversario\n").unwrap();
    writeln!(md, "Sopra il 55% con certezza statistica (intero IC 95% sopra il 55%): **{above} avversari su {}**. Il motore contro se stesso è escluso: lì il 50% è il risultato esatto per costruzione.\n", results.len()).unwrap();
    writeln!(md, "| Avversario | Punteggio | IC 95% | Obiettivo |").unwrap();
    writeln!(md, "|---|---:|---|---|").unwrap();
    let mut sorted: Vec<_> = results.iter().collect();
    sorted.sort_by(|a, b| a.2.score().total_cmp(&b.2.score()));
    for (spec, _, s) in &sorted {
        let (lo, hi) = s.score_ci95();
        writeln!(md, "| {} | {} | {} – {} | {} |", describe(spec), pct(s.score()), pct(lo), pct(hi), verdict(s))
            .unwrap();
    }
    writeln!(md).unwrap();
    writeln!(md, "Come leggere le colonne:\n").unwrap();
    writeln!(md, "- **Punteggio**: vittoria 1, pareggio ½, sconfitta 0. Il 50% è la parità.").unwrap();
    writeln!(
        md,
        "- **IC 95%**: intervallo di confidenza del punteggio, calcolato sulle coppie di smazzate dello stesso mazzo."
    )
    .unwrap();
    writeln!(md, "- **Elo**: la stessa differenza di forza in punti Elo (+35 ≈ 55%).").unwrap();
    writeln!(md, "- **Scarto medio**: punti del motore meno punti dell'avversario, in media per smazzata.\n").unwrap();

    writeln!(md, "## 1. Contro se stesso\n").unwrap();
    header(&mut md);
    writeln!(md, "{}\n", row(&format!("{} (copia)", describe(player)), &selfplay)).unwrap();
    let (lo, hi) = selfplay.score_ci95();
    writeln!(md, "Punteggio {} (IC {} – {}): due copie dello stesso motore si dividono le smazzate a metà, come deve essere in duplicato. Serve a controllare che il banco di prova sia equo: contro un avversario forte quanto lui nessun programma può superare il 50%.\n", pct(selfplay.score()), pct(lo), pct(hi)).unwrap();
    let g = selfplay.games() as f64 / 2.0;
    writeln!(
        md,
        "Chi inizia vince il {} delle smazzate, chi è secondo il {}: il duplicato compensa il vantaggio del posto.\n",
        pct(selfplay.seat_wins[0] as f64 / g),
        pct(selfplay.seat_wins[1] as f64 / g)
    )
    .unwrap();

    writeln!(md, "## 2. Contro gli avversari\n").unwrap();
    header(&mut md);
    for (spec, _, s) in &results {
        writeln!(md, "{}", row(&describe(spec), s)).unwrap();
    }
    writeln!(md, "\nGli avversari:\n").unwrap();
    for (spec, desc, _) in &results {
        writeln!(md, "- **{}** (`{spec}`): {desc}.", describe(spec)).unwrap();
    }
    std::fs::write(out, &md).expect("scrittura del rapporto");
    println!("{md}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name).map(|i| args[i + 1].clone());
    let deals: u64 = flag("--deals").map_or(4000, |v| v.parse().expect("--deals"));
    let seed: u64 = flag("--seed").map_or(20261023, |v| v.parse().expect("--seed"));
    match args.first().map(String::as_str) {
        Some("match") => {
            let s = run(&args[1], &args[2], deals, seed);
            let (lo, hi) = s.score_ci95();
            println!(
                "{} contro {}: punteggio {} ({} – {}), scarto medio {:+.0}, turni medi {:.1}, smazzate al limite di turni {}",
                describe(&args[1]),
                describe(&args[2]),
                pct(s.score()),
                pct(lo),
                pct(hi),
                s.avg_margin(),
                s.turns as f64 / s.games() as f64,
                s.timeouts
            );
        }
        Some("report") => {
            let player = flag("--player").unwrap_or_else(|| PLAYER.to_string());
            let out = flag("--out").unwrap_or_else(|| "bench/BURRACO.md".to_string());
            report(&player, deals, seed, &out);
        }
        _ => eprintln!("uso: burraco-sim match A B [--deals N] | report [--player SPEC] [--deals N] [--out FILE]"),
    }
}
