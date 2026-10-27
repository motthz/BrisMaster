//! Scopa: assistente. Tu descrivi la smazzata (tavolo, carte ricevute, giocate dell'avversario),
//! a mano, da tastiera o facendo leggere lo schermo, e il motore ti dice cosa giocare.

use crate::assistant::is_rank_prefix;
use crate::cards_ui::{self, BEST, GOLD};
use crate::vision::{memory::Memory, read_scopa, CardView, Img, Reader, Reading, ScopaScene};
use eframe::egui::{self, vec2, Color32, Rect, RichText, Stroke, Vec2};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use scopa_core::ai::{evaluate, EngineConfig, MoveEval};
use scopa_core::game::{
    self, for_each_capture, match_winner, name, short, Card, CardSet, HandScore, Move, Observation, TARGET,
};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

const HUMAN: u8 = 0;
const AI: u8 = 1;
const CARD: Vec2 = Vec2::new(78.0, 120.0);

/// Motore per l'app: i mondi si dividono su tutti i core.
fn engine_cfg(samples: u32) -> EngineConfig {
    let mut cfg = EngineConfig::strength(samples);
    cfg.threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    cfg
}

/// Impostazioni della scopa, salvate a parte da quelle della briscola.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ScopaSettings {
    assist_samples: u32,
}

impl Default for ScopaSettings {
    fn default() -> Self {
        ScopaSettings { assist_samples: 1000 }
    }
}

/// Tutte le prese possibili giocando `c` (una sola, vuota, se la carta resta in tavola).
fn options(table: CardSet, c: Card) -> Vec<Move> {
    let mut v = Vec::new();
    if !for_each_capture(table, c, |take| v.push(Move { card: c, take })) {
        v.push(Move { card: c, take: CardSet::EMPTY });
    }
    v
}

fn take_label(m: &Move, table: CardSet) -> String {
    if m.take.is_empty() {
        "la lascia in tavola".into()
    } else {
        let t: Vec<String> = m.take.iter().map(short).collect();
        let scopa = if m.take == table { " · SCOPA" } else { "" };
        format!("prende {}{scopa}", t.join(" + "))
    }
}

fn white(a: u8) -> Color32 {
    Color32::from_white_alpha(a)
}

/// Fila di carte centrata, che va a capo se non ci sta.
fn card_row(
    ui: &mut egui::Ui,
    cards: &[Card],
    size: Vec2,
    glow: impl Fn(Card) -> Option<Color32>,
    clickable: bool,
) -> Option<Card> {
    let mut clicked = None;
    let per_row = ((ui.available_width() - 20.0) / (size.x + 8.0)).max(1.0) as usize;
    for chunk in cards.chunks(per_row.max(1)) {
        ui.horizontal(|ui| {
            let w = chunk.len() as f32 * (size.x + 8.0);
            ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
            for &c in chunk {
                let look = cards_ui::Look { glow: glow(c), no_points: true, ..Default::default() };
                if cards_ui::card_look(ui, Some(c), size, look, clickable).clicked() {
                    clicked = Some(c);
                }
                ui.add_space(2.0);
            }
        });
    }
    clicked
}

/// Riquadro con il punteggio della partita e le prese della smazzata in corso.
fn score_box(ui: &mut egui::Ui, match_score: [u8; 2], captured: [CardSet; 2], scope: [u8; 2], opp: &str) {
    egui::Frame::new().fill(Color32::from_black_alpha(70)).corner_radius(10.0).inner_margin(12.0).show(ui, |ui| {
        ui.set_width(220.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("Tu  {}", match_score[0])).size(24.0).strong().color(Color32::WHITE));
            ui.label(RichText::new(format!("{opp}  {}", match_score[1])).size(18.0).color(white(200)));
            ui.label(RichText::new(format!("Partita a {TARGET}")).color(white(150)));
            ui.add_space(4.0);
            for (who, p) in [("Tu", 0), (opp, 1)] {
                let c = captured[p];
                ui.label(
                    RichText::new(format!(
                        "{who}: {} carte, {} denari{}{}",
                        c.len(),
                        (c.0 & 0x3FF).count_ones(),
                        if c.contains(game::SETTEBELLO) { ", settebello" } else { "" },
                        if scope[p] > 0 { format!(", {} scope", scope[p]) } else { String::new() }
                    ))
                    .small()
                    .color(white(190)),
                );
            }
        });
    });
}

/// Tabella dei punti di una smazzata.
fn hand_summary(ui: &mut egui::Ui, h: &HandScore, opp: &str) {
    let who = |w: Option<u8>| match w {
        Some(0) => "Tu".to_string(),
        Some(_) => opp.to_string(),
        None => "Nessuno (pari)".to_string(),
    };
    egui::Grid::new("hand_summary").striped(true).spacing([18.0, 4.0]).show(ui, |ui| {
        ui.strong("");
        ui.strong("Tu");
        ui.strong(opp);
        ui.strong("Punto a");
        ui.end_row();
        ui.label("Carte");
        ui.label(h.cards[0].to_string());
        ui.label(h.cards[1].to_string());
        ui.label(who(h.cards_to));
        ui.end_row();
        ui.label("Denari");
        ui.label(h.denari[0].to_string());
        ui.label(h.denari[1].to_string());
        ui.label(who(h.denari_to));
        ui.end_row();
        ui.label("Primiera");
        ui.label(h.primiera[0].to_string());
        ui.label(h.primiera[1].to_string());
        ui.label(who(h.primiera_to));
        ui.end_row();
        ui.label("Settebello");
        ui.label("");
        ui.label("");
        ui.label(who(h.settebello));
        ui.end_row();
        ui.label("Scope");
        ui.label(h.scope[0].to_string());
        ui.label(h.scope[1].to_string());
        ui.label("");
        ui.end_row();
        ui.strong("Punti");
        ui.strong(h.points[0].to_string());
        ui.strong(h.points[1].to_string());
        ui.label("");
        ui.end_row();
    });
}

/// Elenco delle mosse valutate dal motore.
fn eval_list(ui: &mut egui::Ui, evals: &[MoveEval], table: CardSet) {
    for (i, e) in evals.iter().enumerate().take(8) {
        let col = if i == 0 { BEST } else { white(210) };
        ui.label(
            RichText::new(format!(
                "{}. {} {} · vittoria {:.0}% · smazzata {:+.1} punti",
                i + 1,
                name(e.mv.card),
                take_label(&e.mv, table),
                e.win_prob * 100.0,
                e.exp_diff
            ))
            .color(col),
        );
    }
}

// ---------------------------------------------------------------------------
// Assistente: tavolo, mano e giocate dell'avversario si inseriscono a mano, da tastiera
// o dalla lettura dello schermo.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    Dealer,
    Table,
    Hand,
    MyMove,
    OppMove,
    Over,
}

/// Distacco minimo fra la carta più probabile e la seconda per inserirla senza chiedere.
const AUTO_MARGIN: f32 = 1.4;

