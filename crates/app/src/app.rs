//! Interfaccia: partite di allenamento contro il motore, suggerimenti e analisi a fine partita.

use crate::update::{self, Release};
use briscola_core::ai::{evaluate, EngineConfig, EnginePlayer, HeuristicPlayer, MoveEval, Player};
use briscola_core::cards::{self, Card, CardSet};
use briscola_core::{Game, Observation, Trick};
use eframe::egui::{self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{channel, Receiver};

const HUMAN: u8 = 0;
const AI: u8 = 1;
const CARD_SIZE: Vec2 = Vec2::new(92.0, 140.0);
const HINT_SAMPLES: u32 = 800;
const REVIEW_SAMPLES: u32 = 2000;

/// Configurazione del motore per l'app: i mondi si dividono su tutti i core del PC.
fn engine_cfg(samples: u32) -> EngineConfig {
    let mut cfg = EngineConfig::strength(samples);
    cfg.threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    cfg
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
enum Difficulty {
    Facile,
    Medio,
    Difficile,
}

impl Difficulty {
    const ALL: [Difficulty; 3] = [Difficulty::Facile, Difficulty::Medio, Difficulty::Difficile];

    fn index(self) -> usize {
        self as usize
    }
    fn label(self) -> &'static str {
        match self {
            Difficulty::Facile => "Facile (euristico)",
            Difficulty::Medio => "Medio (motore 150 mondi)",
            Difficulty::Difficile => "Difficile (motore 1000 mondi)",
        }
    }
    fn player(self) -> Box<dyn Player> {
        match self {
            Difficulty::Facile => Box::new(HeuristicPlayer),
            Difficulty::Medio => Box::new(EnginePlayer::new(engine_cfg(150))),
            Difficulty::Difficile => Box::new(EnginePlayer::new(engine_cfg(1000))),
        }
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Record {
    played: [u32; 3],
    won: [u32; 3],
    drawn: [u32; 3],
}

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    difficulty: Difficulty,
    training: bool,
    auto_update: bool,
    record: Record,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { difficulty: Difficulty::Medio, training: true, auto_update: true, record: Record::default() }
    }
}

#[derive(Clone, Copy)]
struct Decision {
    obs: Observation,
    played: Card,
    trick_no: usize,
}

struct ReviewItem {
    decision: Decision,
    evals: Vec<MoveEval>,
}

impl ReviewItem {
    fn chosen(&self) -> MoveEval {
        *self.evals.iter().find(|e| e.card == self.decision.played).unwrap()
    }
    /// Probabilità di vittoria persa rispetto alla mossa migliore.
    fn loss(&self) -> f64 {
        (self.evals[0].win_prob - self.chosen().win_prob).max(0.0)
    }
}

struct Match {
    game: Game,
    ai: Box<dyn Player>,
    rng: SmallRng,
    difficulty: Difficulty,
    decisions: Vec<Decision>,
    tricks: Vec<Trick>,
    shown_trick: Option<(Trick, f64)>,
    ai_ready_at: Option<f64>,
    hint: Option<Vec<MoveEval>>,
    recorded: bool,
    review_rx: Option<Receiver<Vec<ReviewItem>>>,
    review: Option<Vec<ReviewItem>>,
}

impl Match {
    fn new(difficulty: Difficulty, first: u8) -> Match {
        let mut rng = SmallRng::from_entropy();
        let game = Game::new(&mut rng, first);
        Match {
            game,
            ai: difficulty.player(),
            rng,
            difficulty,
            decisions: Vec::new(),
            tricks: Vec::new(),
            shown_trick: None,
            ai_ready_at: None,
            hint: None,
            recorded: false,
            review_rx: None,
            review: None,
        }
    }

    fn after_play(&mut self, trick: Option<Trick>, now: f64) {
        if let Some(t) = trick {
            self.tricks.push(t);
            self.shown_trick = Some((t, now));
        }
        self.hint = None;
        self.ai_ready_at = None;
    }

    fn human_play(&mut self, c: Card, now: f64) {
        let obs = Observation::of(&self.game, HUMAN);
        self.decisions.push(Decision { obs, played: c, trick_no: self.tricks.len() + 1 });
        let t = self.game.play(c);
        self.after_play(t, now);
    }

    fn start_review(&mut self) {
        let decisions: Vec<Decision> =
            self.decisions.iter().copied().filter(|d| d.obs.hand.len() > 1).collect();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut rng = SmallRng::from_entropy();
            let cfg = engine_cfg(REVIEW_SAMPLES);
            let items = decisions
                .into_iter()
                .map(|d| ReviewItem { evals: evaluate(&d.obs, &cfg, &mut rng), decision: d })
                .collect();
            let _ = tx.send(items);
        });
        self.review_rx = Some(rx);
    }
}

