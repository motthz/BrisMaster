//! Partite vere salvate dall'app (cartella Documenti\BrisMaster\Partite, oppure il file
//! `partite-brismaster.json` del pulsante "Prepara file da inviare") trasformate in dati per
//! allenare la rete di politica (la deduzione delle carte avversarie) su come giocano le
//! persone vere, e misura di quanto bene una rete prevede le loro giocate.
//!
//! Uso:
//!   reali --partite FILE_O_CARTELLA[,...] [--out CARTELLA] [--rete FILE] [--confronta FILE]
//!         [--campioni N] [--tieni K] [--controllo QUOTA] [--seed X] [--finte N]
//!
//! `--finte N` scrive prima in `--partite` (un file) N partite finte nel formato dell'app,
//! contro un avversario "umano" simulato (euristico con errori), per provare tutta la catena.
//!
//! Delle carte avversarie si sa solo quelle giocate: quando le ha pescate no. A partita finita
//! però sono tutte uscite, quindi la mano dell'avversario a ogni sua giocata è quasi
//! determinata. Si campionano `--campioni` ricostruzioni compatibili con tutto ciò che è stato
//! registrato (uniformi: ogni ordine del mazzo compatibile è ugualmente probabile), si pesano
//! con quanto la rete `--rete` spiega le giocate dell'avversario, e se ne tengono `--tieni`
//! (ricampionate per peso) come dati. È un passo di EM: con la rete allenata si può rifare.
//!
//! Con `--out` scrive `politica_00000000.pol` (allenamento) e `politica_99999999.pol` (partite
//! di controllo, una quota `--controllo` scelta in modo fisso dal contenuto della partita): è il
//! formato di `selfplay`, e `train --kind policy --init RETE` usa l'ultimo file come controllo.
//!
//! La misura (verosimiglianza marginale per giocata, sulle partite di controllo e su tutte):
//! quante volte meglio del caso la rete prevede le giocate vere. Con `--confronta` anche
//! un'altra rete sugli stessi campioni: è la verifica dopo l'allenamento.

use briscola_core::cards::{beats, Card, CardSet, NUM_CARDS};
use briscola_core::net::{canon_index, Net, Position};
use briscola_core::{Game, Observation};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const ME: u8 = 0;
const OPP: u8 = 1;

/// Una partita vera come l'ha registrata l'app.
#[derive(Clone, Debug, PartialEq)]
struct Real {
    name: String,
    trump: Card,
    first: u8,
    /// Le mie pescate in ordine (le prime tre sono la mano iniziale).
    my_draws: Vec<Card>,
    /// Giocate in ordine: chi e quale carta.
    plays: Vec<(u8, Card)>,
}

impl Real {
    fn key(&self) -> String {
        format!("{} {} {:?} {:?}", self.trump, self.first, self.my_draws, self.plays)
    }

    /// Dalla partita salvata dall'app (oggetto con `eventi`).
    fn parse(name: &str, v: &Value) -> Result<Real, String> {
        let ev = v["eventi"].as_array().ok_or("manca 'eventi'")?;
        let card = |e: &Value| -> Result<Card, String> {
            let c = e["carta"]["id"].as_u64().ok_or("carta senza id")?;
            if c as usize >= NUM_CARDS {
                return Err(format!("carta {c} inesistente"));
            }
            Ok(c as Card)
        };
        let who = |e: &Value| if e["chi"] == "io" { ME } else { OPP };
        let (mut trump, mut first) = (None, None);
        let (mut my_draws, mut plays) = (Vec::new(), Vec::new());
        for e in ev {
            match e["tipo"].as_str() {
                Some("briscola") => trump = Some(card(e)?),
                Some("primo_di_mano") => first = Some(who(e)),
                Some("pescata") => my_draws.push(card(e)?),
                Some("giocata") => plays.push((who(e), card(e)?)),
                t => return Err(format!("evento sconosciuto {t:?}")),
            }
        }
        Ok(Real {
            name: name.to_string(),
            trump: trump.ok_or("manca la briscola")?,
            first: first.ok_or("manca chi comincia")?,
            my_draws,
            plays,
        })
    }

