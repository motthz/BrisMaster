//! Assistente per il burraco: descrivi la smazzata (carte pescate, calate, scarti dell'avversario)
//! e il motore ti dice cosa fare: pescare o raccogliere gli scarti, cosa calare, cosa scartare.
//!
//! Lo stato è una `Game` vera: le carte che non conosci (tallone, pozzetti, mano dell'avversario)
//! sono segnaposto presi tra quelle non ancora viste. Quando una carta diventa nota (la peschi, l'avversario
//! la cala o la scarta) la si scambia con il segnaposto al suo posto. Il motore ridistribuisce comunque a caso
//! le carte nascoste a ogni simulazione, quindi i segnaposto non influenzano i consigli.

use crate::cards_ui::{paint_felt, BEST, GOLD};
use burraco::ai::{turn_plan, wants_pile, Op, Style};
use burraco::cards::{self, Cards, Kind, JOLLY};
use burraco::engine::{self, Action2, ChoiceEval, EngineConfig};
use burraco::{Action, Game, Meld, Phase};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Vec2,
};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use std::sync::mpsc::{channel, Receiver};

const ME: u8 = 0;
const OPP: u8 = 1;
const CARD: Vec2 = Vec2::new(50.0, 72.0);
const SMALL: Vec2 = Vec2::new(34.0, 48.0);
const PAPER: Color32 = Color32::from_rgb(251, 249, 243);
const RED: Color32 = Color32::from_rgb(200, 40, 45);
const BLACK: Color32 = Color32::from_rgb(30, 30, 35);

/// Cosa serve sapere dall'utente in questo momento.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Step {
    /// Prima della smazzata: le tue 11 carte, la carta scoperta, chi inizia.
    Setup,
    MyDraw,
    /// Hai pescato dal tallone: quale carta?
    MyDrawCard,
    MyPlay,
    /// Hai preso il pozzetto: quali carte?
    MyPozzetto,
    OppDraw,
    OppPlay,
    Over,
}

#[derive(Clone)]
struct Snapshot {
    g: Game,
    step: Step,
    log: Vec<String>,
}

/// Consiglio calcolato in background.
enum Advice {
    Draw(Vec<ChoiceEval>),
    Play { ops: Vec<Op>, evals: Vec<ChoiceEval>, pozzetto: bool, closes: bool },
}

pub struct BurracoAssistant {
    g: Game,
    step: Step,
    log: Vec<String>,
    history: Vec<Snapshot>,
    // Preparazione.
    setup_hand: Vec<Kind>,
    setup_top: Option<Kind>,
    first: u8,
    /// Carte scelte (dalla tua mano o per l'avversario) per la prossima azione.
    selected: Vec<Kind>,
    entry: String,
    error: Option<String>,
    advice: Option<(u64, u32, Advice)>,
    pending: Option<(u64, Receiver<Advice>)>,
    generation: u64,
}

impl Default for BurracoAssistant {
    fn default() -> Self {
        let mut rng = SmallRng::from_entropy();
        BurracoAssistant {
            g: Game::new(&mut rng, ME),
            step: Step::Setup,
            log: Vec::new(),
            history: Vec::new(),
            setup_hand: Vec::new(),
            setup_top: None,
            first: ME,
            selected: Vec::new(),
            entry: String::new(),
            error: None,
            advice: None,
            pending: None,
            generation: 0,
        }
    }
}

/// Le carte di una lista, come multinsieme.
fn multiset(ks: &[Kind]) -> Cards {
    Cards::from_slice(ks)
}

fn names(ks: &[Kind]) -> String {
    ks.iter().map(|&k| cards::name(k)).collect::<Vec<_>>().join(", ")
}

fn sorted(mut ks: Vec<Kind>) -> Vec<Kind> {
    ks.sort_by_key(|&k| cards::display_order(k));
    ks
}

/// Dove può stare una carta che il giocatore non vede.
#[derive(Clone, Copy)]
enum Spot {
    Stock(usize),
    Pozzetto(usize, usize),
    OppHand,
}

/// Cerca una copia nascosta della carta (escludendo il posto `skip`).
fn find_hidden(g: &Game, k: Kind, skip_opp: bool, skip_stock_top: bool) -> Option<Spot> {
    let top = g.stock.len().wrapping_sub(1);
    if let Some(i) = g.stock.iter().enumerate().position(|(i, &x)| x == k && !(skip_stock_top && i == top)) {
        return Some(Spot::Stock(i));
    }
    for p in 0..2 {
        if !g.taken[p] {
            if let Some(i) = g.pozzetti[p].iter().position(|&x| x == k) {
                return Some(Spot::Pozzetto(p, i));
            }
        }
    }
    let o = OPP as usize;
    if !skip_opp && g.hands[o].count(k) > g.known[o].count(k) {
        return Some(Spot::OppHand);
    }
    None
}

/// Mette `k` al posto nascosto `spot` e restituisce la carta che c'era.
fn put(g: &mut Game, spot: Spot, k: Kind, old: Kind) {
    match spot {
        Spot::Stock(i) => g.stock[i] = k,
        Spot::Pozzetto(p, i) => g.pozzetti[p][i] = k,
        Spot::OppHand => {
            g.hands[OPP as usize].remove(old);
            g.hands[OPP as usize].add(k);
        }
    }
}

/// Porta la carta in cima al tallone (la carta che hai appena pescato).
fn bring_to_stock_top(g: &mut Game, k: Kind) -> bool {
    let Some(&top) = g.stock.last() else { return false };
    if top == k {
        return true;
    }
    let Some(spot) = find_hidden(g, k, false, true) else { return false };
    put(g, spot, top, k);
    let n = g.stock.len();
    g.stock[n - 1] = k;
    true
}

