//! Banco di prova della 0.6.0: famiglie di avversari, prova sequenziale, esame delle posizioni,
//! mani storte e partite tirate, torneo tutti contro tutti.
//!
//!   briscola-sim banco   --player SPEC [--famiglie umani,persone,personaggi,motori,cattivi,bari]
//!                        [--min-deals 500] [--max-deals 4000] [--screen SPEC] [--screen-deals 250]
//!                        [--personaggi 200] [--top 20] [--out FILE]
//!   briscola-sim esame   --player SPEC [--positions 20000] [--out FILE]
//!   briscola-sim strati  --player SPEC --baseline SPEC --opponent SPEC [--deals N] [--out FILE]
//!   briscola-sim torneo  [--giocatori A,B,...] [--deals 100] [--out FILE]

use crate::arena::{describe, run_games, MatchStats};
use crate::agents::make_agent;
use crate::persona::STYLES;
use crate::{chunk_cached, pct, CHUNK};
use briscola_core::ai::{Player, Solver};
use briscola_core::cards::{Card, NUM_CARDS};
use briscola_core::{Game, Observation};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rayon::prelude::*;
use std::fmt::Write as _;

/// Una famiglia di avversari e la soglia che il motore deve superare contro ognuno.
pub struct Family {
    pub key: &'static str,
    pub title: &'static str,
    pub why: &'static str,
    /// Punteggio da superare (con certezza statistica); `None` = solo misura.
    pub threshold: Option<f64>,
    pub members: Vec<String>,
}

/// La Massima della v0.5.0 (con la rete di politica in BRISCOLA_POLICY).
pub const MASSIMA_V050: &str = "infer:20000:1:1000:0.25";

pub fn families() -> Vec<Family> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let mut umani = s(&[
        "random", "heuristic", "aggressive", "cautious", "trumphappy", "counter", "noisy:0.3:heuristic", "followsuit",
        "loadleader", "notrump",
    ]);
    umani.extend(STYLES.iter().map(|st| format!("stile:{}", st.0)));
    vec![
        Family {
            key: "umani",
            title: "Stili umani",
            why: "Le abitudini tipiche da tavolo: i 10 stili scritti a mano e i 20 stili con nome.",
            threshold: Some(0.60),
            members: umani,
        },
        Family {
            key: "persone",
            title: "Persone vere",
            why: "Giocatori bravi sotto pressione: il tempo che stringe, il tilt quando sono sotto, la distrazione.",
            threshold: Some(0.60),
            members: s(&[
                "fretta:0.4:counter", "fretta:0.4:stile:circolo", "tilt:0.4:counter", "tilt:0.4:stile:circolo",
                "noisy:0.05:counter", "noisy:0.1:stile:memoria-perfetta", "fretta:0.3:stile:nonno",
                "tilt:0.3:stile:spavaldo", "fretta:0.5:stile:domenica", "noisy:0.1:engine:400",
            ]),
        },
        Family {
            key: "personaggi",
            title: "Popolazione di personaggi",
            why: "Giocatori generati a caso da 12 manopole: si cercano i più difficili e si gioca a fondo contro quelli.",
            threshold: Some(0.60),
            members: vec![],
        },
        Family {
            key: "motori",
            title: "Motori forti",
            why: "Forza pura contro algoritmi diversi, fino a 100.000 iterazioni, e contro le versioni rilasciate.",
            threshold: Some(0.50),
            members: s(&[
                "ismcts:5000", "ismcts:20000", "ismcts:50000", "ismcts:100000", "ismcts+:2000", "ismcts+:20000",
                "ismcts+:50000", "engine:400", "engine:3000", "engine:6000", MASSIMA_V050, "politica",
                "v0.3.0/ismcts+:20000", "v0.2.0/engine:1000", "v0.1.1/engine:1000",
            ]),
        },
        Family {
            key: "cattivi",
            title: "Avversari cattivi",
            why: "Chi non si lascia prevedere: cambia stile a metà partita o sceglie a caso tra le mosse buone. \
                  Gli sfruttatori allenati contro il motore arrivano nella fase 8.",
            threshold: Some(0.50),
            members: s(&["camaleonte:1", "camaleonte:2", "misto:20000"]),
        },
        Family {
            key: "bari",
            title: "Super-bari (tetto)",
            why: "Vedono le carte nascoste: non sono un obiettivo, dicono quanto margine resta.",
            threshold: None,
            members: s(&["peek:20000", "vedetutto:20000"]),
        },
    ]
}

