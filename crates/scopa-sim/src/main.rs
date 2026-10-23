//! Banco di prova della scopa: migliaia di partite a 11 punti tra il motore e altri giocatori.
//!
//! Ogni sequenza di mazzi viene giocata due volte a posti invertiti ("duplicato").
//!
//! Uso:
//!   scopa-sim [--deals N] [--player SPEC] [--opponent SPEC] [--seed X]
//!   scopa-sim --panel [--strong] [--deals N] [--player SPEC] [--out FILE]
//!   scopa-sim calibrate [--player SPEC] [--deals N]
//!   scopa-sim report [--player SPEC] [--deals N] [--self-deals N] [--out FILE]
//!   scopa-sim dist [--player SPEC] [--hands N]   (distribuzione dei punti per smazzata)
//!
//! SPEC: vedi `agents.rs` (es. engine:200, ismcts:2000, noisy:0.3:heuristic).

mod agents;
mod arena;

use agents::{describe, make_player};
use arena::{deck_order, prefixes, run_deals, MatchStats};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use rayon::prelude::*;
use scopa_core::{play_hand, Game};
use std::fmt::Write as _;
use std::process::ExitCode;
use std::time::Instant;

/// Margine del criterio sul numero di partite: ±1 punto percentuale.
const MARGIN: f64 = 0.01;
/// Obiettivo dichiarato: almeno il 55% di partite vinte contro ogni avversario.
const TARGET: f64 = 0.55;

/// Stili di gioco umani e algoritmi deboli.
const STYLES: &[(&str, &str)] = &[
    ("random", "Carta e presa a caso"),
    ("greedy", "Prende sempre la presa più ricca, non guarda cosa lascia in tavola"),
    ("heuristic", "Regole da bar: prende il più possibile, non lascia in tavola meno di 11 (scopa facile)"),
    ("scope", "Punta tutto sulle scope"),
    ("primiera", "Insegue sette e sei per la primiera, non li butta mai"),
    ("denari", "Raccoglie denari"),
    ("counter", "Conta le carte uscite per stimare cosa può prendere l'avversario e gioca il finale in modo perfetto"),
    ("noisy:0.3:heuristic", "Principiante: regole da bar, ma una mossa su tre è a caso"),
    ("noisy:0.1:counter", "Contacarte distratto: sbaglia una mossa su dieci"),
];

/// Algoritmi forti: ricerca ad albero e varianti del motore stesso.
const STRONG: &[(&str, &str)] = &[
    ("ismcts:1000", "Ricerca ad albero Monte Carlo (famiglia diversa dal motore), 1000 iterazioni"),
    ("ismcts:5000", "ISMCTS con 5000 iterazioni"),
    ("engine:30", "Lo stesso motore con 30 mondi (livello Medio dell'app)"),
    ("noisy:0.1:engine:200", "Motore forte ma distratto: sbaglia una mossa su dieci"),
    ("engine:200:0:diff", "Lo stesso motore che gioca per la differenza punti della smazzata e non per la partita"),
];

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0).replace('.', ",")
}

fn dec(x: f64) -> String {
    format!("{x:.2}").replace('.', ",")
}

fn table_header(md: &mut String) {
    writeln!(md, "| Avversario | Partite | Vinte | % vittorie | IC 95% | Elo | Smazzate a partita | Punti a smazzata (fatti – subiti) | Scope a partita |").unwrap();
    writeln!(md, "|---|---:|---:|---:|---|---:|---:|---:|---:|").unwrap();
}

fn table_row(name: &str, s: &MatchStats) -> String {
    let (lo, hi) = s.score_ci95();
    let (pf, pa) = s.per_hand();
    format!(
        "| {name} | {} | {} | **{}** | {} – {} | {:+.0} | {} | {} – {} | {} |",
        s.games, s.wins, pct(s.score()), pct(lo), pct(hi), s.elo(),
        dec(s.hands as f64 / s.games as f64), dec(pf), dec(pa), dec(s.scope_for as f64 / s.games as f64)
    )
}