/// Porta le carte nella mano dell'avversario (le ha calate o scartate).
fn bring_to_opp(g: &mut Game, ks: &[Kind]) -> bool {
    let o = OPP as usize;
    let mut trial = g.clone();
    let need = multiset(ks);
    for k in need.kinds() {
        while trial.hands[o].count(k) < need.count(k) {
            // Un segnaposto della mano avversaria che non è una carta nota né una di quelle da portare.
            let Some(old) = trial.hands[o]
                .kinds()
                .find(|&x| x != k && trial.hands[o].count(x) > trial.known[o].count(x).max(need.count(x)))
            else {
                return false;
            };
            let Some(spot) = find_hidden(&trial, k, true, false) else { return false };
            put(&mut trial, spot, old, k);
            trial.hands[o].remove(old);
            trial.hands[o].add(k);
        }
    }
    *g = trial;
    true
}

/// Sostituisce i segnaposto della tua mano con le carte vere (il pozzetto appena preso).
fn set_my_hand(g: &mut Game, real: &[Kind]) -> bool {
    let m = ME as usize;
    let mut trial = g.clone();
    let want = multiset(real);
    let mut extra = trial.hands[m].minus(&want).to_vec();
    let needed = want.minus(&trial.hands[m]);
    for k in needed.to_vec() {
        let Some(old) = extra.pop() else { return false };
        let Some(spot) = find_hidden(&trial, k, false, false) else { return false };
        put(&mut trial, spot, old, k);
    }
    trial.hands[m] = want;
    *g = trial;
    true
}

/// Partita nuova con la tua mano e la carta scoperta: il resto sono segnaposto.
fn new_game(hand: &[Kind], top: Kind, first: u8) -> Game {
    let mut rest = Cards::full_deck();
    for &k in hand {
        rest.remove(k);
    }
    rest.remove(top);
    let mut rest = rest.to_vec();
    rest.shuffle(&mut SmallRng::from_entropy());
    let mut deck = Vec::with_capacity(108);
    let mut it = rest.into_iter();
    for &k in hand {
        deck.push(k);
        deck.push(it.next().unwrap());
    }
    deck.extend((&mut it).take(22));
    deck.push(top);
    deck.extend(it);
    Game::from_deck(&deck, first)
}

impl BurracoAssistant {
    fn changed(&mut self) {
        self.generation += 1;
        self.advice = None;
        self.pending = None;
        self.selected.clear();
        self.entry.clear();
    }

    fn save(&mut self) {
        self.history.push(Snapshot { g: self.g.clone(), step: self.step.clone(), log: self.log.clone() });
    }

    fn undo(&mut self) {
        if let Some(s) = self.history.pop() {
            self.g = s.g;
            self.step = s.step;
            self.log = s.log;
            self.error = None;
            self.changed();
        }
    }

    fn log(&mut self, s: String) {
        self.log.push(s);
    }

    /// Il passo dopo un'azione, secondo lo stato della partita.
    fn next_step(&mut self, my_taken_before: bool) {
        let g = &self.g;
        self.step = if g.is_over() {
            Step::Over
        } else if !my_taken_before && g.taken[ME as usize] {
            Step::MyPozzetto
        } else {
            match (g.to_move, g.phase) {
                (ME, Phase::Draw) => Step::MyDraw,
                (ME, _) => Step::MyPlay,
                (_, Phase::Draw) => Step::OppDraw,
                _ => Step::OppPlay,
            }
        };
    }

    /// Esegue un'azione sulla partita, con le carte nascoste già sistemate.
    fn act(&mut self, a: Action, text: String) {
        let before = self.g.clone();
        let taken = self.g.taken[ME as usize];
        match self.g.apply(&a) {
            Ok(()) => {
                self.history.push(Snapshot { g: before, step: self.step.clone(), log: self.log.clone() });
                self.log(text);
                self.error = None;
                self.next_step(taken);
                self.changed();
            }
            Err(e) => {
                self.g = before;
                self.error = Some(format!("Mossa non valida: {e}."));
            }
        }
    }

    fn start(&mut self) {
        let Some(top) = self.setup_top else { return };
        if self.setup_hand.len() != 11 {
            return;
        }
        self.save();
        self.g = new_game(&self.setup_hand, top, self.first);
        self.log = vec![format!("Inizia {}", if self.first == ME { "tu" } else { "l'avversario" })];
        self.next_step(false);
        self.changed();
    }

    /// Carte che si possono ancora indicare in preparazione (al più due copie, quattro jolly).
    fn setup_allowed(&self) -> Cards {
        let mut used = multiset(&self.setup_hand);
        used.add_all(&multiset(&self.setup_top.into_iter().collect::<Vec<_>>()));
        Cards::full_deck().minus(&used)
    }

    /// Carte non ancora viste che l'utente può indicare (pescata, pozzetto, carte dell'avversario).
    fn unseen(&self) -> Cards {
        let mut u = self.g.unseen(ME);
        // Anche le carte note in mano all'avversario possono essere calate o scartate da lui.
        if matches!(self.step, Step::OppPlay) {
            u.add_all(&self.g.known[OPP as usize]);
        }
        u
    }

    fn allowed(&self) -> Cards {
        match self.step {
            Step::Setup => self.setup_allowed(),
            Step::MyDrawCard | Step::MyPozzetto | Step::OppPlay => self.unseen().minus(&multiset(&self.selected)),
            Step::MyPlay => self.g.hands[ME as usize].minus(&multiset(&self.selected)),
            _ => Cards::EMPTY,
        }
    }