/// Intervallo del punteggio con `z` deviazioni standard (sulle medie per mazzo).
fn ci(s: &MatchStats, z: f64) -> (f64, f64) {
    let n = s.deals as f64;
    if n < 2.0 {
        return (0.0, 1.0);
    }
    let mean = s.score();
    let var = (s.sum_sq - n * mean * mean) / (n - 1.0);
    let half = z * (var.max(0.0) / n).sqrt();
    (mean - half, mean + half)
}

/// Prova sequenziale: blocchi di mazzi finché il risultato rispetto alla soglia è chiaro
/// (al 99,5%, più severo del 95% perché si guarda il risultato più volte) o si arriva a `max`.
pub fn sequential(player: &str, opp: &str, min: u64, max: u64, thr: Option<f64>, seed: u64) -> MatchStats {
    let mut s = MatchStats::default();
    let mut start = 0;
    while start < max {
        let end = (start + CHUNK).min(max);
        s = chunk_cached(player, opp, seed, start, end).into_iter().fold(s, MatchStats::merge);
        start = end;
        if start >= min {
            let (lo, hi) = ci(&s, 2.807);
            match thr {
                Some(t) if lo > t || hi < t => break,
                None => break,
                _ => {}
            }
        }
    }
    let (lo, hi) = s.score_ci95();
    eprintln!("  {} contro {}: {} ({} – {}), {} partite", describe(player), describe(opp), pct(s.score()), pct(lo), pct(hi), s.deals * 2);
    s
}

fn mark(s: &MatchStats, thr: Option<f64>) -> String {
    let Some(t) = thr else { return "misura".into() };
    let (lo, hi) = s.score_ci95();
    let t100 = (t * 100.0).round();
    if lo > t {
        format!("✅ sopra il {t100}%")
    } else if hi < t {
        format!("❌ sotto il {t100}%")
    } else {
        "≈ al limite".into()
    }
}

fn row(md: &mut String, name: &str, s: &MatchStats, thr: Option<f64>) {
    let (lo, hi) = s.score_ci95();
    writeln!(md, "| {name} | {} | **{}** | {} – {} | {} |", s.deals * 2, pct(s.score()), pct(lo), pct(hi), mark(s, thr)).unwrap();
}

const ROW_HEAD: &str = "| Avversario | Partite | Punteggio | IC 95% | Obiettivo |\n|---|---:|---:|---|---|";

pub struct Banco<'a> {
    pub player: &'a str,
    pub families: Vec<String>,
    pub min: u64,
    pub max: u64,
    pub screen: String,
    pub screen_deals: u64,
    pub personas: u64,
    pub top: usize,
    pub seed: u64,
}

