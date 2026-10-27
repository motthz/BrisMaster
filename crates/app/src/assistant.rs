//! Assistente: inserisci a mano briscola, carte in mano, pescate e giocate; il motore consiglia la mossa.

use crate::cards_ui::{self, BEST, CARD_SIZE, GOLD};
use crate::record::{self, Advice, CardRec, Event, Recorder};
use crate::vision::memory::{similarity, Memory};
use crate::vision::{read_briscola, CardView, Img, Reader, Reading, Scene};
use briscola_core::ai::{evaluate, EngineConfig, MoveEval};
use briscola_core::cards::{self, Card, CardSet};
use briscola_core::{Observation, Trick};
use eframe::egui::{self, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

pub const ME: u8 = 0;
pub const OPP: u8 = 1;
const SMALL_CARD: Vec2 = Vec2::new(60.0, 92.0);

/// Stato della partita come lo conosce il giocatore.
#[derive(Clone, Copy, Debug)]
pub struct Position {
    pub trump_card: Option<Card>,
    /// Chi apre la prima presa (scelto dall'utente).
    pub first: Option<u8>,
    pub hand: CardSet,
    pub played: CardSet,
    /// Carte nel mazzo, briscola scoperta compresa.
    pub deck_len: u8,
    pub opp_hand_len: u8,
    pub leader: u8,
    pub table: Option<Card>,
    pub points: [u8; 2],
    pub trump_card_owner: Option<u8>,
    /// Carte che l'utente deve ancora indicare come pescate.
    pub to_draw: u8,
    pub tricks: u8,
}

/// Cosa serve sapere dall'utente in questo momento.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Trump,
    First,
    Draw(u8),
    MyMove,
    OppMove,
    Over,
}

impl Default for Position {
    fn default() -> Self {
        Position {
            trump_card: None,
            first: None,
            hand: CardSet::EMPTY,
            played: CardSet::EMPTY,
            deck_len: 34,
            opp_hand_len: 3,
            leader: ME,
            table: None,
            points: [0, 0],
            trump_card_owner: None,
            to_draw: 3,
            tricks: 0,
        }
    }
}

impl Position {
    pub fn step(&self) -> Step {
        if self.trump_card.is_none() {
            Step::Trump
        } else if self.first.is_none() {
            Step::First
        } else if self.to_draw > 0 {
            Step::Draw(self.to_draw)
        } else if self.tricks >= 20 {
            Step::Over
        } else if self.to_move() == ME {
            Step::MyMove
        } else {
            Step::OppMove
        }
    }

    pub fn to_move(self) -> u8 {
        if self.table.is_some() {
            1 - self.leader
        } else {
            self.leader
        }
    }

    /// Carte di cui non so dove siano (mano avversaria o mazzo coperto).
    fn unseen(&self) -> CardSet {
        let mut u = CardSet::FULL.minus(self.hand).minus(self.played);
        if let Some(t) = self.table {
            u.remove(t);
        }
        if let Some(t) = self.trump_card {
            u.remove(t);
        }
        u
    }

    /// Carte che l'utente può indicare nel passo corrente.
    pub fn allowed(&self) -> CardSet {
        match self.step() {
            Step::Trump => CardSet::FULL,
            Step::Draw(_) => self.unseen(),
            Step::OppMove => {
                let mut u = self.unseen();
                // La briscola scoperta, se l'ha pescata lui e non l'ha ancora giocata.
                let t = self.trump_card.unwrap();
                if self.trump_card_owner == Some(OPP) && !self.played.contains(t) && self.table != Some(t) {
                    u.insert(t);
                }
                u
            }
            Step::MyMove => self.hand,
            Step::First | Step::Over => CardSet::EMPTY,
        }
    }

    /// Applica una carta indicata dall'utente nel passo corrente. Falso se non è ammessa.
    pub fn enter(&mut self, c: Card) -> Option<Option<Trick>> {
        if !self.allowed().contains(c) {
            return None;
        }
        match self.step() {
            Step::Trump => {
                self.trump_card = Some(c);
                Some(None)
            }
            Step::Draw(_) => {
                self.hand.insert(c);
                self.to_draw -= 1;
                Some(None)
            }
            Step::MyMove | Step::OppMove => Some(self.play(c)),
            _ => None,
        }
    }

    pub fn set_first(&mut self, p: u8) {
        self.first = Some(p);
        self.leader = p;
    }

    fn play(&mut self, c: Card) -> Option<Trick> {
        let p = self.to_move();
        if p == ME {
            self.hand.remove(c);
        } else {
            self.opp_hand_len -= 1;
        }
        let Some(lead) = self.table else {
            self.table = Some(c);
            return None;
        };
        let trump_card = self.trump_card.unwrap();
        let winner = if cards::beats(c, lead, cards::suit(trump_card)) { p } else { self.leader };
        let pts = cards::points(lead) + cards::points(c);
        self.points[winner as usize] += pts;
        self.played.insert(lead);
        self.played.insert(c);
        self.tricks += 1;
        let trick = Trick { leader: self.leader, lead, follow: c, winner, points: pts };
        self.table = None;
        self.leader = winner;
        if self.deck_len > 0 {
            // Pesca prima chi ha preso; l'ultima carta del mazzo è la briscola scoperta.
            self.opp_hand_len += 1;
            if self.deck_len == 2 && winner == OPP {
                self.hand.insert(trump_card);
                self.trump_card_owner = Some(ME);
            } else {
                self.to_draw = 1;
                if self.deck_len == 2 {
                    self.trump_card_owner = Some(OPP);
                }
            }
            self.deck_len -= 2;
        }
        Some(trick)
    }

    pub fn observation(&self) -> Observation {
        let trump_card = self.trump_card.unwrap_or(0);
        Observation {
            me: ME,
            hand: self.hand,
            opp_hand_len: self.opp_hand_len,
            deck_len: self.deck_len,
            trump_card,
            trump: cards::suit(trump_card),
            leader: self.leader,
            table: self.table,
            points: self.points,
            played: self.played,
            trump_card_owner: self.trump_card_owner,
        }
    }
}

/// Spiegazione a parole della mossa consigliata.
pub fn explain(obs: &Observation, evals: &[MoveEval]) -> Vec<String> {
    let best = evals[0];
    let c = best.card;
    let trump = obs.trump;
    let mut out = Vec::new();
    match obs.table {
        Some(t) => {
            let pts = cards::points(t) + cards::points(c);
            if cards::beats(c, t, trump) {
                if cards::suit(c) == trump && cards::suit(t) != trump {
                    out.push(format!("Tagli con la briscola: prendi {} e porti a casa {pts} punti.", cards::name(t)));
                } else {
                    out.push(format!("Prendi la presa con {}: incassi {pts} punti.", cards::name(c)));
                }
            } else if pts == 0 {
                out.push("Lasci la presa: sul tavolo non ci sono punti, scarti senza perdere nulla.".into());
            } else {
                let could = obs.hand.iter().find(|&h| cards::beats(h, t, trump));
                match could {
                    Some(h) => out.push(format!(
                        "Lasci {pts} punti all'avversario: prendere con {} ti costerebbe di più nel resto della partita.",
                        cards::name(h)
                    )),
                    None => out.push(format!(
                        "Non puoi prendere: dai all'avversario il meno possibile ({pts} punti in tutto)."
                    )),
                }
            }
        }
        None => {
            let p = cards::points(c);
            out.push(if cards::suit(c) == trump {
                if p >= 10 {
                    "Apri con un carico di briscola: le simulazioni dicono che conviene incassarlo ora.".to_string()
                } else {
                    "Apri con una briscola: togli briscole all'avversario e tieni la mano.".to_string()
                }
            } else if p == 0 {
                "Apri con un liscio: se l'avversario prende, guadagna poco o niente.".to_string()
            } else if p >= 10 {
                "Apri con un carico: secondo le simulazioni l'avversario difficilmente lo prende o lo taglia."
                    .to_string()
            } else {
                format!("Apri con una figura ({p} punti): rischio contenuto, tieni i lisci per dopo.")
            });
        }
    }
    if let Some(second) = evals.get(1) {
        let d = best.win_prob - second.win_prob;
        if d < 0.015 {
            out.push(format!("Scelta quasi indifferente: {} vale praticamente lo stesso.", cards::name(second.card)));
        } else if d >= 0.08 {
            out.push(format!(
                "Scelta netta: la seconda opzione ({}) ha {:.0} punti percentuali in meno di vittoria.",
                cards::name(second.card),
                d * 100.0
            ));
        }
    }
    if obs.deck_len == 0 {
        out.push("Mazzo finito: le carte dell'avversario sono note e il calcolo è esatto.".into());
    } else if obs.deck_len <= 4 {
        out.push("Ultime pescate: il motore risolve ogni finale possibile in modo esatto.".into());
    }
    let mut out_trumps = obs.unknown();
    if obs.trump_card_owner == Some(1 - obs.me) {
        out_trumps.insert(obs.trump_card);
    }
    let trumps: Vec<Card> = out_trumps.iter().filter(|&x| cards::suit(x) == trump).collect();
    let big: Vec<&str> = trumps
        .iter()
        .filter(|&&x| cards::points(x) >= 10)
        .map(|&x| cards::RANK_NAMES[cards::rank(x) as usize])
        .collect();
    let mut line = format!("Briscole che non hai ancora visto: {}", trumps.len());
    if !big.is_empty() {
        line += &format!(" (tra cui {})", big.join(" e "));
    }
    out.push(line + ".");
    let mine = obs.points[obs.me as usize];
    if mine <= 60 {
        out.push(format!("Hai {mine} punti: te ne mancano {} per arrivare a 61.", 61 - mine));
    } else {
        out.push(format!("Hai già {mine} punti: la partita è vinta."));
    }
    out
}