    /// Una carta indicata dall'utente (dal selettore, dalla tastiera o dalla mano).
    fn enter(&mut self, k: Kind) {
        if !self.allowed().has(k) {
            self.error = Some(format!("{} non è disponibile qui.", cards::name(k)));
            return;
        }
        self.error = None;
        match self.step {
            Step::Setup => {
                if self.setup_hand.len() < 11 {
                    self.setup_hand.push(k);
                } else if self.setup_top.is_none() {
                    self.setup_top = Some(k);
                }
            }
            Step::MyDrawCard => {
                let mut g = self.g.clone();
                if bring_to_stock_top(&mut g, k) {
                    self.g = g;
                    self.act(Action::DrawStock, format!("Hai pescato {}", cards::name(k)));
                } else {
                    self.error = Some("Quella carta non può essere nel tallone.".into());
                }
            }
            Step::MyPozzetto => {
                self.selected.push(k);
                let n = self.g.hands[ME as usize].len();
                if self.selected.len() == n {
                    let real = std::mem::take(&mut self.selected);
                    let mut g = self.g.clone();
                    if set_my_hand(&mut g, &real) {
                        self.save();
                        self.g = g;
                        self.log(format!("Hai preso il pozzetto: {}", names(&sorted(real))));
                        self.next_step(true);
                        self.changed();
                    } else {
                        self.error = Some("Queste carte non possono essere nel pozzetto.".into());
                    }
                }
            }
            Step::MyPlay | Step::OppPlay => self.selected.push(k),
            _ => {}
        }
    }

    fn parse_entry(&mut self) {
        let text = std::mem::take(&mut self.entry);
        let mut bad = Vec::new();
        for w in text.split(|c: char| c.is_whitespace() || c == ',') {
            if w.is_empty() {
                continue;
            }
            match cards::parse(w) {
                Some(k) => self.enter(k),
                None => bad.push(w.to_string()),
            }
        }
        if !bad.is_empty() {
            self.error = Some(format!("Non capisco: {}. Esempi: 7c, Kq, 10p, Af, jk (jolly).", bad.join(" ")));
        }
    }