impl Banco<'_> {
    pub fn run(&self) -> String {
        let t0 = std::time::Instant::now();
        let mut md = String::new();
        let mut summary: Vec<(String, usize, usize, usize)> = Vec::new();
        let mut games = 0u64;
        for f in families().into_iter().filter(|f| self.families.iter().any(|k| k == f.key)) {
            writeln!(md, "## {}\n\n{}\n", f.title, f.why).unwrap();
            let mut rows: Vec<(String, MatchStats)> = Vec::new();
            if f.key == "personaggi" {
                let (hardest, screen_md, g) = self.screen_personas();
                games += g;
                md.push_str(&screen_md);
                for p in hardest {
                    rows.push((describe(&p), sequential(self.player, &p, self.min, self.max, f.threshold, self.seed)));
                }
            } else {
                for opp in &f.members {
                    if opp == self.player {
                        writeln!(md, "_{}: è il giocatore stesso, 50% per costruzione._\n", describe(opp)).unwrap();
                        continue;
                    }
                    rows.push((describe(opp), sequential(self.player, opp, self.min, self.max, f.threshold, self.seed)));
                }
            }
            writeln!(md, "{ROW_HEAD}").unwrap();
            for (n, s) in &rows {
                row(&mut md, n, s, f.threshold);
                games += s.deals * 2;
            }
            writeln!(md).unwrap();
            if let Some(t) = f.threshold {
                let above = rows.iter().filter(|(_, s)| s.score_ci95().0 > t).count();
                let below = rows.iter().filter(|(_, s)| s.score_ci95().1 < t).count();
                summary.push((f.title.to_string(), rows.len(), above, below));
            }
        }
        let mut head = String::new();
        writeln!(head, "# Banco di prova: {}\n", describe(self.player)).unwrap();
        writeln!(
            head,
            "Giocatore `{}`, commit `{}`, {}. Seme `{}`. Prova sequenziale: blocchi di {CHUNK} mazzi in duplicato, \
             da {} a {} mazzi ({} – {} partite); ci si ferma prima quando il risultato rispetto alla soglia è certo al 99,5%.\n",
            self.player, crate::commit(), crate::today(), self.seed, self.min, self.max, self.min * 2, self.max * 2
        ).unwrap();
        writeln!(head, "| Famiglia | Avversari | Sopra la soglia | Sotto la soglia | Al limite |\n|---|---:|---:|---:|---:|").unwrap();
        for (t, n, a, b) in &summary {
            writeln!(head, "| {t} | {n} | {a} | {b} | {} |", n - a - b).unwrap();
        }
        writeln!(head, "\n{games} partite, {:.1} ore di calcolo su questa macchina.\n", t0.elapsed().as_secs_f64() / 3600.0).unwrap();
        head + &md
    }

    /// Tutti i personaggi contro il giocatore di vaglio, pochi mazzi a testa: i più difficili passano alla prova vera.
    fn screen_personas(&self) -> (Vec<String>, String, u64) {
        let mut scores: Vec<(u64, f64)> = (0..self.personas)
            .map(|n| {
                let s = chunk_cached(&self.screen, &format!("persona:{n}"), self.seed, 0, self.screen_deals)
                    .into_iter()
                    .fold(MatchStats::default(), MatchStats::merge);
                (n, s.score())
            })
            .collect();
        scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let mut md = String::new();
        let q = |f: f64| pct(scores[((scores.len() - 1) as f64 * f) as usize].1);
        writeln!(
            md,
            "Vaglio: {} personaggi, {} partite ciascuno contro {}. Punteggio minimo {}, primo quartile {}, mediana {}, massimo {}; \
             sopra il 60%: {} su {}. I {} più difficili passano alla prova completa qui sotto.\n",
            scores.len(), self.screen_deals * 2, describe(&self.screen), q(0.0), q(0.25), q(0.5), q(1.0),
            scores.iter().filter(|s| s.1 > 0.6).count(), scores.len(), self.top.min(scores.len())
        ).unwrap();
        let hardest = scores.iter().take(self.top).map(|(n, _)| format!("persona:{n}")).collect();
        (hardest, md, self.personas * self.screen_deals * 2)
    }
}

// ---------------------------------------------------------------------------
// Esame delle posizioni del finale (mazzo vuoto: informazione completa, mossa giusta esatta).

/// Una posizione d'esame: la partita fino a quel punto e il risultato esatto di ogni mossa.
pub struct Position {
    pub order: Vec<Card>,
    pub moves: Vec<Card>,
    /// Esito esatto (1, ½, 0) per chi muove di ogni carta che ha in mano.
    pub outcomes: Vec<(Card, f64)>,
}

impl Position {
    fn best(&self) -> f64 {
        self.outcomes.iter().map(|o| o.1).fold(0.0, f64::max)
    }
}

