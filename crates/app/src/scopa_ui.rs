//! Scopa: assistente con inserimento manuale delle carte. Tu descrivi la smazzata
//! (tavolo, carte ricevute, giocate dell'avversario) e il motore ti dice cosa giocare.

use crate::cards_ui::{self, BEST, GOLD};
use eframe::egui::{self, vec2, Color32, Rect, RichText, Stroke, Vec2};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use scopa_core::ai::{evaluate, EngineConfig, MoveEval};
use scopa_core::game::{
    self, for_each_capture, match_winner, name, short, Card, CardSet, HandScore, Move, Observation, TARGET,
};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{channel, Receiver};

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
// Assistente: si inseriscono a mano tavolo, mano e giocate dell'avversario.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    Dealer,
    Table,
    Hand,
    MyMove,
    OppMove,
    Over,
}

struct Assist {
    obs: Option<Observation>,
    score: [u8; 2],
    /// Chi ha distribuito nella smazzata in corso.
    dealer: u8,
    history: Vec<Observation>,
    /// Carta scelta con più prese possibili: si aspetta quale presa.
    selected: Option<Card>,
    analysis: Option<(Vec<MoveEval>, u32)>,
    pending: Option<Receiver<(Vec<MoveEval>, u32)>>,
    generation: u64,
    added: bool,
    /// Ultima giocata, da mostrare sul tavolo.
    last: Option<String>,
}

impl Default for Assist {
    fn default() -> Self {
        Assist {
            obs: None,
            score: [0, 0],
            dealer: AI,
            history: Vec::new(),
            selected: None,
            analysis: None,
            pending: None,
            generation: 0,
            added: false,
            last: None,
        }
    }
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
    }

    fn save(&mut self) {
        if let Some(o) = self.obs {
            self.history.push(o);
        }
    }

    fn undo(&mut self) {
        if let Some(o) = self.history.pop() {
            self.obs = Some(o);
            self.added = false;
        } else {
            self.obs = None;
        }
        self.changed();
    }

    fn start(&mut self, dealer: u8) {
        self.dealer = dealer;
        self.history.clear();
        self.obs = Some(Observation::new_hand(HUMAN, dealer, self.score));
        self.last = None;
        self.added = false;
        self.changed();
    }

    fn apply(&mut self, m: Move) {
        self.save();
        if let Some(o) = &mut self.obs {
            let who = if o.to_move == HUMAN { "Hai giocato" } else { "L'avversario ha giocato" };
            let table = o.table;
            let scopa = o.apply(m);
            self.last =
                Some(format!("{who} {}: {}{}", name(m.card), take_label(&m, table), if scopa { "!" } else { "" }));
        }
        self.changed();
    }

    /// Carta indicata dall'utente nel passo corrente.
    fn enter(&mut self, c: Card) {
        let step = self.step();
        let Some(o) = self.obs else { return };
        match step {
            Step::Table => {
                self.save();
                self.obs.as_mut().unwrap().table.insert(c);
                self.changed();
            }
            Step::Hand => {
                self.save();
                self.obs.as_mut().unwrap().hand.insert(c);
                self.changed();
            }
            Step::MyMove | Step::OppMove => {
                let opts = options(o.table, c);
                if opts.len() == 1 {
                    self.apply(opts[0]);
                } else {
                    self.selected = Some(c);
                }
            }
            _ => {}
        }
    }

    /// Carte che si possono indicare adesso.
    fn allowed(&self) -> CardSet {
        let Some(o) = &self.obs else {
            return CardSet::EMPTY;
        };
        match self.step() {
            Step::Table | Step::Hand | Step::OppMove => o.unknown(),
            Step::MyMove => o.hand,
            _ => CardSet::EMPTY,
        }
    }

    fn poll(&mut self, samples: u32, ctx: &egui::Context) {
        if let Some(rx) = &self.pending {
            if let Ok(a) = rx.try_recv() {
                self.analysis = Some(a);
                self.pending = None;
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
        }
    }
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
        let ctx = ui.ctx().clone();
        let full = ui.available_rect_before_wrap();
        let side_w = (full.width() * 0.4).clamp(440.0, 560.0);
        let split = full.max.x - side_w;
        let left = Rect::from_min_max(full.min, egui::pos2(split, full.max.y));
        let right = Rect::from_min_max(egui::pos2(split + 14.0, full.min.y + 10.0), full.max - vec2(10.0, 0.0));
        cards_ui::paint_felt(ui.painter(), left);
        ui.painter().vline(split, full.y_range(), Stroke::new(1.0, Color32::from_gray(60)));
        let samples = &mut self.settings.assist_samples;
        let a = &mut self.assist;
        a.poll(*samples, &ctx);
        ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| assist_side(a, ui, samples));
        ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| assist_table(a, ui));
    }
}