/// Lettura della partita dallo schermo.
#[derive(Default)]
struct Vision {
    reader: Option<Reader<ScopaScene>>,
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
    /// Ultima scena letta: serve a ricordare le carte che l'utente scrive a mano.
    scene: Option<ScopaScene>,
    /// Candidati annullati dall'utente: se ricompaiono si chiede conferma invece di inserirli.
    doubts: Vec<(Step, Card)>,
    /// Distribuzione vista prima di sapere chi comincia (tre carte in mano, quattro in tavola).
    deal: Option<ScopaScene>,
    /// Carta dell'avversario vista di passaggio (anche in una sola lettura) prima che prendesse.
    transit: Option<Card>,
}

struct Confirm {
    step: Step,
    card: Card,
    img: Img,
    texture: Option<egui::TextureHandle>,
}

#[derive(Default)]
struct Assist {
    obs: Option<Observation>,
    /// Punteggio della partita prima della smazzata in corso.
    score: [u8; 2],
    /// Chi ha distribuito nella smazzata in corso.
    dealer: u8,
    /// Stato prima di ogni azione (per annullare) e se l'azione l'ha fatta l'app da sola perché obbligata.
    history: Vec<(Observation, bool)>,
    /// Carta giocata con più prese possibili: si aspetta quale presa.
    selected: Option<Card>,
    /// Presa vista sullo schermo senza la carta che l'ha fatta: si aspetta quale carta.
    pending_take: Option<CardSet>,
    analysis: Option<(Vec<MoveEval>, u32)>,
    pending: Option<Receiver<(Vec<MoveEval>, u32)>>,
    generation: u64,
    /// Ultima giocata, da mostrare sul tavolo.
    last: Option<String>,
    /// Tasti digitati finora per la carta in corso (es. "3" in attesa del seme).
    keys: String,
    entry_error: Option<String>,
    /// Spazio premuto mentre il motore calcolava: gioca il consiglio appena arriva.
    play_best_queued: bool,
    vision: Vision,
}

impl Assist {
    fn step(&self) -> Step {
        let Some(o) = &self.obs else {
            return Step::Dealer;
        };
        // A inizio smazzata (nessuno ha ancora preso) si indicano prima le 4 carte in tavola.
        let fresh = o.deck_len == 30 && o.captured[0].is_empty() && o.captured[1].is_empty();
        // Dopo ogni distribuzione nessuno ha ancora giocato: tocca a chi non distribuisce e
        // l'avversario ha 3 carte. Finché la mano non è completa si chiedono le carte ricevute.
        let round_start = o.opp_hand_len == 3 && o.to_move == 1 - o.dealer;
        if o.is_over() {
            Step::Over
        } else if fresh && round_start && o.hand.is_empty() && o.table.len() < 4 {
            Step::Table
        } else if round_start && o.hand.len() < 3 {
            Step::Hand
        } else if o.to_move == HUMAN {
            Step::MyMove
        } else {
            Step::OppMove
        }
    }

    fn changed(&mut self) {
        self.generation += 1;
        self.analysis = None;
        self.pending = None;
        self.selected = None;
        self.pending_take = None;
        self.keys.clear();
        self.entry_error = None;
        self.play_best_queued = false;
        self.vision.seen = None;
        self.vision.confirm = None;
        self.vision.transit = None;
    }

    /// Annulla l'ultima azione dell'utente, insieme a quelle che l'app ha fatto da sola dopo.
    fn undo(&mut self) {
        loop {
            match self.history.pop() {
                Some((o, auto)) => {
                    self.obs = Some(o);
                    self.changed();
                    if !auto {
                        break;
                    }
                }
                None => {
                    self.obs = None;
                    self.changed();
                    break;
                }
            }
        }
        // Se si è annullata una carta inserita dalla lettura dello schermo, era sbagliata.
        if let Some((c, img, len)) = self.vision.last.take() {
            if self.history.len() <= len {
                self.vision.memory.lock().unwrap().forget(&img);
                self.vision.doubts.push((self.step(), c));
            }
        }
    }

    fn start(&mut self, dealer: u8) {
        self.dealer = dealer;
        self.history.clear();
        self.obs = Some(Observation::new_hand(HUMAN, dealer, self.score));
        self.last = None;
        self.changed();
    }

    /// Nuova partita: si tiene solo la lettura dello schermo.
    fn reset(&mut self) {
        let vision = std::mem::take(&mut self.vision);
        *self = Assist::default();
        self.vision =
            Vision { reader: vision.reader, memory: vision.memory, status: vision.status, ..Default::default() };
    }

    /// Punteggio della partita contando la smazzata appena finita.
    fn total(&self) -> [u8; 2] {
        let h = self.obs.map(|o| o.score().points).unwrap_or_default();
        [self.score[0] + h[0], self.score[1] + h[1]]
    }

    /// Dopo la fine della smazzata: la prossima (distribuisce l'altro) o una partita nuova.
    fn next(&mut self) {
        if self.step() != Step::Over {
            return;
        }
        let total = self.total();
        if match_winner(total).is_some() {
            self.reset();
        } else {
            self.score = total;
            let dealer = 1 - self.dealer;
            self.start(dealer);
        }
    }

    fn apply(&mut self, m: Move, auto: bool) {
        let Some(o) = &mut self.obs else { return };
        if !o.is_legal(m) {
            return;
        }
        self.history.push((*o, auto));
        let who = if o.to_move == HUMAN { "Hai giocato" } else { "L'avversario ha giocato" };
        let table = o.table;
        let scopa = o.apply(m);
        self.last = Some(format!("{who} {}: {}{}", name(m.card), take_label(&m, table), if scopa { "!" } else { "" }));
        self.changed();
    }

    /// Inserisce da solo le giocate obbligate: la tua unica mossa possibile e l'ultima carta
    /// dell'avversario a mazzo finito (se può prendere in un solo modo).
    fn auto_fill(&mut self) {
        while let Some(o) = self.obs {
            match self.step() {
                Step::MyMove => {
                    let mut moves = Vec::new();
                    o.legal_moves(&mut moves);
                    if moves.len() != 1 {
                        break;
                    }
                    self.apply(moves[0], true);
                }
                Step::OppMove if o.deck_len == 0 && o.opp_hand_len == 1 && o.unknown().len() == 1 => {
                    let c = o.unknown().iter().next().unwrap();
                    let opts = options(o.table, c);
                    if opts.len() != 1 {
                        self.selected = Some(c);
                        break;
                    }
                    self.apply(opts[0], true);
                }
                _ => break,
            }
        }
    }

    /// Carte con cui chi è di turno può aver fatto la presa `take`.
    fn take_candidates(&self, take: CardSet) -> CardSet {
        let Some(o) = &self.obs else {
            return CardSet::EMPTY;
        };
        let holds = if o.to_move == HUMAN { o.hand } else { o.unknown() };
        holds.iter().filter(|&c| options(o.table, c).iter().any(|m| m.take == take)).collect()
    }

    /// Carte che si possono indicare adesso.
    fn allowed(&self) -> CardSet {
        let Some(o) = &self.obs else {
            return CardSet::EMPTY;
        };
        match self.step() {
            Step::MyMove | Step::OppMove if self.pending_take.is_some() => {
                self.take_candidates(self.pending_take.unwrap())
            }
            Step::Table | Step::Hand | Step::OppMove => o.unknown(),
            Step::MyMove => o.hand,
            _ => CardSet::EMPTY,
        }
    }

