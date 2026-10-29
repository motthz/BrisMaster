//! Banco di prova: decine di migliaia di partite tra il motore e altri giocatori.
//!
//! Ogni mazzo viene giocato due volte a posti invertiti ("duplicato"),
//! così la fortuna delle carte si compensa e il confronto è più preciso.
//!
//! Uso:
//!   briscola-sim [--deals N] [--player SPEC] [--opponent SPEC] [--seed X]
//!   briscola-sim --panel [--extended | --strong] [--deals N] [--player SPEC] [--out FILE]
//!   briscola-sim gate [--player SPEC] [--baseline SPEC] [--deals N] [--summary FILE]
//!   briscola-sim calibrate [--player SPEC] [--deals N]
//!   briscola-sim report [--player SPEC] [--deals N] [--self-deals N] [--out FILE]
//!   briscola-sim banco | esame | strati | torneo ...   (banco di prova della 0.6.0, vedi banco.rs)
//!
//! SPEC: vedi `agents.rs` (es. engine:1000, ismcts:2000, noisy:0.25:heuristic, v0.1.1/engine:1000).

mod agents;
mod banco;
mod arena;
mod opponents;
mod persona;

use arena::{describe, prefixes, run_deals, MatchStats};
use briscola_core::stats::wilson95;
use std::fmt::Write as _;
use std::process::ExitCode;
use std::time::Instant;

/// Margine del criterio sul numero di partite: ±1 punto percentuale.
const MARGIN: f64 = 0.01;

/// Il pannello storico di 8 avversari (quello di bench/RISULTATI.md).
const PANEL: &[(&str, &str)] = &[
    ("random", "Carta a caso"),
    ("heuristic", "Regole pratiche da bar: liscia quando non c'è niente da prendere, prende i carichi con la briscola più piccola"),
    ("aggressive", "Prova sempre a prendere, apre con le carte forti"),
    ("cautious", "Tiene le briscole fino alla fine, prende solo quando costa poco"),
    ("trumphappy", "Usa la briscola appena ci sono punti in tavola, apre di briscola"),
    ("counter", "Conta le carte uscite, apre i carichi quando sono sicuri e gioca il finale in modo perfetto"),
    ("ismcts:2000", "Ricerca ad albero Monte Carlo: algoritmo di famiglia diversa, forte"),
    ("engine:150", "Lo stesso motore con meno simulazioni (livello Medio dell'app)"),
];

/// Avversari aggiuntivi: errori tipici delle persone e algoritmi a forze diverse.
const EXTRA: &[(&str, &str)] = &[
    ("noisy:0.3:heuristic", "Principiante: regole da bar, ma una mossa su tre è a caso"),
    ("followsuit", "Crede di dover rispondere al seme, come in altri giochi di carte"),
    ("loadleader", "Apre con Assi e Tre per fare punti subito, senza contare le briscole"),
    ("notrump", "Non usa mai la briscola finché c'è il mazzo"),
    ("noisy:0.1:engine:400", "Giocatore forte ma distratto: motore a 400 simulazioni che sbaglia una mossa su dieci"),
    ("engine:50", "Motore con 50 simulazioni"),
    ("engine:400", "Motore con 400 simulazioni"),
    ("ismcts:500", "ISMCTS con 500 iterazioni"),
    ("ismcts:5000", "ISMCTS con 5000 iterazioni, l'avversario algoritmico più forte"),
];

/// Gli avversari più forti: ricerca ad albero con molte iterazioni, anche con le simulazioni del motore.
const STRONG: &[(&str, &str)] = &[
    ("ismcts:10000", "ISMCTS con 10.000 iterazioni"),
    ("ismcts:20000", "ISMCTS con 20.000 iterazioni"),
    ("ismcts+:5000", "ISMCTS potenziato: le simulazioni del motore (a carte scoperte, finale esatto), 5.000 iterazioni"),
    ("ismcts+:10000", "ISMCTS potenziato con 10.000 iterazioni, l'avversario più forte che sappiamo costruire"),
    ("engine:3000", "Lo stesso motore con il triplo delle simulazioni"),
];

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0).replace('.', ",")
}

fn table_header(md: &mut String) {
    writeln!(md, "| Avversario | Partite | V / P / S | % vittorie | Punteggio | IC 95% | Elo | Punti medi |").unwrap();
    writeln!(md, "|---|---:|---:|---:|---:|---|---:|---:|").unwrap();
}