/// Risultato dell'analisi di una posizione.
struct Analysis {
    evals: Vec<MoveEval>,
    reasons: Vec<String>,
    samples: u32,
}

pub struct Assistant {
    pos: Position,
    /// Posizione, prese e eventi registrati prima di ogni azione (per annullare), e se l'azione
    /// l'ha fatta l'app da sola perché obbligata.
    history: Vec<(Position, usize, usize, bool)>,
    tricks: Vec<Trick>,
    rec: Recorder,
    generation: u64,
    analysis: Option<Analysis>,
    pending: Option<Receiver<Analysis>>,
    /// Tasti digitati finora per la carta in corso (es. "3" in attesa del seme).
    keys: String,
    entry_error: Option<String>,
    /// Spazio premuto mentre il motore calcolava: gioca il consiglio appena arriva.
    play_best_queued: bool,
    vision: Vision,
}

/// Distacco minimo fra la carta più probabile e la seconda per inserirla senza chiedere.
const AUTO_MARGIN: f32 = 1.4;

/// Lettura della partita dallo schermo.
#[derive(Default)]
struct Vision {
    reader: Option<Reader<Scene>>,
    memory: Arc<Mutex<Memory>>,
    status: String,
    /// Carte riconosciute all'ultima lettura: si agisce solo se la lettura si ripete uguale
    /// (niente carte a metà animazione).
    seen: Option<Vec<Option<Card>>>,
    /// Carta riconosciuta con poca sicurezza, in attesa che l'utente confermi o la corregga.
    confirm: Option<Confirm>,
    /// Ultima carta inserita dalla lettura (con la lunghezza della cronologia prima di inserirla),
    /// per dimenticarla se l'utente la annulla.
    last: Option<(Card, Img, usize)>,
    /// Ultima scena letta: serve a ricordare la carta pescata quando l'utente la scrive a mano.
    scene: Option<Scene>,
    /// Immagine della briscola di questa partita: se sullo schermo ne compare un'altra, il sito ha
    /// distribuito una nuova partita.
    trump_img: Option<Img>,
    /// Candidati annullati dall'utente: se ricompaiono si chiede conferma invece di inserirli.
    doubts: Vec<(Step, Card)>,
    /// Scelta del monitor e dell'area del tavolo, e diagnosi.
    picker: crate::vision::screen::AreaPicker,
}

struct Confirm {
    step: Step,
    card: Card,
    img: Img,
    /// Fascia della carta che si vedeva sullo schermo.
    vis: [f32; 2],
    texture: Option<egui::TextureHandle>,
}

impl Default for Assistant {
    fn default() -> Self {
        Assistant {
            pos: Position::default(),
            history: Vec::new(),
            tricks: Vec::new(),
            rec: Recorder::default(),
            generation: 0,
            analysis: None,
            pending: None,
            keys: String::new(),
            entry_error: None,
            play_best_queued: false,
            vision: Vision::default(),
        }
    }
}

impl Assistant {
    fn changed(&mut self) {
        self.generation += 1;
        self.analysis = None;
        self.pending = None;
        self.keys.clear();
        self.entry_error = None;
        self.play_best_queued = false;
        self.vision.seen = None;
        self.vision.confirm = None;
    }

    fn set_first(&mut self, p: u8) {
        self.history.push((self.pos, self.tricks.len(), self.rec.events.len(), false));
        self.pos.set_first(p);
        self.rec.events.push(Event::PrimoDiMano { chi: record::who(p) });
        self.changed();
        self.auto_fill();
    }

    /// Annulla l'ultima azione dell'utente, insieme a quelle che l'app ha fatto da sola dopo.
    fn undo(&mut self) {
        self.undo_entries();
        // Se si è annullata una carta inserita dalla lettura dello schermo, era sbagliata.
        if let Some((c, img, len)) = self.vision.last.take() {
            if self.history.len() <= len {
                self.vision.memory.lock().unwrap().forget(&img);
                self.vision.doubts.push((self.pos.step(), c));
            }
        }
    }

    fn undo_entries(&mut self) {
        while let Some((p, n, e, auto)) = self.history.pop() {
            self.pos = p;
            self.tricks.truncate(n);
            self.rec.events.truncate(e);
            self.rec.write(&self.pos);
            self.changed();
            if !auto {
                break;
            }
        }
    }

    /// Inserisce da sola le carte obbligate: la pescata quando resta una sola carta possibile,
    /// l'ultima carta dell'avversario a mazzo finito, la tua ultima carta.
    fn auto_fill(&mut self) {
        while matches!(self.pos.step(), Step::Draw(_) | Step::MyMove | Step::OppMove) {
            let allowed = self.pos.allowed();
            if allowed.len() != 1 {
                break;
            }
            let c = allowed.iter().next().unwrap();
            if !self.record_entry(c, true) {
                break;
            }
        }
    }

    fn reset(&mut self) {
        let vision = std::mem::take(&mut self.vision);
        *self = Assistant::default();
        self.vision = Vision {
            reader: vision.reader,
            memory: vision.memory,
            status: vision.status,
            picker: vision.picker,
            ..Default::default()
        };
        self.vision.doubts.clear();
    }

    /// Inserisce una carta indicata dall'utente. Falso se non è possibile adesso.
    fn enter(&mut self, c: Card) -> bool {
        let ok = self.record_entry(c, false);
        if ok {
            self.auto_fill();
        }
        ok
    }

    fn record_entry(&mut self, c: Card, auto: bool) -> bool {
        let before = self.pos;
        let event = match before.step() {
            Step::Trump => Event::Briscola { carta: CardRec::of(c) },
            Step::Draw(_) => Event::Pescata { carta: CardRec::of(c) },
            Step::MyMove => {
                let advice = self.analysis.as_ref().map(|a| Advice::new(&a.evals, a.samples));
                Event::play(&before, c, advice)
            }
            _ => Event::play(&before, c, None),
        };
        match self.pos.enter(c) {
            Some(trick) => {
                self.history.push((before, self.tricks.len(), self.rec.events.len(), auto));
                self.tricks.extend(trick);
                self.rec.events.push(event);
                self.rec.write(&self.pos);
                self.changed();
                true
            }
            None => false,
        }
    }

