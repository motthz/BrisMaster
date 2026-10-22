//! Banco di prova: decine di migliaia di partite tra il motore e altri giocatori.
//!
//! Ogni mazzo viene giocato due volte a posti invertiti ("duplicato"),
//! così la fortuna delle carte si compensa e il confronto è più preciso.
//!
//! Uso:
//!   briscola-sim [--deals N] [--player SPEC] [--opponent SPEC] [--seed X]
//!   briscola-sim --panel [--extended] [--deals N] [--player SPEC] [--out FILE]
//!   briscola-sim gate [--player SPEC] [--baseline SPEC] [--deals N] [--summary FILE]
//!   briscola-sim report [--player SPEC] [--deals N] [--out FILE]
//!
//! SPEC: vedi `agents.rs` (es. engine:1000, ismcts:2000, noisy:0.25:heuristic, v0.1.1/engine:1000).

mod agents;
mod arena;
mod opponents;

use arena::{describe, run_match, MatchStats};
use briscola_core::stats::wilson95;
use std::fmt::Write as _;
use std::process::ExitCode;
use std::time::Instant;

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

fn run(player: &str, opponent: &str, deals: u64, seed: u64) -> MatchStats {
    let start = Instant::now();
    let s = run_match(player, opponent, deals, seed).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    eprintln!("  {} contro {}: {}  ({:.0}s)", describe(player), describe(opponent), pct(s.score()), start.elapsed().as_secs_f64());
    s
}

/// Tabella contro un pannello di avversari. Restituisce le righe Markdown.
fn panel(player: &str, opponents: &[(&str, &str)], deals: u64, seed: u64, md: &mut String) {
    table_header(md);
    for (opp, _) in opponents {
        let s = run(player, opp, deals, seed);
        writeln!(md, "{}", table_row(&describe(opp), &s)).unwrap();
    }
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

/// Rapporto completo: pannello esteso, partite contro se stesso, versione precedente.
fn report(player: &str, deals: u64, seed: u64) -> String {
    let start = Instant::now();
    let name = describe(player);
    let base = format!("{}/{player}", latest_release());
    let mut md = String::new();
    writeln!(md, "# Rapporto dei test massivi\n").unwrap();
    writeln!(md, "Motore provato: **{name}** (`{player}`), commit `{}`, {}.", commit(), today()).unwrap();
    writeln!(md, "Ogni riga: {deals} mazzi giocati due volte a posti invertiti ({} partite), seme `{seed}`. Gli stessi mazzi per tutti gli avversari.\n", deals * 2).unwrap();
    writeln!(md, "Come leggere le colonne:\n").unwrap();
    writeln!(md, "- **% vittorie**: partite vinte (i pareggi 60-60 non contano).").unwrap();
    writeln!(md, "- **Punteggio**: vittoria 1, pareggio ½, sconfitta 0. Il 50% è la parità: sopra il 50% il motore è più forte.").unwrap();
    writeln!(md, "- **IC 95%**: intervallo di confidenza del punteggio, calcolato sulle coppie di partite dello stesso mazzo. Se è tutto sopra il 50%, il vantaggio non è dovuto al caso.").unwrap();
    writeln!(md, "- **Elo**: la stessa differenza di forza espressa in punti Elo (+100 ≈ 64% di punteggio).\n").unwrap();

    writeln!(md, "## 1. Contro tanti stili di gioco\n").unwrap();
    let all: Vec<(&str, &str)> = PANEL.iter().chain(EXTRA).copied().collect();
    let mut games = 0u64;
    let mut rows = String::new();
    panel(player, &all, deals, seed, &mut rows);
    games += all.len() as u64 * deals * 2;
    md.push_str(&rows);
    writeln!(md, "\n| Avversario | Come gioca |\n|---|---|").unwrap();
    for (spec, how) in &all {
        writeln!(md, "| {} | {how} |", describe(spec)).unwrap();
    }

    writeln!(md, "\n## 2. Contro se stesso\n").unwrap();
    let selfp = run(player, player, deals, seed);
    games += deals * 2;
    let (lo, hi) = selfp.score_ci95();
    table_header(&mut md);
    writeln!(md, "{}\n", table_row(&format!("{name} (copia)"), &selfp)).unwrap();
    let opener = selfp.seat_wins[0] as f64 / deals as f64;
    let (olo, ohi) = wilson95(selfp.seat_wins[0] as f64, deals as f64);
    let responder = selfp.seat_wins[1] as f64 / deals as f64;
    writeln!(
        md,
        "Punteggio {} (IC {} – {}): come previsto, due copie dello stesso motore si dividono le partite a metà. \
         Non è un difetto: in duplicato ogni mazzo viene giocato da entrambe le copie da entrambi i posti, \
         quindi per simmetria nessuna delle due può fare meglio del 50%. Vale per qualsiasi programma: \
         contro un avversario forte quanto lui nessuno può garantire più del 50%. \
         Questo test serve a controllare che il banco di prova sia equo (un risultato lontano dal 50% indicherebbe un errore).\n",
        pct(selfp.score()), pct(lo), pct(hi)
    ).unwrap();
    writeln!(
        md,
        "Chi apre la prima presa vince il {} delle partite (IC {} – {}), chi risponde il {}: \
         il posto conta poco, ed è comunque compensato dal duplicato.\n",
        pct(opener), pct(olo), pct(ohi), pct(responder)
    ).unwrap();

    writeln!(md, "## 3. Contro la versione precedente\n").unwrap();
    let old = run(player, &base, deals, seed);
    games += deals * 2;
    let (lo, hi) = old.score_ci95();
    table_header(&mut md);
    writeln!(md, "{}\n", table_row(&describe(&base), &old)).unwrap();
    let verdict = if hi < 0.5 {
        "la versione attuale è **più debole** della precedente: è una regressione."
    } else if lo > 0.5 {
        "la versione attuale è **più forte** della precedente."
    } else {
        "nessuna differenza significativa tra le due versioni."
    };
    writeln!(md, "Contro {}: {verdict}", describe(&base)).unwrap();
    writeln!(
        md,
        "La stessa prova gira in automatico su ogni pull request (job *Regressione* in `.github/workflows/ci.yml`) \
         e blocca le modifiche che rendono il motore significativamente più debole della release {}.\n",
        latest_release()
    ).unwrap();

    writeln!(md, "## Come riprodurlo\n").unwrap();
    writeln!(md, "```\ncargo run --release -p briscola-sim -- report --player {player} --deals {deals} --seed {seed} --out bench/RAPPORTO.md\n```\n").unwrap();
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
        Some("report") => {
            let md = report(&player, deals, seed);
            println!("{md}");
            write_out(&md);
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    if has("--panel") {
        let opponents: Vec<(&str, &str)> =
            if has("--extended") { PANEL.iter().chain(EXTRA).copied().collect() } else { PANEL.to_vec() };
        println!("{} contro il pannello di avversari, {} partite ciascuno", describe(&player), deals * 2);
        let mut md = String::new();
        panel(&player, &opponents, deals, seed, &mut md);
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
