//! Banco di replay offline: le verità del tavolo non entrano nell'assistente.
use super::*;
use crate::vision::{capture, detect, forget_focus, shrink_by, shrink_factor};
use briscola_core::Game;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Play {
    trick: u8,
    player: u8,
    card: Card,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Turn {
    hand: Vec<Card>,
    table: Option<Card>,
    trump: Card,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    ms: u64,
    image: Option<PathBuf>,
    #[serde(default)]
    capture_error: bool,
    #[serde(default)]
    geometry_changed: bool,
    check_turn: Option<Turn>,
    #[serde(default)]
    queue_delay_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u8,
    plays: Vec<Play>,
    frames: Vec<Frame>,
}

impl Manifest {
    fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1 || self.frames.is_empty() || self.plays.len() != 40 {
            return Err("servono il formato 1, almeno un frame e tutte le 40 giocate");
        }
        let mut deck = BTreeSet::new();
        for (index, pair) in self.plays.chunks_exact(2).enumerate() {
            if pair.iter().any(|p| p.trick as usize != index + 1 || p.player > 1 || p.card >= 40 || !deck.insert(p.card))
                || pair[0].player == pair[1].player
            {
                return Err("giocate duplicate, fuori mazzo, o non ordinate per presa");
            }
        }
        if self.frames.windows(2).any(|w| w[0].ms >= w[1].ms) {
            return Err("timestamp non strettamente crescenti");
        }
        let mut own_plays = self.plays.iter().enumerate().filter(|(_, play)| play.player == ME);
        let mut trump = None;
        for frame in &self.frames {
            if frame.image.is_some() == frame.capture_error {
                return Err("ogni frame deve avere un'immagine oppure un errore di cattura");
            }
            if let Some(turn) = &frame.check_turn {
                let Some((index, play)) = own_plays.next() else {
                    return Err("più di 20 checkpoint del giocatore");
                };
                let expected_table = (index % 2 == 1).then(|| self.plays[index - 1].card);
                let mut visible = BTreeSet::new();
                if !turn.hand.contains(&play.card) || turn.hand.len() > 3 || turn.trump >= 40
                    || turn.table != expected_table || *trump.get_or_insert(turn.trump) != turn.trump
                    || turn.hand.iter().copied().chain(turn.table).any(|c| c >= 40 || !visible.insert(c))
                    || self.plays[..index - index % 2].iter().any(|p| visible.contains(&p.card))
                {
                    return Err("annotazione del turno incompatibile con le giocate");
                }
            }
        }
        if own_plays.next().is_some() {
            return Err("servono 20 checkpoint, uno per ciascun turno del giocatore");
        }
        Ok(())
    }
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
struct Report {
    first_visible: usize,
    first_face_correct: usize,
    first_face_and_side_correct: usize,
    first_registered: usize,
    first_registered_confirmed: usize,
    first_hidden: usize,
    correct: usize,
    correct_identified: usize,
    missing: usize,
    wrong: usize,
    wrong_identified: usize,
    duplicate_events: usize,
    tricks: u8,
    points: [u8; 2],
    my_turns: usize,
    ready_with_correct_position: usize,
    ready_with_wrong_position: usize,
    wrong_position_with_partial_notice: usize,
    correct_position_with_partial_notice: usize,
    blocked_wrong_phase: usize,
    blocked_confirmation: usize,
    blocked_screen: usize,
    first_unavailable_turn_ms: Option<u64>,
    longest_unavailable_turns: usize,
    empty_reads: usize,
    capture_errors: usize,
    first_incorrect_play: Option<(u8, u8)>,
}

struct Session {
    assistant: Assistant,
    card_px: u32,
    sequence: u64,
    capture_size: Option<(usize, usize)>,
    last_image: Option<Img>,
    last_scene: Option<Scene>,
    report: Report,
    unavailable_turns: usize,
}

impl Session {
    fn new() -> Self {
        detect::forget_reading_state();
        let mut assistant = Assistant::default();
        assistant.rec.off = true;
        assistant.vision.memory = Arc::new(Mutex::new(Memory::in_memory()));
        // Attiva gli stessi controlli di blocco del pannello Consiglio in modalità schermo.
        // Il replay avanza con gli orari del manifest e non avvia worker o catture reali.
        assistant.vision.reader = Some(Reader::from_readings([]));
        Self {
            assistant,
            card_px: 0,
            sequence: 0,
            capture_size: None,
            last_image: None,
            last_scene: None,
            report: Report::default(),
            unavailable_turns: 0,
        }
    }

    fn capture(&mut self, ms: u64, image: Option<&Img>, geometry_changed: bool) {
        let reading = if let Some(img) = image {
            let k = shrink_factor(img.w, self.card_px);
            let size = (img.w, img.h);
            if geometry_changed || self.capture_size != Some(size) {
                forget_focus();
                self.capture_size = Some(size);
                self.last_image = None;
                self.last_scene = None;
            }
            let reduced = shrink_by(img.clone(), k);
            let same = self.last_image.as_ref().is_some_and(|old| {
                old.w == reduced.w && old.h == reduced.h && old.px == reduced.px
            });
            if !same {
                self.last_scene = read_briscola(&reduced, &self.assistant.vision.memory);
                if k > 1 {
                    if let Some(scene) = &mut self.last_scene {
                        crate::vision::Located::refine_native(scene, img);
                    }
                }
                self.card_px = (detect::last_card_l() * k as f32).round() as u32;
                self.last_image = Some(reduced);
            }
            self.sequence += 1;
            if let Some(scene) = &self.last_scene {
                Reading::Scene(scene.clone(), self.sequence)
            } else {
                self.report.empty_reads += 1;
                Reading::NoTable
            }
        } else {
            self.report.capture_errors += 1;
            Reading::Error("cattura interrotta nel replay".into())
        };
        self.assistant.apply_reading(reading, |_| ms);
    }

    fn check_turn(&mut self, ms: u64, turn: &Turn, played: CardSet, queue_delay_ms: u64) {
        let a = &self.assistant;
        let correct = a.pos.hand == turn.hand.iter().copied().collect()
            && a.pos.table == turn.table && a.pos.trump_card == Some(turn.trump)
            && a.pos.played == played;
        self.report.my_turns += 1;
        if a.pos.step() != Step::MyMove {
            self.report.blocked_wrong_phase += 1;
        } else if a.vision.confirm.is_some() {
            self.report.blocked_confirmation += 1;
        } else if a.screen_is_behind_with_lag(Some(|| Duration::from_millis(queue_delay_ms))) {
            self.report.blocked_screen += 1;
        } else if !correct {
            if std::env::var_os("BRIS_ADVICE_TRACE").is_some() {
                println!("consiglio ms={ms} turno={} avviso={} letture_discordanti={} mano={:?}/{:?} tavolo={:?}/{:?} uscite={:?}/{:?}",
                    self.report.my_turns, a.partial_advice(), a.vision.stuck, a.pos.hand.to_vec(), turn.hand,
                    a.pos.table, turn.table, a.pos.played.to_vec(), played.to_vec());
            }
            self.report.ready_with_wrong_position += 1;
            self.report.wrong_position_with_partial_notice += usize::from(a.partial_advice());
        } else {
            self.report.correct_position_with_partial_notice += usize::from(a.partial_advice());
            self.report.ready_with_correct_position += 1;
            self.unavailable_turns = 0;
            return;
        }
        self.report.first_unavailable_turn_ms.get_or_insert(ms);
        self.unavailable_turns += 1;
        self.report.longest_unavailable_turns = self.report.longest_unavailable_turns.max(self.unavailable_turns);
    }

    fn finish(mut self, expected: &[Play]) -> Report {
        let mut actual = BTreeMap::new();
        for event in &self.assistant.rec.events {
            if let Event::Giocata { presa, chi, carta, identificata, .. } = event {
                let player = if *chi == record::who(ME) { ME } else { OPP };
                if actual.insert((*presa, player), (carta.id, *identificata)).is_some() {
                    self.report.duplicate_events += 1;
                }
            }
        }
        for play in expected {
            let key = (play.trick, play.player);
            match actual.remove(&key) {
                Some((card, identified)) if card == play.card => {
                    self.report.correct += 1;
                    self.report.correct_identified += usize::from(identified);
                    continue;
                }
                Some((_, identified)) => {
                    self.report.wrong += 1;
                    self.report.wrong_identified += usize::from(identified);
                }
                None => self.report.missing += 1,
            }
            self.report.first_incorrect_play.get_or_insert(key);
        }
        self.report.wrong += actual.len();
        self.report.wrong_identified += actual.values().filter(|(_, identified)| *identified).count();
        self.report.tricks = self.assistant.pos.tricks;
        self.report.points = self.assistant.pos.points;
        self.report
    }
}

#[test]
#[ignore = "richiede una sessione locale annotata e i suoi fotogrammi"]
fn replay_briscola_annotata() {
    let path = PathBuf::from(std::env::var_os("BRIS_REPLAY_SESSIONE").expect("BRIS_REPLAY_SESSIONE=manifest.json"));
    let root = path.parent().unwrap_or(Path::new("."));
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    manifest.validate().unwrap();
    let mut session = Session::new();
    let mut own_plays = manifest.plays.iter().enumerate().filter(|(_, play)| play.player == ME);
    for frame in manifest.frames {
        let image = frame.image.map(|path| {
            let path = root.join(path);
            capture::load_png(&path).unwrap_or_else(|why| panic!("{}: {why}", path.display()))
        });
        session.capture(frame.ms, image.as_ref(), frame.geometry_changed);
        if let Some(turn) = frame.check_turn {
            let (index, _) = own_plays.next().unwrap();
            let played = manifest.plays[..index - index % 2].iter().map(|play| play.card).collect();
            session.check_turn(frame.ms, &turn, played, frame.queue_delay_ms);
        }
    }
    println!("{}", serde_json::to_string(&session.finish(&manifest.plays)).unwrap());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Window {
    Stable,
    ResizeAndMove,
    SmallTableWithChrome,
    SmallTableBare,
    LightOnly,
    WritingOnly,
    LogoOnly,
    TinyTableWithChrome,
    TinyTableBare,
    CaptureLoss,
    Interrupted,
}

fn synthetic_session(seed: u64, window: Window) -> (Report, [u8; 2]) {
    use crate::vision::reference::card_image;
    use crate::vision::synth::{table, Bg, Deal, Look};
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut game = Game::new(&mut rng, (seed % 2) as u8);
    let imgs: Vec<_> = (0..40).map(|c| card_image(0, c)).collect();
    let mut session = Session::new();
    let mut plays = Vec::new();
    let mut ms = 0;
    let mut hand: Vec<Card> = game.hands[ME as usize].iter().collect();
    let small = !matches!(window, Window::Stable | Window::ResizeAndMove | Window::CaptureLoss);
    let tiny = matches!(window, Window::TinyTableBare | Window::TinyTableWithChrome | Window::Interrupted);
    let with_chrome = !matches!(window, Window::SmallTableBare | Window::TinyTableBare | Window::LightOnly | Window::WritingOnly | Window::LogoOnly);
    let interrupted = matches!(window, Window::CaptureLoss | Window::Interrupted);
    let moving = !matches!(window, Window::Stable | Window::CaptureLoss);
    let mut previous_geometry = (1, 0);
    let mut show = |session: &mut Session, game: &Game, slots: [Option<Card>; 2], hand: &[Card], trick: u8, closing: bool, play: Option<(u8, Card)>| {
        let compact = moving && matches!(trick, 2..=4 | 9..=12 | 18..=19);
        let offset = if compact && with_chrome {
            if trick % 2 == 0 || closing { 56 } else { 28 }
        } else { 0 };
        let look = if small && compact {
            Look {
                bg: if matches!(window, Window::WritingOnly | Window::LogoOnly) { Look::default().bg } else { Bg::Felt([220, 215, 200]) },
                writing: !matches!(window, Window::LightOnly | Window::LogoOnly),
                logo: if matches!(window, Window::LightOnly | Window::WritingOnly) { 0.0 } else { 0.7 },
                ..Look::default()
            }
        } else {
            Look::default()
        };
        let deal = Deal {
            hand: hand.iter().map(|&c| &imgs[c as usize]).collect(),
            mine: slots[ME as usize].map(|c| &imgs[c as usize]),
            opp: slots[OPP as usize].map(|c| &imgs[c as usize]),
            trump: (game.deck_len > 0).then(|| &imgs[game.trump_card as usize]),
            deck: game.deck_len > 1,
        };
        let reduction = if compact && tiny { 4 } else if compact { 2 } else { 1 };
        let mut image = table(&deal, &look).shrink(reduction);
        if offset != 0 {
            let mut desktop = Img::new(image.w + 56, image.h + 40);
            desktop.px.fill([39, 42, 47]);
            for (y, row) in image.px.chunks_exact(image.w).enumerate() {
                let start = (y + 20) * desktop.w + offset;
                desktop.px[start..start + image.w].copy_from_slice(row);
            }
            image = desktop;
        }
        let geometry = (reduction, offset);
        let changed = geometry != previous_geometry;
        previous_geometry = geometry;

        if closing && interrupted && matches!(trick, 3 | 11 | 18) {
            // Copertura e perdita cattura mentre si sta chiudendo davvero una presa.
            session.report.first_hidden += usize::from(play.is_some());
            let blank = Img::new(image.w, image.h);
            let before = session.assistant.pos.clone();
            for input in [Some(&blank), None, None] {
                ms += 400;
                session.capture(ms, input, false);
            }
            let after = &session.assistant.pos;
            assert_eq!(before.tricks, after.tricks);
            assert_eq!(before.played, after.played);
            assert_eq!(before.table, after.table);
            assert_eq!(before.hand, after.hand);
            return ms;
        }
        for repeat in 0..4 {
            ms += 400;
            session.capture(ms, Some(&image), repeat == 0 && changed);
            if let Some((player, card)) = play.filter(|_| repeat == 0) {
                session.report.first_visible += 1;
                if let Some(scene) = &session.last_scene {
                    let views = [&scene.my_slot, &scene.opp_slot];
                    let matches = |view: &Option<CardView>| view.as_ref()
                        .and_then(|v| v.best(CardSet::FULL)).is_some_and(|(id, _)| id == card);
                    session.report.first_face_correct += usize::from(views.iter().any(|v| matches(v)));
                    session.report.first_face_and_side_correct += usize::from(matches(views[player as usize]));
                }
                let entry = session.assistant.rec.events.iter().find_map(|event| match event {
                    Event::Giocata { presa, chi, carta, identificata, .. }
                        if *presa == trick && *chi == record::who(player) && carta.id == card => Some(*identificata),
                    _ => None,
                });
                session.report.first_registered += usize::from(entry.is_some());
                session.report.first_registered_confirmed += usize::from(entry == Some(true));
            }
        }
        if std::env::var_os("BRIS_WINDOW_TRACE").is_some() {
            let seen = |card: &CardView| card.best(CardSet::FULL).map(|(id, margin)| (id, margin));
            let last = session.assistant.rec.events.iter().rev().filter_map(|event| match event {
                Event::Giocata { presa, chi, carta, identificata, .. } => Some((*presa, *chi, carta.id, *identificata)),
                _ => None,
            }).take(4).collect::<Vec<_>>();
            println!("ms={ms} presa={trick} mano={hand:?} tavolo={slots:?} lettura={:?} registro={last:?}",
                session.last_scene.as_ref().map(|s| (s.hand.iter().map(seen).collect::<Vec<_>>(),
                    s.my_slot.as_ref().and_then(seen), s.opp_slot.as_ref().and_then(seen))));
        }
        ms
    };

    let mut checkpoint = show(&mut session, &game, [None; 2], &hand, 1, false, None);
    while !game.is_over() {
        let trick = (game.played.len() / 2 + 1) as u8;
        let mut slots = [None; 2];
        for closing in [false, true] {
            let player = game.to_move();
            if player == ME {
                session.check_turn(checkpoint, &Turn {
                    hand: hand.clone(),
                    table: game.table,
                    trump: game.trump_card,
                }, game.played, 0);
            }
            let choices = game.legal_moves().to_vec();
            let card = choices[rng.gen_range(0..choices.len())];
            plays.push(Play { trick, player, card });
            game.play(card);
            slots[player as usize] = Some(card);
            if player == ME {
                hand.retain(|&c| c != card);
            }
            checkpoint = show(&mut session, &game, slots, &hand, trick, closing, Some((player, card)));
        }
        hand.extend(game.hands[ME as usize].iter().filter(|c| !hand.contains(c)).collect::<Vec<_>>());
        checkpoint = show(&mut session, &game, [None; 2], &hand, trick, false, None);
    }
    (session.finish(&plays), game.points)
}

#[test]
#[ignore = "stress test completo, da eseguire anche sul riferimento di rilascio"]
fn window_changes_during_whole_games() {
    let selected = std::env::var("BRIS_STRESS_WINDOW").ok();
    let mut ran = 0;
    let seeds = if std::env::var_os("BRIS_WINDOW_HOLDOUT").is_some() { [31, 42] } else { [1, 2] };
    for seed in seeds {
        for window in [
            Window::Stable,
            Window::ResizeAndMove,
            Window::SmallTableWithChrome,
            Window::SmallTableBare,
            Window::LightOnly,
            Window::WritingOnly,
            Window::LogoOnly,
            Window::TinyTableWithChrome,
            Window::TinyTableBare,
            Window::CaptureLoss,
            Window::Interrupted,
        ] {
            if selected.as_ref().is_some_and(|name| name != &format!("{window:?}")) {
                continue;
            }
            ran += 1;
            let (report, truth_points) = synthetic_session(seed, window);
            println!("seed={seed} scenario={window:?} punti_veri={truth_points:?} {}", serde_json::to_string(&report).unwrap());
            assert_eq!(report.first_visible + report.first_hidden, 40);
            assert!(report.first_registered_confirmed <= report.first_registered);
            assert_eq!(report.my_turns, 20);
            assert_eq!(report.correct + report.wrong + report.missing, 40);
            assert_eq!(report.duplicate_events, 0);
            // In questo replay la mano può coincidere mentre manca l'apertura
            // avversaria. Verifica anche che l'avviso non copra i turni corretti.
            if seed == 42 && matches!(window, Window::CaptureLoss) {
                assert_eq!(report.wrong_position_with_partial_notice, report.ready_with_wrong_position);
                assert_eq!(report.correct_position_with_partial_notice, 0);
            }
            if matches!(window, Window::Stable | Window::ResizeAndMove) {
                assert_eq!((report.correct, report.wrong, report.missing), (40, 0, 0));
                assert_eq!(report.tricks, 20);
                assert_eq!(report.points, truth_points);
            }
        }
    }
    assert!(ran > 0, "scenario sconosciuto");
}

#[test]
fn validates_checkpoints_against_the_independent_play_log() {
    let plays: Vec<_> = (0..40)
        .map(|card| Play {
            card,
            trick: card / 2 + 1,
            player: card % 2,
        })
        .collect();
    let frames = plays
        .iter()
        .filter(|play| play.player == ME)
        .map(|play| Frame {
            ms: u64::from(play.trick) * 400,
            image: Some("frame.png".into()),
            capture_error: false,
            geometry_changed: false,
            check_turn: Some(Turn { hand: vec![play.card], table: None, trump: 34 }),
            queue_delay_ms: 0,
        })
        .collect();
    let mut manifest = Manifest { version: 1, plays, frames };
    assert!(manifest.validate().is_ok());

    manifest.frames[9].check_turn.as_mut().unwrap().hand = vec![0, 18];
    assert!(manifest.validate().is_err());
    manifest.frames[9].check_turn.as_mut().unwrap().hand = vec![18];
    manifest.frames[9].ms = manifest.frames[8].ms;
    assert!(manifest.validate().is_err());
    manifest.frames[9].ms += 400;
    manifest.plays[19].card = manifest.plays[18].card;
    assert!(manifest.validate().is_err());
}

#[test]
fn separates_wrong_confident_plays_from_missing_plays() {
    let expected = [
        Play { trick: 1, player: ME, card: 0 },
        Play { trick: 1, player: OPP, card: 1 },
        Play { trick: 2, player: ME, card: 2 },
    ];
    let mut session = Session::new();
    for (presa, player, card, identificata) in [(1, ME, 0, false), (1, OPP, 3, true)] {
        session.assistant.rec.events.push(Event::Giocata {
            presa,
            chi: record::who(player),
            carta: record::CardRec::of(card),
            identificata,
            ritaglio: None,
            prima: None,
            consiglio: None,
            seguito_consiglio: None,
        });
    }
    let report = session.finish(&expected);
    assert_eq!(report.correct, 1);
    assert_eq!(report.correct_identified, 0);
    assert_eq!(report.wrong_identified, 1);
    assert_eq!(report.wrong, 1);
    assert_eq!(report.missing, 1);
    assert_eq!(report.first_incorrect_play, Some((1, OPP)));
}

/// Prova interattiva sul desktop Windows usando il backend di cattura di produzione.
/// I pixel del desktop restano in RAM. Il marcatore individua la finestra anche con DPI diversi.
#[cfg(windows)]
#[test]
#[ignore = "richiede un desktop Windows interattivo"]
fn native_window_move_resize_minimize_restore() {
    use crate::vision::reference::card_image;
    use crate::vision::synth::{table, Deal, Look};
    use std::{fs, process::{Child, Command}, thread, time::Instant};

    struct Host(Child, PathBuf);
    impl Drop for Host {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
            let _ = fs::remove_dir_all(&self.1);
        }
    }

    assert!(std::env::var_os("BRISMASTER_SCHERMO").is_none());
    let folder = std::env::temp_dir().join(format!("brismaster-window-{}", std::process::id()));
    fs::create_dir(&folder).unwrap();
    let cards: Vec<_> = [0, 12, 25, 37].map(|c| card_image(0, c)).into_iter().collect();
    let mut painted = table(&Deal {
        hand: cards[..3].iter().collect(), opp: Some(&cards[3]), mine: None, trump: None, deck: false,
    }, &Look::default()).shrink(2);
    let marker = [189, 8, 173];
    for row in painted.px.chunks_exact_mut(painted.w).take(16) { row[..16].fill(marker); }
    painted.save_png(&folder.join("table.png")).unwrap();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/replay-window.ps1");
    let child = Command::new("powershell.exe").args(["-NoProfile", "-STA", "-File"])
        .arg(script).arg(&folder).spawn().unwrap();
    let mut host = Host(child, folder.clone());
    let memory = Mutex::new(Memory::in_memory());
    let mut first_origin = None;
    for phase in 1..=4 {
        fs::write(folder.join(format!("go-{phase}")), []).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            assert!(Instant::now() < deadline, "fase Windows {phase} non osservata");
            assert!(host.0.try_wait().unwrap().is_none(), "finestra terminata");
            thread::sleep(Duration::from_millis(150));
            if !folder.join(format!("ready-{phase}")).exists() { continue; }
            let (primary, count, geom) = capture::grab_geom(0).expect("cattura desktop Windows");
            let mut primary = Some((primary, geom));
            let mut window = None;
            for monitor in 0..count {
                let (img, geom) = primary.take().unwrap_or_else(|| {
                    let (img, _, geom) = capture::grab_geom(monitor).expect("cattura monitor Windows");
                    (img, geom)
                });
                if let Some(index) = img.px.iter().position(|&px| px == marker) {
                    window = Some((monitor, index % img.w, index / img.w, img, geom));
                    break;
                }
            }
            if phase == 3 {
                if window.is_none() { break; }
                continue;
            }
            let Some((monitor, x, y, img, geom)) = window else { continue };
            let (w, h) = if phase == 1 { (700, 450) } else { (350, 225) };
            if x+w > img.w || y+h > img.h { continue; }
            detect::forget_reading_state();
            let Some(scene) = read_briscola(&img.crop(x,y,x+w,y+h), &memory) else { continue };
            let ids: Vec<_> = scene.hand.iter().filter_map(|v| v.best(CardSet::FULL).map(|b| b.0)).collect();
            if ids != [0,12,25] || scene.opp_slot.as_ref().and_then(|v| v.best(CardSet::FULL)).map(|b| b.0) != Some(37) {
                continue;
            }
            if phase == 1 {
                first_origin = Some((monitor, x, y));
            } else {
                assert_ne!(Some((monitor, x, y)), first_origin);
                if phase == 2 && count > 1 { assert_ne!(monitor, first_origin.unwrap().0); }
            }
            println!("fase Windows {phase}, monitor {monitor}/{count}, DPI {}, tavolo {w}x{h} a {x},{y}", geom.scale);
            break;
        }
    }
}

#[test]
#[ignore = "Misura separata del tempo motore, senza cattura o interfaccia"]
fn measure_advice_time() {
    for seed in [1, 2, 31, 42] {
        let mut deal = SmallRng::seed_from_u64(seed);
        let mut game = Game::new(&mut deal, (seed % 2) as u8);
        while !game.is_over() {
            let player = game.to_move();
            let choices = game.legal_moves().to_vec();
            if player == ME && choices.len() > 1 {
                let obs = Observation::of(&game, ME);
                let mut engine = SmallRng::seed_from_u64(seed + game.played.len() as u64);
                let start = std::time::Instant::now();
                let evals = evaluate_with_log(&obs, &crate::app::engine_cfg(1000), &[], &mut engine);
                assert!(choices.contains(&evals[0].card));
                println!("motore,{seed},{},{},{}", game.played.len(), choices.len(), start.elapsed().as_micros());
            }
            game.play(choices[deal.gen_range(0..choices.len())]);
        }
    }
}