    /// Avvia o raccoglie l'analisi della posizione corrente.
    fn poll(&mut self, samples: u32, ctx: &egui::Context) {
        if let Some(rx) = &self.pending {
            if let Ok(a) = rx.try_recv() {
                let best = a.evals.first().map(|e| e.card);
                self.analysis = Some(a);
                self.pending = None;
                if std::mem::take(&mut self.play_best_queued) {
                    if let Some(c) = best {
                        self.enter(c);
                        return;
                    }
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        }
        let wants = self.pos.step() == Step::MyMove && self.pos.hand.len() > 1;
        let stale = self.analysis.as_ref().is_some_and(|a| a.samples != samples);
        if wants && self.pending.is_none() && (self.analysis.is_none() || stale) {
            let obs = self.pos.observation();
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let mut rng = SmallRng::from_entropy();
                let evals = evaluate(&obs, &crate::app::engine_cfg(samples), &mut rng);
                let reasons = explain(&obs, &evals);
                let _ = tx.send(Analysis { evals, reasons, samples });
            });
            self.pending = Some(rx);
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    /// Tastiera sempre attiva, senza campo di testo né Invio: "3c" inserisce il tre di coppe appena
    /// digitato il seme; al tuo turno Spazio (o Invio) gioca il consiglio e 1, 2, 3 la carta in mano
    /// in quella posizione; per chi comincia 1 = tu, 2 = l'avversario.
    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx) {
            return;
        }
        // Niente pulsanti col focus: Spazio e Invio li premerebbero insieme alla scorciatoia.
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            self.undo();
        }
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            let step = self.pos.step();
            match ev {
                egui::Event::Key { key, pressed: true, modifiers, .. } if modifiers.is_none() => match key {
                    egui::Key::Space | egui::Key::Enter if self.vision.confirm.is_some() => self.accept_vision(),
                    egui::Key::Space | egui::Key::Enter if step == Step::MyMove => self.play_best(),
                    egui::Key::Enter if step == Step::Over => self.reset(),
                    egui::Key::Backspace => {
                        self.keys.pop();
                    }
                    egui::Key::Escape => {
                        self.keys.clear();
                        self.entry_error = None;
                        if let Some(c) = self.vision.confirm.take() {
                            self.vision.doubts.push((c.step, c.card));
                        }
                    }
                    _ => {}
                },
                egui::Event::Text(t) => {
                    for ch in t.chars().filter(|c| !c.is_whitespace()) {
                        self.type_char(ch.to_ascii_lowercase());
                    }
                }
                _ => {}
            }
        }
    }

    fn type_char(&mut self, ch: char) {
        match self.pos.step() {
            Step::First => match ch {
                '1' => self.set_first(ME),
                '2' => self.set_first(OPP),
                _ => {}
            },
            Step::MyMove => {
                if let Some(i) = ch.to_digit(10).filter(|d| *d >= 1) {
                    if let Some(c) = self.pos.hand.to_vec().get(i as usize - 1) {
                        self.enter(*c);
                    }
                }
            }
            Step::Trump | Step::Draw(_) | Step::OppMove => {
                self.keys.push(ch);
                self.entry_error = None;
                if let Some(c) = cards_ui::parse_card(&self.keys) {
                    self.keys.clear();
                    let seen = self.vision.confirm.take();
                    // La carta che l'utente scrive a mano si impara dall'immagine sullo schermo:
                    // la pescata (l'unica carta della mano non ancora nota) o quella dell'avversario.
                    let shown = match self.pos.step() {
                        Step::Trump => {
                            let trump = self.vision.scene.as_ref().and_then(|s| s.trump.as_ref());
                            trump.map(|v| (v.img.clone(), v.vis))
                        }
                        Step::Draw(_) => self.new_hand_view().map(|v| (v.img.clone(), v.vis)),
                        Step::OppMove => self.opp_view_of(c),
                        _ => None,
                    };
                    if self.enter(c) {
                        if let Some((img, vis)) = seen.map(|v| (v.img, v.vis)).or(shown) {
                            self.vision.memory.lock().unwrap().remember(&img, vis, c);
                        }
                    } else {
                        self.entry_error = Some(format!("{} non è possibile adesso", cards::name(c)));
                    }
                } else if !is_rank_prefix(&self.keys) {
                    self.entry_error = Some(format!("\"{}\" non è una carta: scrivi valore e seme, es. 3c", self.keys));
                    self.keys.clear();
                }
            }
            Step::Over => {}
        }
    }

    /// Accende o spegne la lettura dello schermo.
    fn set_vision(&mut self, on: bool, ctx: &egui::Context) {
        if on && self.vision.reader.is_none() {
            self.vision.memory = Memory::shared();
            let every = std::time::Duration::from_millis(350);
            self.vision.reader = Some(Reader::start(self.vision.memory.clone(), ctx.clone(), every, read_briscola));
            self.vision.status = "Cerco il tavolo sullo schermo...".into();
        } else if !on {
            self.vision.reader = None;
            self.vision.confirm = None;
            self.vision.status.clear();
        }
    }

    /// Raccoglie l'ultima lettura dello schermo e la confronta con la partita.
    fn poll_vision(&mut self) {
        let Some(reader) = &self.vision.reader else { return };
        let mut latest = None;
        while let Ok(r) = reader.rx.try_recv() {
            latest = Some(r);
        }
        match latest {
            Some(Reading::Scene(scene)) => {
                self.vision.status = format!(
                    "Vedo: briscola {} · {} carte in mano · {} in tavola",
                    if scene.trump.is_some() { "sì" } else { "no" },
                    scene.hand.len(),
                    scene.opp_slot.is_some() as u8 + scene.my_slot.is_some() as u8
                );
                self.sync(&scene);
                self.vision.scene = Some(scene);
            }
            Some(Reading::NoTable) => {
                self.vision.status = "Non vedo il tavolo: tieni la partita aperta e visibile sullo schermo".into()
            }
            Some(Reading::Error(e)) => self.vision.status = format!("Lettura dello schermo non riuscita: {e}"),
            None => {}
        }
    }

    /// Inserisce nella partita quello che si vede sullo schermo, se è uguale alla lettura precedente.
    fn sync(&mut self, scene: &Scene) {
        let best = |v: &CardView| v.best(CardSet::FULL).map(|b| b.0);
        let mut sig: Vec<Option<Card>> =
            [&scene.trump, &scene.opp_slot, &scene.my_slot].iter().map(|v| v.as_ref().and_then(best)).collect();
        sig.extend(scene.hand.iter().map(best));
        if self.vision.seen.as_ref() != Some(&sig) {
            self.vision.seen = Some(sig);
            return;
        }
        // Più passi in una volta: briscola, chi comincia, le tre carte, la prima giocata...
        for _ in 0..8 {
            let before = (self.pos.step(), self.history.len());
            self.sync_step(scene);
            if (self.pos.step(), self.history.len()) == before || self.vision.confirm.is_some() {
                break;
            }
        }
        self.vision.seen = Some(sig);
    }

    /// Vero se lo schermo mostra una partita nuova: briscola scoperta, tre carte in mano e tavolo
    /// vuoto, quando la partita in corso è finita, ha il mazzo esaurito (la briscola non si
    /// vedrebbe più) o aveva un'altra briscola. Così si riparte anche dopo una partita interrotta.
    fn new_deal(&mut self, scene: &Scene) -> bool {
        let Some(t) = &scene.trump else { return false };
        if self.pos.trump_card.is_none() {
            return false;
        }
        let fresh = scene.hand.len() == 3 && scene.opp_slot.is_none() && scene.my_slot.is_none();
        let Some(known) = &self.vision.trump_img else {
            // Prima lettura della briscola di questa partita (anche se l'utente l'ha scritta a mano).
            self.vision.trump_img = Some(t.img.clone());
            return fresh && self.pos.step() == Step::Over;
        };
        let other_trump = similarity(known, &t.img, t.vis) < 0.75;
        if !other_trump {
            // Segue i piccoli cambiamenti (il mazzo che si assottiglia sopra la briscola).
            self.vision.trump_img = Some(t.img.clone());
        }
        fresh && (self.pos.step() == Step::Over || self.pos.deck_len == 0 || other_trump)
    }

    /// Un passo della partita a partire dalla lettura dello schermo.
    fn sync_step(&mut self, scene: &Scene) {
        if self.new_deal(scene) {
            self.reset();
        }
        let step = self.pos.step();
        if step == Step::Over {
            return;
        }
        if self.vision.confirm.is_some() {
            return;
        }
        let pos = self.pos;
        let allowed = pos.allowed();
        let mut known = pos.played.union(pos.hand);
        if let Some(t) = pos.table {
            known.insert(t);
        }
        // Carta candidata per il passo corrente: (carta, distacco, immagine, vista a metà).
        let candidate = match step {
            Step::Trump => scene.trump.as_ref().and_then(|v| v.best(allowed).map(|(c, m)| (c, m, v))),
            Step::First => {
                // Chi gioca la prima carta ha cominciato.
                if scene.opp_slot.is_some() {
                    self.set_first(OPP);
                } else if scene.my_slot.is_some() {
                    self.set_first(ME);
                }
                return;
            }
            Step::Draw(_) => unmatched_hand(scene, &pos)
                .into_iter()
                .filter_map(|v| v.best(allowed.union(pos.played)).map(|(c, m)| (c, m, v)))
                .filter(|(c, _, _)| allowed.contains(*c))
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()),
            Step::OppMove => scene
                .opp_slot
                .as_ref()
                .and_then(|v| v.best(allowed.union(known)).map(|(c, m)| (c, m, v)))
                .filter(|(c, _, _)| allowed.contains(*c)),
            Step::MyMove => scene
                .my_slot
                .as_ref()
                .and_then(|v| v.best(known).map(|(c, m)| (c, m, v)))
                .filter(|(c, _, _)| pos.hand.contains(*c)),
            Step::Over => None,
        };
        let Some((card, margin, view)) = candidate else { return };
        let doubtful = self.vision.doubts.contains(&(step, card));
        if margin >= AUTO_MARGIN && !doubtful {
            let img = view.img.clone();
            let len = self.history.len();
            if self.enter(card) {
                self.vision.memory.lock().unwrap().remember(&img, view.vis, card);
                self.vision.last = Some((card, img, len));
            }
        } else {
            self.vision.confirm =
                Some(Confirm { step, card, img: view.img.clone(), vis: view.vis, texture: None });
        }
    }

    /// La carta della mano sullo schermo che non è nessuna di quelle già in mano: la pescata.
    fn new_hand_view(&self) -> Option<&CardView> {
        let scene = self.vision.scene.as_ref()?;
        let rest = unmatched_hand(scene, &self.pos);
        if rest.len() == 1 {
            Some(rest[0])
        } else {
            None
        }
    }

    /// L'immagine della carta dell'avversario sullo schermo, se somiglia a `c` più che alle carte
    /// già giocate (altrimenti è ancora quella della presa precedente).
    fn opp_view_of(&self, c: Card) -> Option<(Img, [f32; 2])> {
        let v = self.vision.scene.as_ref()?.opp_slot.as_ref()?;
        let old = self.pos.played.iter().map(|p| v.scores[p as usize]).fold(f32::MIN, f32::max);
        (v.scores[c as usize] > old).then(|| (v.img.clone(), v.vis))
    }

    /// L'utente conferma la carta riconosciuta con poca sicurezza: si inserisce e si ricorda.
    fn accept_vision(&mut self) {
        let Some(c) = self.vision.confirm.take() else { return };
        if c.step == self.pos.step() && self.enter(c.card) {
            self.vision.memory.lock().unwrap().remember(&c.img, c.vis, c.card);
        }
    }

    /// Testo per la diagnosi: cosa si vede e cosa è stato riconosciuto.
    fn vision_summary(&self) -> String {
        let mut out = format!("{}\n", self.vision.status);
        let Some(s) = &self.vision.scene else { return out };
        let mut line = |what: &str, v: &CardView| {
            let mut sc: Vec<(Card, f32)> = (0..40u8).map(|c| (c, v.scores[c as usize])).collect();
            sc.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let top: Vec<String> = sc[..3].iter().map(|(c, x)| format!("{} {x:.1}", cards::name(*c))).collect();
            out += &format!("{what}: {}{}\n", top.join(" · "), if v.partial { " (vista a metà)" } else { "" });
        };
        for (what, v) in [("Briscola", &s.trump), ("Avversario", &s.opp_slot), ("Tua giocata", &s.my_slot)] {
            if let Some(v) = v {
                line(what, v);
            }
        }
        for v in &s.hand {
            line("Mano", v);
        }
        out
    }

    /// Interruttore, stato e richiesta di conferma della lettura dello schermo.
    fn vision_ui(&mut self, ui: &mut egui::Ui) {
        let mut on = self.vision.reader.is_some();
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut on, RichText::new("Leggi la partita dallo schermo").strong())
                .on_hover_text("BrisMaster guarda il tavolo del sito e inserisce da solo le carte. Non tocca il sito: le carte le giochi sempre tu.")
                .changed()
            {
                self.set_vision(on, ui.ctx());
            }
            if on {
                let n = self.vision.memory.lock().unwrap().len();
                ui.label(RichText::new(format!("{n}/40 carte in memoria")).small().weak())
                    .on_hover_text("Le carte già riconosciute si ricordano da sole e la volta dopo si riconoscono al primo colpo");
            }
        });
        if !on {
            return;
        }
        ui.label(RichText::new(&self.vision.status).small().weak());
        let last = self.vision.reader.as_ref().and_then(|r| r.last_frame());
        let summary = self.vision_summary();
        self.vision.picker.controls(ui, last, &summary);
        let ctx = ui.ctx().clone();
        if let Some(c) = &mut self.vision.confirm {
            let tex = c.texture.get_or_insert_with(|| {
                let rgba: Vec<u8> = c.img.px.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                let image = egui::ColorImage::from_rgba_unmultiplied([c.img.w, c.img.h], &rgba);
                ctx.load_texture("vision_confirm", image, Default::default())
            });
            egui::Frame::new()
                .fill(Color32::from_rgb(60, 48, 20))
                .stroke(Stroke::new(1.5, GOLD))
                .corner_radius(8.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Image::new(&*tex).fit_to_exact_size(vec2(50.0, 80.0)));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("Mi sembra {}", cards::name(c.card))).size(17.0).strong());
                            ui.label("Spazio = giusto · se sbaglio scrivi la carta (es. Rd) · Esc = ignora");
                        });
                    });
                });
        }
        ui.add_space(6.0);
    }

    /// Gioca la carta consigliata, o la prenota se il motore sta ancora calcolando.
    fn play_best(&mut self) {
        match self.analysis.as_ref().and_then(|a| a.evals.first()) {
            Some(e) => {
                let c = e.card;
                self.enter(c);
            }
            None => self.play_best_queued = true,
        }
    }

    fn eval_of(&self, c: Card) -> Option<(usize, MoveEval)> {
        let a = self.analysis.as_ref()?;
        a.evals.iter().position(|e| e.card == c).map(|i| (i, a.evals[i]))
    }

    /// Area di sinistra: tavolo con briscola, mazzo, presa in corso e mano.
    pub fn table_ui(&mut self, ui: &mut egui::Ui) {
        let pos = self.pos;
        let step = pos.step();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            steps_bar(ui, step, pos.tricks);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                if ui.button("Nuova partita").clicked() {
                    self.reset();
                }
                let undo = ui
                    .add_enabled(!self.history.is_empty(), egui::Button::new("Annulla ultima azione"))
                    .on_hover_text("Ctrl+Z");
                if undo.clicked() {
                    self.undo();
                }
            });
        });
        ui.add_space(10.0);

        // Avversario.
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!("Avversario · {} carte in mano · {} punti", pos.opp_hand_len, pos.points[1]))
                    .size(15.0)
                    .color(Color32::from_white_alpha(200)),
            );
            ui.horizontal(|ui| {
                ui.set_min_height(SMALL_CARD.y);
                let n = pos.opp_hand_len as usize;
                center(ui, n as f32 * (SMALL_CARD.x + 6.0));
                for _ in 0..n {
                    cards_ui::card(ui, None, SMALL_CARD, None, false);
                }
            });
        });
        ui.add_space(12.0);

        // Briscola e mazzo, presa in corso, punteggio.
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Briscola").color(GOLD).strong());
                let (rect, _) = ui.allocate_exact_size(CARD_SIZE, Sense::hover());
                match pos.trump_card {
                    Some(t) => {
                        let look = cards_ui::Look { dim: pos.deck_len == 0, ..Default::default() };
                        cards_ui::paint_face(ui.painter(), rect, t, look);
                    }
                    None => cards_ui::paint_slot(ui.painter(), rect, "?"),
                }
                let deck = match (pos.deck_len, pos.trump_card_owner) {
                    (0, Some(ME)) => "Mazzo finito · briscola tua".to_string(),
                    (0, _) => "Mazzo finito · briscola all'avversario".to_string(),
                    (n, _) => format!("Mazzo: {n} carte"),
                };
                ui.label(RichText::new(deck).color(Color32::from_white_alpha(190)));
            });
            ui.add_space(30.0);
            let (opp_c, my_c) = if pos.leader == ME { (None, pos.table) } else { (pos.table, None) };
            for (label, c, waiting) in
                [("Avversario", opp_c, step == Step::OppMove), ("Tu", my_c, step == Step::MyMove)]
            {
                ui.vertical(|ui| {
                    ui.label(RichText::new(label).color(Color32::from_white_alpha(200)));
                    let (rect, _) = ui.allocate_exact_size(CARD_SIZE, Sense::hover());
                    match c {
                        Some(c) => cards_ui::paint_face(ui.painter(), rect, c, Default::default()),
                        None => {
                            cards_ui::paint_slot(ui.painter(), rect, if waiting { "tocca a te" } else { "" });
                            if waiting {
                                let t = ui.input(|i| i.time) as f32;
                                let a = 0.5 + 0.5 * (t * 3.0).sin();
                                ui.painter().rect_stroke(
                                    rect,
                                    10.0,
                                    Stroke::new(2.0, GOLD.gamma_multiply(0.4 + 0.6 * a)),
                                    StrokeKind::Inside,
                                );
                                ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
                            }
                        }
                    }
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.add_space(20.0);
                score_box(ui, pos.points, pos.tricks, "Avversario");
            });
        });
        ui.add_space(6.0);
        if let Some(t) = self.tricks.last() {
            let (mine, theirs) = if t.leader == ME { (t.lead, t.follow) } else { (t.follow, t.lead) };
            let who = if t.winner == ME { "hai preso tu" } else { "ha preso l'avversario" };
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(format!(
                        "Ultima presa: tu {} · avversario {}: {who} ({} punti)",
                        cards::name(mine),
                        cards::name(theirs),
                        t.points
                    ))
                    .color(Color32::from_white_alpha(200)),
                );
            });
        }
        ui.add_space(10.0);

        // Mano, con le valutazioni del motore sotto ogni carta.
        let mut clicked = None;
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("La tua mano").size(16.0).strong().color(Color32::WHITE));
            ui.horizontal(|ui| {
                ui.set_min_height(CARD_SIZE.y + 40.0);
                let hand = pos.hand.to_vec();
                center(ui, hand.len() as f32 * (CARD_SIZE.x + 14.0));
                for (n, c) in hand.into_iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(CARD_SIZE.x + 6.0);
                        let ev = self.eval_of(c);
                        let best = matches!(ev, Some((0, _)));
                        let my_turn = step == Step::MyMove;
                        let glow = (my_turn && best).then_some(BEST);
                        let resp = cards_ui::card(ui, Some(c), CARD_SIZE, glow, my_turn);
                        if resp.clicked() {
                            clicked = Some(c);
                        }
                        if let Some((i, e)) = ev {
                            badge(ui, i == 0, e);
                        }
                        if my_turn {
                            ui.label(RichText::new(format!("tasto {}", n + 1)).color(GOLD));
                        }
                    });
                    ui.add_space(8.0);
                }
            });
            if step == Step::MyMove {
                ui.label(
                    RichText::new("Clicca la carta che giochi, o premi il suo tasto (Spazio = quella consigliata)")
                        .color(Color32::from_white_alpha(170)),
                );
            }
        });
        if let Some(c) = clicked {
            self.enter(c);
        }
    }

    /// Pannello di destra: richiesta di input oppure consiglio del motore. Nella finestra compatta
    /// sostituisce il tavolo: in cima c'è un riepilogo con la tua mano, niente elenco delle uscite.
    pub fn side_ui(&mut self, ui: &mut egui::Ui, samples: &mut u32, compact: bool) {
        let ctx = ui.ctx().clone();
        self.keyboard(&ctx);
        self.poll_vision();
        self.poll(*samples, &ctx);
        let step = self.pos.step();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if compact {
                self.compact_header(ui);
                ui.separator();
            }
            self.vision_ui(ui);
            match step {
                Step::Trump => {
                    title(ui, "Qual è la briscola?", "Indica la carta scoperta sotto il mazzo.");
                    self.picker_and_entry(ui);
                }
                Step::First => {
                    title(ui, "Chi gioca per primo?", "Chi apre la prima presa della partita (tasti 1 e 2).");
                    ui.horizontal(|ui| {
                        let w = if compact { 150.0 } else { 200.0 };
                        let big = |t: &str| egui::Button::new(RichText::new(t).size(17.0)).min_size(vec2(w, 44.0));
                        if ui.add(big("1 · Comincio io")).clicked() {
                            self.set_first(ME);
                        }
                        if ui.add(big("2 · L'avversario")).clicked() {
                            self.set_first(OPP);
                        }
                    });
                }
                Step::Draw(n) => {
                    if self.pos.tricks == 0 {
                        title(ui, "Le tue tre carte", &format!("Indica le carte che hai in mano: ne mancano {n}."));
                    } else {
                        title(ui, "Che carta hai pescato?", "Il mazzo l'ha data a te dopo l'ultima presa.");
                    }
                    self.picker_and_entry(ui);
                }
                Step::OppMove => {
                    let sub =
                        if self.pos.table.is_some() { "Ha risposto alla tua carta." } else { "Apre lui questa presa." };
                    title(ui, "Che carta ha giocato l'avversario?", sub);
                    self.picker_and_entry(ui);
                }
                Step::MyMove => {
                    self.advice(ui, samples);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(if self.play_best_queued {
                            "Spazio premuto: inserisco il consiglio appena il motore ha finito."
                        } else {
                            "Spazio (o Invio) = hai giocato il consiglio · 1 2 3 = la carta in quella posizione della mano · oppure clicca la carta"
                        })
                        .color(GOLD),
                    );
                }
                Step::Over => {
                    let mine = self.pos.points[ME as usize];
                    let (t, col) = match mine {
                        p if p > 60 => (format!("Hai vinto {mine} a {}!", 120 - mine), BEST),
                        60 => ("Pareggio 60 a 60".to_string(), Color32::YELLOW),
                        _ => (format!("Hai perso {mine} a {}", 120 - mine), Color32::LIGHT_RED),
                    };
                    ui.add_space(10.0);
                    ui.label(RichText::new(t).size(28.0).strong().color(col));
                    ui.add_space(8.0);
                    if ui.button(RichText::new("Nuova partita (Invio)").size(17.0)).clicked() {
                        self.reset();
                    }
                }
            }
            if compact {
                return;
            }
            ui.add_space(14.0);
            ui.separator();
            ui.heading("Carte uscite");
            let mut gone = self.pos.played;
            if let Some(t) = self.pos.table {
                gone.insert(t);
            }
            tracker(ui, gone, self.pos.hand);
            ui.add_space(14.0);
            ui.separator();
            self.recording_ui(ui);
        });
    }

    /// Riepilogo della finestra compatta: briscola, mazzo, punti, presa in corso e la tua mano
    /// (numerata per i tasti 1 2 3 e cliccabile quando tocca a te).
    fn compact_header(&mut self, ui: &mut egui::Ui) {
        let pos = self.pos;
        let step = pos.step();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Presa {}/20", (pos.tricks + 1).min(20))).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Nuova partita").clicked() {
                    self.reset();
                }
                if ui
                    .add_enabled(!self.history.is_empty(), egui::Button::new("Annulla").small())
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    self.undo();
                }
            });
        });
        let mini = vec2(44.0, 68.0);
        let mut clicked = None;
        ui.horizontal(|ui| {
            for (label, c) in [("Briscola", pos.trump_card), ("In tavola", pos.table)] {
                ui.vertical(|ui| {
                    ui.label(RichText::new(label).small().color(GOLD));
                    let (rect, _) = ui.allocate_exact_size(mini, Sense::hover());
                    match c {
                        Some(c) => cards_ui::paint_face(ui.painter(), rect, c, Default::default()),
                        None => cards_ui::paint_slot(ui.painter(), rect, ""),
                    }
                });
            }
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("La tua mano").small());
                ui.horizontal(|ui| {
                    let my_turn = step == Step::MyMove;
                    for (i, c) in pos.hand.to_vec().into_iter().enumerate() {
                        ui.vertical(|ui| {
                            let best = matches!(self.eval_of(c), Some((0, _)));
                            let glow = (my_turn && best).then_some(BEST);
                            if cards_ui::card(ui, Some(c), mini, glow, my_turn).clicked() {
                                clicked = Some(c);
                            }
                            if my_turn {
                                ui.label(RichText::new(format!("{}", i + 1)).strong().color(GOLD));
                            }
                        });
                    }
                });
            });
        });
        ui.label(
            RichText::new(format!(
                "Tu {} · avversario {} punti · mazzo {} carte",
                pos.points[ME as usize], pos.points[OPP as usize], pos.deck_len
            ))
            .weak(),
        );
        if let Some(c) = clicked {
            self.enter(c);
        }
    }

    /// Dove viene salvata la partita.
    fn recording_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("● Registrazione automatica").color(Color32::LIGHT_RED).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Apri cartella partite").clicked() {
                    record::open_dir();
                }
            });
        });
        ui.label(
            RichText::new(format!(
                "Ogni partita viene salvata, dalla prima carta giocata, in {}",
                self.rec.path().display()
            ))
            .small()
            .weak(),
        );
        if let Some(e) = &self.rec.error {
            ui.label(RichText::new(e).color(Color32::LIGHT_RED));
        }
    }

    fn picker_and_entry(&mut self, ui: &mut egui::Ui) {
        if let Some(c) = cards_ui::picker(ui, "assist_picker", self.pos.allowed()) {
            self.enter(c);
        }
        ui.add_space(8.0);
        self.keys_hint(ui);
    }

    /// Cosa hai digitato finora e promemoria dei tasti.
    fn keys_hint(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Da tastiera:").weak());
            let shown = if self.keys.is_empty() { "_".to_string() } else { format!("{}_", self.keys.to_uppercase()) };
            ui.label(RichText::new(shown).monospace().size(18.0).color(GOLD));
        });
        if let Some(e) = &self.entry_error {
            ui.label(RichText::new(e).color(Color32::LIGHT_RED));
        }
        ui.label(
            RichText::new("Scrivi valore e seme, senza Invio: A 2 3 4 5 6 7 F C R · d c s b (es. 3c). Ctrl+Z annulla")
                .small()
                .weak(),
        );
    }

    fn advice(&mut self, ui: &mut egui::Ui, samples: &mut u32) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Consiglio").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("precision")
                    .selected_text(precision_label(*samples))
                    .show_ui(ui, |ui| {
                        for s in [1000, 3000, crate::app::ISMCTS_FROM] {
                            ui.selectable_value(samples, s, precision_label(s));
                        }
                    })
                    .response
                    .on_hover_text("Quanto calcola il motore: Veloce e Normale simulano distribuzioni delle carte coperte, Massima usa la ricerca ad albero (la più forte, circa un decimo di secondo)");
            });
        });
        if self.pos.hand.len() == 1 {
            let c = self.pos.hand.iter().next().unwrap();
            ui.label(RichText::new(format!("Hai solo {}: giocala.", cards::name(c))).size(18.0));
            return;
        }
        let Some(a) = &self.analysis else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Il motore sta simulando le possibili mani dell'avversario...");
            });
            return;
        };
        let best = a.evals[0];
        egui::Frame::new()
            .fill(Color32::from_rgb(24, 52, 36))
            .stroke(Stroke::new(1.5, BEST))
            .corner_radius(10.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    cards_ui::card(ui, Some(best.card), vec2(62.0, 96.0), None, false);
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Gioca").color(Color32::from_white_alpha(180)));
                        ui.label(RichText::new(cards::name(best.card)).size(24.0).strong().color(Color32::WHITE));
                        ui.label(
                            RichText::new(format!(
                                "Vittoria {:.0}% · punti attesi {:.0}",
                                best.win_prob * 100.0,
                                best.exp_points
                            ))
                            .size(16.0)
                            .color(BEST),
                        );
                    });
                });
            });
        ui.add_space(8.0);
        for r in &a.reasons {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("•").color(GOLD));
                ui.label(r);
            });
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Tutte le carte").strong());
        for (i, e) in a.evals.iter().enumerate() {
            ui.horizontal(|ui| {
                let name = RichText::new(format!("{:<20}", cards::name(e.card))).monospace();
                ui.label(if i == 0 { name.color(BEST) } else { name });
                let (rect, _) = ui.allocate_exact_size(vec2(130.0, 14.0), Sense::hover());
                ui.painter().rect_filled(rect, 4.0, Color32::from_gray(50));
                let mut fill = rect;
                fill.set_width(rect.width() * e.win_prob as f32);
                ui.painter().rect_filled(fill, 4.0, if i == 0 { BEST } else { Color32::from_gray(150) });
                ui.label(format!("{:>3.0}%", e.win_prob * 100.0));
                ui.label(RichText::new(format!("{:.0} pt", e.exp_points)).weak());
            });
        }
        ui.label(
            RichText::new(if a.samples >= crate::app::ISMCTS_FROM {
                format!(
                    "Stime della ricerca ad albero ({} partite simulate sulle carte che non vedi), usando solo ciò che sai.",
                    a.samples
                )
            } else {
                format!("Stime su {} distribuzioni delle carte che non vedi, usando solo ciò che sai.", a.samples)
            })
            .small()
            .weak(),
        );
    }
}