/// Risultati per mazzo, con la cache a blocchi (variabile SCOPA_SIM_CACHE = cartella).
fn deals_cached(player: &str, opponent: &str, deals: u64, seed: u64) -> Vec<MatchStats> {
    const CHUNK: u64 = 250;
    let play = |r: std::ops::Range<u64>| {
        run_deals(player, opponent, r, seed).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(2)
        })
    };
    let Ok(dir) = std::env::var("SCOPA_SIM_CACHE") else { return play(0..deals) };
    let mut out = Vec::with_capacity(deals as usize);
    for start in (0..deals).step_by(CHUNK as usize) {
        let end = (start + CHUNK).min(deals);
        let key: String = format!("{player}_{opponent}_{seed}_{start}-{end}")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
            .collect();
        let path = std::path::Path::new(&dir).join(key);
        let cached: Option<Vec<MatchStats>> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| t.lines().map(MatchStats::from_line).collect::<Option<Vec<_>>>())
            .filter(|v| v.len() as u64 == end - start);
        let chunk = cached.unwrap_or_else(|| {
            let v = play(start..end);
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(&path, v.iter().map(|s| s.to_line() + "\n").collect::<String>());
            v
        });
        out.extend(chunk);
    }
    out
}

fn run(player: &str, opponent: &str, deals: u64, seed: u64) -> MatchStats {
    let start = Instant::now();
    let s = deals_cached(player, opponent, deals, seed).into_iter().fold(MatchStats::default(), MatchStats::merge);
    eprintln!("  {} contro {}: {}  ({:.0}s)", describe(player), describe(opponent), pct(s.score()), start.elapsed().as_secs_f64());
    s
}

fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let z = secs / 86400 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y}-{m:02}-{d:02}")
}

fn commit() -> String {
    std::env::var("GITHUB_SHA")
        .ok()
        .or_else(|| {
            let out = std::process::Command::new("git").args(["rev-parse", "HEAD"]).output().ok()?;
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        })
        .map(|s| s.chars().take(7).collect())
        .unwrap_or_else(|| "?".into())
}

/// Il motore contro se stesso deve dare il 50%: il primo numero di mazzi da cui il punteggio resta
/// entro `margin` dal 50% con IC largo al più ±`margin`.
fn convergence(per_deal: &[MatchStats], margin: f64, md: &mut String) -> Option<u64> {
    let total = per_deal.len() as u64;
    let mut checkpoints: Vec<u64> = [50, 100, 250, 500, 1000, 1500, 2000, 3000, 4000, 5000, 7500, 10000]
        .into_iter()
        .filter(|&n| n < total)
        .collect();
    checkpoints.push(total);
    let stats = prefixes(per_deal, &checkpoints);
    let ok: Vec<bool> = stats
        .iter()
        .map(|s| {
            let (lo, hi) = s.score_ci95();
            (s.score() - 0.5).abs() <= margin && (hi - lo) / 2.0 <= margin
        })
        .collect();
    let first = (0..ok.len()).find(|&i| ok[i..].iter().all(|&b| b));
    writeln!(md, "| Mazzi | Partite | Punteggio | IC 95% | Distanza dal 50% | Criterio |").unwrap();
    writeln!(md, "|---:|---:|---:|---|---:|:---:|").unwrap();
    for (i, (n, s)) in checkpoints.iter().zip(&stats).enumerate() {
        let (lo, hi) = s.score_ci95();
        let mark = if Some(i) == first { "✅ da qui" } else if first.is_some_and(|f| i > f) { "✅" } else { "—" };
        writeln!(
            md,
            "| {n} | {} | {} | {} – {} | {} punti | {mark} |",
            n * 2, pct(s.score()), pct(lo), pct(hi), format!("{:.1}", (s.score() - 0.5).abs() * 100.0).replace('.', ",")
        )
        .unwrap();
    }
    first.map(|i| checkpoints[i])
}

fn target_mark(s: &MatchStats) -> &'static str {
    let (lo, hi) = s.score_ci95();
    if lo > TARGET {
        "✅ sopra il 55%"
    } else if hi < TARGET {
        "❌ sotto il 55%"
    } else {
        "≈ al limite"
    }
}