    /// Carta indicata nel passo corrente. Falso se non è possibile adesso.
    fn enter(&mut self, c: Card) -> bool {
        let step = self.step();
        let Some(o) = self.obs else { return false };
        if !self.allowed().contains(c) {
            return false;
        }
        match step {
            Step::Table | Step::Hand => {
                self.history.push((o, false));
                let obs = self.obs.as_mut().unwrap();
                if step == Step::Table {
                    obs.table.insert(c);
                } else {
                    obs.hand.insert(c);
                }
                self.changed();
            }
            Step::MyMove | Step::OppMove => {
                if let Some(take) = self.pending_take {
                    self.apply(Move { card: c, take }, false);
                } else {
                    let opts = options(o.table, c);
                    if opts.len() > 1 {
                        self.selected = Some(c);
                        return true;
                    }
                    self.apply(opts[0], false);
                }
            }
            _ => return false,
        }
        self.auto_fill();
        true
    }

    /// Sceglie la presa numero `i` per la carta selezionata.
    fn choose(&mut self, i: usize) {
        let (Some(c), Some(o)) = (self.selected, self.obs) else { return };
        if let Some(&m) = options(o.table, c).get(i) {
            self.apply(m, false);
            self.auto_fill();
        }
    }

    /// Per la carta selezionata, la presa migliore secondo il motore.
    fn choose_best(&mut self) {
        let Some(c) = self.selected else { return };
        let best = self.analysis.as_ref().and_then(|(ev, _)| ev.iter().find(|e| e.mv.card == c)).map(|e| e.mv);
        if let Some(m) = best {
            self.apply(m, false);
            self.auto_fill();
        }
    }

    /// Gioca la mossa consigliata, o la prenota se il motore sta ancora calcolando.
    fn play_best(&mut self) {
        match self.analysis.as_ref().and_then(|(ev, _)| ev.first()) {
            Some(e) => {
                let m = e.mv;
                self.apply(m, false);
                self.auto_fill();
            }
            None => self.play_best_queued = true,
        }
    }