/// Carte della mano sullo schermo tolte quelle che corrispondono alle carte già in mano
/// (a ciascuna carta nota la vista che le somiglia di più).
fn unmatched_hand<'a>(scene: &'a Scene, pos: &Position) -> Vec<&'a CardView> {
    let mut views: Vec<&CardView> = scene.hand.iter().collect();
    for k in pos.hand.iter() {
        let best = (0..views.len())
            .max_by(|&a, &b| views[a].scores[k as usize].partial_cmp(&views[b].scores[k as usize]).unwrap());
        if let Some(i) = best {
            views.remove(i);
        }
    }
    views
}

/// Vero se i tasti digitati possono ancora diventare una carta (manca il seme).
pub fn is_rank_prefix(keys: &str) -> bool {
    matches!(keys, "a" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "f" | "j" | "c" | "q" | "r" | "k" | "10")
}

fn precision_label(s: u32) -> String {
    match s {
        0..=1000 => format!("Veloce ({s} mondi)"),
        s if s < crate::app::ISMCTS_FROM => format!("Normale ({s} mondi)"),
        _ => format!("Massima (ISMCTS {s})"),
    }
}

fn title(ui: &mut egui::Ui, t: &str, sub: &str) {
    ui.label(RichText::new(t).size(22.0).strong().color(Color32::WHITE));
    ui.label(RichText::new(sub).color(Color32::from_white_alpha(170)));
    ui.add_space(8.0);
}