    fn complete(&self) -> bool {
        self.plays.len() == NUM_CARDS
    }
}

/// Chi pesca ogni carta, nell'ordine in cui si pesca, e dopo quante prese chiuse.
/// Controlla anche che le giocate rispettino i turni.
fn draw_slots(r: &Real) -> Result<Vec<(u8, usize)>, String> {
    let mut slots = Vec::with_capacity(NUM_CARDS);
    for _ in 0..3 {
        slots.push((r.first, 0));
        slots.push((1 - r.first, 0));
    }
    let trump_suit = briscola_core::cards::suit(r.trump);
    let mut leader = r.first;
    for (t, pair) in r.plays.chunks(2).enumerate() {
        if pair[0].0 != leader || pair.get(1).is_some_and(|p| p.0 == leader) {
            return Err(format!("turni sbagliati alla presa {}", t + 1));
        }
        let [(_, lead), (p2, follow)] = pair else { break };
        let winner = if beats(*follow, *lead, trump_suit) { *p2 } else { leader };
        if slots.len() < NUM_CARDS {
            slots.push((winner, t + 1));
            slots.push((1 - winner, t + 1));
        }
        leader = winner;
    }
    Ok(slots)
}

/// Una ricostruzione: le decisioni dell'avversario (posizione vista da lui e carta giocata)
/// e la probabilità di ciascuna per ogni rete.
struct Sample {
    decisions: Vec<(Position, u8)>,
    logp: Vec<f64>,
}

/// Ricostruisce la partita a caso fra quelle compatibili con il registro e misura le reti.
fn sample(r: &Real, slots: &[(u8, usize)], nets: &[&Net], rng: &mut SmallRng) -> Result<Sample, String> {
    let n = slots.len();
    let mut fixed: Vec<Option<Card>> = vec![None; n];
    let mut used = CardSet::EMPTY;
    let take = |c: Card, used: &mut CardSet| -> Result<(), String> {
        if used.contains(c) {
            return Err(format!("carta {c} vista due volte"));
        }
        used.insert(c);
        Ok(())
    };
    take(r.trump, &mut used)?;
    // Le mie pescate, in ordine; l'ultima carta del mazzo è la briscola scoperta.
    let mine: Vec<usize> = (0..n).filter(|&i| slots[i].0 == ME).collect();
    if r.my_draws.len() > mine.len() {
        return Err("più pescate del possibile".into());
    }
    for (&i, &c) in mine.iter().zip(&r.my_draws) {
        if i == NUM_CARDS - 1 {
            if c != r.trump {
                return Err("l'ultima pescata non è la briscola scoperta".into());
            }
        } else {
            take(c, &mut used)?;
        }
        fixed[i] = Some(c);
    }
    if n == NUM_CARDS && slots[n - 1].0 == OPP {
        fixed[n - 1] = Some(r.trump);
    }
    // Le carte giocate dall'avversario vanno pescate prima di giocarle: si assegnano in ordine
    // di giocata, ognuna a caso fra le sue pescate libere. Gli insiemi ammessi sono uno dentro
    // l'altro, quindi le scelte sono uniformi su tutte le ricostruzioni compatibili.
    let mut opp_slots: Vec<usize> = (0..n).filter(|&i| slots[i].0 == OPP && fixed[i].is_none()).collect();
    for (k, &(p, c)) in r.plays.iter().enumerate() {
        if p != OPP {
            if !used.contains(c) {
                return Err(format!("ho giocato {c} senza averla pescata"));
            }
            continue;
        }
        if c == r.trump && fixed.contains(&Some(c)) {
            continue;
        }
        take(c, &mut used)?;
        let trick = k / 2;
        let ok: Vec<usize> = opp_slots.iter().enumerate().filter(|(_, &i)| slots[i].1 <= trick).map(|(j, _)| j).collect();
        let &j = ok.choose(rng).ok_or_else(|| format!("l'avversario non poteva avere {c}"))?;
        fixed[opp_slots.swap_remove(j)] = Some(c);
    }
    // Il resto (carte mai viste) a caso.
    let mut rest: Vec<Card> = (0..NUM_CARDS as Card).filter(|&c| !used.contains(c)).collect();
    rest.shuffle(rng);
    let mut order = [0 as Card; NUM_CARDS];
    for i in 0..NUM_CARDS {
        order[NUM_CARDS - 1 - i] = match fixed.get(i).copied().flatten() {
            Some(c) => c,
            None if i == NUM_CARDS - 1 => r.trump,
            None => rest.pop().ok_or("carte che non tornano")?,
        };
    }
    let mut g = Game::from_order(&order, r.first);
    let mut out = Sample { decisions: Vec::new(), logp: vec![0.0; nets.len()] };
    let mut probs = [0f32; 3];
    for &(p, c) in &r.plays {
        if g.to_move() != p || !g.hands[p as usize].contains(c) {
            return Err(format!("giocata impossibile: {c}"));
        }
        if p == OPP && g.hands[p as usize].len() > 1 {
            let obs = Observation::of(&g, OPP);
            let at = obs.hand.iter().position(|h| h == c).unwrap();
            for (k, net) in nets.iter().enumerate() {
                net.policy(&obs, 1.0, &mut probs);
                out.logp[k] += (probs[at].max(1e-9) as f64).ln();
            }
            out.decisions.push((Position::of_obs(&obs), canon_index(c, g.trump) as u8));
        }
        g.play(c);
    }
    Ok(out)
}

