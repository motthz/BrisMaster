//! Banco offline del riconoscitore, con l'identità di ogni carta annotata.
use super::{capture, detect, read_briscola, read_scopa, CardView, Memory};
use briscola_core::cards::CardSet;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Deserialize)]
struct Frame {
    ms: u64,
    image: String,
    /// Ruoli e numeri di carta (seme * 10 + valore).
    cards: BTreeMap<String, u8>,
}

#[derive(Deserialize)]
struct Replay {
    scopa: bool,
    frames: Vec<Frame>,
}

#[test]
#[ignore]
fn replay_carte_annotate() {
    let manifest = PathBuf::from(std::env::var_os("BRIS_REPLAY_CARTE").expect("BRIS_REPLAY_CARTE=manifest.json"));
    let suite: Replay = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    assert!(!suite.frames.is_empty());
    let root = std::env::var_os("BRIS_REPLAY_FRAMES").map(PathBuf::from)
        .unwrap_or_else(|| manifest.parent().unwrap().to_path_buf());
    detect::forget_reading_state();
    let memory = Mutex::new(Memory::in_memory());
    let mut last = None;
    for frame in suite.frames {
        assert!(last.is_none_or(|previous| previous < frame.ms), "timestamp fuori ordine");
        assert!(frame.cards.values().all(|&card| card < 40), "carta fuori mazzo");
        last = Some(frame.ms);
        let img = capture::load_png(&root.join(&frame.image)).unwrap();
        let mut seen: BTreeMap<String, CardView> = BTreeMap::new();
        let started = Instant::now();
        if suite.scopa {
            if let Some(scene) = read_scopa(&img, &memory) {
                seen.extend(scene.hand.into_iter().enumerate().map(|(i, v)| (format!("mano{i}"), v)));
                seen.extend(scene.table.into_iter().enumerate().map(|(i, v)| (format!("tavolo{i}"), v)));
            }
        } else if let Some(scene) = read_briscola(&img, &memory) {
            seen.extend(scene.hand.into_iter().enumerate().map(|(i, v)| (format!("mano{i}"), v)));
            for (role, view) in [("briscola", scene.trump), ("avversario", scene.opp_slot), ("tua", scene.my_slot)] {
                if let Some(view) = view {
                    seen.insert(role.to_owned(), view);
                }
            }
        }
        let us = started.elapsed().as_micros();
        for (role, expected) in frame.cards {
            let best = seen.remove(&role).and_then(|view| view.best(CardSet::FULL));
            let (got, margin) = best.map_or((-1, 0.0), |(card, score)| (i16::from(card), score));
            let automatic = u8::from(best.is_some() && margin >= 1.4);
            println!("lettura {} {role} {expected} {got} {automatic}", frame.ms);
            if best.is_some() {
                println!("margine {} {role} {expected} {got} {margin}", frame.ms);
            }
        }
        println!("extra {} {}", frame.ms, seen.len());
        println!("tempo {} {us}", frame.ms);
    }
}

#[test]
fn negative_interface_frames_do_not_produce_cards() {
    use super::synth::{table, writing_only, Deal, Look};
    let memory = Mutex::new(Memory::in_memory());
    let look = Look { writing: true, logo: 0.9, ..Look::default() };
    for frame in [
        writing_only(&look),
        table(&Deal { hand: vec![], opp: None, mine: None, trump: None, deck: true },
              &Look { writing: false, logo: 0.0, ..look }),
    ] {
        detect::forget_reading_state();
        assert!(read_briscola(&frame, &memory).is_none());
    }
}

#[test]
fn maps_the_same_table_to_distinct_monitor_origins_and_scales() {
    use super::{capture::Geom, place_on_desktop, screen::Area};
    let area = Area { monitor: 1, rect: Some([0.25, 0.25, 0.75, 0.75]) };
    let box_at = [0.25, 0.25, 0.75, 0.75];
    for (x, y, scale) in [(-1920.0, 0.0, 1.0), (0.0, -1200.0, 1.25), (1920.0, 0.0, 2.0)] {
        let g = Geom { x, y, w: 1200.0 * scale, h: 800.0 * scale, scale };
        assert_eq!(place_on_desktop(box_at, g, area),
            [x + g.w * 0.375, y + g.h * 0.375, x + g.w * 0.625, y + g.h * 0.625]);
    }
}