enum UpdateState {
    Idle,
    Checking(Receiver<Result<Option<Release>, String>>),
    UpToDate,
    Available(Release),
    Installing(Receiver<String>),
    Error(String),
}

pub struct BrisApp {
    settings: Settings,
    current: Match,
    games_started: u32,
    update: UpdateState,
}

impl BrisApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings: Settings =
            cc.storage.and_then(|s| eframe::get_value(s, "settings")).unwrap_or_default();
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.visuals = egui::Visuals::dark();
        cc.egui_ctx.set_global_style(style);
        let mut app = BrisApp {
            current: Match::new(settings.difficulty, HUMAN),
            settings,
            games_started: 1,
            update: UpdateState::Idle,
        };
        if app.settings.auto_update {
            app.check_updates();
        }
        app
    }

    fn check_updates(&mut self) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(update::check());
        });
        self.update = UpdateState::Checking(rx);
    }

    fn new_game(&mut self) {
        // Si alterna chi apre la prima presa.
        let first = if self.games_started % 2 == 0 { HUMAN } else { AI };
        self.games_started += 1;
        self.current = Match::new(self.settings.difficulty, first);
    }

    fn poll_background(&mut self) {
        let next = match &self.update {
            UpdateState::Checking(rx) => rx.try_recv().ok().map(|r| match r {
                Ok(Some(rel)) => UpdateState::Available(rel),
                Ok(None) => UpdateState::UpToDate,
                Err(e) => UpdateState::Error(e),
            }),
            UpdateState::Installing(rx) => rx.try_recv().ok().map(UpdateState::Error),
            _ => None,
        };
        if let Some(n) = next {
            self.update = n;
        }
        if let Some(rx) = &self.current.review_rx {
            if let Ok(items) = rx.try_recv() {
                self.current.review = Some(items);
                self.current.review_rx = None;
            }
        }
    }

    /// Avanza la partita: mosse del computer, fine presa, fine partita.
    fn step(&mut self, now: f64, ctx: &egui::Context) {
        let m = &mut self.current;
        if let Some((_, t)) = m.shown_trick {
            if now - t > 1.4 {
                m.shown_trick = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                return;
            }
        }
        if m.game.is_over() {
            if !m.recorded {
                m.recorded = true;
                let i = m.difficulty.index();
                let r = &mut self.settings.record;
                r.played[i] += 1;
                match m.game.points[HUMAN as usize] {
                    p if p > 60 => r.won[i] += 1,
                    60 => r.drawn[i] += 1,
                    _ => {}
                }
                m.start_review();
            }
            if m.review_rx.is_some() {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            return;
        }
        if m.game.to_move() == AI {
            match m.ai_ready_at {
                None => m.ai_ready_at = Some(now + 0.6),
                Some(t) if now >= t => {
                    let obs = Observation::of(&m.game, AI);
                    let c = m.ai.choose(&obs, &mut m.rng);
                    let trick = m.game.play(c);
                    m.after_play(trick, now);
                }
                _ => {}
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        } else if self.settings.training && m.hint.is_none() && m.game.legal_moves().len() > 1 {
            let obs = Observation::of(&m.game, HUMAN);
            m.hint = Some(evaluate(&obs, &engine_cfg(HINT_SAMPLES), &mut m.rng));
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("BrisMaster").strong());
            ui.separator();
            if ui.button("Nuova partita").clicked() {
                self.new_game();
            }
            egui::ComboBox::from_id_salt("difficulty")
                .selected_text(self.settings.difficulty.label())
                .show_ui(ui, |ui| {
                    for d in Difficulty::ALL {
                        ui.selectable_value(&mut self.settings.difficulty, d, d.label());
                    }
                })
                .response
                .on_hover_text("Vale dalla prossima partita");
            ui.checkbox(&mut self.settings.training, "Allenamento: mostra i consigli");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.update_widget(ui);
            });
        });
    }

    fn update_widget(&mut self, ui: &mut egui::Ui) {
        let mut action = None;
        match &self.update {
            UpdateState::Idle => {
                if ui.small_button("Cerca aggiornamenti").clicked() {
                    action = Some(0);
                }
            }
            UpdateState::Checking(_) => {
                ui.spinner();
                ui.label("Controllo aggiornamenti...");
            }
            UpdateState::UpToDate => {
                ui.label(RichText::new(format!("v{} aggiornata", update::CURRENT)).weak());
            }
            UpdateState::Available(rel) => {
                let btn = ui.button(RichText::new(format!("Installa v{}", rel.version)).strong());
                let btn = if rel.notes.is_empty() { btn } else { btn.on_hover_text(&rel.notes) };
                if btn.clicked() {
                    action = Some(1);
                }
                ui.hyperlink_to("Novità", &rel.page_url);
                ui.label(RichText::new("Aggiornamento disponibile").color(Color32::LIGHT_GREEN));
            }
            UpdateState::Installing(_) => {
                ui.spinner();
                ui.label("Installazione in corso...");
            }
            UpdateState::Error(e) => {
                if ui.small_button("Riprova").clicked() {
                    action = Some(0);
                }
                ui.label(RichText::new("Aggiornamento non riuscito").color(Color32::LIGHT_RED))
                    .on_hover_text(e);
            }
        }
        match action {
            Some(0) => self.check_updates(),
            Some(1) => {
                if let UpdateState::Available(rel) = &self.update {
                    let rel = rel.clone();
                    let (tx, rx) = channel();
                    std::thread::spawn(move || {
                        if let Err(e) = update::install(&rel) {
                            let _ = tx.send(e);
                        }
                    });
                    self.update = UpdateState::Installing(rx);
                }
            }
            _ => {}
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        let g = &self.current.game;
        ui.heading("Carte uscite");
        ui.label(RichText::new("Allenati a contarle anche senza guardare!").weak().small());
        ui.add_space(4.0);
        let mut gone = g.played;
        if let Some(t) = g.table {
            gone.insert(t);
        }
        egui::Grid::new("tracker").min_col_width(18.0).spacing([6.0, 4.0]).show(ui, |ui| {
            for s in 0..4u8 {
                ui.label(RichText::new(&cards::SUIT_NAMES[s as usize][..1]).color(suit_color(s)).strong());
                for r in [0u8, 2, 9, 8, 7, 6, 5, 4, 3, 1] {
                    let c = cards::make(s, r);
                    let txt = RichText::new(cards::RANK_SHORT[r as usize]).monospace();
                    let txt = if gone.contains(c) {
                        txt.strikethrough().weak()
                    } else if g.hands[HUMAN as usize].contains(c) {
                        txt.color(Color32::LIGHT_BLUE)
                    } else {
                        txt.color(suit_color(s)).strong()
                    };
                    ui.label(txt);
                }
                ui.end_row();
            }
        });
        let remaining: u32 = CardSet::FULL.minus(gone).iter().map(|c| cards::points(c) as u32).sum();
        ui.add_space(6.0);
        ui.label(format!("Punti ancora in gioco: {remaining}"));
        ui.label(RichText::new("Barrate = uscite, azzurre = in mano a te").weak().small());

        ui.separator();
        ui.heading("Il tuo record");
        let r = &self.settings.record;
        for d in Difficulty::ALL {
            let i = d.index();
            if r.played[i] == 0 {
                continue;
            }
            let pct = 100.0 * r.won[i] as f64 / r.played[i] as f64;
            ui.label(format!(
                "{}: {} vinte, {} pari, {} perse ({pct:.0}%)",
                d.label().split(' ').next().unwrap(),
                r.won[i],
                r.drawn[i],
                r.played[i] - r.won[i] - r.drawn[i]
            ));
        }
        if r.played.iter().all(|&p| p == 0) {
            ui.label(RichText::new("Ancora nessuna partita conclusa").weak());
        }
        ui.separator();
        ui.label(
            RichText::new(
                "BrisMaster è uno strumento di allenamento: gioca contro il motore e \
                 studia l'analisi a fine partita. Non usarlo durante partite online \
                 contro persone: le piattaforme lo vietano.",
            )
            .weak()
            .small(),
        );
    }

    fn table(&mut self, ui: &mut egui::Ui, now: f64) {
        let m = &self.current;
        let g = &m.game;
        let opp_n = g.hands[AI as usize].len() as usize;

        ui.vertical_centered(|ui| {
            ui.label(RichText::new(format!("Computer · {}", m.difficulty.label())).size(16.0));
            ui.horizontal(|ui| {
                // Altezza fissa, così il tavolo non si sposta quando l'avversario finisce le carte.
                ui.set_min_height(CARD_SIZE.y);
                center_row(ui, opp_n);
                for _ in 0..opp_n {
                    paint_card(ui, None, false, 0.0);
                }
            });
        });
        ui.add_space(14.0);

        ui.horizontal(|ui| {
            ui.add_space(30.0);
            ui.vertical(|ui| {
                ui.label(format!("Mazzo: {} carte", g.deck_len));
                ui.label(RichText::new("Briscola").weak());
                paint_card(ui, Some(g.trump_card), false, if g.deck_len == 0 { 0.4 } else { 1.0 });
                if g.deck_len == 0 {
                    let who = match g.trump_card_owner {
                        Some(HUMAN) => "l'hai pescata tu",
                        _ => "pescata dal computer",
                    };
                    ui.label(RichText::new(who).weak().small());
                }
            });
            ui.add_space(60.0);
            ui.vertical(|ui| {
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    let (a, b, caption) = match m.shown_trick {
                        Some((t, _)) => {
                            let (h, c) = if t.leader == HUMAN { (t.lead, t.follow) } else { (t.follow, t.lead) };
                            let who = if t.winner == HUMAN { "Prendi tu" } else { "Prende il computer" };
                            (Some(h), Some(c), format!("{who} · {} punti", t.points))
                        }
                        None => {
                            let lead = g.table;
                            let caption = match g.to_move() {
                                HUMAN if !g.is_over() => "Tocca a te".to_string(),
                                AI if !g.is_over() => "Il computer pensa...".to_string(),
                                _ => String::new(),
                            };
                            if g.leader == HUMAN { (lead, None, caption) } else { (None, lead, caption) }
                        }
                    };
                    ui.vertical(|ui| {
                        ui.label("Tu");
                        slot(ui, a);
                    });
                    ui.vertical(|ui| {
                        ui.label("Computer");
                        slot(ui, b);
                    });
                    ui.add_space(20.0);
                    ui.vertical(|ui| {
                        ui.add_space(50.0);
                        ui.label(RichText::new(caption).size(18.0).strong());
                    });
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.add_space(30.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("Tu: {}", g.points[HUMAN as usize])).size(22.0).strong());
                    ui.label(RichText::new(format!("Computer: {}", g.points[AI as usize])).size(22.0));
                    ui.label(RichText::new(format!("Presa {} di 20", (m.tricks.len() + 1).min(20))).weak());
                });
            });
        });
        ui.add_space(14.0);

        // Mano del giocatore, con i consigli se è attivo l'allenamento.
        let my_turn = m.shown_trick.is_none() && !g.is_over() && g.to_move() == HUMAN;
        let hand = g.hands[HUMAN as usize].to_vec();
        let mut clicked = None;
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("La tua mano").size(16.0));
            ui.horizontal(|ui| {
                center_row(ui, hand.len());
                for &c in &hand {
                    ui.vertical(|ui| {
                        let eval = m.hint.as_ref().and_then(|h| h.iter().position(|e| e.card == c).map(|i| (i, h[i])));
                        let best = matches!(eval, Some((0, _)));
                        let resp = paint_card(ui, Some(c), my_turn && best, 1.0);
                        if let Some((_, e)) = eval {
                            let txt = format!("vittoria {:.0}%", e.win_prob * 100.0);
                            let rt = RichText::new(txt).size(13.0);
                            ui.label(if best { rt.color(Color32::LIGHT_GREEN).strong() } else { rt.weak() });
                            ui.label(RichText::new(format!("punti attesi {:.0}", e.exp_points)).small().weak());
                        }
                        if my_turn && resp.clicked() {
                            clicked = Some(c);
                        }
                        if my_turn {
                            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        }
                    });
                }
            });
            if self.settings.training && my_turn && m.hint.is_some() {
                ui.label(
                    RichText::new("In verde la carta consigliata dal motore (stima su 800 distribuzioni possibili)")
                        .weak()
                        .small(),
                );
            }
        });
        if let Some(c) = clicked {
            self.current.human_play(c, now);
        }
    }

    fn game_over(&mut self, ui: &mut egui::Ui) {
        let m = &self.current;
        let mine = m.game.points[HUMAN as usize];
        let (title, color) = match mine {
            p if p > 60 => (format!("Hai vinto {mine} a {}!", 120 - mine), Color32::LIGHT_GREEN),
            60 => ("Pareggio 60 a 60".to_string(), Color32::YELLOW),
            _ => (format!("Hai perso {mine} a {}", 120 - mine), Color32::LIGHT_RED),
        };
        let mut again = false;
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(title).size(30.0).strong().color(color));
            if ui.button(RichText::new("Gioca ancora").size(18.0)).clicked() {
                again = true;
            }
        });
        ui.add_space(10.0);
        ui.heading("Analisi della partita");
        match &m.review {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Il motore sta analizzando le tue mosse...");
                });
            }
            Some(items) => {
                let errors = items.iter().filter(|i| i.loss() >= 0.06).count();
                let inaccuracies = items.iter().filter(|i| (0.02..0.06).contains(&i.loss())).count();
                let good = items.len() - errors - inaccuracies;
                ui.label(format!(
                    "Mosse ottime: {good} · imprecisioni: {inaccuracies} · errori: {errors} (su {} scelte non obbligate)",
                    items.len()
                ));
                ui.label(
                    RichText::new(
                        "Le percentuali sono la probabilità di vincere stimata dal motore con 2000 \
                         distribuzioni delle carte coperte, usando solo ciò che sapevi in quel momento.",
                    )
                    .weak()
                    .small(),
                );
                ui.add_space(6.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for it in items {
                        let chosen = it.chosen();
                        let best = it.evals[0];
                        let loss = it.loss();
                        let (tag, col) = if loss >= 0.06 {
                            ("Errore", Color32::LIGHT_RED)
                        } else if loss >= 0.02 {
                            ("Imprecisione", Color32::YELLOW)
                        } else {
                            ("Ottima", Color32::LIGHT_GREEN)
                        };
                        let ctx = match it.decision.obs.table {
                            Some(t) => format!("rispondendo a {}", cards::name(t)),
                            None => "aprendo la presa".to_string(),
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("Presa {:>2}", it.decision.trick_no)).monospace());
                            ui.label(RichText::new(tag).color(col).strong());
                            ui.label(format!(
                                "{ctx}: hai giocato {} (vittoria {:.0}%)",
                                cards::name(chosen.card),
                                chosen.win_prob * 100.0
                            ));
                            if loss >= 0.02 {
                                ui.label(
                                    RichText::new(format!(
                                        "meglio {} ({:.0}%)",
                                        cards::name(best.card),
                                        best.win_prob * 100.0
                                    ))
                                    .strong(),
                                );
                            }
                        });
                    }
                });
            }
        }
        if again {
            self.new_game();
        }
    }
}