fn panel_rows(player: &str, opponents: &[(&str, &str)], deals: u64, seed: u64, md: &mut String) -> Vec<(String, MatchStats)> {
    table_header(md);
    let mut out = Vec::new();
    for (opp, _) in opponents {
        let s = run(player, opp, deals, seed);
        writeln!(md, "{}", table_row(&describe(opp), &s)).unwrap();
        out.push((describe(opp), s));
    }
    writeln!(md, "\n| Avversario | Come gioca |\n|---|---|").unwrap();
    for (spec, how) in opponents {
        writeln!(md, "| {} | {how} |", describe(spec)).unwrap();
    }
    out
}

fn report(player: &str, deals: u64, self_deals: u64, seed: u64) -> String {
    let start = Instant::now();
    let name = describe(player);
    let mut games = 0u64;
    let mut all_rows: Vec<(String, MatchStats)> = Vec::new();

    let mut sec1 = String::new();
    writeln!(sec1, "## 1. Contro se stesso: quante partite servono\n").unwrap();
    let per_deal = deals_cached(player, player, self_deals, seed);
    eprintln!("  contro se stesso: fatto");
    games += self_deals * 2;
    let selfp = per_deal.iter().copied().fold(MatchStats::default(), MatchStats::merge);
    let (lo, hi) = selfp.score_ci95();
    table_header(&mut sec1);
    writeln!(sec1, "{}\n", table_row(&format!("{name} (copia)"), &selfp)).unwrap();
    writeln!(
        sec1,
        "Punteggio {} (IC {} – {}): due copie dello stesso motore si dividono le partite a metà, come deve essere. \
         In duplicato ogni sequenza di mazzi viene giocata da entrambe le copie da entrambi i posti, quindi nessuna \
         delle due può fare meglio del 50%: contro un avversario forte quanto lui nessun programma lo supera. \
         Il test controlla che il banco di prova sia equo e stabilisce quante partite servono.\n",
        pct(selfp.score()), pct(lo), pct(hi)
    )
    .unwrap();
    let (olo, ohi) = selfp.seat_ci(0);
    writeln!(
        sec1,
        "Chi gioca per primo nella prima smazzata vince il {} delle partite (IC {} – {}), chi distribuisce il {}. \
         Il duplicato compensa il vantaggio del posto.\n",
        pct(selfp.seat_wins[0] as f64 / self_deals as f64), pct(olo), pct(ohi), pct(selfp.seat_wins[1] as f64 / self_deals as f64)
    )
    .unwrap();
    writeln!(
        sec1,
        "**Quante partite servono.** Si aumentano le partite del motore contro se stesso finché il punteggio resta \
         stabilmente entro {m:.0} punto percentuale dal 50% e l'intervallo di confidenza è largo al più ±{m:.0} punto.\n",
        m = MARGIN * 100.0
    )
    .unwrap();
    match convergence(&per_deal, MARGIN, &mut sec1) {
        Some(n) => writeln!(
            sec1,
            "\nServono almeno **{n} mazzi ({} partite)** per confronto. Questo rapporto ne usa {deals} ({} partite) per riga{}.\n",
            n * 2,
            deals * 2,
            if n <= deals { ": le differenze di più di un punto percentuale non sono dovute al caso" } else { ": non bastano, rilancialo con più mazzi" }
        )
        .unwrap(),
        None => writeln!(
            sec1,
            "\nCon {self_deals} mazzi il punteggio è entro il margine ma l'IC non è ancora largo al più ±1 punto: \
             la precisione di ogni riga è quella indicata dal suo IC.\n"
        )
        .unwrap(),
    }

    let mut sec2 = String::new();
    writeln!(sec2, "## 2. Contro tanti stili di gioco\n").unwrap();
    all_rows.extend(panel_rows(player, STYLES, deals, seed, &mut sec2));
    games += STYLES.len() as u64 * deals * 2;

    let mut sec3 = String::new();
    writeln!(sec3, "\n## 3. Contro gli avversari più forti\n").unwrap();
    writeln!(
        sec3,
        "Ricerca ad albero Monte Carlo (ISMCTS), che è un algoritmo diverso dal motore, e varianti del motore stesso. \
         Contro le proprie varianti il motore parte vicino al 50% per costruzione.\n"
    )
    .unwrap();
    all_rows.extend(panel_rows(player, STRONG, deals, seed, &mut sec3));
    games += STRONG.len() as u64 * deals * 2;

    let mut md = String::new();
    writeln!(md, "# Scopa: rapporto dei test massivi\n").unwrap();
    writeln!(md, "Motore provato: **{name}** (`{player}`), commit `{}`, {}.", commit(), today()).unwrap();
    writeln!(
        md,
        "Ogni partita va a 11 punti (scopa classica a due: carte, denari, settebello, primiera e scope). \
         Ogni riga: {deals} sequenze di mazzi giocate due volte a posti invertiti ({} partite), seme `{seed}`, \
         gli stessi mazzi per tutti gli avversari.\n",
        deals * 2
    )
    .unwrap();
    writeln!(md, "## Riepilogo: obiettivo 55% contro ogni avversario\n").unwrap();
    let above = all_rows.iter().filter(|(_, s)| s.score_ci95().0 > TARGET).count();
    writeln!(
        md,
        "Sopra il 55% con certezza statistica (intero IC 95% sopra il 55%): **{above} avversari su {}**. \
         Il motore contro se stesso è escluso: lì il 50% è il risultato esatto per costruzione.\n",
        all_rows.len()
    )
    .unwrap();
    writeln!(md, "| Avversario | Partite vinte | IC 95% | Obiettivo |\n|---|---:|---|---|").unwrap();
    let mut sorted = all_rows.clone();
    sorted.sort_by(|a, b| a.1.score().partial_cmp(&b.1.score()).unwrap());
    for (n, s) in &sorted {
        let (lo, hi) = s.score_ci95();
        writeln!(md, "| {n} | {} | {} – {} | {} |", pct(s.score()), pct(lo), pct(hi), target_mark(s)).unwrap();
    }
    writeln!(md, "\nCome leggere le colonne:\n").unwrap();
    writeln!(md, "- **% vittorie**: partite a 11 vinte. Il 50% è la parità.").unwrap();
    writeln!(md, "- **IC 95%**: intervallo di confidenza, calcolato sulle coppie di partite con gli stessi mazzi. Se è tutto sopra il 50%, il vantaggio non è dovuto al caso.").unwrap();
    writeln!(md, "- **Elo**: la stessa differenza di forza in punti Elo (+35 ≈ 55%, +100 ≈ 64%).").unwrap();
    writeln!(md, "- **Punti a smazzata**: media dei punti fatti e subiti dal motore in ogni smazzata.\n").unwrap();
    md.push_str(&sec1);
    md.push_str(&sec2);
    md.push_str(&sec3);
    writeln!(md, "\n## Come riprodurlo\n").unwrap();
    writeln!(md, "```\ncargo run --release -p scopa-sim -- report --player {player} --deals {deals} --self-deals {self_deals} --seed {seed} --out bench/SCOPA.md\n```\n").unwrap();
    writeln!(md, "{games} partite in tutto, {:.0} minuti su questa macchina.", start.elapsed().as_secs_f64() / 60.0).unwrap();
    md
}

