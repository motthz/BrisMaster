//! Converte l'esportazione JSON del sito di Motthz (partite contro Massima) nel formato
//! `human_decision_v1`, rigiocando ogni partita dall'ordine del mazzo.
//!
//! cargo run --release -p briscola-lab --bin lab-importa-sito -- esportazione.json > partite.csv
//!
//! Esportazione attesa: `{games: [{id, participant, ip_code, engine, cohort, status,
//! human_points, state: {order, first, moves}}], turns: [{game_id, revision, elapsed_ms}]}`.
//! La persona siede al posto 0; `order` ha la convenzione di `Game::from_order`; `revision` k
//! è la k-esima carta della persona. Le partite di verifica (`cohort` diverso da `human`) e
//! quelle non completate sono saltate e contate su stderr.

use briscola_core::cards::Card;
use briscola_core::game::Game;
use briscola_lab::{Origine, Riga, INTESTAZIONE, REGOLE, SCHEMA};
use serde_json::Value;
use std::collections::HashMap;

fn carte(v: &Value) -> Option<Vec<Card>> {
    v.as_array()?.iter().map(|x| x.as_u64().filter(|&c| c < 40).map(|c| c as Card)).collect()
}

fn main() {
    let f = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("uso: lab-importa-sito ESPORTAZIONE.json > partite.csv");
        std::process::exit(2)
    });
    let d: Value = serde_json::from_str(&std::fs::read_to_string(&f).expect("file non leggibile")).expect("JSON non valido");
    // Tempo di ogni carta della persona: (partita, revisione) -> ms.
    let mut tempi: HashMap<(String, u64), u64> = HashMap::new();
    for t in d["turns"].as_array().into_iter().flatten() {
        if let (Some(g), Some(r), Some(ms)) = (t["game_id"].as_str(), t["revision"].as_u64(), t["elapsed_ms"].as_u64()) {
            tempi.insert((g.to_string(), r), ms);
        }
    }
    let (mut ok, mut saltate, mut rotte) = (0, 0, 0);
    println!("{INTESTAZIONE},codice_ip,consenso");
    for g in d["games"].as_array().into_iter().flatten() {
        let id = g["id"].as_str().unwrap_or("").to_string();
        if g["cohort"].as_str() != Some("human") || g["status"].as_str() != Some("completed") {
            saltate += 1;
            continue;
        }
        let st = &g["state"];
        let (Some(order), Some(moves), Some(first)) = (carte(&st["order"]), carte(&st["moves"]), st["first"].as_u64()) else {
            eprintln!("partita {id}: stato incompleto");
            rotte += 1;
            continue;
        };
        if order.len() != 40 || moves.len() != 40 || first > 1 {
            eprintln!("partita {id}: mazzo o mosse di lunghezza sbagliata");
            rotte += 1;
            continue;
        }
        let giocatore = format!("p-{}", g["participant"].as_str().unwrap_or("?"));
        let sessione = format!("s-{}-{}", g["participant"].as_str().unwrap_or("?"), g["started_at"].as_str().unwrap_or("").get(..10).unwrap_or(""));
        let extra = format!(",ip-{},{}", g["ip_code"], g["consent_version"].as_str().unwrap_or(""));
        let motore = g["engine"].as_str().unwrap_or("").to_string();
        let mut gm = Game::from_order(&order, first as u8);
        let mut storia: Vec<Card> = Vec::new();
        let mut righe = String::new();
        let mut umane = 0u64;
        let mut errore = None;
        for (m, &c) in moves.iter().enumerate() {
            let p = gm.to_move();
            if !gm.hands[p as usize].contains(c) {
                errore = Some(format!("mossa {m}: carta {c} non in mano"));
                break;
            }
            let origine = if p == 0 { Origine::Umano } else { Origine::Massima };
            let tempo_ms = if p == 0 {
                umane += 1;
                tempi.get(&(id.clone(), umane)).map(|&t| t.min(u32::MAX as u64) as u32)
            } else {
                None
            };
            let r = Riga {
                schema: SCHEMA.into(),
                partita: id.clone(),
                giocatore: giocatore.clone(),
                sessione: sessione.clone(),
                regole: REGOLE.into(),
                fonte: "sito-motthz".into(),
                origine,
                mossa: m as u8,
                presa: (m / 2) as u8,
                posto: p,
                apre: gm.leader,
                briscola: gm.trump_card,
                mano: gm.hands[p as usize].to_vec(),
                tavolo: gm.table,
                mazzo_rimasto: gm.deck_len,
                punti_miei: gm.points[p as usize],
                punti_avversario: gm.points[1 - p as usize],
                legali: gm.legal_moves().to_vec(),
                scelta: c,
                storia: storia.clone(),
                consigli_visibili: false,
                tempo_ms,
                qualita: "exact".into(),
                mazzo: if m == 0 { Some(order.clone()) } else { None },
                motore: motore.clone(),
            };
            righe += &(r.to_csv() + &extra + "\n");
            storia.push(c);
            gm.play(c);
        }
        if errore.is_none() && g["human_points"].as_u64() != Some(gm.points[0] as u64) {
            errore = Some(format!("punti della persona {} nel file, {} rigiocando", g["human_points"], gm.points[0]));
        }
        match errore {
            Some(e) => {
                eprintln!("partita {id}: {e}");
                rotte += 1;
            }
            None => {
                print!("{righe}");
                ok += 1;
            }
        }
    }
    eprintln!("Convertite: {ok}, saltate (verifica o non finite): {saltate}, rotte: {rotte}");
}