impl eframe::App for BrisApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = ui.input(|i| i.time);
        let ctx = ui.ctx().clone();
        self.poll_background();
        self.step(now, &ctx);

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            // Area di gioco a sinistra, pannello informativo a destra.
            let full = ui.available_rect_before_wrap();
            let split = full.max.x - 290.0;
            let left = Rect::from_min_max(full.min, egui::pos2(split - 12.0, full.max.y));
            let right = Rect::from_min_max(egui::pos2(split + 8.0, full.min.y), full.max);
            ui.painter().vline(split, full.y_range(), Stroke::new(1.0, Color32::from_gray(60)));
            ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| self.side_panel(ui));
            ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| {
                if self.current.game.is_over() && self.current.shown_trick.is_none() {
                    self.game_over(ui);
                } else {
                    self.table(ui, now);
                }
            });
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "settings", &self.settings);
    }
}

fn suit_color(s: u8) -> Color32 {
    match s {
        0 => Color32::from_rgb(214, 160, 30),
        1 => Color32::from_rgb(200, 60, 50),
        2 => Color32::from_rgb(70, 110, 200),
        _ => Color32::from_rgb(60, 150, 70),
    }
}

fn center_row(ui: &mut egui::Ui, n: usize) {
    let w = n as f32 * (CARD_SIZE.x + ui.spacing().item_spacing.x);
    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
}