fn outcome(pts: u8) -> f64 {
    if pts > 60 { 1.0 } else if pts == 60 { 0.5 } else { 0.0 }
}

/// `n` posizioni a mazzo vuoto in cui la scelta conta (almeno una carta cambia l'esito),
/// da partite tra personaggi diversi. Sempre le stesse per lo stesso seme.
pub fn exam_positions(n: usize, seed: u64) -> Vec<Position> {
    let mut out = Vec::with_capacity(n);
    let mut next = 0u64;
    while out.len() < n {
        let batch = (n - out.len()) as u64 * 2;
        let found: Vec<Position> = (next..next + batch).into_par_iter().filter_map(|i| exam_game(i, seed)).collect();
        next += batch;
        out.extend(found);
    }
    out.truncate(n);
    out
}

/// Una partita tra due personaggi; restituisce una delle sue posizioni decisive a mazzo vuoto, a caso.
fn exam_game(i: u64, seed: u64) -> Option<Position> {
    let mut rng = SmallRng::seed_from_u64(seed ^ i.wrapping_mul(0xA24B_AED4_963E_E407));
    let mut order: Vec<Card> = (0..NUM_CARDS as u8).collect();
    order.shuffle(&mut rng);
    let mut players = [crate::persona::Persona::numbered(i % 200), crate::persona::Persona::numbered((i * 7 + 3) % 200)];
    let mut g = Game::from_order(&order, 0);
    let mut moves = Vec::new();
    let mut found = Vec::new();
    let mut solver = Solver::default();
    while !g.is_over() {
        let p = g.to_move();
        if g.deck_len == 0 && g.hands[p as usize].len() >= 2 {
            let outcomes: Vec<(Card, f64)> = g
                .legal_moves()
                .iter()
                .map(|c| {
                    let mut n = g;
                    n.play(c);
                    (c, outcome(solver.solve(&n, p)))
                })
                .collect();
            if outcomes.iter().any(|o| o.1 != outcomes[0].1) {
                found.push(Position { order: order.clone(), moves: moves.clone(), outcomes });
            }
        }
        let c = players[p as usize].choose(&Observation::of(&g, p), &mut rng);
        g.play(c);
        moves.push(c);
    }
    if found.is_empty() {
        return None;
    }
    let k = rng.gen_range(0..found.len());
    Some(found.swap_remove(k))
}

pub fn exam(player: &str, n: usize, seed: u64) -> Result<String, String> {
    make_agent(player)?;
    let positions = exam_positions(n, seed);
    let results: Vec<(bool, usize, bool)> = positions
        .par_iter()
        .enumerate()
        .map(|(i, pos)| {
            let mut a = make_agent(player).unwrap();
            let mut rng = SmallRng::seed_from_u64(seed.wrapping_add(i as u64));
            a.start(&pos.order, 0);
            let mut g = Game::from_order(&pos.order, 0);
            for &c in &pos.moves {
                g.play(c);
                a.observe(c);
            }
            let me = g.to_move();
            let c = a.choose(&g, me, &mut rng);
            let got = pos.outcomes.iter().find(|o| o.0 == c).map_or(0.0, |o| o.1);
            (got == pos.best(), g.hands[me as usize].len() as usize, g.table.is_some())
        })
        .collect();
    let mut md = String::new();
    let ok = results.iter().filter(|r| r.0).count();
    writeln!(md, "## Esame del finale: {}\n", describe(player)).unwrap();
    writeln!(
        md,
        "{} posizioni a mazzo vuoto prese da partite tra personaggi, scelte solo dove la carta giocata cambia l'esito. \
         La mossa giusta è calcolata in modo esatto. Giuste: **{} su {} ({})**.\n",
        results.len(), ok, results.len(), pct(ok as f64 / results.len().max(1) as f64)
    ).unwrap();
    writeln!(md, "| Situazione | Posizioni | Giuste |\n|---|---:|---:|").unwrap();
    for (label, f) in [
        ("3 carte in mano, apre", (3usize, false)),
        ("3 carte in mano, risponde", (3, true)),
        ("2 carte in mano, apre", (2, false)),
        ("2 carte in mano, risponde", (2, true)),
    ] {
        let sel: Vec<_> = results.iter().filter(|r| (r.1, r.2) == f).collect();
        let good = sel.iter().filter(|r| r.0).count();
        writeln!(md, "| {label} | {} | {} |", sel.len(), pct(good as f64 / sel.len().max(1) as f64)).unwrap();
    }
    Ok(md)
}