    fn poll(&mut self, samples: u32, ctx: &egui::Context) {
        if let Some(rx) = &self.pending {
            if let Ok(a) = rx.try_recv() {
                self.analysis = Some(a);
                self.pending = None;
                if std::mem::take(&mut self.play_best_queued) {
                    self.play_best();
                    return;
                }
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        }
        let Some(obs) = self.obs else { return };
        let mut moves = Vec::new();
        obs.legal_moves(&mut moves);
        let wants = self.step() == Step::MyMove && moves.len() > 1;
        let stale = self.analysis.as_ref().is_some_and(|a| a.1 != samples);
        if wants && self.pending.is_none() && (self.analysis.is_none() || stale) {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let mut rng = SmallRng::from_entropy();
                let _ = tx.send((evaluate(&obs, &engine_cfg(samples), &mut rng), samples));
            });
            self.pending = Some(rx);
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    // --- Tastiera ------------------------------------------------------------------------------

    /// Tastiera sempre attiva, senza campo di testo né Invio: "3c" inserisce il tre di coppe appena
    /// digitato il seme; al tuo turno Spazio gioca il consiglio e 1, 2, 3 la carta in mano in quella
    /// posizione; quando una carta può prendere in più modi 1, 2, 3... scelgono la presa.
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
            match ev {
                egui::Event::Key { key, pressed: true, modifiers, .. } if modifiers.is_none() => self.key(key),
                egui::Event::Text(t) => {
                    for ch in t.chars().filter(|c| !c.is_whitespace()) {
                        self.type_char(ch.to_ascii_lowercase());
                    }
                }
                _ => {}
            }
        }
    }

    fn key(&mut self, key: egui::Key) {
        let step = self.step();
        match key {
            egui::Key::Space | egui::Key::Enter if self.vision.confirm.is_some() => self.accept_vision(),
            egui::Key::Space | egui::Key::Enter if self.selected.is_some() => self.choose_best(),
            egui::Key::Space | egui::Key::Enter if step == Step::MyMove => self.play_best(),
            egui::Key::Enter if step == Step::Over => self.next(),
            egui::Key::Backspace => {
                self.keys.pop();
            }
            egui::Key::Escape => {
                self.keys.clear();
                self.entry_error = None;
                self.selected = None;
                if let Some(c) = self.vision.confirm.take() {
                    self.vision.doubts.push((c.step, c.card));
                }
            }
            _ => {}
        }
    }

    fn type_char(&mut self, ch: char) {
        let digit = ch.to_digit(10).filter(|d| *d >= 1).map(|d| d as usize - 1);
        if self.selected.is_some() {
            if let Some(i) = digit {
                self.choose(i);
            }
            return;
        }
        let step = self.step();
        match step {
            Step::Dealer => match ch {
                '1' => self.start(AI),
                '2' => self.start(HUMAN),
                _ => {}
            },
            Step::MyMove if self.pending_take.is_none() => {
                let hand = self.obs.map(|o| o.hand.to_vec()).unwrap_or_default();
                if let Some(&c) = digit.and_then(|i| hand.get(i)) {
                    self.enter(c);
                }
            }
            Step::Table | Step::Hand | Step::OppMove | Step::MyMove => {
                self.keys.push(ch);
                self.entry_error = None;
                if let Some(c) = cards_ui::parse_card(&self.keys) {
                    self.keys.clear();
                    let seen = self.vision.confirm.take();
                    // La carta scritta a mano si impara dall'immagine sullo schermo.
                    let shown = self.unmatched_view(step).map(|v| v.img.clone());
                    if self.enter(c) {
                        if let Some(img) = seen.map(|v| v.img).or(shown) {
                            self.vision.memory.lock().unwrap().remember(&img, c);
                        }
                    } else {
                        self.entry_error = Some(format!("{} non è possibile adesso", name(c)));
                    }
                } else if !is_rank_prefix(&self.keys) {
                    self.entry_error = Some(format!("\"{}\" non è una carta: scrivi valore e seme, es. 3c", self.keys));
                    self.keys.clear();
                }
            }
            Step::Over => {}
        }
    }

    // --- Lettura dello schermo -----------------------------------------------------------------

    /// Accende o spegne la lettura dello schermo.
    fn set_vision(&mut self, on: bool, ctx: &egui::Context) {
        if on && self.vision.reader.is_none() {
            self.vision.memory = Memory::shared();
            let every = std::time::Duration::from_millis(250);
            self.vision.reader = Some(Reader::start(self.vision.memory.clone(), ctx.clone(), every, read_scopa));
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
                self.vision.status =
                    format!("Vedo: {} carte in mano · {} in tavola", scene.hand.len(), scene.table.len());
                self.watch_transit(&scene);
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

    /// Al turno dell'avversario ricorda la sua carta anche se si vede per un attimo solo, prima che
    /// il sito la porti via insieme alla presa.
    fn watch_transit(&mut self, scene: &ScopaScene) {
        let Some(o) = self.obs else { return };
        if self.step() != Step::OppMove {
            return;
        }
        let ghosts = o.captured[0].union(o.captured[1]);
        let (new, _) = match_views(&scene.table, o.table, o.unknown(), ghosts);
        if let [i] = new.as_slice() {
            if let Some((c, m)) = scene.table[*i].best(o.unknown()) {
                if m >= AUTO_MARGIN {
                    self.vision.transit = Some(c);
                }
            }
        }
    }

    /// Inserisce nella partita quello che si vede sullo schermo, se è uguale alla lettura precedente.
    fn sync(&mut self, scene: &ScopaScene) {
        let best = |v: &CardView| v.best(CardSet::FULL).map(|b| b.0);
        let mut sig: Vec<Option<Card>> = scene.table.iter().map(best).collect();
        sig.push(None);
        sig.extend(scene.hand.iter().map(best));
        if self.vision.seen.as_ref() != Some(&sig) {
            self.vision.seen = Some(sig);
            return;
        }
        // Più passi in una volta: le quattro carte in tavola, le tre in mano, la prima giocata...
        for _ in 0..16 {
            let before = (self.step(), self.history.len(), self.selected, self.pending_take);
            self.sync_step(scene);
            let after = (self.step(), self.history.len(), self.selected, self.pending_take);
            if after == before || self.vision.confirm.is_some() {
                break;
            }
        }
        self.vision.seen = Some(sig);
    }

    /// Un passo della partita a partire dalla lettura dello schermo.
    fn sync_step(&mut self, scene: &ScopaScene) {
        if self.vision.confirm.is_some() {
            return;
        }
        let step = self.step();
        let new_deal = scene.hand.len() == 3 && scene.table.len() == 4;
        let Some(o) = self.obs else {
            self.sync_deal(scene);
            return;
        };
        let ghosts = o.captured[0].union(o.captured[1]);
        let unknown = o.unknown();
        match step {
            Step::Dealer => {}
            Step::Over => {
                // Il sito ha distribuito la smazzata successiva.
                if new_deal {
                    if match_winner(self.total()).is_some() {
                        self.reset();
                        self.vision.deal = Some(scene.clone());
                    } else {
                        self.next();
                    }
                }
            }
            Step::Table => {
                if scene.table.len() > 4 {
                    return;
                }
                let (new, _) = match_views(&scene.table, o.table, unknown, ghosts);
                if let Some(&i) = new.first() {
                    self.offer(step, &scene.table[i], unknown);
                }
            }
            Step::Hand => {
                let (new, _) = match_views(&scene.hand, o.hand, unknown, ghosts.union(o.table));
                if let Some(&i) = new.first() {
                    self.offer(step, &scene.hand[i], unknown);
                }
            }
            Step::OppMove => {
                let (new, missing) = match_views(&scene.table, o.table, unknown, ghosts);
                if let Some(c) = self.selected {
                    // Carta già vista, con più prese possibili: si aspetta che il sito tolga la presa.
                    if new.is_empty() && !missing.is_empty() {
                        self.apply(Move { card: c, take: missing }, false);
                        self.auto_fill();
                    }
                    return;
                }
                if self.pending_take.is_some() {
                    return;
                }
                if let ([i], true) = (new.as_slice(), missing.is_empty()) {
                    // La carta giocata è sul tavolo: resta lì, o il sito la porterà via con la presa.
                    self.offer(step, &scene.table[*i], unknown);
                } else if new.is_empty() && !missing.is_empty() {
                    self.took_unseen(missing);
                }
            }
            Step::MyMove => {
                let (_, gone) = match_views(&scene.hand, o.hand, CardSet::EMPTY, ghosts.union(o.table));
                let (new, missing) = match_views(&scene.table, o.table, unknown.union(gone).union(o.hand), ghosts);
                if let Some(c) = self.selected {
                    if new.is_empty() && !missing.is_empty() {
                        self.apply(Move { card: c, take: missing }, false);
                        self.auto_fill();
                    }
                    return;
                }
                // Dalla mano manca una carta: l'hai giocata.
                if gone.len() == 1 && scene.hand.len() + 1 == o.hand.len() as usize {
                    self.enter(gone.iter().next().unwrap());
                }
            }
        }
    }

    /// Dal tavolo è sparita una presa senza che si vedesse la carta che l'ha fatta.
    fn took_unseen(&mut self, take: CardSet) {
        let cands = self.take_candidates(take);
        let seen = self.vision.transit.filter(|c| cands.contains(*c));
        match (cands.len(), seen) {
            (0, _) => {}
            (1, _) => {
                let c = cands.iter().next().unwrap();
                self.apply(Move { card: c, take }, false);
                self.auto_fill();
            }
            (_, Some(c)) => {
                self.apply(Move { card: c, take }, false);
                self.auto_fill();
            }
            _ => self.pending_take = Some(take),
        }
    }

    /// Chi comincia la prima smazzata della partita: lo si capisce dalla prima giocata dopo la
    /// distribuzione. Poi si inseriscono le carte della distribuzione e si prosegue.
    fn sync_deal(&mut self, scene: &ScopaScene) {
        if scene.hand.len() == 3 && scene.table.len() == 4 {
            self.vision.deal = Some(scene.clone());
            return;
        }
        let Some(deal) = self.vision.deal.clone() else { return };
        let first = match (scene.hand.len(), scene.table.len()) {
            (2, _) => HUMAN,
            (3, t) if t != 4 => AI,
            _ => return,
        };
        self.start(1 - first);
        for _ in 0..8 {
            let before = self.history.len();
            self.sync_step(&deal);
            if self.history.len() == before || self.vision.confirm.is_some() {
                break;
            }
        }
    }

    /// Propone la carta riconosciuta in una vista: la inserisce se è sicura, altrimenti chiede.
    fn offer(&mut self, step: Step, view: &CardView, among: CardSet) {
        let Some((card, margin)) = view.best(among) else { return };
        let doubtful = self.vision.doubts.contains(&(step, card));
        if margin >= AUTO_MARGIN && !doubtful {
            let img = view.img.clone();
            let len = self.history.len();
            if self.enter(card) {
                self.vision.memory.lock().unwrap().remember(&img, card);
                self.vision.last = Some((card, img, len));
            }
        } else {
            self.vision.confirm = Some(Confirm { step, card, img: view.img.clone(), texture: None });
        }
    }

    /// L'utente conferma la carta riconosciuta con poca sicurezza: si inserisce e si ricorda.
    fn accept_vision(&mut self) {
        let Some(c) = self.vision.confirm.take() else { return };
        if c.step == self.step() && self.enter(c.card) {
            self.vision.memory.lock().unwrap().remember(&c.img, c.card);
        }
    }

    /// La carta sullo schermo non ancora inserita per il passo corrente, se ce n'è una sola.
    fn unmatched_view(&self, step: Step) -> Option<&CardView> {
        let scene = self.vision.scene.as_ref()?;
        let o = self.obs?;
        let ghosts = o.captured[0].union(o.captured[1]);
        let (views, known, ghosts) = match step {
            Step::Table | Step::OppMove => (&scene.table, o.table, ghosts),
            Step::Hand => (&scene.hand, o.hand, ghosts.union(o.table)),
            _ => return None,
        };
        match match_views(views, known, o.unknown(), ghosts).0.as_slice() {
            [i] => Some(&views[*i]),
            _ => None,
        }
    }
}

/// Abbina le carte viste sullo schermo a quelle già note (`known`). Una vista corrisponde a una
/// carta nota se questa è fra le più probabili per quella vista. Restituisce le viste nuove (che
/// sembrano una carta di `plausible`; quelle che sembrano una carta di `ghosts` sono carte fantasma
/// che il sito disegna ancora) e le carte note che non si vedono più.
fn match_views(views: &[CardView], known: CardSet, plausible: CardSet, ghosts: CardSet) -> (Vec<usize>, CardSet) {
    let mut pairs = Vec::new();
    for (i, v) in views.iter().enumerate() {
        let top = v.best(known.union(plausible)).map_or(f32::MIN, |b| v.scores[b.0 as usize]);
        for k in known.iter() {
            let s = v.scores[k as usize];
            if s >= top - AUTO_MARGIN {
                pairs.push((s, i, k));
            }
        }
    }
    pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let mut used = vec![false; views.len()];
    let mut missing = known;
    for (_, i, k) in pairs {
        if !used[i] && missing.contains(k) {
            used[i] = true;
            missing.remove(k);
        }
    }
    let new = (0..views.len())
        .filter(|&i| !used[i])
        .filter(|&i| views[i].best(plausible.union(ghosts)).is_some_and(|(c, _)| plausible.contains(c)))
        .collect();
    (new, missing)
}

// ---------------------------------------------------------------------------

pub struct ScopaUi {
    pub settings: ScopaSettings,
    assist: Assist,
}

impl ScopaUi {
    pub fn new(settings: ScopaSettings) -> Self {
        ScopaUi { settings, assist: Assist::default() }
    }

    /// Area centrale: tavolo verde a sinistra, richieste e consigli a destra.
    pub fn central(&mut self, ui: &mut egui::Ui) {
        let full = ui.available_rect_before_wrap();
        let side_w = (full.width() * 0.4).clamp(440.0, 560.0);
        let split = full.max.x - side_w;
        let left = Rect::from_min_max(full.min, egui::pos2(split, full.max.y));
        let right = Rect::from_min_max(egui::pos2(split + 14.0, full.min.y + 10.0), full.max - vec2(10.0, 0.0));
        cards_ui::paint_felt(ui.painter(), left);
        ui.painter().vline(split, full.y_range(), Stroke::new(1.0, Color32::from_gray(60)));
        let samples = &mut self.settings.assist_samples;
        let a = &mut self.assist;
        ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| assist_side(a, ui, samples, false));
        ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| assist_table(a, ui));
    }

    /// Finestra compatta: solo l'inserimento e il consiglio, con un riepilogo del tavolo in cima.
    pub fn compact(&mut self, ui: &mut egui::Ui) {
        assist_side(&mut self.assist, ui, &mut self.settings.assist_samples, true);
    }
}