    /// Avvia o raccoglie l'analisi della posizione corrente.
    fn poll(&mut self, samples: u32, ctx: &egui::Context) {
        if let Some((gen, rx)) = &self.pending {
            if let Ok(a) = rx.try_recv() {
                if *gen == self.generation {
                    self.advice = Some((*gen, samples, a));
                }
                self.pending = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }
        }
        let wants = matches!(self.step, Step::MyDraw | Step::MyPlay);
        let stale = self.advice.as_ref().is_some_and(|(g, s, _)| *g != self.generation || *s != samples);
        if wants && self.pending.is_none() && (self.advice.is_none() || stale) {
            let g = self.g.clone();
            let step = self.step.clone();
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let _ = tx.send(analyze(&g, &step, samples));
            });
            self.pending = Some((self.generation, rx));
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    fn apply_advice(&mut self) {
        let Some((_, _, Advice::Play { ops, evals, .. })) = &self.advice else { return };
        let ops = ops.clone();
        let discard = evals.first().map(|e| e.action);
        let start = self.history.len();
        for op in ops {
            let text = op_text(&op, &self.g.melds[ME as usize], true);
            self.act(op.action(), text);
            if self.step != Step::MyPlay {
                break;
            }
        }
        if self.step == Step::MyPlay {
            if let Some(Action2::Discard(k)) = discard {
                self.act(Action::Discard(k), format!("Hai scartato {}", cards::name(k)));
            }
        }
        // Un solo passo da annullare per tutta la mossa.
        if self.history.len() > start + 1 {
            self.history.truncate(start + 1);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, samples: &mut u32) {
        self.poll(*samples, &ui.ctx().clone());
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            self.undo();
        }
        let full = ui.available_rect_before_wrap();
        let side_w = (full.width() * 0.36).clamp(380.0, 500.0);
        let split = full.max.x - side_w;
        let left = Rect::from_min_max(full.min, pos2(split, full.max.y));
        let right = Rect::from_min_max(pos2(split + 14.0, full.min.y + 10.0), full.max - vec2(10.0, 0.0));
        paint_felt(ui.painter(), left);
        ui.painter().vline(split, full.y_range(), Stroke::new(1.0, Color32::from_gray(60)));
        ui.scope_builder(egui::UiBuilder::new().max_rect(left.shrink(12.0)), |ui| self.table_ui(ui));
        ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| self.side_ui(ui, samples));
    }

    fn table_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Burraco").color(GOLD).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Nuova smazzata").clicked() {
                    *self = BurracoAssistant::default();
                }
                let undo = ui
                    .add_enabled(!self.history.is_empty(), egui::Button::new("Annulla ultima azione"))
                    .on_hover_text("Ctrl+Z");
                if undo.clicked() {
                    self.undo();
                }
            });
        });
        if self.step == Step::Setup {
            self.setup_table(ui);
            return;
        }
        let g = self.g.clone();
        let white = |a: u8| Color32::from_white_alpha(a);
        let o = OPP as usize;
        let m = ME as usize;
        let status = |p: usize| {
            if g.taken[p] {
                "pozzetto preso"
            } else {
                "pozzetto da prendere"
            }
        };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "Avversario · {} carte in mano · {} · in tavola {} punti",
                    g.hands[o].len(),
                    status(o),
                    table_points(&g.melds[o])
                ))
                .color(white(220)),
            );
            let known = g.known[o].to_vec();
            if !known.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Carte che sai che ha (raccolte dagli scarti):").color(white(170)).small());
                    for k in sorted(known) {
                        card(ui, k, SMALL, None);
                    }
                });
            }
            melds_row(ui, &g.melds[o], None);
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Tallone: {} carte", g.stock.len())).color(white(220)).strong());
                ui.separator();
                ui.label(RichText::new(format!("Scarti: {}", g.pile.len())).color(white(220)).strong());
            });
            ui.horizontal_wrapped(|ui| {
                ui.set_min_height(CARD.y);
                for (i, &k) in g.pile.iter().enumerate() {
                    let glow = (i + 1 == g.pile.len()).then_some(GOLD);
                    card(ui, k, CARD, glow);
                }
                if g.pile.is_empty() {
                    ui.label(RichText::new("(nessuno scarto)").color(white(140)));
                }
            });
            ui.separator();
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "Le tue combinazioni · {} · in tavola {} punti",
                    status(m),
                    table_points(&g.melds[m])
                ))
                .color(white(220)),
            );
            let attach_to =
                melds_row(ui, &g.melds[m], (self.step == Step::MyPlay && !self.selected.is_empty()).then_some(()));
            ui.add_space(8.0);
            ui.label(RichText::new(format!("La tua mano · {} carte", g.hands[m].len())).strong().color(Color32::WHITE));
            let mut clicked = None;
            let mut unselect = None;
            let mut left = multiset(&self.selected);
            ui.horizontal_wrapped(|ui| {
                for k in sorted(g.hands[m].to_vec()) {
                    let sel = self.step == Step::MyPlay && left.remove(k);
                    let resp = card(ui, k, CARD, sel.then_some(BEST));
                    if self.step == Step::MyPlay && resp.clicked() {
                        if sel {
                            unselect = Some(k);
                        } else {
                            clicked = Some(k);
                        }
                    }
                }
            });
            if let Some(k) = clicked {
                self.enter(k);
            }
            if let Some(k) = unselect {
                if let Some(i) = self.selected.iter().position(|&x| x == k) {
                    self.selected.remove(i);
                }
            }
            if let Some(i) = attach_to {
                let ks = self.selected.clone();
                self.act(Action::Attach(i, ks.clone()), format!("Hai attaccato {}", names(&ks)));
            }
            ui.add_space(10.0);
            ui.separator();
            ui.label(RichText::new("Ultime azioni").color(white(170)).small());
            for l in self.log.iter().rev().take(6) {
                ui.label(RichText::new(l).color(white(190)).small());
            }
        });
    }

    fn setup_table(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.label(RichText::new("Le tue 11 carte").strong().color(Color32::WHITE));
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            ui.set_min_height(CARD.y);
            for (i, &k) in self.setup_hand.iter().enumerate() {
                if card(ui, k, CARD, None).on_hover_text("Clicca per toglierla").clicked() {
                    remove = Some(i);
                }
            }
        });
        if let Some(i) = remove {
            self.setup_hand.remove(i);
        }
        ui.add_space(10.0);
        ui.label(RichText::new("Carta scoperta sugli scarti").strong().color(Color32::WHITE));
        ui.horizontal(|ui| {
            ui.set_min_height(CARD.y);
            if let Some(k) = self.setup_top {
                if card(ui, k, CARD, None).on_hover_text("Clicca per toglierla").clicked() {
                    self.setup_top = None;
                }
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Chi inizia:").color(Color32::WHITE));
            ui.selectable_value(&mut self.first, ME, "Io");
            ui.selectable_value(&mut self.first, OPP, "L'avversario");
        });
        ui.add_space(10.0);
        let ready = self.setup_hand.len() == 11 && self.setup_top.is_some();
        if ui.add_enabled(ready, egui::Button::new(RichText::new("Inizia la smazzata").size(17.0))).clicked() {
            self.start();
        }
    }

    fn side_ui(&mut self, ui: &mut egui::Ui, samples: &mut u32) {
        egui::ScrollArea::vertical().id_salt("burraco_side").auto_shrink([false, false]).show(ui, |ui| {
            match self.step.clone() {
                Step::Setup => {
                    let what = if self.setup_hand.len() < 11 {
                        format!("Indica le tue carte ({} di 11)", self.setup_hand.len())
                    } else if self.setup_top.is_none() {
                        "Indica la carta scoperta sugli scarti".to_string()
                    } else {
                        "Tutto pronto: scegli chi inizia e premi Inizia".to_string()
                    };
                    title(ui, &what, "Scrivi i codici (7c 10q Kf Ap jk...) o clicca le carte qui sotto.");
                    self.entry_and_picker(ui);
                }
                Step::MyDraw => {
                    title(ui, "Tocca a te: pesca", "Cosa ti consiglio, poi dimmi cosa hai fatto.");
                    self.advice_ui(ui, samples);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if big_button(ui, "Ho pescato dal tallone").clicked() {
                            self.step = Step::MyDrawCard;
                            self.changed();
                        }
                        let can = !self.g.pile.is_empty();
                        if ui.add_enabled(can, egui::Button::new(RichText::new("Ho raccolto gli scarti").size(16.0)).min_size(vec2(190.0, 40.0))).clicked() {
                            let pile = self.g.pile.clone();
                            self.act(Action::TakePile, format!("Hai raccolto gli scarti: {}", names(&pile)));
                        }
                    });
                }
                Step::MyDrawCard => {
                    title(ui, "Che carta hai pescato?", "Scrivila o cliccala.");
                    self.entry_and_picker(ui);
                    if ui.button("Indietro").clicked() {
                        self.step = Step::MyDraw;
                        self.changed();
                    }
                }
                Step::MyPozzetto => {
                    let n = self.g.hands[ME as usize].len();
                    title(
                        ui,
                        &format!("Hai preso il pozzetto: quali carte? ({} di {n})", self.selected.len()),
                        "Indica le carte del pozzetto.",
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.set_min_height(SMALL.y + 4.0);
                        for &k in &self.selected.clone() {
                            card(ui, k, SMALL, None);
                        }
                    });
                    if ui.add_enabled(!self.selected.is_empty(), egui::Button::new("Ricomincia le carte del pozzetto")).clicked() {
                        self.selected.clear();
                    }
                    self.entry_and_picker(ui);
                }
                Step::MyPlay => {
                    title(ui, "Tocca a te: cala e scarta", "Il consiglio del motore, oppure fai la tua mossa.");
                    self.advice_ui(ui, samples);
                    ui.add_space(8.0);
                    self.my_manual(ui);
                }
                Step::OppDraw => {
                    title(ui, "Tocca all'avversario: cosa ha pescato?", "");
                    ui.horizontal(|ui| {
                        if big_button(ui, "Dal tallone").clicked() {
                            self.act(Action::DrawStock, "L'avversario ha pescato dal tallone".into());
                        }
                        let can = !self.g.pile.is_empty();
                        if ui.add_enabled(can, egui::Button::new(RichText::new("Ha raccolto gli scarti").size(16.0)).min_size(vec2(190.0, 40.0))).clicked() {
                            let pile = self.g.pile.clone();
                            self.act(Action::TakePile, format!("L'avversario ha raccolto gli scarti: {}", names(&pile)));
                        }
                    });
                }
                Step::OppPlay => {
                    title(
                        ui,
                        "Cosa ha calato e scartato l'avversario?",
                        "Scegli le carte, poi indica se le ha calate, attaccate o scartate. Una combinazione alla volta.",
                    );
                    ui.horizontal(|ui| {
                        // Altezza fissa: il selettore sotto non si sposta mentre scegli le carte.
                        ui.set_min_height(SMALL.y + 4.0);
                        ui.label("Carte scelte:");
                        for &k in &self.selected.clone() {
                            card(ui, k, SMALL, None);
                        }
                    });
                    self.opp_actions(ui);
                    ui.add_space(6.0);
                    self.entry_and_picker(ui);
                }
                Step::Over => self.over_ui(ui),
            }
            if let Some(e) = &self.error {
                ui.add_space(6.0);
                ui.label(RichText::new(e).color(Color32::LIGHT_RED));
            }
            ui.add_space(14.0);
            ui.label(
                RichText::new(
                    "Usa l'assistente nel rispetto del regolamento delle piattaforme su cui giochi.",
                )
                .weak()
                .small(),
            );
        });
    }

    fn entry_and_picker(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let resp =
                ui.add(egui::TextEdit::singleline(&mut self.entry).hint_text("es. 7c Kq jk").desired_width(220.0));
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Aggiungi").clicked() || enter {
                self.parse_entry();
                resp.request_focus();
            }
        });
        ui.add_space(4.0);
        if let Some(k) = picker(ui, &self.allowed()) {
            self.enter(k);
        }
    }

    fn advice_ui(&mut self, ui: &mut egui::Ui, samples: &mut u32) {
        ui.horizontal(|ui| {
            ui.label("Precisione:");
            for (n, label) in [(32, "veloce"), (96, "normale"), (300, "alta")] {
                ui.selectable_value(samples, n, label);
            }
        });
        let Some((_, _, advice)) = &self.advice else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Il motore sta simulando la smazzata...");
            });
            return;
        };
        let frame = egui::Frame::new().fill(Color32::from_rgb(28, 44, 36)).corner_radius(8.0).inner_margin(10.0);
        let mut apply = false;
        frame.show(ui, |ui| match advice {
            Advice::Draw(evals) => {
                let best = evals[0];
                let what = match best.action {
                    Action2::Pile => format!("Raccogli gli scarti ({} carte)", self.g.pile.len()),
                    _ => "Pesca dal tallone".to_string(),
                };
                ui.label(RichText::new(what).size(19.0).strong().color(BEST));
                if best.win_prob.is_nan() {
                    ui.label(if best.action == Action2::Pile {
                        "Negli scarti c'è una carta sola e la puoi calare subito: prendila."
                    } else {
                        "Negli scarti c'è una carta sola che non cali subito: meglio pescare dal tallone."
                    });
                }
                for e in evals.iter().filter(|e| !e.win_prob.is_nan()) {
                    let name = if e.action == Action2::Pile { "Raccogliere gli scarti" } else { "Pescare dal tallone" };
                    ui.label(format!(
                        "{name}: vinci la smazzata nel {:.0}% dei casi, scarto medio {:+.0} punti",
                        e.win_prob * 100.0,
                        e.margin
                    ));
                }
            }
            Advice::Play { ops, evals, pozzetto, closes } => {
                if ops.is_empty() {
                    ui.label(RichText::new("Non calare niente").strong());
                } else {
                    ui.label(RichText::new("Cala in quest'ordine:").strong());
                    let mut melds = self.g.melds[ME as usize].clone();
                    for op in ops {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(op_text(op, &melds, false));
                            for &k in op.cards() {
                                card(ui, k, SMALL, None);
                            }
                        });
                        match op {
                            Op::New(v) => melds.extend(Meld::new(v)),
                            Op::Add(i, v) => {
                                if let Some(m) = melds[*i].with(v) {
                                    melds[*i] = m;
                                }
                            }
                        }
                    }
                }
                if *pozzetto {
                    ui.label(
                        RichText::new(
                            "Così finisci le carte e prendi il pozzetto al volo: poi indicami le carte del pozzetto.",
                        )
                        .color(GOLD),
                    );
                } else if let Some(best) = evals.first() {
                    if let Action2::Discard(k) = best.action {
                        let verb = if *closes { "Chiudi scartando" } else { "Scarta" };
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{verb} {}", cards::name(k))).size(19.0).strong().color(BEST),
                            );
                            card(ui, k, SMALL, Some(BEST));
                        });
                        for e in evals {
                            if let Action2::Discard(k) = e.action {
                                ui.label(format!(
                                    "Scartare {}: vinci nel {:.0}% dei casi, scarto medio {:+.0}",
                                    cards::name(k),
                                    e.win_prob * 100.0,
                                    e.margin
                                ));
                            }
                        }
                    }
                }
                ui.add_space(6.0);
                if ui.button(RichText::new("Ho fatto come consigliato").size(16.0)).clicked() {
                    apply = true;
                }
            }
        });
        if apply {
            self.apply_advice();
        }
    }

    fn my_manual(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Oppure fai la tua mossa: clicca le carte della mano, poi").weak());
        let sel = self.selected.clone();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(sel.len() >= 3, egui::Button::new("Cala combinazione")).clicked() {
                self.act(Action::Meld(sel.clone()), format!("Hai calato {}", names(&sel)));
            }
            if ui.add_enabled(sel.len() == 1, egui::Button::new("Scarta")).clicked() {
                self.act(Action::Discard(sel[0]), format!("Hai scartato {}", cards::name(sel[0])));
            }
            if !sel.is_empty() && ui.button("Deseleziona").clicked() {
                self.selected.clear();
            }
        });
        if !sel.is_empty() {
            ui.label(RichText::new("Per attaccare: clicca la combinazione sul tavolo.").weak().small());
        }
    }

    fn opp_actions(&mut self, ui: &mut egui::Ui) {
        let sel = self.selected.clone();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(sel.len() >= 3, egui::Button::new("Ha calato una combinazione")).clicked() {
                self.opp_act(Action::Meld(sel.clone()), format!("L'avversario ha calato {}", names(&sel)), &sel);
            }
            if ui.add_enabled(sel.len() == 1, egui::Button::new("Ha scartato questa carta")).clicked() {
                self.opp_act(
                    Action::Discard(sel[0]),
                    format!("L'avversario ha scartato {}", cards::name(sel[0])),
                    &sel,
                );
            }
            if ui.add_enabled(!sel.is_empty(), egui::Button::new("Ricomincia")).clicked() {
                self.selected.clear();
            }
        });
        if self.g.melds[OPP as usize].is_empty() {
            ui.label(
                RichText::new("Esempio: scegli 7c 8c 9c e premi \"Ha calato\"; alla fine indica lo scarto.")
                    .weak()
                    .small(),
            );
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.label("Le ha attaccate a:");
                for i in 0..self.g.melds[OPP as usize].len() {
                    let m = self.g.melds[OPP as usize][i];
                    if ui.add_enabled(!sel.is_empty(), egui::Button::new(meld_label(&m))).clicked() {
                        self.opp_act(
                            Action::Attach(i, sel.clone()),
                            format!("L'avversario ha attaccato {}", names(&sel)),
                            &sel,
                        );
                    }
                }
            });
        }
    }

    fn opp_act(&mut self, a: Action, text: String, ks: &[Kind]) {
        let mut g = self.g.clone();
        if !bring_to_opp(&mut g, ks) {
            self.error = Some("L'avversario non può avere tutte queste carte in mano.".into());
            return;
        }
        let saved = std::mem::replace(&mut self.g, g);
        let before_len = self.history.len();
        self.act(a, text);
        if self.history.len() == before_len {
            self.g = saved;
        } else {
            // Annullando si torna allo stato prima dello scambio dei segnaposto.
            self.history.last_mut().unwrap().g = saved;
        }
    }

    fn over_ui(&mut self, ui: &mut egui::Ui) {
        let g = &self.g;
        let (me, opp) = (g.score(ME), g.score(OPP));
        let (t, c) = if me > opp {
            (format!("Hai vinto la smazzata {me} a {opp}"), Color32::LIGHT_GREEN)
        } else if me == opp {
            (format!("Pareggio {me} a {opp}"), Color32::YELLOW)
        } else {
            (format!("Hai perso la smazzata {me} a {opp}"), Color32::LIGHT_RED)
        };
        ui.label(RichText::new(t).size(22.0).strong().color(c));
        let how = match g.closed_by {
            Some(ME) => "Hai chiuso tu (+100).",
            Some(_) => "Ha chiuso l'avversario (+100 per lui).",
            None => "Il tallone è finito: nessuna chiusura.",
        };
        ui.label(how);
        ui.label(
            RichText::new("Il punteggio dell'avversario conta le carte che gli restano come le stima l'assistente: correggilo con il conto reale.")
                .weak()
                .small(),
        );
        if ui.button("Nuova smazzata").clicked() {
            *self = BurracoAssistant::default();
        }
    }
}