fn center(ui: &mut egui::Ui, w: f32) {
    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
}

/// Barra dei passi: briscola, chi inizia, mano iniziale, partita.
fn steps_bar(ui: &mut egui::Ui, step: Step, tricks: u8) {
    let current = match step {
        Step::Trump => 0,
        Step::First => 1,
        Step::Draw(_) if tricks == 0 => 2,
        _ => 3,
    };
    for (i, name) in ["Briscola", "Chi inizia", "Mano iniziale", "Partita"].iter().enumerate() {
        let done = i < current;
        let active = i == current;
        let (fill, text) = if active {
            (GOLD, Color32::from_rgb(40, 30, 10))
        } else if done {
            (Color32::from_rgb(40, 110, 70), Color32::WHITE)
        } else {
            (Color32::from_black_alpha(60), Color32::from_white_alpha(150))
        };
        let label = format!("{} {}", i + 1, name);
        egui::Frame::new().fill(fill).corner_radius(12.0).inner_margin(vec2(10.0, 4.0)).show(ui, |ui| {
            ui.label(RichText::new(label).color(text).strong());
        });
    }
}

pub fn score_box(ui: &mut egui::Ui, points: [u8; 2], tricks: u8, opp: &str) {
    egui::Frame::new().fill(Color32::from_black_alpha(70)).corner_radius(10.0).inner_margin(12.0).show(ui, |ui| {
        ui.set_width(160.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("Tu  {}", points[0])).size(24.0).strong().color(Color32::WHITE));
            ui.label(RichText::new(format!("{opp}  {}", points[1])).size(18.0).color(Color32::from_white_alpha(200)));
            ui.label(
                RichText::new(format!("Presa {} di 20", (tricks + 1).min(20))).color(Color32::from_white_alpha(150)),
            );
            let (rect, _) = ui.allocate_exact_size(vec2(150.0, 8.0), Sense::hover());
            ui.painter().rect_filled(rect, 4.0, Color32::from_gray(60));
            let mut mine = rect;
            mine.set_width(rect.width() * points[0] as f32 / 120.0);
            ui.painter().rect_filled(mine, 4.0, BEST);
            let x = rect.left() + rect.width() * 61.0 / 120.0;
            ui.painter().vline(x, rect.expand(3.0).y_range(), Stroke::new(1.5, GOLD));
        });
    });
}

