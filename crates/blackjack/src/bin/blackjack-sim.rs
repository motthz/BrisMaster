//! Test massivi del blackjack: rendimento delle strategie contro il banco.
//!
//! ```text
//! cargo run --release -p blackjack-core --bin blackjack-sim -- report --hands 100000000 --out bench/BLACKJACK.md
//! cargo run --release -p blackjack-core --bin blackjack-sim -- chart --rules 0
//! ```

use blackjack_core::sim::{self, Basic, HiLo, MimicDealer, NeverBust, Optimal, Random, Sloppy, Stats, Strategy};
use blackjack_core::{cards::idx_label, Chart, Rules};
use std::fmt::Write as _;
use std::time::Instant;

struct Args {
    cmd: String,
    hands: u64,
    optimal_hands: u64,
    seed: u64,
    threads: usize,
    rules: usize,
    out: Option<String>,
}

fn parse() -> Args {
    let mut a = Args {
        cmd: "report".into(),
        hands: 10_000_000,
        optimal_hands: 400_000,
        seed: 20261023,
        threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
        rules: 0,
        out: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        let mut val = || it.next().expect("valore mancante");
        match x.as_str() {
            "--hands" => a.hands = val().parse().unwrap(),
            "--optimal-hands" => a.optimal_hands = val().parse().unwrap(),
            "--seed" => a.seed = val().parse().unwrap(),
            "--threads" => a.threads = val().parse().unwrap(),
            "--rules" => a.rules = val().parse().unwrap(),
            "--out" => a.out = Some(val()),
            c if !c.starts_with('-') => a.cmd = c.into(),
            o => panic!("opzione sconosciuta {o}"),
        }
    }
    a
}

fn pct(x: f64) -> String {
    format!("{:+.3}%", 100.0 * x).replace('.', ",")
}

fn pct1(x: f64) -> String {
    format!("{:.1}%", x).replace('.', ",")
}

fn eur(x: f64) -> String {
    format!("{:+.2} €", x).replace('.', ",")
}

/// Riga della tabella delle strategie.
fn row(out: &mut String, name: &str, st: &Stats, ev: f64, ci: f64) {
    let _ = writeln!(
        out,
        "| {name} | {} | ± {} | {} | {} / {} / {} | {} | {} |",
        pct(ev),
        pct(ci).trim_start_matches('+'),
        pct(-ev).trim_start_matches('+'),
        pct1(st.pct(st.won)),
        pct1(st.pct(st.pushed)),
        pct1(st.pct(st.lost)),
        pct1(100.0 * st.sessions_up as f64 / st.sessions.max(1) as f64),
        eur(ev * 100.0 * 10.0),
    );
}

const HEADER: &str = "| Strategia | Rendimento per mano | IC 95% | Vantaggio del banco | Mani vinte / pari / perse | Sessioni da 100 mani in attivo | Risultato atteso in 100 mani da 10 € |\n|---|---:|---:|---:|---|---:|---:|";