fn title(ui: &mut egui::Ui, t: &str, sub: &str) {
    ui.label(RichText::new(t).size(22.0).strong().color(GOLD));
    ui.label(RichText::new(sub).color(white(190)));
    ui.add_space(8.0);
}

fn assist_side(a: &mut Assist, ui: &mut egui::Ui, samples: &mut u32, compact: bool) {
    let ctx = ui.ctx().clone();
    a.keyboard(&ctx);
    a.poll_vision();
    a.poll(*samples, &ctx);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if compact {
            compact_header(a, ui);
            ui.separator();
        }
        vision_ui(a, ui);
        let step = a.step();
        match step {
            Step::Dealer => {
                title(ui, "Nuova partita di scopa", "Chi gioca per primo? (tasti 1 e 2)");
                ui.horizontal(|ui| {
                    ui.label("Tuoi punti");
                    ui.add(egui::DragValue::new(&mut a.score[0]).range(0..=30));
                    ui.label("Avversario");
                    ui.add(egui::DragValue::new(&mut a.score[1]).range(0..=30));
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let w = if compact { 150.0 } else { 200.0 };
                    let big = |t: &str| egui::Button::new(RichText::new(t).size(17.0)).min_size(vec2(w, 44.0));
                    if ui.add(big("1 · Comincio io")).clicked() {
                        a.start(AI);
                    }
                    if ui.add(big("2 · L'avversario")).clicked() {
                        a.start(HUMAN);
                    }
                });
                if a.vision.reader.is_some() {
                    ui.label(
                        RichText::new("Con la lettura dello schermo lo capisco da solo alla prima carta giocata.")
                            .small()
                            .weak(),
                    );
                }
            }
            Step::Table => {
                let n = a.obs.map_or(0, |o| o.table.len());
                title(ui, "Carte in tavola", &format!("Indica le 4 carte scoperte: ne mancano {}.", 4 - n));
                picker(a, ui);
            }
            Step::Hand => {
                let n = a.obs.map_or(0, |o| o.hand.len());
                title(ui, "Le tue carte", &format!("Indica le carte che hai ricevuto: ne mancano {}.", 3 - n));
                picker(a, ui);
            }
            Step::OppMove => {
                title(
                    ui,
                    "Che carta ha giocato l'avversario?",
                    "Se poteva prendere in più modi, scegli cosa ha preso.",
                );
                capture_choice(a, ui);
                picker(a, ui);
            }
            Step::MyMove => {
                capture_choice(a, ui);
                advice(a, ui, compact);
                if !compact {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Mondi simulati");
                        ui.add(egui::Slider::new(samples, 100..=5000).logarithmic(true));
                    });
                }
                if a.pending_take.is_some() {
                    picker(a, ui);
                }
            }
            Step::Over => {
                let h = a.obs.unwrap().score();
                let total = a.total();
                title(ui, "Fine smazzata", &format!("Partita: tu {} · avversario {}", total[0], total[1]));
                hand_summary(ui, &h, "Avversario");
                ui.add_space(8.0);
                if let Some(w) = match_winner(total) {
                    let t = if w == HUMAN { "Hai vinto la partita!" } else { "L'avversario ha vinto la partita" };
                    ui.label(RichText::new(t).size(22.0).strong());
                }
                let label =
                    if match_winner(total).is_some() { "Nuova partita (Invio)" } else { "Prossima smazzata (Invio)" };
                if ui.button(RichText::new(label).size(17.0)).clicked() {
                    a.next();
                }
            }
        }
        if compact {
            return;
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let undo =
                ui.add_enabled(a.obs.is_some(), egui::Button::new("Annulla ultima azione")).on_hover_text("Ctrl+Z");
            if undo.clicked() {
                a.undo();
            }
            if ui.button("Ricomincia").clicked() {
                a.reset();
            }
        });
    });
}