/// Etichetta sotto una carta della mano con la probabilità di vittoria.
pub fn badge(ui: &mut egui::Ui, best: bool, e: MoveEval) {
    let text = format!("{:.0}%", e.win_prob * 100.0);
    let (fill, col) =
        if best { (BEST, Color32::from_rgb(10, 40, 20)) } else { (Color32::from_black_alpha(90), Color32::WHITE) };
    let w = CARD_SIZE.x;
    let (rect, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
    let pill = Rect::from_center_size(rect.center(), vec2(64.0, 20.0));
    ui.painter().rect_filled(pill, 10.0, fill);
    ui.painter().text(pill.center(), Align2::CENTER_CENTER, text, FontId::proportional(14.0), col);
    ui.label(RichText::new(format!("{:.0} punti attesi", e.exp_points)).small().color(Color32::from_white_alpha(170)));
}

/// Griglia delle carte uscite: barrate se uscite, evidenziate se in mano.
pub fn tracker(ui: &mut egui::Ui, gone: CardSet, mine: CardSet) {
    egui::Grid::new("tracker").min_col_width(20.0).spacing([6.0, 4.0]).show(ui, |ui| {
        for s in 0..4u8 {
            let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
            cards_ui::paint_suit(ui.painter(), r.center(), 18.0, s);
            for k in cards_ui::RANK_ORDER {
                let c = cards::make(s, k);
                let txt = RichText::new(cards::RANK_SHORT[k as usize]).monospace().size(15.0);
                let txt = if gone.contains(c) {
                    txt.strikethrough().color(Color32::from_gray(90))
                } else if mine.contains(c) {
                    txt.color(Color32::BLACK).background_color(Color32::LIGHT_BLUE)
                } else {
                    txt.color(cards_ui::suit_color_light(s)).strong()
                };
                ui.label(txt);
            }
            ui.end_row();
        }
    });
    let remaining: u32 = CardSet::FULL.minus(gone).iter().map(|c| cards::points(c) as u32).sum();
    ui.label(format!("Punti ancora in gioco: {remaining}"));
    ui.label(RichText::new("Barrate = uscite, evidenziate = in mano a te").weak().small());
}

#[cfg(test)]
mod tests {
    use super::*;
    use briscola_core::Game;
    use rand::Rng;

    /// Riproduce partite casuali inserendo solo ciò che vede il giocatore 0,
    /// e controlla che la posizione dell'assistente coincida con quella reale.
    #[test]
    fn tracks_real_games() {
        let mut rng = SmallRng::seed_from_u64(11);
        for n in 0..300 {
            let first = (n % 2) as u8;
            let mut g = Game::new(&mut rng, first);
            let mut a = Position::default();
            assert!(a.enter(g.trump_card).is_some());
            a.set_first(first);
            for c in g.hands[0].iter() {
                assert!(a.enter(c).is_some());
            }
            while !g.is_over() {
                let p = g.to_move();
                let m = g.legal_moves().to_vec();
                let c = m[rng.gen_range(0..m.len())];
                assert_eq!(a.step(), if p == ME { Step::MyMove } else { Step::OppMove });
                let before = g.hands[0];
                g.play(c);
                assert!(a.enter(c).is_some(), "carta rifiutata");
                // Le carte pescate dal giocatore 0 (tranne la briscola, che è automatica).
                for d in g.hands[0].minus(before).iter() {
                    if !a.hand.contains(d) {
                        assert!(matches!(a.step(), Step::Draw(1)));
                        assert!(a.enter(d).is_some());
                    }
                }
                let o = a.observation();
                let real = Observation::of(&g, 0);
                assert_eq!(o.hand, real.hand);
                assert_eq!(o.opp_hand_len, real.opp_hand_len);
                assert_eq!(o.deck_len, real.deck_len);
                assert_eq!(o.leader, real.leader);
                assert_eq!(o.table, real.table);
                assert_eq!(o.points, real.points);
                assert_eq!(o.played, real.played);
                assert_eq!(o.trump_card_owner, real.trump_card_owner);
            }
            assert_eq!(a.step(), Step::Over);
        }
    }

    /// Una partita inserita nell'assistente finisce per intero nel file, con esito e consigli.
    #[test]
    fn records_games() {
        let dir = std::env::temp_dir().join(format!("brismaster-test-{}", std::process::id()));
        let mut rng = SmallRng::seed_from_u64(5);
        let mut g = Game::new(&mut rng, ME);
        let mut a = Assistant::default();
        a.rec.dir = dir.clone();
        assert!(a.enter(g.trump_card));
        a.set_first(ME);
        for c in g.hands[0].iter() {
            assert!(a.enter(c));
        }
        let mut advised = 0;
        while !g.is_over() {
            if a.pos.step() == Step::MyMove && a.pos.hand.len() > 1 && advised < 2 {
                // Consiglio finto, per non far girare il motore nel test.
                let evals: Vec<MoveEval> =
                    a.pos.hand.iter().map(|card| MoveEval { card, win_prob: 0.5, exp_points: 60.0 }).collect();
                a.analysis = Some(Analysis { evals, reasons: Vec::new(), samples: 7 });
                advised += 1;
            }
            let c = g.legal_moves().to_vec()[0];
            let before = g.hands[0];
            g.play(c);
            // Le carte obbligate le ha già inserite l'app.
            if !a.pos.played.contains(c) && a.pos.table != Some(c) {
                assert!(a.enter(c));
            }
            for d in g.hands[0].minus(before).iter() {
                if !a.pos.hand.contains(d) {
                    assert!(a.enter(d));
                }
            }
        }
        let path = a.rec.path().to_path_buf();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["finita"], true);
        assert_eq!(v["risultato"]["punti_miei"], g.points[0]);
        let ev = v["eventi"].as_array().unwrap();
        assert_eq!(ev[0]["tipo"], "briscola");
        assert_eq!(ev[1]["chi"], "io");
        assert_eq!(ev.iter().filter(|e| e["tipo"] == "giocata").count(), 40);
        assert_eq!(ev.iter().filter(|e| e["consiglio"]["mondi"] == 7).count(), 2);
        // L'ultima presa si inserisce da sola: la tua ultima carta e quella dell'avversario.
        let autos: Vec<_> = a.history.iter().filter(|h| h.3).map(|h| h.0.step()).collect();
        assert!(autos.len() >= 2, "{autos:?} {:?}", a.pos.step());
        // Annullare tutto cancella il file di una partita mai cominciata.
        while !a.history.is_empty() {
            a.undo();
        }
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Inserimento da tastiera: la carta entra appena si digita il seme, senza Invio.
    #[test]
    fn types_cards_without_enter() {
        let mut a = Assistant::default();
        a.type_char('3');
        assert_eq!(a.pos.step(), Step::Trump, "manca ancora il seme");
        a.type_char('c');
        assert_eq!(a.pos.trump_card, Some(cards::make(1, 2)));
        a.type_char('1');
        assert_eq!(a.pos.first, Some(ME), "1 = comincio io");
        a.type_char('x');
        assert!(a.entry_error.is_some() && a.keys.is_empty());
        for ch in "10bas3c".chars() {
            a.type_char(ch);
        }
        assert_eq!(a.pos.hand.len(), 2, "il tre di coppe è la briscola scoperta: rifiutato");
        assert!(a.pos.hand.contains(cards::make(3, 9)) && a.pos.hand.contains(cards::make(2, 0)));
        assert!(a.entry_error.as_deref().unwrap().contains("non è possibile"));
        a.type_char('r');
        a.type_char('d');
        assert_eq!(a.pos.step(), Step::MyMove);
        // 2 = seconda carta della mano, nell'ordine in cui è mostrata.
        let second = a.pos.hand.to_vec()[1];
        a.type_char('2');
        assert_eq!(a.pos.table, Some(second));
        assert_eq!(a.pos.step(), Step::OppMove);
    }

    /// Spazio gioca il consiglio; se il motore sta ancora calcolando lo gioca appena arriva.
    #[test]
    fn space_plays_advice() {
        let mut a = Assistant::default();
        a.enter(cards::make(0, 4));
        a.set_first(ME);
        for c in [cards::make(1, 0), cards::make(2, 1), cards::make(3, 5)] {
            a.enter(c);
        }
        a.play_best();
        assert!(a.play_best_queued && a.pos.table.is_none());
        let (tx, rx) = channel();
        let evals = [cards::make(2, 1), cards::make(1, 0), cards::make(3, 5)]
            .map(|card| MoveEval { card, win_prob: 0.5, exp_points: 60.0 })
            .to_vec();
        tx.send(Analysis { evals, reasons: Vec::new(), samples: 7 }).unwrap();
        a.pending = Some(rx);
        a.poll(7, &egui::Context::default());
        assert_eq!(a.pos.table, Some(cards::make(2, 1)));
        assert!(!a.play_best_queued);
        // Annulla torna a prima della giocata.
        a.undo();
        assert_eq!(a.pos.step(), Step::MyMove);
        assert_eq!(a.pos.hand.len(), 3);
    }

    /// Una carta vista sullo schermo, riconosciuta come `c` con distacco `margin`.
    fn seen(c: Card, margin: f32) -> CardView {
        let mut scores = [0.0; 40];
        scores[c as usize] = margin;
        // Un disegno diverso per ogni carta, per la memoria delle immagini.
        let mut img = Img::new(crate::vision::CARD_W, crate::vision::CARD_H);
        for (i, p) in img.px.iter_mut().enumerate() {
            let v = ((i / 7) as u32).wrapping_mul(c as u32 + 3).wrapping_mul(2654435761).wrapping_shr(24) as u8;
            *p = [v, v / 2, 255 - v];
        }
        CardView { img, partial: false, vis: crate::vision::WHOLE, sim: [0.0; 40], scores }
    }

    /// Ogni lettura arriva due volte, come dallo schermo (la prima si scarta per prudenza).
    fn show(a: &mut Assistant, scene: &Scene) {
        a.sync(scene);
        a.sync(scene);
    }

    /// La partita si segue da sola dallo schermo; le carte fantasma (già giocate ma ancora
    /// disegnate in mano dal sito) si ignorano.
    #[test]
    fn follows_the_screen() {
        let mut a = Assistant::default();
        let c = |s, r| cards::make(s, r);
        let mut scene = Scene { trump: Some(seen(c(0, 8), 4.0)), ..Default::default() };
        show(&mut a, &scene);
        assert_eq!(a.pos.trump_card, Some(c(0, 8)));
        scene.hand = vec![seen(c(3, 1), 4.0), seen(c(2, 6), 4.0), seen(c(1, 0), 4.0)];
        show(&mut a, &scene);
        assert_eq!(a.pos.step(), Step::First, "chi comincia si capisce dalla prima carta giocata");
        scene.opp_slot = Some(seen(c(3, 2), 4.0));
        show(&mut a, &scene);
        assert_eq!(a.pos.first, Some(OPP));
        assert_eq!(a.pos.hand.len(), 3);
        assert_eq!(a.pos.table, Some(c(3, 2)));
        assert_eq!(a.pos.step(), Step::MyMove);
        // Rispondo col 2 di bastoni: sparisce dalla mano e compare in tavola.
        scene.hand.remove(0);
        scene.my_slot = Some(seen(c(3, 1), 4.0));
        show(&mut a, &scene);
        assert_eq!(a.pos.tricks, 1);
        assert!(matches!(a.pos.step(), Step::Draw(1)));
        // Pesco; il sito disegna ancora il 2 di bastoni in mano (carta fantasma).
        scene.hand = vec![seen(c(3, 1), 4.0), seen(c(2, 6), 4.0), seen(c(1, 0), 4.0), seen(c(0, 3), 4.0)];
        show(&mut a, &scene);
        assert!(a.pos.hand.contains(c(0, 3)));
        assert!(!a.pos.hand.contains(c(3, 1)), "la carta fantasma non torna in mano");
        // La presa vecchia ancora in tavola non viene contata due volte.
        assert_eq!(a.pos.step(), Step::OppMove);
        assert_eq!(a.pos.tricks, 1);
    }

    /// Una partita interrotta a metà: quando compare un'altra briscola con tre carte nuove in mano
    /// e il tavolo vuoto, si riparte da capo senza premere "Nuova partita".
    #[test]
    fn notices_new_deal_mid_game() {
        let mut a = Assistant::default();
        let c = |s, r| cards::make(s, r);
        let hand = vec![seen(c(3, 1), 4.0), seen(c(2, 6), 4.0), seen(c(1, 0), 4.0)];
        let scene = Scene { trump: Some(seen(c(0, 8), 4.0)), hand: hand.clone(), ..Default::default() };
        show(&mut a, &scene);
        a.set_first(ME);
        show(&mut a, &scene);
        assert_eq!(a.pos.step(), Step::MyMove);
        // Stessa briscola, stessa mano: niente da rifare.
        show(&mut a, &scene);
        assert_eq!(a.pos.trump_card, Some(c(0, 8)));
        assert_eq!(a.pos.first, Some(ME));
        // Il sito distribuisce di nuovo: briscola diversa.
        let new_hand = vec![seen(c(2, 0), 4.0), seen(c(3, 4), 4.0), seen(c(1, 5), 4.0)];
        let deal = Scene { trump: Some(seen(c(1, 2), 4.0)), hand: new_hand, ..Default::default() };
        show(&mut a, &deal);
        assert_eq!(a.pos.trump_card, Some(c(1, 2)));
        assert_eq!(a.pos.step(), Step::First);
    }

    /// Se la carta è incerta si chiede conferma; se l'utente annulla una carta letta male,
    /// la memoria la dimentica e la volta dopo si chiede invece di inserirla.
    #[test]
    fn asks_when_unsure() {
        let mut a = Assistant::default();
        let scene = Scene { trump: Some(seen(cards::make(0, 7), 1.0)), ..Default::default() };
        show(&mut a, &scene);
        assert!(a.pos.trump_card.is_none());
        assert_eq!(a.vision.confirm.as_ref().map(|c| c.card), Some(cards::make(0, 7)));
        a.accept_vision();
        assert_eq!(a.pos.trump_card, Some(cards::make(0, 7)));
        assert_eq!(a.vision.memory.lock().unwrap().len(), 1, "confermata: ora la ricorda");

        let mut b = Assistant::default();
        let scene = Scene { trump: Some(seen(cards::make(2, 2), 4.0)), ..Default::default() };
        show(&mut b, &scene);
        assert_eq!(b.pos.trump_card, Some(cards::make(2, 2)));
        b.undo();
        assert!(b.pos.trump_card.is_none());
        assert_eq!(b.vision.memory.lock().unwrap().len(), 0, "annullata: dimenticata");
        show(&mut b, &scene);
        assert!(b.pos.trump_card.is_none() && b.vision.confirm.is_some(), "ora chiede conferma");
    }

    #[test]
    fn rejects_impossible_cards() {
        let mut a = Position::default();
        a.enter(cards::make(0, 0));
        a.set_first(ME);
        assert!(a.enter(cards::make(0, 0)).is_none(), "la briscola scoperta non si pesca");
        assert!(a.enter(cards::make(1, 0)).is_some());
        assert!(a.enter(cards::make(1, 0)).is_none(), "carta già in mano");
    }

    #[test]
    fn explains_advice() {
        let mut a = Position::default();
        a.enter(cards::make(0, 4));
        a.set_first(OPP);
        for c in [cards::make(1, 0), cards::make(2, 1), cards::make(3, 5)] {
            a.enter(c);
        }
        a.enter(cards::make(1, 2)); // l'avversario apre col Tre di Coppe
        let obs = a.observation();
        let mut rng = SmallRng::seed_from_u64(1);
        let evals = evaluate(&obs, &EngineConfig::strength(300), &mut rng);
        assert_eq!(evals[0].card, cards::make(1, 0), "l'Asso di Coppe prende il Tre");
        let r = explain(&obs, &evals);
        assert!(r[0].starts_with("Prendi la presa"), "{r:?}");
    }
}
