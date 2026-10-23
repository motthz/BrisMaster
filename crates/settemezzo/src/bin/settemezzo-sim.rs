//! Rapporto sul Sette e mezzo: valore atteso esatto di ogni strategia contro ogni modello di banco,
//! verificato con una simulazione indipendente di milioni di mani.
//!
//! Uso: settemezzo-sim [--hands N] [--seed X] [--out FILE]

use settemezzo::sim::{simulate, Giocatore, Strategia};
use settemezzo::{Banco, Dist, Rules, Solver};
use std::fmt::Write as _;
use std::process::ExitCode;
use std::time::Instant;

fn pct(x: f64) -> String {
    format!("{:+.2}%", x * 100.0).replace('.', ",")
}
fn pct0(x: f64) -> String {
    format!("{:.1}%", x * 100.0).replace('.', ",")
}

fn strategie(banco: Banco) -> Vec<Strategia> {
    let mut v = vec![Strategia::Ottimale];
    for b in [Banco::Esperto, Banco::Soglia(11)] {
        if b != banco {
            v.push(Strategia::OttimaleContro(b));
        }
    }
    v.extend((8..=13).map(Strategia::Soglia));
    v.push(Strategia::Casuale);
    v
}

fn exact(rules: Rules, banco: Banco, s: Strategia) -> Dist {
    let mut solver = Solver::new(rules, banco);
    let mut g = Giocatore::new(s, rules, banco);
    solver.evaluate(&rules.full_deck(), &mut |d, hc, up| g.hit_prob(d, hc, up))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let hands: u64 = arg("--hands").map_or(1_000_000, |s| s.parse().expect("--hands"));
    let seed: u64 = arg("--seed").map_or(20261023, |s| s.parse().expect("--seed"));
    let out = arg("--out");
    let rules = Rules::default();
    let start = Instant::now();
    let mut ok = true;
    let mut md = String::new();

    writeln!(md, "# Sette e mezzo: rapporto\n").unwrap();
    writeln!(
        md,
        "Generato da `cargo run --release -p settemezzo -- --hands {hands} --seed {seed}`.\n\n\
         **Regole.** Mazzo da 40 carte; Asso 1, dal Due al Sette il loro valore, figure ½. \
         Il Re di Denari è la matta e vale quello che conviene (½ oppure da 1 a 7). \
         Il giocatore riceve una carta coperta, chiede carte scoperte o si ferma; se supera 7½ perde subito. \
         Poi gioca il banco: se sballa paga, altrimenti vince chi ha di più e **il pareggio va al banco**. \
         Il sette e mezzo reale (7½ con due carte) batte il 7½ normale ed è pagato **il doppio**. \
         Mazzo nuovo a ogni mano, un giocatore contro il banco.\n"
    )
    .unwrap();
    writeln!(
        md,
        "## Criteri\n\n\
         Nel Sette e mezzo contro il banco non c'è un avversario da battere \"più del 50% delle volte\": \
         il banco ha il vantaggio dei pareggi e del fatto che il giocatore sballa per primo. \
         La misura giusta è la **vincita media per mano**, in percentuale della puntata (rendimento atteso).\n\n\
         1. **Valore esatto.** Per ogni strategia il rendimento si calcola enumerando tutte le carte che possono uscire, \
            per il giocatore e per il banco: nessun errore di campionamento.\n\
         2. **Verifica con simulazione.** {hands} mani simulate per ogni coppia strategia–banco, con mazzi mescolati davvero. \
            Il valore esatto deve cadere nell'intervallo di confidenza al 95% della simulazione \
            (su tante righe qualcuna può uscirne per caso, una su venti); se una riga si discosta di più di 3,5 deviazioni standard \
            il programma esce con errore.\n\
         3. **Confronto appaiato.** Tutte le strategie giocano gli stessi mazzi, e la differenza dalla strategia ottimale \
            ha il suo intervallo di confidenza, molto più stretto di quello delle singole strategie.\n\
         4. **Robustezza.** La strategia ottimale presuppone un modello di banco: si misura quanto si perde \
            usando quella pensata per un banco diverso da quello reale.\n\
         5. **Quante mani servono.** Con le 8.000 partite usate per la briscola il rendimento di una strategia \
            si conosce solo a ±2 punti; con {hands} mani a circa ±0,2.\n"
    )
    .unwrap();
    writeln!(md, "**Modelli di banco.**\n").unwrap();
    for b in Banco::ALL {
        writeln!(md, "- *{}*: {}.", b.label(), b.description().to_lowercase()).unwrap();
    }
    writeln!(md).unwrap();

    let mut summary = Vec::new();
    for banco in Banco::ALL {
        let strat = strategie(banco);
        let t0 = Instant::now();
        let sim = simulate(rules, banco, &strat, hands, seed);
        let secs = t0.elapsed().as_secs_f64();
        writeln!(md, "## Contro il {}\n", banco.label().to_lowercase()).unwrap();
        writeln!(
            md,
            "| Strategia | Rendimento esatto | Vince | di cui reale | Sballa | Perde al confronto | Simulato (IC 95%) | Esatto nell'IC | Distanza dall'ottimale (IC 95%) |"
        )
        .unwrap();
        writeln!(md, "|---|---:|---:|---:|---:|---:|---|:-:|---|").unwrap();
        let opt_ev = exact(rules, banco, Strategia::Ottimale).ev(&rules);
        let mut best_naive: Option<(Strategia, f64)> = None;
        for (s, st) in strat.iter().zip(&sim) {
            let d = exact(rules, banco, *s);
            let ev = d.ev(&rules);
            let h = st.profit.half95();
            let inside = (st.profit.mean - ev).abs() <= h;
            // Il controllo automatico usa 3,5 deviazioni standard: con decine di righe, uscire dall'IC 95%
            // capita per caso circa una volta su venti, mentre 3,5 deviazioni segnalano un errore vero.
            ok &= (st.profit.mean - ev).abs() <= h * 3.5 / 1.96;
            if ev > opt_ev + 1e-12 {
                ok = false;
            }
            if let Strategia::Soglia(_) = s {
                if best_naive.is_none_or(|(_, b)| ev > b) {
                    best_naive = Some((*s, ev));
                }
            }
            let diff = if *s == Strategia::Ottimale {
                "—".to_string()
            } else {
                let dh = st.diff.half95();
                format!("{} ({} – {})", pct(-st.diff.mean), pct(-st.diff.mean - dh), pct(-st.diff.mean + dh))
            };
            let name = if *s == Strategia::Ottimale { format!("**{}**", s.label()) } else { s.label() };
            writeln!(
                md,
                "| {name} | **{}** | {} | {} | {} | {} | {} ± {} | {} | {diff} |",
                pct(ev),
                pct0(d.p_win()),
                pct0(d.win_reale),
                pct0(d.bust),
                pct0(d.lose),
                pct(st.profit.mean),
                format!("{:.2}", h * 100.0).replace('.', ","),
                if inside { "sì" } else { "**NO**" },
            )
            .unwrap();
        }
        let (bn, bv) = best_naive.unwrap();
        writeln!(md, "\n{} mani per strategia simulate in {:.0} s.\n", hands, secs).unwrap();
        summary.push((banco, opt_ev, bn, bv));
        eprintln!("{} fatto in {:.0} s", banco.label(), secs);
    }

    writeln!(md, "## Robustezza: strategia pensata per un banco, giocata contro un altro\n").unwrap();
    write!(md, "| Strategia pensata per ↓ / banco reale → |").unwrap();
    for b in Banco::ALL {
        write!(md, " {} |", b.label()).unwrap();
    }
    writeln!(md, "\n|---|{}", "---:|".repeat(Banco::ALL.len())).unwrap();
    for pensata in Banco::ALL {
        write!(md, "| {} |", pensata.label()).unwrap();
        for reale in Banco::ALL {
            let s = if pensata == reale { Strategia::Ottimale } else { Strategia::OttimaleContro(pensata) };
            let v = exact(rules, reale, s).ev(&rules);
            if pensata == reale {
                write!(md, " **{}** |", pct(v)).unwrap();
            } else {
                write!(md, " {} |", pct(v)).unwrap();
            }
        }
        writeln!(md).unwrap();
    }
    writeln!(md, "\nIn grassetto la strategia giusta per quel banco, il massimo della colonna.\n").unwrap();

    writeln!(md, "## Varianti di regole (valori esatti, strategia ottimale)\n").unwrap();
    writeln!(md, "| Regole | {} |", Banco::ALL.map(|b| b.label()).join(" | ")).unwrap();
    writeln!(md, "|---|{}", "---:|".repeat(Banco::ALL.len())).unwrap();
    for (label, r) in [
        ("Con matta, reale pagato doppio (quelle sopra)", Rules { matta: true, reale_paga: 2 }),
        ("Con matta, reale pagato semplice", Rules { matta: true, reale_paga: 1 }),
        ("Senza matta, reale pagato doppio", Rules { matta: false, reale_paga: 2 }),
        ("Senza matta, reale pagato semplice", Rules { matta: false, reale_paga: 1 }),
    ] {
        write!(md, "| {label} |").unwrap();
        for b in Banco::ALL {
            let v = Solver::new(r, b).optimal_value(&r.full_deck()).ev(&r);
            write!(md, " {} |", pct(v)).unwrap();
        }
        writeln!(md).unwrap();
    }

    writeln!(md, "\n## In sintesi\n").unwrap();
    writeln!(md, "| Banco | Ottimale | Migliore strategia a soglia fissa | Guadagno dell'ottimale |").unwrap();
    writeln!(md, "|---|---:|---|---:|").unwrap();
    for (b, o, s, v) in &summary {
        writeln!(md, "| {} | {} | {}: {} | {} |", b.label(), pct(*o), s.label(), pct(*v), pct(o - v)).unwrap();
    }
    writeln!(
        md,
        "\nControlli: simulazione entro 3,5 deviazioni standard dal valore esatto in ogni riga e nessuna strategia sopra l'ottimale: **{}**. \
         Tempo totale {:.0} s.",
        if ok { "superati" } else { "NON superati" },
        start.elapsed().as_secs_f64()
    )
    .unwrap();

    match out {
        Some(f) => std::fs::write(&f, &md).expect("scrittura rapporto"),
        None => print!("{md}"),
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