/// Riepilogo della finestra compatta: punti, tavolo e la tua mano (numerata per i tasti 1 2 3 e
/// cliccabile quando tocca a te).
fn compact_header(a: &mut Assist, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let line = match a.obs {
            Some(o) => format!(
                "Mazzo {} · partita {}-{} · scope {}-{}",
                o.deck_len, a.score[0], a.score[1], o.scope[0], o.scope[1]
            ),
            None => format!("Partita {}-{}", a.score[0], a.score[1]),
        };
        ui.label(RichText::new(line).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Nuova partita").clicked() {
                a.reset();
            }
            if ui.add_enabled(a.obs.is_some(), egui::Button::new("Annulla").small()).on_hover_text("Ctrl+Z").clicked() {
                a.undo();
            }
        });
    });
    let Some(o) = a.obs else { return };
    let mini = vec2(40.0, 62.0);
    let best = a.analysis.as_ref().map(|(e, _)| e[0].mv);
    ui.label(RichText::new("In tavola").small().color(GOLD));
    let table = o.table.to_vec();
    card_row(ui, &table, mini, |c| best.is_some_and(|b| b.take.contains(c)).then_some(GOLD), false);
    let my_turn = a.step() == Step::MyMove && a.selected.is_none() && a.pending_take.is_none();
    ui.label(RichText::new("La tua mano").small());
    let hand = o.hand.to_vec();
    let mut clicked = None;
    ui.horizontal(|ui| {
        for (i, &c) in hand.iter().enumerate() {
            ui.vertical(|ui| {
                let glow = (my_turn && best.is_some_and(|b| b.card == c)).then_some(BEST);
                let look = cards_ui::Look { glow, no_points: true, ..Default::default() };
                if cards_ui::card_look(ui, Some(c), mini, look, my_turn).clicked() {
                    clicked = Some(c);
                }
                if my_turn {
                    ui.label(RichText::new(format!("{}", i + 1)).strong().color(GOLD));
                }
            });
        }
    });
    if let Some(t) = &a.last {
        ui.label(RichText::new(t).weak());
    }
    if let Some(c) = clicked {
        a.enter(c);
    }
}

/// Interruttore, stato e richiesta di conferma della lettura dello schermo.
fn vision_ui(a: &mut Assist, ui: &mut egui::Ui) {
    let mut on = a.vision.reader.is_some();
    ui.horizontal(|ui| {
        if ui
            .checkbox(&mut on, RichText::new("Leggi la partita dallo schermo").strong())
            .on_hover_text("BrisMaster guarda il tavolo del sito e inserisce da solo carte in tavola, mano, giocate e prese. Non tocca il sito: le carte le giochi sempre tu.")
            .changed()
        {
            a.set_vision(on, ui.ctx());
        }
        if on {
            let n = a.vision.memory.lock().unwrap().len();
            ui.label(RichText::new(format!("{n}/40 carte in memoria")).small().weak())
                .on_hover_text("Le carte già riconosciute si ricordano da sole e la volta dopo si riconoscono al primo colpo");
        }
    });
    if !on {
        return;
    }
    ui.label(RichText::new(&a.vision.status).small().weak());
    let ctx = ui.ctx().clone();
    if let Some(c) = &mut a.vision.confirm {
        let tex = c.texture.get_or_insert_with(|| {
            let rgba: Vec<u8> = c.img.px.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
            let image = egui::ColorImage::from_rgba_unmultiplied([c.img.w, c.img.h], &rgba);
            ctx.load_texture("scopa_vision_confirm", image, Default::default())
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
                        ui.label(RichText::new(format!("Mi sembra {}", name(c.card))).size(17.0).strong());
                        ui.label("Spazio = giusto · se sbaglio scrivi la carta (es. Rd) · Esc = ignora");
                    });
                });
            });
    }
    ui.add_space(6.0);
}

fn picker(a: &mut Assist, ui: &mut egui::Ui) {
    if a.selected.is_some() {
        return;
    }
    if let Some(take) = a.pending_take {
        let t: Vec<String> = take.iter().map(short).collect();
        ui.label(RichText::new(format!("Ha preso {}: con quale carta?", t.join(" + "))).strong().color(GOLD));
    }
    if let Some(c) = cards_ui::picker(ui, "scopa_picker", a.allowed()) {
        a.enter(c);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Da tastiera:").weak());
        let shown = if a.keys.is_empty() { "_".to_string() } else { format!("{}_", a.keys.to_uppercase()) };
        ui.label(RichText::new(shown).monospace().size(18.0).color(GOLD));
    });
    if let Some(e) = &a.entry_error {
        ui.label(RichText::new(e).color(Color32::LIGHT_RED));
    }
    ui.label(
        RichText::new("Scrivi valore e seme, senza Invio: A 2 3 4 5 6 7 F C R · d c s b (es. 3c). Ctrl+Z annulla")
            .small()
            .weak(),
    );
}

/// Prese possibili per la carta selezionata, numerate per i tasti.
fn capture_choice(a: &mut Assist, ui: &mut egui::Ui) {
    let (Some(sel), Some(o)) = (a.selected, a.obs) else { return };
    ui.label(RichText::new(format!("Cosa prende {}? (tasti 1 2 3..., Esc annulla)", name(sel))).strong().color(GOLD));
    let best = a.analysis.as_ref().and_then(|(ev, _)| ev.iter().find(|e| e.mv.card == sel)).map(|e| e.mv);
    let mut chosen = None;
    for (i, opt) in options(o.table, sel).into_iter().enumerate() {
        let mut text = format!("{} · {}", i + 1, take_label(&opt, o.table));
        if best == Some(opt) {
            text += " · consigliata (Spazio)";
        }
        if ui.button(RichText::new(text).size(16.0)).clicked() {
            chosen = Some(i);
        }
    }
    if ui.button("Annulla").clicked() {
        a.selected = None;
    }
    if let Some(i) = chosen {
        a.choose(i);
    }
    ui.add_space(8.0);
}

/// Consiglio del motore: la mossa migliore in grande, poi le alternative.
fn advice(a: &mut Assist, ui: &mut egui::Ui, compact: bool) {
    let Some(o) = a.obs else { return };
    let mut moves = Vec::new();
    o.legal_moves(&mut moves);
    let Some((ev, _)) = &a.analysis else {
        if moves.len() > 1 {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Il motore sta calcolando...");
            });
        }
        return;
    };
    let b = ev[0];
    egui::Frame::new()
        .fill(Color32::from_rgb(24, 52, 36))
        .stroke(Stroke::new(1.5, BEST))
        .corner_radius(10.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let look = cards_ui::Look { no_points: true, ..Default::default() };
                cards_ui::card_look(ui, Some(b.mv.card), vec2(56.0, 86.0), look, false);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Gioca").color(white(180)));
                    ui.label(RichText::new(name(b.mv.card)).size(22.0).strong().color(Color32::WHITE));
                    ui.label(RichText::new(take_label(&b.mv, o.table)).size(16.0).color(Color32::WHITE));
                    ui.label(RichText::new(format!("Vittoria {:.0}%", b.win_prob * 100.0)).color(BEST));
                });
            });
        });
    ui.add_space(4.0);
    ui.label(
        RichText::new(if a.play_best_queued {
            "Spazio premuto: inserisco il consiglio appena il motore ha finito."
        } else {
            "Spazio = hai giocato il consiglio · 1 2 3 = la carta in quella posizione della mano"
        })
        .color(GOLD),
    );
    if !compact {
        ui.add_space(6.0);
        eval_list(ui, ev, o.table);
    } else {
        eval_list(ui, &ev[..ev.len().min(3)], o.table);
    }
}