// ---------------------------------------------------------------------------
// Mani storte e partite tirate.

pub fn strata(player: &str, baseline: &str, opponent: &str, deals: u64, seed: u64) -> Result<String, String> {
    let a = run_games(player, opponent, 0..deals, seed)?;
    let b = run_games(baseline, opponent, 0..deals, seed)?;
    let score = |v: &[&crate::arena::GameRec]| v.iter().map(|g| outcome(g.points)).sum::<f64>() / v.len().max(1) as f64;
    let mut md = String::new();
    writeln!(md, "## Mani storte e partite tirate\n").unwrap();
    writeln!(
        md,
        "{} e {} contro {}, sugli stessi {deals} mazzi in duplicato ({} partite ciascuno). Le partite sono divise \
         per com'era la mano iniziale del giocatore.\n",
        describe(player), describe(baseline), describe(opponent), deals * 2
    ).unwrap();
    writeln!(md, "| Gruppo | Partite | {} | {} | Differenza |\n|---|---:|---:|---:|---:|", describe(player), describe(baseline)).unwrap();
    type Filter = Box<dyn Fn(&crate::arena::GameRec) -> bool>;
    let groups: Vec<(String, Filter)> = vec![
        ("Tutte".into(), Box::new(|_| true)),
        ("Nessuna briscola in mano".into(), Box::new(|g| g.trumps == 0)),
        ("Una briscola".into(), Box::new(|g| g.trumps == 1)),
        ("Due briscole".into(), Box::new(|g| g.trumps == 2)),
        ("Tre briscole".into(), Box::new(|g| g.trumps == 3)),
        ("Mano senza punti".into(), Box::new(|g| g.hand_points == 0)),
        ("Mano con 1–10 punti".into(), Box::new(|g| (1..=10).contains(&g.hand_points))),
        ("Mano con più di 10 punti".into(), Box::new(|g| g.hand_points > 10)),
        ("Nessuna briscola e nessun punto".into(), Box::new(|g| g.trumps == 0 && g.hand_points == 0)),
        ("Apre la prima presa".into(), Box::new(|g| g.seat == 0)),
        ("Risponde alla prima presa".into(), Box::new(|g| g.seat == 1)),
    ];
    for (label, f) in &groups {
        let sa: Vec<_> = a.iter().filter(|g| f(g)).collect();
        let sb: Vec<_> = b.iter().filter(|g| f(g)).collect();
        let (x, y) = (score(&sa), score(&sb));
        writeln!(md, "| {label} | {} | {} | {} | {:+.1} |", sa.len(), pct(x), pct(y), (x - y) * 100.0).unwrap();
    }
    let close = |v: &[crate::arena::GameRec]| {
        let c: Vec<_> = v.iter().filter(|g| (55..=65).contains(&g.points)).collect();
        (c.len(), score(&c))
    };
    let (ca, xa) = close(&a);
    let (cb, xb) = close(&b);
    writeln!(
        md,
        "\nPartite tirate (decise da 10 punti o meno): {} ne vince il {} su {ca}, {} il {} su {cb}.\n",
        describe(player), pct(xa), describe(baseline), pct(xb)
    ).unwrap();
    Ok(md)
}

// ---------------------------------------------------------------------------
// Torneo tutti contro tutti con classifica Elo.