/// Analisi della posizione: pescata o calate e scarto.
fn analyze(g: &Game, step: &Step, samples: u32) -> Advice {
    let mut rng = SmallRng::from_entropy();
    let mut cfg = EngineConfig::new(samples);
    cfg.parallel = true;
    cfg.discards = 4;
    match step {
        Step::MyDraw => {
            let c = engine::candidates(g, &cfg);
            if c.is_empty() {
                // Scarto di una carta sola: si raccoglie solo se si cala subito (come fa il motore nei test).
                let take = wants_pile(g, ME, &Style::EXPERT, &mut rng);
                let action = if take { Action2::Pile } else { Action2::Stock };
                return Advice::Draw(vec![ChoiceEval { action, win_prob: f64::NAN, margin: f64::NAN }]);
            }
            Advice::Draw(engine::evaluate_choices(g, &c, &cfg, &mut rng))
        }
        _ => {
            let plan = turn_plan(g, ME, &Style::EXPERT);
            let mut after = g.clone();
            let mut ops = Vec::new();
            for op in plan.ops {
                if after.apply(&op.action()).is_err() {
                    break;
                }
                ops.push(op);
                if after.taken[ME as usize] != g.taken[ME as usize] {
                    return Advice::Play { ops, evals: Vec::new(), pozzetto: true, closes: false };
                }
            }
            let hand = after.hands[ME as usize];
            let closes = after.taken[ME as usize] && hand.len() == 1;
            let mut cands = engine::candidates(&after, &cfg);
            if cands.is_empty() {
                // Scelta obbligata: l'unica carta che si può scartare.
                cands = hand.kinds().filter(|&k| after.can_discard(k)).take(1).map(Action2::Discard).collect();
            }
            let evals = engine::evaluate_choices(&after, &cands, &cfg, &mut rng);
            Advice::Play { ops, evals, pozzetto: false, closes }
        }
    }
}