fn log_mean_exp(v: &[f64]) -> f64 {
    let m = v.iter().cloned().fold(f64::MIN, f64::max);
    m + (v.iter().map(|x| (x - m).exp()).sum::<f64>() / v.len() as f64).ln()
}

/// Risultato di una partita: verosimiglianza marginale per rete, giocate, peso efficace.
struct GameResult {
    loglik: Vec<f64>,
    uniform: f64,
    decisions: usize,
    ess: f64,
    kept: Vec<(Position, u8)>,
}

fn process(r: &Real, nets: &[&Net], samples: usize, keep: usize, seed: u64) -> Result<GameResult, String> {
    let slots = draw_slots(r)?;
    let mut rng = SmallRng::seed_from_u64(seed);
    let all: Vec<Sample> = (0..samples).map(|_| sample(r, &slots, nets, &mut rng)).collect::<Result<_, _>>()?;
    let loglik: Vec<f64> =
        (0..nets.len()).map(|k| log_mean_exp(&all.iter().map(|s| s.logp[k]).collect::<Vec<_>>())).collect();
    // Il caso: ogni carta della mano ugualmente probabile (le mani hanno sempre la stessa misura).
    let mut g_uniform = 0.0;
    let decisions = all[0].decisions.len();
    for (pos, _) in &all[0].decisions {
        let n = pos.loc.iter().filter(|&&l| l == 0).count();
        g_uniform -= (n as f64).ln();
    }
    // Ricampionamento sistematico per il peso della prima rete.
    let w: Vec<f64> = all.iter().map(|s| (s.logp[0] - loglik[0]).exp()).collect();
    let total: f64 = w.iter().sum();
    let ess = total * total / w.iter().map(|x| x * x).sum::<f64>();
    let mut kept = Vec::new();
    let step = total / keep as f64;
    let mut u = rng.gen::<f64>() * step;
    let mut acc = 0.0;
    for (s, wi) in all.iter().zip(&w) {
        acc += wi;
        while u < acc && kept.len() < keep * decisions {
            kept.extend_from_slice(&s.decisions);
            u += step;
        }
    }
    Ok(GameResult { loglik, uniform: g_uniform, decisions, ess, kept })
}

/// Partite contenute in un file: una sola (salvata dall'app) o tante (file da inviare).
fn read_file(path: &Path, out: &mut Vec<Real>, errors: &mut Vec<String>) {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
    let v: Value = match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string())) {
        Ok(v) => v,
        Err(e) => return errors.push(format!("{name}: {e}")),
    };
    let games: Vec<(String, &Value)> = match v["partite"].as_array() {
        Some(a) => a.iter().enumerate().map(|(i, g)| (g["file"].as_str().map_or(format!("{name}#{i}"), String::from), g)).collect(),
        None => vec![(name, &v)],
    };
    for (n, g) in games {
        match Real::parse(&n, g) {
            Ok(r) => out.push(r),
            Err(e) => errors.push(format!("{n}: {e}")),
        }
    }
}