fn title(ui: &mut egui::Ui, t: &str, sub: &str) {
    ui.label(RichText::new(t).size(22.0).strong().color(GOLD));
    ui.label(RichText::new(sub).color(white(190)));
    ui.add_space(8.0);
}

fn assist_side(a: &mut Assist, ui: &mut egui::Ui, samples: &mut u32) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let step = a.step();
        match step {
            Step::Dealer => {
                title(ui, "Nuova smazzata di scopa", "Punteggio della partita e chi distribuisce.");
                ui.horizontal(|ui| {
                    ui.label("Tuoi punti");
                    ui.add(egui::DragValue::new(&mut a.score[0]).range(0..=30));
                    ui.label("Avversario");
                    ui.add(egui::DragValue::new(&mut a.score[1]).range(0..=30));
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let big = |t: &str| egui::Button::new(RichText::new(t).size(17.0)).min_size(vec2(200.0, 44.0));
                    if ui.add(big("Distribuisce l'avversario")).clicked() {
                        a.start(AI);
                    }
                    if ui.add(big("Distribuisco io")).clicked() {
                        a.start(HUMAN);
                    }
                });
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
                    "Clicca la carta; se poteva prendere in più modi, scegli cosa ha preso.",
                );
                capture_choice(a, ui);
                picker(a, ui);
            }
            Step::MyMove => {
                title(ui, "Tocca a te", "Clicca nella tua mano la carta che giochi (anche se non è la consigliata).");
                capture_choice(a, ui);
                match &a.analysis {
                    Some((ev, _)) => eval_list(ui, ev, a.obs.unwrap().table),
                    None => {
                        let mut moves = Vec::new();
                        a.obs.unwrap().legal_moves(&mut moves);
                        if moves.len() == 1 {
                            ui.label("Hai una sola mossa possibile.");
                        } else {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Il motore sta calcolando...");
                            });
                        }
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Mondi simulati");
                    ui.add(egui::Slider::new(samples, 100..=5000).logarithmic(true));
                });
            }
            Step::Over => {
                let o = a.obs.unwrap();
                let h = o.score();
                if !a.added {
                    a.score[0] += h.points[0];
                    a.score[1] += h.points[1];
                    a.added = true;
                }
                title(ui, "Fine smazzata", &format!("Partita: tu {} · avversario {}", a.score[0], a.score[1]));
                hand_summary(ui, &h, "Avversario");
                ui.add_space(8.0);
                match match_winner(a.score) {
                    Some(w) => {
                        let t = if w == HUMAN { "Hai vinto la partita!" } else { "L'avversario ha vinto la partita" };
                        ui.label(RichText::new(t).size(22.0).strong());
                        if ui.button("Nuova partita").clicked() {
                            *a = Assist::default();
                        }
                    }
                    None => {
                        if ui.button(RichText::new("Prossima smazzata").size(17.0)).clicked() {
                            let dealer = 1 - a.dealer;
                            a.start(dealer);
                        }
                    }
                }
            }
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(a.obs.is_some(), egui::Button::new("Annulla ultima azione")).clicked() {
                a.undo();
            }
            if ui.button("Ricomincia").clicked() {
                *a = Assist::default();
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Inserisci tu le carte: il programma non legge lo schermo né si collega ai siti di gioco. \
                 Rispetta il regolamento delle piattaforme su cui giochi.",
            )
            .weak()
            .small(),
        );
    });
}

fn picker(a: &mut Assist, ui: &mut egui::Ui) {
    if a.selected.is_some() {
        return;
    }
    if let Some(c) = cards_ui::picker(ui, "scopa_picker", a.allowed()) {
        a.enter(c);
    }
}

fn capture_choice(a: &mut Assist, ui: &mut egui::Ui) {
    let (Some(sel), Some(o)) = (a.selected, a.obs) else {
        return;
    };
    ui.label(RichText::new(format!("Cosa prende {}?", name(sel))).strong());
    let mut chosen = None;
    for opt in options(o.table, sel) {
        if ui.button(take_label(&opt, o.table)).clicked() {
            chosen = Some(opt);
        }
    }
    if ui.button("Annulla").clicked() {
        a.selected = None;
    }
    if let Some(m) = chosen {
        a.apply(m);
    }
    ui.add_space(8.0);
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
    let my_turn = a.step() == Step::MyMove && a.selected.is_none();
    let hand = o.hand.to_vec();
    if let Some(c) =
        card_row(ui, &hand, CARD, |c| (my_turn && best.is_some_and(|b| b.card == c)).then_some(BEST), my_turn)
    {
        a.enter(c);
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