fn table_points(ms: &[Meld]) -> i32 {
    ms.iter().map(|m| m.value() + m.bonus()).sum()
}

fn meld_label(m: &Meld) -> String {
    let cs = m.cards();
    let label = |k: Kind| {
        if k == JOLLY || cards::is_wild(k) {
            "matta".to_string()
        } else {
            cards::RANK_LABELS[cards::rank(k) as usize].to_string()
        }
    };
    if m.seq {
        format!("scala di {} ({}–{})", cards::SUIT_NAMES[m.suit as usize], label(cs[0]), label(*cs.last().unwrap()))
    } else {
        format!("tris di {} ({} carte)", cards::RANK_LABELS[m.rank as usize], m.len())
    }
}

fn op_text(op: &Op, melds: &[Meld], past: bool) -> String {
    match op {
        Op::New(v) => format!("{} {}", if past { "Hai calato" } else { "Cala" }, names(v)),
        Op::Add(i, v) => format!(
            "{} {} alla {}",
            if past { "Hai attaccato" } else { "Attacca" },
            names(v),
            melds.get(*i).map(meld_label).unwrap_or_default()
        ),
    }
}

/// Combinazioni in fila; con `clickable` restituisce quella cliccata.
fn melds_row(ui: &mut egui::Ui, ms: &[Meld], clickable: Option<()>) -> Option<usize> {
    let mut hit = None;
    if ms.is_empty() {
        ui.label(RichText::new("(nessuna combinazione)").color(Color32::from_white_alpha(140)));
        return None;
    }
    ui.horizontal_wrapped(|ui| {
        for (i, m) in ms.iter().enumerate() {
            let resp = ui
                .scope(|ui| {
                    ui.spacing_mut().item_spacing.x = -SMALL.x * 0.45;
                    ui.horizontal(|ui| {
                        for k in m.cards() {
                            card(ui, k, SMALL, m.is_burraco().then_some(GOLD));
                        }
                    });
                })
                .response;
            let resp = ui.interact(
                resp.rect,
                ui.make_persistent_id(("meld", i)),
                if clickable.is_some() { Sense::click() } else { Sense::hover() },
            );
            if m.is_burraco() {
                ui.painter().text(
                    resp.rect.center_bottom() + vec2(0.0, 2.0),
                    Align2::CENTER_TOP,
                    if m.is_clean() { "burraco pulito" } else { "burraco sporco" },
                    FontId::proportional(11.0),
                    GOLD,
                );
            }
            if clickable.is_some() {
                if resp.hovered() {
                    ui.painter().rect_stroke(resp.rect.expand(3.0), 6.0, Stroke::new(2.0, BEST), StrokeKind::Outside);
                }
                if resp.on_hover_text("Attacca qui le carte scelte").clicked() {
                    hit = Some(i);
                }
            }
            ui.add_space(SMALL.x * 0.45 + 14.0);
        }
    });
    ui.add_space(12.0);
    hit
}