fn slot(ui: &mut egui::Ui, c: Option<Card>) {
    match c {
        Some(c) => {
            paint_card(ui, Some(c), false, 1.0);
        }
        None => {
            let (rect, _) = ui.allocate_exact_size(CARD_SIZE, Sense::hover());
            ui.painter().rect_stroke(rect, 8.0, Stroke::new(1.0, Color32::from_gray(70)), StrokeKind::Inside);
        }
    }
}

/// Disegna una carta (o il dorso se `card` è `None`).
fn paint_card(ui: &mut egui::Ui, card: Option<Card>, highlight: bool, alpha: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(CARD_SIZE, Sense::click());
    let p = ui.painter();
    let hovered = resp.hovered() && card.is_some();
    let rect = if hovered { rect.translate(Vec2::new(0.0, -4.0)) } else { rect };
    match card {
        None => {
            p.rect_filled(rect, 8.0, Color32::from_rgb(120, 30, 40));
            p.rect_stroke(rect.shrink(6.0), 5.0, Stroke::new(1.5, Color32::from_rgb(220, 190, 120)), StrokeKind::Inside);
            p.text(rect.center(), Align2::CENTER_CENTER, "B", FontId::proportional(34.0), Color32::from_rgb(220, 190, 120));
        }
        Some(c) => {
            let col = suit_color(cards::suit(c)).gamma_multiply(alpha);
            p.rect_filled(rect, 8.0, Color32::from_rgb(250, 246, 236).gamma_multiply(alpha));
            let border = if highlight { Stroke::new(4.0, Color32::LIGHT_GREEN) } else { Stroke::new(2.0, col) };
            p.rect_stroke(rect, 8.0, border, StrokeKind::Inside);
            let rank = cards::RANK_SHORT[cards::rank(c) as usize];
            p.text(rect.left_top() + Vec2::new(10.0, 6.0), Align2::LEFT_TOP, rank, FontId::proportional(26.0), col);
            p.text(rect.right_bottom() - Vec2::new(10.0, 6.0), Align2::RIGHT_BOTTOM, rank, FontId::proportional(26.0), col);
            let inner = Rect::from_center_size(rect.center(), Vec2::new(54.0, 54.0));
            p.circle_filled(inner.center(), 24.0, col.gamma_multiply(0.18));
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                cards::RANK_NAMES[cards::rank(c) as usize],
                FontId::proportional(15.0),
                Color32::from_gray(40).gamma_multiply(alpha),
            );
            p.text(
                rect.center() + Vec2::new(0.0, 36.0),
                Align2::CENTER_CENTER,
                cards::SUIT_NAMES[cards::suit(c) as usize],
                FontId::proportional(14.0),
                col,
            );
            let pts = cards::points(c);
            if pts > 0 {
                p.text(
                    rect.center_top() + Vec2::new(0.0, 10.0),
                    Align2::CENTER_TOP,
                    format!("{pts} pt"),
                    FontId::proportional(11.0),
                    Color32::from_gray(90).gamma_multiply(alpha),
                );
            }
        }
    }
    resp
}