fn report(a: &Args) -> String {
    let started = Instant::now();
    let mut out = String::new();
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let m = a.hands as f64 / 1e6;
    let _ = writeln!(out, "# Blackjack: rapporto dei test massivi\n");
    let _ = writeln!(
        out,
        "Commit `{commit}`, seme `{}`. Ogni riga: **{m:.0} milioni di mani** simulate da un sabot vero \
         (il consigliere dell'app: {:.1} milioni di mani, confrontato sulle stesse carte con la strategia di base, perché ogni sua decisione richiede un calcolo esatto).\n",
        a.seed,
        a.optimal_hands as f64 / 1e6
    );
    let _ = writeln!(
        out,
        "Il blackjack si gioca contro il banco, non contro un avversario: il criterio non può essere \
         \"vincere più del 50% delle partite\". Le misure sono:\n\n\
         - **Rendimento per mano**: guadagno medio per mano in percentuale della puntata. Negativo = si perde. \
           Con l'IC 95%, l'intervallo in cui cade il valore vero con il 95% di probabilità.\n\
         - **Vantaggio del banco**: lo stesso numero cambiato di segno, la misura standard dei casinò.\n\
         - **Mani vinte / pari / perse**: le mani con raddoppi e divisioni contano una volta, col risultato netto.\n\
         - **Sessioni da 100 mani in attivo**: quante serate da 100 mani si chiudono in guadagno. È la cosa più \
           vicina alla \"percentuale di partite vinte\" degli altri giochi.\n\
         - **Risultato atteso in 100 mani da 10 €**: quanto si perde in media.\n\n\
         Perché così tante mani: lo scarto tipo di una mano è circa 1,15 puntate, quindi con le 8.000 \
         partite usate per la briscola l'incertezza sarebbe ± 2,5%, cinque volte il vantaggio del banco. \
         Con 100 milioni di mani è ± 0,02%.\n"
    );

    // 1. Strategie con le regole online tipiche.
    let rules = Rules::preset(0);
    let chart = Chart::compute(&rules);
    let _ = writeln!(out, "## 1. Strategie a confronto: {}\n", Rules::PRESETS[0]);
    let _ = writeln!(out, "Regole: {}.\n", rules.short());
    let _ = writeln!(out, "{HEADER}");
    let strategies: Vec<(&str, Box<dyn Fn() -> Box<dyn Strategy> + Sync>)> = vec![
        (
            "Strategia di base",
            Box::new({
                let c = chart.clone();
                move || Box::new(Basic(c.clone())) as Box<dyn Strategy>
            }),
        ),
        (
            "Strategia di base con 5% di errori",
            Box::new({
                let c = chart.clone();
                move || Box::new(Sloppy(c.clone(), 0.05)) as Box<dyn Strategy>
            }),
        ),
        (
            "Strategia di base con 20% di errori",
            Box::new({
                let c = chart.clone();
                move || Box::new(Sloppy(c.clone(), 0.20)) as Box<dyn Strategy>
            }),
        ),
        ("Mai sballare (sta da 12 in su)", Box::new(|| Box::new(NeverBust) as Box<dyn Strategy>)),
        ("Imita il banco (carta fino a 17)", Box::new(|| Box::new(MimicDealer) as Box<dyn Strategy>)),
        ("Casuale (carta o stai a caso)", Box::new(|| Box::new(Random) as Box<dyn Strategy>)),
    ];
    // Strategia ottima: confronto appaiato con la strategia di base sulle stesse carte.
    let t = Instant::now();
    let paired = sim::compare(&rules, a.optimal_hands, a.seed, a.threads, &|| Box::new(Basic(chart.clone())), &|| {
        Box::new(Optimal::new(&rules))
    });
    eprintln!("ottima: {:?}", t.elapsed());
    let (gain, gain_ci) = paired.diff();
    let mut basic_stats = None;
    let mut rows = String::new();
    for (name, make) in &strategies {
        let t = Instant::now();
        let st = sim::simulate(&rules, a.hands, a.seed, a.threads, make.as_ref());
        eprintln!("{name}: {:?}", t.elapsed());
        row(&mut rows, name, &st, st.ev(), st.ci95());
        if basic_stats.is_none() {
            basic_stats = Some(st);
        }
    }
    let basic = basic_stats.unwrap();
    // La riga della strategia ottima: rendimento della base + guadagno misurato sulle stesse carte.
    let mut opt = paired.b.clone();
    opt.sessions = 0;
    let mut opt_row = String::new();
    row(
        &mut opt_row,
        "Consigliere dell'app (scelta esatta per le carte sul tavolo)",
        &opt,
        basic.ev() + gain,
        (basic.ci95().powi(2) + gain_ci.powi(2)).sqrt(),
    );
    let opt_row = opt_row.replace("| 0,0% |", "| — |");
    out += &opt_row;
    out += &rows;
    let _ = writeln!(
        out,
        "\nIl consigliere calcola per ogni decisione il valore atteso esatto di ogni azione con il sabot pieno meno le carte \
         sul tavolo: con il rimescolamento a ogni mano è la strategia ottima. Guadagno sulla strategia di base, \
         misurato sulle stesse carte: {} ± {} per mano. Le sessioni non sono calcolate per questa riga (confronto mano per mano).\n",
        pct(gain),
        pct(gain_ci).trim_start_matches('+')
    );
    let _ = writeln!(
        out,
        "Calcolo esatto per la strategia di base (EV del giro con la scelta migliore per ogni prima mano): {}; \
         simulazione: {} ± {}.\n",
        pct(chart.round_ev),
        pct(basic.ev()),
        pct(basic.ci95()).trim_start_matches('+')
    );

    // 2. Effetto delle regole.
    let _ = writeln!(out, "## 2. Quanto contano le regole del tavolo (strategia di base)\n");
    let _ = writeln!(out, "| Regole | Calcolo esatto | Simulazione | IC 95% | Sessioni da 100 mani in attivo | Dettaglio |\n|---|---:|---:|---:|---:|---|");
    for (i, name) in Rules::PRESETS.iter().enumerate() {
        let r = Rules::preset(i);
        let c = Chart::compute(&r);
        let t = Instant::now();
        let st = sim::simulate(&r, a.hands, a.seed, a.threads, &|| Box::new(Basic(c.clone())));
        eprintln!("regole {name}: {:?}", t.elapsed());
        let _ = writeln!(
            out,
            "| {name} | {} | {} | ± {} | {} | {} |",
            pct(c.round_ev),
            pct(st.ev()),
            pct(st.ci95()).trim_start_matches('+'),
            pct1(100.0 * st.sessions_up as f64 / st.sessions as f64),
            r.short()
        );
    }
    let _ = writeln!(
        out,
        "\nIl calcolo esatto valuta ogni prima mano con le probabilità esatte del sabot e approssima le \
         divisioni (niente ridivisioni); la simulazione gioca tutto, ridivisioni comprese. \
         Le differenze tra le regole corrispondono a quelle pubblicate: il banco che chiede carta sul 17 morbido \
         costa circa 0,2 punti, il blackjack pagato 6:5 circa 1,4 punti.\n"
    );

    // 3. Conteggio delle carte.
    let _ = writeln!(out, "## 3. Contare le carte (Hi-Lo) serve solo con un sabot vero\n");
    let _ = writeln!(
        out,
        "Strategia di base (senza deviazioni) con puntata variabile secondo il conteggio Hi-Lo: 1 unità \
         fino a conteggio vero 1, poi 2 unità per ogni punto in più, fino a 12. Il rendimento è per mano, \
         in unità minime, e per unità effettivamente puntata.\n"
    );
    let _ = writeln!(out, "| Situazione | Rendimento per mano (unità) | IC 95% | Puntata media | Rendimento per unità puntata |\n|---|---:|---:|---:|---:|");
    let ac = Rules::preset(1);
    let ac_chart = Chart::compute(&ac);
    let csm = Rules { penetration: 0.0, ..ac };
    for (name, r, spread) in [
        ("Sabot da 8 mazzi rimescolato al 75%, puntata fissa", ac, 1.0),
        ("Sabot da 8 mazzi rimescolato al 75%, Hi-Lo 1-12", ac, 12.0),
        ("Rimescolamento a ogni mano (online), Hi-Lo 1-12", csm, 12.0),
    ] {
        let c = ac_chart.clone();
        let st = sim::simulate(&r, a.hands, a.seed, a.threads, &move || Box::new(HiLo { chart: c.clone(), spread }));
        let _ = writeln!(
            out,
            "| {name} | {} | ± {} | {} | {} |",
            pct(st.ev()),
            pct(st.ci95()).trim_start_matches('+'),
            format!("{:.2}", st.bet / st.hands as f64).replace('.', ","),
            pct(st.ev_per_bet())
        );
    }
    let _ = writeln!(
        out,
        "\nNei giochi online a generatore casuale (e nelle macchine a rimescolamento continuo) il mazzo è \
         completo a ogni mano: il conteggio resta a zero e non dà alcun vantaggio. Nei casinò fisici con \
         sabot il conteggio può rendere positivo il rendimento, ma i casinò lo contrastano \
         (rimescolamenti anticipati, limiti di puntata, allontanamento).\n"
    );

    // 4. Conclusioni.
    let _ = writeln!(out, "## Conclusioni\n");
    let _ = writeln!(
        out,
        "- Contro il banco non esiste una strategia che vinca nel lungo periodo con un mazzo rimescolato a ogni mano: \
           anche la strategia perfetta, quella del consigliere, perde circa {} per mano con le regole online tipiche.\n\
         - La strategia di base, la tabella classica, è quasi perfetta (perde solo {} per mano rispetto al consigliere, che usa le carte esatte della mano).\n\
         - Giocare \"a sensazione\" costa molto: imitare il banco o non sballare mai perde da 5 a 10 volte di più.\n\
         - Le regole contano: cerca tavoli 3:2, con il banco che sta sul 17 morbido e con la resa; evita il 6:5.\n\
         - Nelle sessioni brevi la fortuna conta più della strategia: anche giocando perfettamente circa il {} \
           delle sessioni da 100 mani si chiude in attivo, ma la media resta negativa.\n",
        pct(-(basic.ev() + gain)).trim_start_matches('+'),
        pct(gain.abs()).trim_start_matches('+'),
        pct1(100.0 * basic.sessions_up as f64 / basic.sessions as f64),
    );
    let _ = writeln!(out, "Tempo di calcolo: {:.0} s.", started.elapsed().as_secs_f64());
    out
}

fn print_chart(rules: &Rules) {
    let c = Chart::compute(rules);
    println!("{}", rules.short());
    print!("{:>6}", "");
    for up in [1usize, 2, 3, 4, 5, 6, 7, 8, 9, 0] {
        print!("{:>4}", idx_label(up));
    }
    println!();
    for (label, cells) in c.display_rows() {
        print!("{label:>6}");
        for up in [1usize, 2, 3, 4, 5, 6, 7, 8, 9, 0] {
            print!("{:>4}", cells[up].code());
        }
        println!();
    }
    println!("EV del giro: {}", pct(c.round_ev));
}

fn main() {
    let a = parse();
    match a.cmd.as_str() {
        "report" => {
            let r = report(&a);
            match &a.out {
                Some(p) => std::fs::write(p, r).unwrap(),
                None => print!("{r}"),
            }
        }
        "chart" => print_chart(&Rules::preset(a.rules)),
        c => panic!("comando sconosciuto {c} (report, chart)"),
    }
}