fn title(ui: &mut egui::Ui, t: &str, sub: &str) {
    ui.label(RichText::new(t).size(20.0).strong().color(Color32::WHITE));
    if !sub.is_empty() {
        ui.label(RichText::new(sub).weak());
    }
    ui.add_space(6.0);
}

fn big_button(ui: &mut egui::Ui, t: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(t).size(16.0)).min_size(vec2(190.0, 40.0)))
}

fn suit_color(k: Kind) -> Color32 {
    if cards::is_red(k) {
        RED
    } else {
        BLACK
    }
}

/// Disegna il seme francese centrato in `c`, alto circa `d`.
fn paint_suit(p: &Painter, c: Pos2, d: f32, suit: u8) {
    let col = if suit < 2 { RED } else { BLACK };
    let at = |x: f32, y: f32| pos2(c.x + x * d, c.y + y * d);
    match suit {
        0 => {
            // Cuori.
            p.circle_filled(at(-0.22, -0.12), 0.25 * d, col);
            p.circle_filled(at(0.22, -0.12), 0.25 * d, col);
            p.add(Shape::convex_polygon(vec![at(-0.46, -0.04), at(0.46, -0.04), at(0.0, 0.48)], col, Stroke::NONE));
        }
        1 => {
            // Quadri.
            p.add(Shape::convex_polygon(
                vec![at(0.0, -0.5), at(0.36, 0.0), at(0.0, 0.5), at(-0.36, 0.0)],
                col,
                Stroke::NONE,
            ));
        }
        2 => {
            // Fiori.
            p.circle_filled(at(0.0, -0.24), 0.22 * d, col);
            p.circle_filled(at(-0.24, 0.08), 0.22 * d, col);
            p.circle_filled(at(0.24, 0.08), 0.22 * d, col);
            p.add(Shape::convex_polygon(
                vec![at(-0.06, 0.0), at(0.06, 0.0), at(0.16, 0.48), at(-0.16, 0.48)],
                col,
                Stroke::NONE,
            ));
        }
        _ => {
            // Picche.
            p.circle_filled(at(-0.22, 0.1), 0.24 * d, col);
            p.circle_filled(at(0.22, 0.1), 0.24 * d, col);
            p.add(Shape::convex_polygon(vec![at(0.0, -0.5), at(0.45, 0.04), at(-0.45, 0.04)], col, Stroke::NONE));
            p.add(Shape::convex_polygon(
                vec![at(-0.06, 0.1), at(0.06, 0.1), at(0.16, 0.5), at(-0.16, 0.5)],
                col,
                Stroke::NONE,
            ));
        }
    }
}