fn table_row(name: &str, s: &MatchStats) -> String {
    let t = &s.tally;
    let (lo, hi) = s.score_ci95();
    format!(
        "| {name} | {} | {} / {} / {} | {} | **{}** | {} – {} | {:+.0} | {} |",
        t.games(), t.wins, t.draws, t.losses, pct(t.win_rate()), pct(s.score()),
        pct(lo), pct(hi), s.elo(), format!("{:.1}", t.avg_points()).replace('.', ",")
    )
}

/// Risultati dei mazzi `start..end`. Con la variabile BRISCOLA_SIM_CACHE (una cartella) si salvano
/// a blocchi di mazzi, così un rapporto lungo interrotto riparte da dove era arrivato.
fn chunk_cached(player: &str, opponent: &str, seed: u64, start: u64, end: u64) -> Vec<MatchStats> {
    let play = |r: std::ops::Range<u64>| {
        run_deals(player, opponent, r, seed).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(2)
        })
    };
    let Ok(dir) = std::env::var("BRISCOLA_SIM_CACHE") else { return play(start..end) };
    // Le reti fanno parte del giocatore: con reti diverse i risultati salvati non valgono.
    let nets: String = ["BRISCOLA_NET", "BRISCOLA_POLICY"]
        .iter()
        .filter(|_| format!("{player} {opponent}").contains("net:") || format!("{player} {opponent}").contains("infer:"))
        .filter_map(|v| std::env::var(v).ok())
        .map(|p| format!("_{}", std::path::Path::new(&p).file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned())))
        .collect();
    let key: String = format!("{player}_{opponent}_{seed}_{start}-{end}{nets}")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    let path = std::path::Path::new(&dir).join(key);
    let cached: Option<Vec<MatchStats>> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| t.lines().map(MatchStats::from_line).collect::<Option<Vec<_>>>())
        .filter(|v| v.len() as u64 == end - start);
    cached.unwrap_or_else(|| {
        let v = play(start..end);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&path, v.iter().map(|s| s.to_line() + "\n").collect::<String>());
        v
    })
}

/// Mazzi per blocco di salvataggio.
const CHUNK: u64 = 250;

/// Risultati per mazzo dei primi `deals` mazzi, a blocchi salvati (vedi `chunk_cached`).
fn deals_cached(player: &str, opponent: &str, deals: u64, seed: u64) -> Vec<MatchStats> {
    let mut out = Vec::with_capacity(deals as usize);
    for start in (0..deals).step_by(CHUNK as usize) {
        out.extend(chunk_cached(player, opponent, seed, start, (start + CHUNK).min(deals)));
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
    // Data civile dal numero di giorni (algoritmo di H. Hinnant).
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
    std::env::var("GITHUB_SHA").ok().or_else(|| {
        let out = std::process::Command::new("git").args(["rev-parse", "HEAD"]).output().ok()?;
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    })
    .map(|s| s.chars().take(7).collect())
    .unwrap_or_else(|| "?".into())
}

fn latest_release() -> String {
    agents::RELEASED.last().unwrap().to_string()
}

/// Confronto con la versione rilasciata: fallisce se la versione attuale è
/// significativamente più debole (limite alto dell'IC sotto il 50%).
fn gate(player: &str, baseline: &str, deals: u64, seed: u64, summary: Option<&str>) -> ExitCode {
    let s = run(player, baseline, deals, seed);
    let (lo, hi) = s.score_ci95();
    let verdict = if hi < 0.5 {
        "❌ **Regressione**: la versione attuale è significativamente più debole."
    } else if lo > 0.5 {
        "✅ **Miglioramento**: la versione attuale è significativamente più forte."
    } else {
        "✅ **Nessuna differenza significativa**: la versione attuale non è più debole."
    };
    let mut md = String::new();
    writeln!(md, "### Confronto con la versione rilasciata\n").unwrap();
    writeln!(md, "{} contro {}, {} mazzi in duplicato ({} partite), seme {seed}.\n", describe(player), describe(baseline), deals, deals * 2).unwrap();
    table_header(&mut md);
    writeln!(md, "{}\n", table_row(&describe(baseline), &s)).unwrap();
    writeln!(md, "{verdict}").unwrap();
    println!("{md}");
    if let Some(path) = summary {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("riepilogo");
        f.write_all(md.as_bytes()).expect("riepilogo");
    }
    if hi < 0.5 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// Criterio per il numero di partite: due copie dello stesso motore devono dare il 50%.
/// Con pochi mazzi il risultato oscilla; il numero giusto è il primo da cui in poi il punteggio
/// resta entro `margin` dal 50% e anche l'IC 95% è largo al più ±`margin`. Restituisce i mazzi necessari.
fn convergence(per_deal: &[MatchStats], margin: f64, md: &mut String) -> Option<u64> {
    let total = per_deal.len() as u64;
    let mut checkpoints: Vec<u64> = [50, 100, 250, 500, 1000, 1500, 2000, 3000, 4000, 5000, 7500, 10000, 15000, 20000, 30000, 50000]
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
    // Il primo punto da cui il criterio vale fino alla fine.
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
        ).unwrap();
    }
    first.map(|i| checkpoints[i])
}

