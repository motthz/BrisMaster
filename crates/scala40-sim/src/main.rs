//! Banco di prova di Scala 40: migliaia di partite tra il motore e altri giocatori.
//!
//! Uso:
//!   scala40-sim [--player SPEC] [--opponent SPEC] [--matches N] [--seed X]
//!   scala40-sim report [--player SPEC] [--matches N] [--seed X] [--out FILE]
//!   scala40-sim tune [--opponents "A;B;C"] [--matches N] [--rounds R] [--start PESI]
//!
//! SPEC: vedi `agents.rs`. `--matches N` sono N coppie di partite in duplicato (2N partite).

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

/// Il gruppo di avversari del rapporto: stili da persona e algoritmi.
const PANEL: &[(&str, &str)] = &[
    ("random", "Carta a caso: cala quando può, scarta a caso, non prende mai dal pozzo"),
    ("noisy:0.3:basic", "Principiante: regole semplici, ma una mossa su tre è a caso"),
    ("basic", "Regole semplici: prende dal pozzo se può usarlo, cala appena può, scarta la carta spaiata più alta"),
    ("pairs", "Tiene le coppie: protegge le carte che possono diventare combinazioni, scarta la spaiata più alta"),
    ("defensive", "Difensivo: non scarta mai carte che l'avversario può attaccare, preferisce valori che l'avversario ha già scartato"),
    ("hoarder", "Chiude in mano: tiene le combinazioni nascoste e cala tutto insieme, a meno che l'avversario non abbia 5 carte o meno"),
    ("hoarder:2", "Chiude in mano, più azzardato: cala prima solo se l'avversario ha 2 carte o meno"),
    ("noisy:0.1:engine", "Motore distratto: lo stesso motore che sbaglia una mossa su dieci"),
    ("engine:pair=14,outs=0.5,dpartner=1", "Motore con i pesi scritti a mano prima della taratura (valuta diversamente coppie e rischi)"),
];

/// Risultati coppia per coppia; con `SCALA40_SIM_CACHE=dir` si salvano su file e un rapporto
/// interrotto riparte dai confronti già finiti.
fn cached(player: &str, opponent: &str, matches: u64, seed: u64, duplicate: bool) -> Vec<arena::MatchStats> {
    let file = std::env::var("SCALA40_SIM_CACHE").ok().map(|dir| {
        let name: String = format!("{player}_{opponent}_{matches}_{seed}_{duplicate}")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '_' { c } else { '-' })
            .collect();
        std::path::Path::new(&dir).join(name + ".txt")
    });
    if let Some(f) = &file {
        if let Ok(text) = std::fs::read_to_string(f) {
            let v: Vec<_> = text.lines().filter_map(arena::MatchStats::from_line).collect();
            if v.len() as u64 == matches {
                return v;
            }
        }
    }
    let v = arena::run_matches_with(player, opponent, 0..matches, seed, duplicate).unwrap();
    if let Some(f) = &file {
        let _ = std::fs::create_dir_all(f.parent().unwrap());
        let _ = std::fs::write(f, v.iter().map(|s| s.to_line() + "\n").collect::<String>());
    }
    v
}

fn total(v: &[arena::MatchStats]) -> arena::MatchStats {
    v.iter().copied().fold(arena::MatchStats::default(), arena::MatchStats::merge)
}