fn paint_card(p: &Painter, rect: Rect, k: Kind, glow: Option<Color32>, hovered: bool) {
    let h = rect.height();
    let rect = if hovered { rect.translate(vec2(0.0, -3.0)) } else { rect };
    p.rect_filled(rect, 0.08 * h, PAPER);
    let border = match glow {
        Some(c) => Stroke::new(2.5, c),
        None => Stroke::new(1.0, Color32::from_gray(150)),
    };
    p.rect_stroke(rect, 0.08 * h, border, StrokeKind::Inside);
    if k == JOLLY {
        p.circle_filled(rect.center() - vec2(0.0, 0.1 * h), 0.17 * h, Color32::from_rgb(230, 170, 40));
        p.circle_filled(rect.center() - vec2(0.0, 0.1 * h), 0.08 * h, Color32::from_rgb(120, 60, 140));
        p.text(
            rect.center() + vec2(0.0, 0.25 * h),
            Align2::CENTER_CENTER,
            "JOLLY",
            FontId::proportional(0.17 * h),
            Color32::from_rgb(120, 60, 140),
        );
        return;
    }
    let col = suit_color(k);
    p.text(
        rect.left_top() + vec2(0.1 * h, 0.04 * h),
        Align2::LEFT_TOP,
        cards::RANK_LABELS[cards::rank(k) as usize],
        FontId::proportional(0.3 * h),
        col,
    );
    paint_suit(p, rect.center() + vec2(0.0, 0.16 * h), 0.4 * h, cards::suit(k));
    if cards::rank(k) == 2 {
        // Pinella: un segno dorato nell'angolo, è una matta.
        p.circle_filled(rect.right_top() + vec2(-0.14 * h, 0.14 * h), 0.07 * h, Color32::from_rgb(230, 170, 40));
    }
}

fn card(ui: &mut egui::Ui, k: Kind, size: Vec2, glow: Option<Color32>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    paint_card(ui.painter(), rect, k, glow, resp.hovered());
    resp.on_hover_text(cards::name(k))
}

/// Griglia 4×13 più il jolly: solo le carte ammesse sono cliccabili.
fn picker(ui: &mut egui::Ui, allowed: &Cards) -> Option<Kind> {
    let mut chosen = None;
    let size = vec2(26.0, 37.0);
    egui::Grid::new("burraco_picker").min_col_width(0.0).spacing([2.0, 3.0]).show(ui, |ui| {
        for s in 0..4u8 {
            for r in [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13] {
                let k = cards::kind(s, r);
                chosen = chosen.or(pick_cell(ui, k, size, allowed.has(k)));
            }
            ui.end_row();
        }
        chosen = chosen.or(pick_cell(ui, JOLLY, size, allowed.has(JOLLY)));
        ui.end_row();
    });
    chosen
}

fn pick_cell(ui: &mut egui::Ui, k: Kind, size: Vec2, ok: bool) -> Option<Kind> {
    let (rect, resp) = ui.allocate_exact_size(size, if ok { Sense::click() } else { Sense::hover() });
    let mut p = ui.painter().clone();
    if !ok {
        p.multiply_opacity(0.22);
    }
    paint_card(&p, rect, k, None, ok && resp.hovered());
    (ok && resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(cards::name(k)).clicked()).then_some(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ks(s: &str) -> Vec<Kind> {
        s.split_whitespace().map(|c| cards::parse(c).unwrap()).collect()
    }

    #[test]
    fn tracks_a_described_hand() {
        let mut a = BurracoAssistant::default();
        for k in ks("3c 4c 5c 7q 7f 9p Jp Qp Kq Af jk") {
            a.enter(k);
        }
        a.enter(cards::parse("8c").unwrap());
        a.first = ME;
        a.start();
        assert_eq!(a.step, Step::MyDraw);
        assert_eq!(a.g.pile, ks("8c"));
        assert_eq!(a.g.total_cards(), 108);
        // Pesco il 6 di cuori.
        a.step = Step::MyDrawCard;
        a.enter(cards::parse("6c").unwrap());
        assert_eq!(a.step, Step::MyPlay);
        assert!(a.g.hands[0].has(cards::parse("6c").unwrap()));
        // Calo 3-4-5-6 di cuori e scarto il 9 di picche.
        for k in ks("3c 4c 5c 6c") {
            a.enter(k);
        }
        a.act(Action::Meld(a.selected.clone()), "".into());
        a.act(Action::Discard(cards::parse("9p").unwrap()), "".into());
        assert_eq!(a.step, Step::OppDraw);
        // L'avversario raccoglie gli scarti, cala tre K e scarta il 2 di fiori.
        a.act(Action::TakePile, "".into());
        assert!(a.g.known[1].has(cards::parse("8c").unwrap()));
        let kings = ks("Kc Kf Kp");
        a.opp_act(Action::Meld(kings.clone()), "".into(), &kings);
        assert_eq!(a.g.melds[1].len(), 1);
        let two = ks("2f");
        a.opp_act(Action::Discard(two[0]), "".into(), &two);
        assert_eq!(a.step, Step::MyDraw);
        assert_eq!(a.g.total_cards(), 108);
        assert_eq!(a.g.hands[1].len(), 11 + 2 - 3 - 1);
        // Annullare torna indietro di un passo.
        a.undo();
        assert_eq!(a.step, Step::OppPlay);
        assert_eq!(a.g.total_cards(), 108);
    }

    #[test]
    fn advice_is_legal() {
        let mut rng = SmallRng::seed_from_u64(3);
        let mut g = Game::new(&mut rng, ME);
        g.apply(&Action::DrawStock).unwrap();
        match analyze(&g, &Step::MyPlay, 8) {
            Advice::Play { ops, evals, .. } => {
                let mut w = g.clone();
                for op in &ops {
                    w.apply(&op.action()).unwrap();
                }
                if let Some(e) = evals.first() {
                    w.apply(&e.action.action()).unwrap();
                }
            }
            Advice::Draw(_) => panic!(),
        }
    }
}