/// Distribuzione dei punti per smazzata tra due copie di `player`, come codice Rust per `hand_dist.rs`.
fn hand_distribution(player: &str, hands: u64, seed: u64) -> String {
    let counts: Vec<[[u64; 16]; 16]> = (0..hands)
        .into_par_iter()
        .map(|h| {
            let mut c = [[0u64; 16]; 16];
            let mut a = make_player(player).unwrap();
            let mut b = make_player(player).unwrap();
            let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(h));
            let g = Game::from_order(&deck_order(seed, h, 0), (h % 2) as u8);
            let s = play_hand(g, &mut [a.as_mut(), b.as_mut()], [0, 0], &mut rng);
            // Simmetrizzata: il primo e il secondo giocatore valgono uguale.
            let (x, y) = (s.points[0].min(15) as usize, s.points[1].min(15) as usize);
            c[x][y] += 1;
            c[y][x] += 1;
            c
        })
        .collect();
    let mut total = [[0u64; 16]; 16];
    for c in &counts {
        for i in 0..16 {
            for j in 0..16 {
                total[i][j] += c[i][j];
            }
        }
    }
    let n = (hands * 2) as f64;
    let mut out = String::new();
    writeln!(out, "//! Distribuzione dei punti di una smazzata tra due giocatori di pari forza: `HAND_DIST[i][j]` è la").unwrap();
    writeln!(out, "//! probabilità che uno faccia `i` punti e l'altro `j`. Generata con").unwrap();
    writeln!(out, "//! `cargo run --release -p scopa-sim -- dist --player {player} --hands {hands} --seed {seed}`.\n").unwrap();
    writeln!(out, "pub const HAND_DIST: [[f64; 16]; 16] = [").unwrap();
    for row in &total {
        let cells: Vec<String> = row.iter().map(|&k| format!("{:.6}", k as f64 / n)).collect();
        writeln!(out, "    [{}],", cells.join(", ")).unwrap();
    }
    writeln!(out, "];").unwrap();
    out
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str, d: &str| -> String {
        args.iter().position(|a| a == k).map(|i| args[i + 1].clone()).unwrap_or(d.to_string())
    };
    let has = |k: &str| args.iter().any(|a| a == k);
    let deals: u64 = get("--deals", "4000").parse().expect("--deals");
    let seed: u64 = get("--seed", "2026").parse().expect("--seed");
    let player = get("--player", "engine:200");
    let out = args.iter().position(|a| a == "--out").map(|i| args[i + 1].clone());
    let write_out = |md: &str| {
        if let Some(path) = &out {
            std::fs::write(path, md).expect("scrittura risultati");
        }
    };

    match args.get(1).map(String::as_str) {
        Some("calibrate") => {
            let per_deal = deals_cached(&player, &player, deals, seed);
            let mut md = format!("{} contro se stesso, fino a {deals} mazzi in duplicato\n\n", describe(&player));
            match convergence(&per_deal, MARGIN, &mut md) {
                Some(n) => writeln!(md, "\nServono almeno {n} mazzi ({} partite).", n * 2).unwrap(),
                None => writeln!(md, "\nCriterio non raggiunto con {deals} mazzi: aumenta --deals.").unwrap(),
            }
            println!("{md}");
            write_out(&md);
            return ExitCode::SUCCESS;
        }
        Some("report") => {
            let self_deals: u64 = get("--self-deals", "4000").parse().expect("--self-deals");
            let md = report(&player, deals, self_deals, seed);
            println!("{md}");
            write_out(&md);
            return ExitCode::SUCCESS;
        }
        Some("dist") => {
            let hands: u64 = get("--hands", "20000").parse().expect("--hands");
            let code = hand_distribution(&player, hands, seed);
            println!("{code}");
            write_out(&code);
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    if has("--panel") {
        let opponents: Vec<(&str, &str)> = if has("--strong") { STRONG.to_vec() } else { STYLES.iter().chain(STRONG).copied().collect() };
        println!("{} contro il pannello di avversari, {} partite ciascuno", describe(&player), deals * 2);
        let mut md = String::new();
        panel_rows(&player, &opponents, deals, seed, &mut md);
        println!("{md}");
        write_out(&md);
        return ExitCode::SUCCESS;
    }

    let opponent = get("--opponent", "heuristic");
    let start = Instant::now();
    let s = run(&player, &opponent, deals, seed);
    let (lo, hi) = s.score_ci95();
    let (pf, pa) = s.per_hand();
    println!("{} contro {}", describe(&player), describe(&opponent));
    println!("Partite: {} ({} mazzi in duplicato), seme {seed}", s.games, deals);
    println!("Vinte {} ({:.2}%, IC 95% {:.2}% - {:.2}%, Elo {:+.0})", s.wins, s.score() * 100.0, lo * 100.0, hi * 100.0, s.elo());
    println!("Smazzate a partita {:.2}, punti a smazzata {:.2} - {:.2}", s.hands as f64 / s.games as f64, pf, pa);
    println!("Tempo: {:.1}s", start.elapsed().as_secs_f64());
    ExitCode::SUCCESS
}