fn read_inputs(spec: &str) -> (Vec<Real>, Vec<String>) {
    let (mut games, mut errors) = (Vec::new(), Vec::new());
    for p in spec.split(',').map(PathBuf::from) {
        if p.is_dir() {
            let mut files: Vec<_> = std::fs::read_dir(&p)
                .expect("cartella delle partite")
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|f| f.extension().is_some_and(|x| x == "json"))
                .collect();
            files.sort();
            for f in files {
                read_file(&f, &mut games, &mut errors);
            }
        } else {
            read_file(&p, &mut games, &mut errors);
        }
    }
    // La stessa partita inviata più volte conta una volta sola.
    let mut seen = std::collections::HashSet::new();
    games.retain(|g| seen.insert(g.key()));
    (games, errors)
}

/// Partita di controllo: scelta dal contenuto, così resta la stessa a ogni nuovo invio.
fn is_control(r: &Real, share: f64) -> bool {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in r.key().bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    (h % 10_000) as f64 / 10_000.0 < share
}

fn load_net(path: &str) -> Net {
    Net::from_bytes(&std::fs::read(path).expect("file della rete")).expect("rete valida")
}

/// Partite finte nel formato del file da inviare: io gioco come l'euristico, l'avversario è un
/// "umano" simulato (euristico 60%, prendi-tutto 25%, a caso 15%), diverso dai motori su cui è
/// allenata la rete.
fn fake_bundle(n: usize, seed: u64) -> Value {
    use briscola_core::ai::{AggressivePlayer, HeuristicPlayer, Player, RandomPlayer};
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut partite = Vec::new();
    for i in 0..n {
        let first = (i % 2) as u8;
        let mut g = Game::new(&mut rng, first);
        let cr = |c: Card| serde_json::json!({ "id": c, "nome": briscola_core::cards::name(c) });
        let who = |p: u8| if p == ME { "io" } else { "avversario" };
        let mut ev = vec![
            serde_json::json!({"tipo": "briscola", "carta": cr(g.trump_card)}),
            serde_json::json!({"tipo": "primo_di_mano", "chi": who(first)}),
        ];
        let initial: Vec<Card> =
            (0..6).filter(|k| (k % 2 == 0) == (first == ME)).map(|k| g.deck[NUM_CARDS - 1 - k]).collect();
        for c in initial {
            ev.push(serde_json::json!({"tipo": "pescata", "carta": cr(c)}));
        }
        while !g.is_over() {
            let p = g.to_move();
            let obs = Observation::of(&g, p);
            let x: f64 = rng.gen();
            let c = if p == ME || x < 0.6 {
                HeuristicPlayer.choose(&obs, &mut rng)
            } else if x < 0.85 {
                AggressivePlayer.choose(&obs, &mut rng)
            } else {
                RandomPlayer.choose(&obs, &mut rng)
            };
            ev.push(serde_json::json!({"tipo": "giocata", "chi": who(p), "carta": cr(c)}));
            let before = g.hands[ME as usize];
            g.play(c);
            for d in g.hands[ME as usize].minus(before).iter() {
                ev.push(serde_json::json!({"tipo": "pescata", "carta": cr(d)}));
            }
        }
        partite.push(serde_json::json!({ "file": format!("finta_{i:05}.json"), "finita": true, "eventi": ev }));
    }
    serde_json::json!({ "versione_app": "finte", "partite": partite })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut opts = BTreeMap::new();
    for kv in args[1..].chunks(2) {
        opts.insert(kv[0].trim_start_matches("--").to_string(), kv.get(1).cloned().unwrap_or_default());
    }
    let get = |k: &str| opts.get(k).cloned();
    let inputs = get("partite").expect("serve --partite FILE_O_CARTELLA");
    if let Some(n) = get("finte") {
        let seed: u64 = get("seed").map_or(1, |v| v.parse().unwrap());
        std::fs::write(&inputs, fake_bundle(n.parse().unwrap(), seed).to_string()).unwrap();
    }
    let samples: usize = get("campioni").map_or(512, |v| v.parse().unwrap());
    let keep: usize = get("tieni").map_or(16, |v| v.parse().unwrap());
    let share: f64 = get("controllo").map_or(0.2, |v| v.parse().unwrap());
    let seed: u64 = get("seed").map_or(1, |v| v.parse().unwrap());
    let base = match get("rete") {
        Some(f) => load_net(&f),
        None => briscola_core::net::policy().clone(),
    };
    let other = get("confronta").map(|f| load_net(&f));
    let mut nets = vec![&base];
    nets.extend(other.as_ref());

    let (games, errors) = read_inputs(&inputs);
    for e in &errors {
        eprintln!("scartata {e}");
    }
    use rayon::prelude::*;
    let results: Vec<(usize, Result<GameResult, String>)> = games
        .par_iter()
        .enumerate()
        .map(|(i, g)| (i, process(g, &nets, samples, keep, seed.wrapping_mul(1_000_003).wrapping_add(i as u64))))
        .collect();

    let (mut train, mut val) = (Vec::new(), Vec::new());
    // [tutte, controllo]: somma delle verosimiglianze per rete, del caso, giocate, partite.
    let mut sums = [(vec![0.0; nets.len()], 0.0, 0usize, 0usize, 0.0), (vec![0.0; nets.len()], 0.0, 0usize, 0usize, 0.0)];
    let (mut complete, mut bad) = (0, 0);
    for (i, res) in results {
        let g = &games[i];
        let r = match res {
            Ok(r) => r,
            Err(e) => {
                eprintln!("scartata {}: {e}", g.name);
                bad += 1;
                continue;
            }
        };
        complete += g.complete() as usize;
        let ctl = is_control(g, share);
        for (k, s) in sums.iter_mut().enumerate() {
            if k == 1 && !ctl {
                continue;
            }
            for (a, b) in s.0.iter_mut().zip(&r.loglik) {
                *a += b;
            }
            s.1 += r.uniform;
            s.2 += r.decisions;
            s.3 += 1;
            s.4 += r.ess;
        }
        let target = if ctl { &mut val } else { &mut train };
        for (pos, c) in r.kept {
            pos.write(target);
            target.push(c);
        }
    }
    println!(
        "Partite lette: {} (finite {complete}), scartate {}, campioni {samples}, tenuti {keep} per partita",
        games.len() - bad,
        errors.len() + bad
    );
    for (label, s) in ["tutte", "di controllo"].iter().zip(&sums) {
        if s.2 == 0 {
            println!("  {label}: nessuna giocata dell'avversario con più di una carta");
            continue;
        }
        let per = |x: f64| (x / s.2 as f64).exp();
        print!("  {label}: {} partite, {} giocate dell'avversario, peso efficace medio {:.0}", s.3, s.2, s.4 / s.3 as f64);
        for (k, l) in s.0.iter().enumerate() {
            print!(
                "; rete {}: prob. media per giocata {:.3} (caso {:.3}), {:.2}x il caso",
                k + 1,
                per(*l),
                per(s.1),
                per(l - s.1)
            );
        }
        println!();
    }
    if let Some(out) = get("out") {
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(Path::new(&out).join("politica_00000000.pol"), &train).unwrap();
        std::fs::write(Path::new(&out).join("politica_99999999.pol"), &val).unwrap();
        let rec = Position::BYTES + 1;
        println!("Scritti in {out}: {} decisioni di allenamento, {} di controllo", train.len() / rec, val.len() / rec);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use briscola_core::cards;

    /// Gioca una partita con la rete come avversario e la registra come farebbe l'app.
    fn fake_record(seed: u64, net: &Net) -> Value {
        let mut rng = SmallRng::seed_from_u64(seed);
        let first = (seed % 2) as u8;
        let mut g = Game::new(&mut rng, first);
        let cr = |c: Card| serde_json::json!({ "id": c, "nome": cards::name(c) });
        let who = |p: u8| if p == ME { "io" } else { "avversario" };
        let mut ev = vec![
            serde_json::json!({"tipo": "briscola", "carta": cr(g.trump_card)}),
            serde_json::json!({"tipo": "primo_di_mano", "chi": who(first)}),
        ];
        // Mano iniziale, nell'ordine in cui si pesca.
        let mut order: Vec<Card> = g.deck[..].to_vec();
        order.reverse();
        let initial: Vec<Card> = (0..6).filter(|k| (k % 2 == 0) == (first == ME)).map(|k| order[k]).collect();
        for c in initial {
            ev.push(serde_json::json!({"tipo": "pescata", "carta": cr(c)}));
        }
        let mut probs = [0f32; 3];
        while !g.is_over() {
            let p = g.to_move();
            let obs = Observation::of(&g, p);
            let c = if p == OPP {
                let n = net.policy(&obs, 1.0, &mut probs);
                let x: f32 = rng.gen();
                let (mut acc, mut pick) = (0.0, n - 1);
                for k in 0..n {
                    acc += probs[k];
                    if x < acc {
                        pick = k;
                        break;
                    }
                }
                obs.hand.iter().nth(pick).unwrap()
            } else {
                obs.hand.iter().next().unwrap()
            };
            ev.push(serde_json::json!({"tipo": "giocata", "chi": who(p), "carta": cr(c)}));
            let before = g.hands[ME as usize];
            g.play(c);
            let mut drawn = g.hands[ME as usize];
            drawn.remove(c);
            for d in drawn.minus(before).iter() {
                ev.push(serde_json::json!({"tipo": "pescata", "carta": cr(d)}));
            }
        }
        serde_json::json!({ "versione_app": "test", "finita": true, "eventi": ev })
    }

    #[test]
    fn reconstructs_real_games_and_scores_the_net() {
        let net = briscola_core::net::policy();
        let uniform = Net::random(7, briscola_core::net::POLICY_OUT);
        let (mut better, mut total) = (0, 0);
        for seed in 0..12 {
            let v = fake_record(seed, net);
            let r = Real::parse("prova", &v).unwrap();
            assert!(r.complete());
            assert_eq!(r.my_draws.len(), 20, "tre iniziali e 17 pescate");
            let res = process(&r, &[net, &uniform], 64, 4, seed).unwrap();
            assert!(res.decisions >= 17, "{}", res.decisions);
            assert_eq!(res.kept.len(), 4 * res.decisions);
            // La rete che ha giocato spiega le giocate meglio del caso e di una rete a caso.
            total += 1;
            better += (res.loglik[0] > res.uniform && res.loglik[0] > res.loglik[1]) as usize;
        }
        assert!(better >= 10, "{better}/{total}");
    }

    #[test]
    fn accepts_unfinished_games_and_bundles() {
        let net = briscola_core::net::policy();
        let mut v = fake_record(3, net);
        let ev = v["eventi"].as_array_mut().unwrap();
        ev.truncate(25);
        let r = Real::parse("a metà", &v).unwrap();
        assert!(!r.complete());
        assert!(process(&r, &[net], 16, 2, 1).is_ok());
        // Il file da inviare, con dentro due volte la stessa partita: conta una volta.
        let dir = std::env::temp_dir().join(format!("reali-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("partite-brismaster.json");
        let full = fake_record(4, net);
        std::fs::write(&f, serde_json::json!({ "partite": [v, full.clone(), full] }).to_string()).unwrap();
        let (games, errors) = read_inputs(f.to_str().unwrap());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(games.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_inconsistent_games() {
        let net = briscola_core::net::policy();
        let mut v = fake_record(5, net);
        // L'avversario gioca una carta che ho in mano io.
        let mine = v["eventi"][2]["carta"].clone();
        let ev = v["eventi"].as_array_mut().unwrap();
        let k = ev.iter().position(|e| e["tipo"] == "giocata" && e["chi"] == "avversario").unwrap();
        ev[k]["carta"] = mine;
        let r = Real::parse("sbagliata", &v).unwrap();
        assert!(process(&r, &[net], 8, 2, 1).is_err());
    }
}