/// Rapporto completo in Markdown.
fn report(player: &str, matches: u64, seed: u64) -> String {
    use std::fmt::Write as _;
    let mut md = String::new();
    let t0 = Instant::now();
    writeln!(md, "# Scala 40: rapporto del banco di prova\n").unwrap();
    writeln!(
        md,
        "Giocatore: `{player}`. Ogni riga sono **{} partite** complete (smazzate fino a 101 punti): {matches} coppie \
         giocate in duplicato, cioè gli stessi mazzi a posti invertiti. Punteggio: vittoria 1, pari ½; il 50% è la parità. \
         L'intervallo di confidenza al 95% è calcolato sulle coppie. Seme {seed}.\n",
        matches * 2
    )
    .unwrap();
    writeln!(md, "| Avversario | Punteggio | IC 95% | Smazzate chiuse | Punti presi / dati per smazzata | Obiettivo 55% |").unwrap();
    writeln!(md, "|---|---:|---:|---:|---:|:---:|").unwrap();
    let mut desc = String::new();
    for (spec, d) in PANEL {
        let t = Instant::now();
        let s = total(&cached(player, spec, matches, seed, true));
        let (lo, hi) = s.score_ci95();
        let mark = if s.score() > 0.55 { "✅" } else { "❌" };
        writeln!(
            md,
            "| `{spec}` | {:.1}% | {:.1}–{:.1}% | {:.1}% | {:.1} / {:.1} | {mark} |",
            s.score() * 100.0,
            lo * 100.0,
            hi * 100.0,
            s.deal_rate() * 100.0,
            s.pen_for as f64 / s.deals as f64,
            s.pen_against as f64 / s.deals as f64
        )
        .unwrap();
        writeln!(desc, "- `{spec}`: {d}").unwrap();
        eprintln!("{spec}: {:.1}% ({:.0}s)", s.score() * 100.0, t.elapsed().as_secs_f64());
    }
    writeln!(md, "\n{desc}").unwrap();

    // Controllo statistico: contro se stesso, senza duplicato, deve venire 50% entro l'errore.
    let per = cached(player, player, matches, seed + 1, false);
    let s = total(&per);
    let (lo, hi) = s.score_ci95();
    writeln!(md, "## Contro se stesso\n").unwrap();
    writeln!(
        md,
        "Il motore contro una sua copia, con mazzi diversi nelle due partite di ogni coppia (in duplicato \
         verrebbe 50% esatto per costruzione): **{:.1}%** (IC 95% {:.1}–{:.1}%) su {} partite. \
         Il risultato atteso è il 50%: serve a controllare che il banco di prova non favorisca un posto.\n",
        s.score() * 100.0,
        lo * 100.0,
        hi * 100.0,
        s.games()
    )
    .unwrap();
    writeln!(md, "Andamento al crescere delle partite:\n").unwrap();
    writeln!(md, "| Partite | Punteggio | IC 95% |").unwrap();
    writeln!(md, "|---:|---:|---:|").unwrap();
    for n in [500u64, 1000, 2000, 3000, 4000, 6000, 8000].into_iter().filter(|&n| n <= matches * 2) {
        let p = per[..(n / 2) as usize].iter().copied().fold(arena::MatchStats::default(), arena::MatchStats::merge);
        let (lo, hi) = p.score_ci95();
        writeln!(md, "| {n} | {:.1}% | {:.1}–{:.1}% |", p.score() * 100.0, lo * 100.0, hi * 100.0).unwrap();
    }
    writeln!(md, "\nTempo totale: {:.0} minuti su {} core.\n", t0.elapsed().as_secs_f64() / 60.0, rayon::current_num_threads())
        .unwrap();
    writeln!(md, "Per rifarlo:\n\n```\ncargo run --release -p scala40-sim -- report --player {player} --matches {matches} --seed {seed} --out bench/SCALA40.md\n```").unwrap();
    md
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("tune") {
        tune(&args);
        return;
    }
    if args.get(1).map(String::as_str) == Some("report") {
        let player = arg(&args, "--player").unwrap_or("engine".into());
        let n: u64 = arg(&args, "--matches").and_then(|x| x.parse().ok()).unwrap_or(4000);
        let seed: u64 = arg(&args, "--seed").and_then(|x| x.parse().ok()).unwrap_or(20261023);
        let md = report(&player, n, seed);
        match arg(&args, "--out") {
            Some(f) => std::fs::write(f, md).unwrap(),
            None => print!("{md}"),
        }
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