/// Obiettivo dichiarato: almeno il 60% di punteggio contro ogni avversario.
const TARGET: f64 = 0.60;

fn target_mark(s: &MatchStats) -> &'static str {
    let (lo, hi) = s.score_ci95();
    if lo > TARGET {
        "✅ sopra il 60%"
    } else if hi < TARGET {
        "❌ sotto il 60%"
    } else {
        "≈ al limite"
    }
}

/// Pannello che restituisce anche i risultati, per il riepilogo.
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

/// Rapporto completo: contro se stesso, tanti stili, avversari forti, versione precedente.
fn report(player: &str, deals: u64, self_deals: u64, seed: u64) -> String {
    let start = Instant::now();
    let name = describe(player);
    let base = format!("{}/{player}", latest_release());
    let mut games = 0u64;
    let mut all_rows: Vec<(String, MatchStats)> = Vec::new();

    // 1. Contro se stesso e numero di partite.
    let mut sec1 = String::new();
    writeln!(sec1, "## 1. Contro se stesso: quante partite servono\n").unwrap();
    let per_deal = deals_cached(player, player, self_deals, seed);
    eprintln!("  contro se stesso: fatto");
    games += self_deals * 2;
    let selfp = per_deal.iter().copied().fold(MatchStats::default(), MatchStats::merge);
    let (lo, hi) = selfp.score_ci95();
    table_header(&mut sec1);
    writeln!(sec1, "{}\n", table_row(&format!("{name} (copia)"), &selfp)).unwrap();
    let opener = selfp.seat_wins[0] as f64 / self_deals as f64;
    let (olo, ohi) = wilson95(selfp.seat_wins[0] as f64, self_deals as f64);
    let responder = selfp.seat_wins[1] as f64 / self_deals as f64;
    writeln!(
        sec1,
        "Punteggio {} (IC {} – {}): due copie dello stesso motore si dividono le partite a metà. \
         Non è un difetto: in duplicato ogni mazzo viene giocato da entrambe le copie da entrambi i posti, \
         quindi per simmetria nessuna delle due può fare meglio del 50%. Vale per qualsiasi programma: \
         contro un avversario forte quanto lui nessuno può superare il 50%. \
         Questo test serve a controllare che il banco di prova sia equo e a stabilire quante partite servono.\n",
        pct(selfp.score()), pct(lo), pct(hi)
    ).unwrap();
    writeln!(
        sec1,
        "Chi apre la prima presa vince il {} delle partite (IC {} – {}), chi risponde il {}. \
         Qualunque vantaggio dia il posto, il duplicato lo compensa: ogni mazzo si gioca da entrambi i posti.\n",
        pct(opener), pct(olo), pct(ohi), pct(responder)
    ).unwrap();
    writeln!(
        sec1,
        "**Quante partite servono.** Si gioca il motore contro se stesso aumentando le partite, \
         finché il punteggio resta stabilmente vicino al 50% (entro {m:.0} punto percentuale) e l'intervallo di confidenza \
         è largo al più ±{m:.0} punto. Con meno partite il caso pesa troppo: anche due giocatori identici sembrano diversi.\n",
        m = MARGIN * 100.0
    ).unwrap();
    let needed = convergence(&per_deal, MARGIN, &mut sec1);
    match needed {
        Some(n) => writeln!(
            sec1,
            "\nServono almeno **{n} mazzi ({} partite)** per confronto. Questo rapporto ne usa {deals} ({} partite) per riga{}.\n",
            n * 2, deals * 2,
            if n <= deals { ": le differenze di più di un punto percentuale non sono dovute al caso" } else { ": non bastano, rilancialo con più mazzi" }
        ).unwrap(),
        None => writeln!(sec1, "\nCon {self_deals} mazzi il criterio non è ancora soddisfatto: servono più partite.\n").unwrap(),
    }

    // 2. Tanti stili di gioco.
    let mut sec2 = String::new();
    writeln!(sec2, "## 2. Contro tanti stili di gioco\n").unwrap();
    let styles: Vec<(&str, &str)> = PANEL.iter().chain(EXTRA).copied().collect();
    all_rows.extend(panel_rows(player, &styles, deals, seed, &mut sec2));
    games += styles.len() as u64 * deals * 2;

    // 3. Gli avversari più forti che sappiamo costruire.
    let mut sec3 = String::new();
    writeln!(sec3, "\n## 3. Contro gli avversari più forti\n").unwrap();
    writeln!(
        sec3,
        "Gli avversari più difficili che il banco di prova sa costruire: ricerca ad albero con molte più iterazioni, \
         ISMCTS potenziato con le stesse simulazioni del motore, e il motore stesso con più simulazioni. Contro le varianti di se stesso \
         il motore parte vicino al 50% per costruzione: superarle del 60% richiede un motore nettamente migliore, non solo più calcolo.\n"
    ).unwrap();
    all_rows.extend(panel_rows(player, STRONG, deals, seed, &mut sec3));
    games += STRONG.len() as u64 * deals * 2;

    // 4. Versione precedente.
    let mut sec4 = String::new();
    writeln!(sec4, "\n## 4. Contro la versione precedente\n").unwrap();
    let old = run(player, &base, deals, seed);
    games += deals * 2;
    let (lo, hi) = old.score_ci95();
    table_header(&mut sec4);
    writeln!(sec4, "{}\n", table_row(&describe(&base), &old)).unwrap();
    all_rows.push((describe(&base), old));
    let verdict = if hi < 0.5 {
        "la versione attuale è **più debole** della precedente: è una regressione."
    } else if lo > 0.5 {
        "la versione attuale è **più forte** della precedente."
    } else {
        "nessuna differenza significativa tra le due versioni."
    };
    writeln!(sec4, "Contro {}: {verdict}", describe(&base)).unwrap();
    writeln!(
        sec4,
        "La stessa prova gira in automatico su ogni pull request (job *regressione* in `.github/workflows/ci.yml`) \
         e blocca le modifiche che rendono il motore significativamente più debole della release {}.\n",
        latest_release()
    ).unwrap();

    // Intestazione e riepilogo sull'obiettivo del 60%.
    let mut md = String::new();
    writeln!(md, "# Rapporto dei test massivi\n").unwrap();
    writeln!(md, "Motore provato: **{name}** (`{player}`), commit `{}`, {}.", commit(), today()).unwrap();
    writeln!(md, "Ogni riga: {deals} mazzi giocati due volte a posti invertiti ({} partite), seme `{seed}`. Gli stessi mazzi per tutti gli avversari.\n", deals * 2).unwrap();
    writeln!(md, "## Riepilogo: obiettivo 60% contro ogni avversario\n").unwrap();
    let above = all_rows.iter().filter(|(_, s)| s.score_ci95().0 > TARGET).count();
    writeln!(
        md,
        "Sopra il 60% con certezza statistica (intero IC 95% sopra il 60%): **{above} avversari su {}**. \
         Il motore contro se stesso è escluso: lì il 50% è il risultato esatto per costruzione.\n",
        all_rows.len()
    ).unwrap();
    writeln!(md, "| Avversario | Punteggio | IC 95% | Obiettivo |\n|---|---:|---|---|").unwrap();
    let mut sorted = all_rows.clone();
    sorted.sort_by(|a, b| a.1.score().partial_cmp(&b.1.score()).unwrap());
    for (n, s) in &sorted {
        let (lo, hi) = s.score_ci95();
        writeln!(md, "| {n} | {} | {} – {} | {} |", pct(s.score()), pct(lo), pct(hi), target_mark(s)).unwrap();
    }
    writeln!(md, "\nCome leggere le colonne:\n").unwrap();
    writeln!(md, "- **% vittorie**: partite vinte (i pareggi 60-60 non contano).").unwrap();
    writeln!(md, "- **Punteggio**: vittoria 1, pareggio ½, sconfitta 0. Il 50% è la parità: sopra il 50% il motore è più forte.").unwrap();
    writeln!(md, "- **IC 95%**: intervallo di confidenza del punteggio, calcolato sulle coppie di partite dello stesso mazzo. Se è tutto sopra il 50%, il vantaggio non è dovuto al caso.").unwrap();
    writeln!(md, "- **Elo**: la stessa differenza di forza espressa in punti Elo (+35 ≈ 55%, +100 ≈ 64% di punteggio).\n").unwrap();
    md.push_str(&sec1);
    md.push_str(&sec2);
    md.push_str(&sec3);
    md.push_str(&sec4);
    writeln!(md, "## Come riprodurlo\n").unwrap();
    writeln!(md, "```\ncargo run --release -p briscola-sim -- report --player {player} --deals {deals} --self-deals {self_deals} --seed {seed} --out bench/RAPPORTO.md\n```\n").unwrap();
    writeln!(md, "{games} partite in tutto, {:.0} minuti su questa macchina.", start.elapsed().as_secs_f64() / 60.0).unwrap();
    md
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str, d: &str| -> String {
        args.iter().position(|a| a == k).map(|i| args[i + 1].clone()).unwrap_or(d.to_string())
    };
    let has = |k: &str| args.iter().any(|a| a == k);
    let deals: u64 = get("--deals", "5000").parse().expect("--deals");
    let seed: u64 = get("--seed", "2026").parse().expect("--seed");
    let player = get("--player", "engine:1000");
    let out = args.iter().position(|a| a == "--out").map(|i| args[i + 1].clone());
    let write_out = |md: &str| {
        if let Some(path) = &out {
            std::fs::write(path, md).expect("scrittura risultati");
        }
    };

    match args.get(1).map(String::as_str) {
        Some("gate") => {
            let baseline = get("--baseline", &format!("{}/{player}", latest_release()));
            let summary = args.iter().position(|a| a == "--summary").map(|i| args[i + 1].clone());
            return gate(&player, &baseline, deals, seed, summary.as_deref());
        }
        Some("calibrate") => {
            // Motore contro se stesso fino a `--deals` mazzi: quante partite servono per dati affidabili.
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
        Some("banco") => {
            let fam = get("--famiglie", "umani,persone,personaggi,motori,cattivi,bari");
            let md = banco::Banco {
                player: &player,
                families: fam.split(',').map(String::from).collect(),
                only: args.iter().position(|a| a == "--solo").map(|i| {
                    let (a, b) = args[i + 1].split_once('-').expect("--solo DA-A");
                    a.parse().expect("--solo")..b.parse().expect("--solo")
                }),
                min: get("--min-deals", "500").parse().expect("--min-deals"),
                max: get("--max-deals", "4000").parse().expect("--max-deals"),
                screen: get("--screen", &player),
                screen_deals: get("--screen-deals", "250").parse().expect("--screen-deals"),
                personas: get("--personaggi", "200").parse().expect("--personaggi"),
                top: get("--top", "20").parse().expect("--top"),
                seed,
            }
            .run();
            println!("{md}");
            write_out(&md);
            return ExitCode::SUCCESS;
        }
        Some("esame") => {
            let n: usize = get("--positions", "20000").parse().expect("--positions");
            return match banco::exam(&player, n, seed) {
                Ok(md) => {
                    println!("{md}");
                    write_out(&md);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("strati") => {
            let baseline = get("--baseline", &format!("{}/{player}", latest_release()));
            let opponent = get("--opponent", "heuristic");
            return match banco::strata(&player, &baseline, &opponent, deals, seed) {
                Ok(md) => {
                    println!("{md}");
                    write_out(&md);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("diagnosi") => {
            let oracle: u32 = get("--oracolo", "20000").parse().expect("--oracolo");
            return match banco::diagnose(&player, get("--deals", "300").parse().expect("--deals"), oracle, seed) {
                Ok(md) => {
                    println!("{md}");
                    write_out(&md);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("dati") => {
            // Dati per la rete di politica, a blocchi: un blocco già scritto si salta.
            let out = get("--out", "dati");
            let opponent = get("--opponent", &player);
            let games: u64 = get("--games", "10000").parse().expect("--games");
            let chunk: u64 = get("--chunk", "500").parse().expect("--chunk");
            std::fs::create_dir_all(&out).expect("cartella dei dati");
            let start = std::time::Instant::now();
            for from in (0..games).step_by(chunk as usize) {
                let path = std::path::Path::new(&out).join(format!("politica_{from:08}.pol"));
                if path.exists() {
                    continue;
                }
                let to = (from + chunk).min(games);
                match arena::policy_records(&player, &opponent, from..to, seed) {
                    Ok(b) => {
                        let tmp = path.with_extension("tmp");
                        std::fs::write(&tmp, b).expect("scrittura dati");
                        std::fs::rename(&tmp, &path).expect("scrittura dati");
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::FAILURE;
                    }
                }
                eprintln!("{to}/{games} partite ({:.0}s)", start.elapsed().as_secs_f64());
            }
            return ExitCode::SUCCESS;
        }
        Some("deduzione") => {
            use briscola_core::net::Net;
            let nets: Vec<(String, std::sync::Arc<Net>)> = match args.iter().position(|a| a == "--reti") {
                None => vec![("rete della v0.5.0".into(), std::sync::Arc::new(briscola_core::net::policy().clone()))],
                Some(i) => args[i + 1]
                    .split(',')
                    .map(|p| {
                        let net = Net::from_bytes(&std::fs::read(p).expect("lettura rete")).expect("rete");
                        (std::path::Path::new(p).file_stem().unwrap().to_string_lossy().into_owned(), std::sync::Arc::new(net))
                    })
                    .collect(),
            };
            let mut variants = Vec::new();
            for (name, net) in &nets {
                for v in get("--varianti", "0:0,1:0.25,1:0,1.5:0.25,2:0.25,1.5:0.1").split(',') {
                    let (t, f) = v.split_once(':').expect("--varianti TEMP:MINIMO");
                    let (temp, floor): (f32, f32) = (t.parse().expect("temperatura"), f.parse().expect("minimo"));
                    let label = if temp == 0.0 { "nessuna deduzione (a caso)".to_string() } else { format!("{name}, temperatura {}, minimo {}", t.replace('.', ","), f.replace('.', ",")) };
                    variants.push(banco::Variant { label, net: net.clone(), temp, floor });
                }
            }
            return match banco::deduction_quality(&player, get("--deals", "300").parse().expect("--deals"), &variants, seed) {
                Ok(md) => {
                    println!("{md}");
                    write_out(&md);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("torneo") => {
            let list = get("--giocatori", &banco::REPRESENTATIVES.join(","));
            let players: Vec<String> = list.split(',').map(String::from).collect();
            let md = banco::tournament(&players, get("--deals", "100").parse().expect("--deals"), seed);
            println!("{md}");
            write_out(&md);
            return ExitCode::SUCCESS;
        }
        Some("report") => {
            let self_deals: u64 = get("--self-deals", "10000").parse().expect("--self-deals");
            let md = report(&player, deals, self_deals, seed);
            println!("{md}");
            write_out(&md);
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    if has("--panel") {
        let opponents: Vec<(&str, &str)> = if has("--strong") {
            STRONG.to_vec()
        } else if has("--extended") {
            PANEL.iter().chain(EXTRA).chain(STRONG).copied().collect()
        } else {
            PANEL.to_vec()
        };
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
    let total = s.tally;
    let n = total.games() as f64;
    let (lo, hi) = wilson95(total.wins as f64, n);
    let (slo, shi) = s.score_ci95();
    println!("{} contro {}", describe(&player), describe(&opponent));
    println!("Partite: {} ({} mazzi in duplicato), seme {seed}", total.games(), deals);
    println!("Vittorie {} | Pareggi {} | Sconfitte {}", total.wins, total.draws, total.losses);
    println!(
        "Percentuale vittorie: {:.2}%  (IC 95%: {:.2}% - {:.2}%)",
        total.win_rate() * 100.0, lo * 100.0, hi * 100.0
    );
    println!(
        "Punteggio (pareggio = ½): {:.2}%  (IC 95% sui mazzi: {:.2}% - {:.2}%, Elo {:+.0})",
        s.score() * 100.0, slo * 100.0, shi * 100.0, s.elo()
    );
    println!("Punti medi del giocatore: {:.2} / 120", total.avg_points());
    println!("Tempo: {:.1}s", start.elapsed().as_secs_f64());
    ExitCode::SUCCESS
}