/// Rappresentanti delle famiglie per il torneo.
pub const REPRESENTATIVES: &[&str] = &[
    "random", "heuristic", "aggressive", "cautious", "trumphappy", "counter", "followsuit", "loadleader", "notrump",
    "noisy:0.3:heuristic", "stile:circolo", "stile:liscio", "stile:caricatore", "stile:memoria-perfetta", "stile:domenica",
    "stile:nonno", "stile:prende-tutto", "stile:avaro", "stile:tilt", "stile:cacciatore", "persona:0", "persona:1",
    "persona:2", "persona:3", "persona:4", "fretta:0.4:counter", "tilt:0.4:stile:circolo", "noisy:0.1:engine:400",
    "camaleonte:1", "politica", "misto:5000", "engine:400", "engine:3000", "ismcts:2000", "ismcts:20000",
    "ismcts+:2000", "ismcts+:20000", "v0.3.0/ismcts+:20000", "v0.1.1/engine:1000", MASSIMA_V050,
];

pub fn tournament(players: &[String], deals: u64, seed: u64) -> String {
    let n = players.len();
    let mut score = vec![vec![0.0f64; n]; n];
    let pairs: Vec<(usize, usize)> = (0..n).flat_map(|i| (i + 1..n).map(move |j| (i, j))).collect();
    for (i, j) in pairs {
        let mut s = MatchStats::default();
        for start in (0..deals).step_by(CHUNK as usize) {
            s = chunk_cached(&players[i], &players[j], seed, start, (start + CHUNK).min(deals)).into_iter().fold(s, MatchStats::merge);
        }
        score[i][j] = s.score();
        score[j][i] = 1.0 - s.score();
    }
    // Elo col modello di Bradley-Terry: salita del gradiente sulla verosimiglianza.
    let mut elo = vec![0.0f64; n];
    for _ in 0..5000 {
        let mut grad = vec![0.0; n];
        for i in 0..n {
            for j in 0..n {
                if i != j {
                    let e = 1.0 / (1.0 + 10f64.powf((elo[j] - elo[i]) / 400.0));
                    grad[i] += score[i][j] - e;
                }
            }
        }
        for i in 0..n {
            elo[i] += 10.0 * grad[i];
        }
        let mean = elo.iter().sum::<f64>() / n as f64;
        elo.iter_mut().for_each(|e| *e -= mean);
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| elo[b].partial_cmp(&elo[a]).unwrap());
    let mut md = String::new();
    writeln!(md, "## Torneo tutti contro tutti\n").unwrap();
    writeln!(
        md,
        "{n} giocatori, {} sfide da {} partite in duplicato, seme `{seed}`. Elo con il modello di Bradley-Terry (media 0).\n",
        n * (n - 1) / 2, deals * 2
    ).unwrap();
    writeln!(md, "| Posto | Giocatore | Elo | Punteggio medio |\n|---:|---|---:|---:|").unwrap();
    for (k, &i) in idx.iter().enumerate() {
        let avg = score[i].iter().sum::<f64>() / (n - 1) as f64;
        writeln!(md, "| {} | {} | {:+.0} | {} |", k + 1, describe(&players[i]), elo[i], pct(avg)).unwrap();
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_parse() {
        for f in families() {
            for m in &f.members {
                if m != MASSIMA_V050 {
                    assert!(make_agent(m).is_ok(), "{m}");
                }
            }
        }
        for r in REPRESENTATIVES.iter().filter(|r| **r != MASSIMA_V050) {
            assert!(make_agent(r).is_ok(), "{r}");
        }
    }

    #[test]
    fn exam_positions_are_decisive_and_the_solver_passes() {
        let pos = exam_positions(30, 1);
        assert_eq!(pos.len(), 30);
        for p in &pos {
            assert!(p.outcomes.iter().any(|o| o.1 != p.best()));
        }
        // Il contacarte gioca il finale col risolutore esatto: deve prendere il 100%.
        let md = exam("counter", 30, 1).unwrap();
        assert!(md.contains("30 su 30"), "{md}");
    }

    #[test]
    fn sequential_stops_early_when_clear() {
        let s = sequential("engine:20", "random", 500, 4000, Some(0.6), 3);
        assert_eq!(s.deals, 500);
    }
}