fn assist_table(a: &mut Assist, ui: &mut egui::Ui) {
    let Some(o) = a.obs else {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("Assistente di scopa").size(26.0).color(GOLD)));
        return;
    };
    ui.add_space(8.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(format!("Avversario · {} carte in mano", o.opp_hand_len)).size(16.0).color(white(210)));
    });
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        ui.label(RichText::new(format!("Mazzo: {} carte", o.deck_len)).color(white(190)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.add_space(20.0);
            score_box(ui, a.score, o.captured, o.scope, "Avversario");
        });
    });
    ui.vertical_centered(|ui| ui.label(RichText::new("Tavolo").color(GOLD).strong()));
    let best = a.analysis.as_ref().map(|(e, _)| e[0].mv);
    let table = o.table.to_vec();
    ui.allocate_ui(vec2(ui.available_width(), CARD.y * 2.0 + 20.0), |ui| {
        ui.vertical(|ui| {
            card_row(ui, &table, CARD, |c| best.is_some_and(|b| b.take.contains(c)).then_some(GOLD), false);
        });
    });
    ui.vertical_centered(|ui| {
        if let Some(t) = &a.last {
            ui.label(RichText::new(t).size(16.0).color(white(220)));
        }
    });
    ui.add_space(10.0);
    ui.vertical_centered(|ui| ui.label(RichText::new("La tua mano").size(16.0).strong().color(Color32::WHITE)));
    let my_turn = a.step() == Step::MyMove && a.selected.is_none() && a.pending_take.is_none();
    let hand = o.hand.to_vec();
    if let Some(c) =
        card_row(ui, &hand, CARD, |c| (my_turn && best.is_some_and(|b| b.card == c)).then_some(BEST), my_turn)
    {
        a.enter(c);
    }
    if my_turn {
        ui.vertical_centered(|ui| {
            let keys: Vec<String> =
                hand.iter().enumerate().map(|(i, &c)| format!("{} = {}", i + 1, short(c))).collect();
            ui.label(RichText::new(format!("Tasti: {} · Spazio = consiglio", keys.join(" · "))).color(GOLD));
        });
    }
    if let Some((ev, _)) = &a.analysis {
        let b = ev[0];
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!("Consiglio: {} {}", name(b.mv.card), take_label(&b.mv, o.table)))
                    .size(18.0)
                    .strong()
                    .color(BEST),
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;
    use scopa_core::game::{make, Game};

    fn same(a: &Observation, b: &Observation) -> bool {
        a.hand == b.hand
            && a.table == b.table
            && a.captured == b.captured
            && a.scope == b.scope
            && a.opp_hand_len == b.opp_hand_len
            && a.deck_len == b.deck_len
            && a.to_move == b.to_move
    }

    /// Riproduce smazzate casuali inserendo solo ciò che vede il giocatore 0 e controlla che lo
    /// stato dell'assistente coincida con quello vero; le giocate obbligate entrano da sole.
    #[test]
    fn tracks_real_games() {
        let mut rng = SmallRng::seed_from_u64(3);
        let mut autos = 0;
        for n in 0..200 {
            let dealer = (n % 2) as u8;
            let mut g = Game::new(&mut rng, dealer);
            let mut a = Assist::default();
            a.start(dealer);
            for c in g.table.iter().chain(g.hands[0].iter()) {
                assert!(a.enter(c));
            }
            while !g.is_over() {
                let mut moves = Vec::new();
                g.legal_moves(&mut moves);
                let m = moves[rng.gen_range(0..moves.len())];
                let table = g.table;
                let mine = g.hands[0];
                g.play(m);
                assert_eq!(a.step(), if mine.contains(m.card) { Step::MyMove } else { Step::OppMove });
                assert!(a.enter(m.card), "carta rifiutata");
                if a.selected == Some(m.card) {
                    let i = options(table, m.card).iter().position(|o| o.take == m.take).unwrap();
                    a.choose(i);
                }
                loop {
                    // Nuova distribuzione: le tre carte ricevute.
                    for c in g.hands[0].iter() {
                        // (Se l'assistente è avanti, in mano c'è ancora la carta che ha già giocato da solo.)
                        if a.step() == Step::Hand && a.allowed().contains(c) {
                            assert!(a.enter(c));
                        }
                    }
                    let real = Observation::of(&g, 0, [0, 0]);
                    if same(&a.obs.unwrap(), &real) {
                        break;
                    }
                    g.legal_moves(&mut moves);
                    if let (Some(c), 1) = (a.selected, g.to_move) {
                        // Ultima carta dell'avversario, nota ma con più prese: si sceglie la presa.
                        let m = moves[rng.gen_range(0..moves.len())];
                        assert_eq!(m.card, c);
                        let table = g.table;
                        g.play(m);
                        a.choose(options(table, c).iter().position(|o| o.take == m.take).unwrap());
                        continue;
                    }
                    // L'assistente è avanti: ha inserito da solo una giocata obbligata.
                    assert_eq!(moves.len(), 1, "smazzata {n}: {:?}\n{real:?}", a.obs.unwrap());
                    g.play(moves[0]);
                    autos += 1;
                }
            }
            assert_eq!(a.step(), Step::Over);
            let h = g.score().points;
            a.next();
            assert_eq!(a.score, h);
            assert_eq!(a.step(), Step::Table);
            assert_eq!(a.dealer, 1 - dealer, "la smazzata dopo distribuisce l'altro");
        }
        assert!(autos > 100, "giocate obbligate inserite da sole: {autos}");
    }

    /// Inizio con il tavolo A 2 3 4 di denari e in mano 5 di coppe, 7 di spade, re di bastoni.
    fn started() -> Assist {
        let mut a = Assist::default();
        a.type_char('1');
        assert_eq!(a.step(), Step::Table, "1 = comincio io");
        assert_eq!(a.dealer, AI);
        for ch in "1d2d3d4d5c7srb".chars() {
            a.type_char(ch);
        }
        a
    }

    /// Tastiera: le carte entrano appena si scrive il seme; al tuo turno 1 2 3 scelgono la carta
    /// della mano e, se può prendere in più modi, 1 2... la presa.
    #[test]
    fn keyboard_flow() {
        let mut a = started();
        assert_eq!(a.step(), Step::MyMove);
        // Il 5 di coppe (prima carta della mano) prende A+4 oppure 2+3.
        a.type_char('1');
        assert_eq!(a.selected, Some(make(1, 4)));
        a.key(egui::Key::Escape);
        assert_eq!(a.selected, None);
        a.type_char('1');
        a.type_char('2');
        let o = a.obs.unwrap();
        assert_eq!(o.table, [make(0, 0), make(0, 3)].into_iter().collect());
        assert_eq!(o.captured[0].len(), 3);
        // L'avversario gioca il 7 di denari: non prende nulla.
        a.type_char('7');
        a.type_char('x');
        assert!(a.entry_error.is_some() && a.keys.is_empty());
        a.type_char('7');
        a.type_char('d');
        assert!(a.obs.unwrap().table.contains(game::SETTEBELLO));
        assert_eq!(a.step(), Step::MyMove);
        // Ctrl+Z torna indietro di un'azione.
        a.undo();
        assert_eq!(a.step(), Step::OppMove);
        a.type_char('7');
        a.type_char('d');
        // Spazio col motore ancora al lavoro: la mossa consigliata entra appena arriva.
        a.key(egui::Key::Space);
        assert!(a.play_best_queued);
        let best = Move { card: make(2, 6), take: single(game::SETTEBELLO) };
        let (tx, rx) = channel();
        tx.send((vec![MoveEval { mv: best, win_prob: 0.6, exp_diff: 1.0 }], 7)).unwrap();
        a.pending = Some(rx);
        a.poll(7, &egui::Context::default());
        assert!(a.obs.unwrap().captured[0].contains(game::SETTEBELLO));
        assert_eq!(a.step(), Step::OppMove);
    }

    fn single(c: Card) -> CardSet {
        game::single(c)
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
        CardView { img, partial: false, scores }
    }

    fn scene(table: &[Card], hand: &[Card]) -> ScopaScene {
        ScopaScene {
            table: table.iter().map(|&c| seen(c, 4.0)).collect(),
            hand: hand.iter().map(|&c| seen(c, 4.0)).collect(),
        }
    }

    /// Ogni lettura arriva due volte, come dallo schermo (la prima si scarta per prudenza).
    fn show(a: &mut Assist, s: &ScopaScene) {
        a.watch_transit(s);
        a.sync(s);
        a.watch_transit(s);
        a.sync(s);
    }

    const AD: Card = 0;
    const C2: Card = 11;
    const S3: Card = 22;
    const B4: Card = 33;
    const FD: Card = 7;
    const CC: Card = 18;
    const RS: Card = 29;

    /// La smazzata si segue da sola: chi comincia si capisce dalla prima giocata, le carte in
    /// tavola e in mano entrano da sole, le prese si leggono dal tavolo e le carte fantasma
    /// (già prese ma ancora disegnate) si ignorano.
    #[test]
    fn follows_the_screen() {
        let mut a = Assist::default();
        show(&mut a, &scene(&[AD, C2, S3, B4], &[FD, CC, RS]));
        assert_eq!(a.step(), Step::Dealer, "chi comincia lo dice la prima giocata");
        // Gioco il fante di denari: sparisce dalla mano e compare in tavola. Prende A+3+4.
        show(&mut a, &scene(&[AD, C2, S3, B4, FD], &[CC, RS]));
        assert_eq!(a.dealer, AI, "ho cominciato io");
        let o = a.obs.unwrap();
        assert_eq!(o.captured[0], [AD, S3, B4, FD].into_iter().collect());
        assert_eq!(o.table, single(C2));
        assert_eq!(a.step(), Step::OppMove);
        // Il sito disegna ancora il fante preso (carta fantasma): non è una giocata dell'avversario.
        show(&mut a, &scene(&[C2, FD], &[CC, RS]));
        assert_eq!(a.step(), Step::OppMove);
        assert_eq!(a.history.len(), 8);
        // L'avversario gioca il 2 di spade e prende il 2 di coppe: scopa.
        show(&mut a, &scene(&[C2, make(2, 1)], &[CC, RS]));
        let o = a.obs.unwrap();
        assert_eq!(o.scope, [0, 1]);
        assert!(o.table.is_empty());
        assert_eq!(a.step(), Step::MyMove);
    }

    /// Tavolo A 2 3 4 e mano fante, cavallo, re; comincia l'avversario.
    fn opp_starts() -> Assist {
        let mut a = Assist::default();
        a.start(HUMAN);
        for c in [AD, C2, S3, B4, FD, CC, RS] {
            assert!(a.enter(c));
        }
        assert_eq!(a.step(), Step::OppMove);
        a
    }

    /// Il 5 dell'avversario può prendere A+4 o 2+3: si aspetta di vedere cosa sparisce dal tavolo.
    #[test]
    fn reads_the_capture() {
        let mut a = opp_starts();
        let c5 = make(1, 4);
        show(&mut a, &scene(&[AD, C2, S3, B4, c5], &[FD, CC, RS]));
        assert_eq!(a.selected, Some(c5));
        show(&mut a, &scene(&[AD, B4], &[FD, CC, RS]));
        let o = a.obs.unwrap();
        assert_eq!(o.captured[1], [C2, S3, c5].into_iter().collect());
        assert_eq!(a.step(), Step::MyMove);
    }

    /// Presa fatta senza vedere la carta: se l'avversario poteva farla con più carte si chiede quale
    /// (basta scriverla); se la carta si era vista anche per un attimo si usa quella.
    #[test]
    fn capture_without_the_card() {
        let mut a = opp_starts();
        show(&mut a, &scene(&[AD, B4], &[FD, CC, RS]));
        assert_eq!(a.pending_take, Some([C2, S3].into_iter().collect()));
        assert_eq!(a.allowed().len(), 4, "uno dei quattro 5");
        a.type_char('5');
        a.type_char('s');
        assert!(a.obs.unwrap().captured[1].contains(make(2, 4)));
        assert_eq!(a.step(), Step::MyMove);

        let mut b = opp_starts();
        let b5 = make(3, 4);
        b.watch_transit(&scene(&[AD, C2, S3, B4, b5], &[FD, CC, RS]));
        show(&mut b, &scene(&[AD, B4], &[FD, CC, RS]));
        assert!(b.obs.unwrap().captured[1].contains(b5));
    }

    /// Se la carta è incerta si chiede conferma; se l'utente annulla una carta letta male,
    /// la memoria la dimentica e la volta dopo si chiede invece di inserirla.
    #[test]
    fn asks_when_unsure() {
        let mut a = Assist::default();
        a.start(AI);
        let s = ScopaScene { table: vec![seen(AD, 1.0)], hand: Vec::new() };
        show(&mut a, &s);
        assert!(a.obs.unwrap().table.is_empty());
        assert_eq!(a.vision.confirm.as_ref().map(|c| c.card), Some(AD));
        a.accept_vision();
        assert_eq!(a.obs.unwrap().table, single(AD));
        assert_eq!(a.vision.memory.lock().unwrap().len(), 1, "confermata: ora la ricorda");

        let mut b = Assist::default();
        b.start(AI);
        let s = scene(&[C2], &[]);
        show(&mut b, &s);
        assert_eq!(b.obs.unwrap().table, single(C2));
        b.undo();
        assert!(b.obs.unwrap().table.is_empty());
        assert_eq!(b.vision.memory.lock().unwrap().len(), 0, "annullata: dimenticata");
        show(&mut b, &s);
        assert!(b.obs.unwrap().table.is_empty() && b.vision.confirm.is_some(), "ora chiede conferma");
    }
}
