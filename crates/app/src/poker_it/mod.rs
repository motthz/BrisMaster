//! Poker all'italiana: descrivi la mano (le tue carte, le azioni dell'avversario, il cambio)
//! e il motore ti dice cosa fare. Le carte si possono anche far leggere dallo schermo, e tutto si
//! inserisce dalla tastiera senza Invio (vedi `input`).

mod cards;
mod input;

use crate::cards_ui::{self, BEST, GOLD};
use cards::{card, picker, CARD};
use eframe::egui::{self, Color32, RichText};
use poker_it::bots::start_quantile;
use poker_it::cards::{rank, suit, Card, NUM_CARDS, RANK_SHORT, SUIT_NAMES};
use poker_it::draw::{strength, summary};
use poker_it::eval::{describe, evaluate};
use poker_it::game::{BIG_BET, CAP, SMALL_BET};
use poker_it::{Action, Engine, Hand, Observation, Phase};

/// A che punto è l'inserimento della mano.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    /// Si sceglie chi parla per primo e si inseriscono le 5 carte.
    Setup,
    /// La mano è in corso: si registrano le azioni.
    Playing,
    /// Hai cambiato: si inseriscono le carte pescate.
    NewCards,
}

pub struct PokerIt {
    engine: Engine,
    stage: Stage,
    /// Il tuo posto: 0 parla e cambia per primo, 1 per secondo.
    me: u8,
    cards: Vec<Card>,
    hand: Option<Hand>,
    /// Carte scelte da scartare (posizioni nella mano ordinata).
    discard: [bool; 5],
    kept: Vec<Card>,
    drawn: Vec<Card>,
    /// Carte uscite dalla tua mano: non possono tornare.
    gone: Vec<Card>,
    undo: Vec<(Stage, Option<Hand>, Vec<Card>, Vec<Card>, Vec<Card>)>,
    advice: Option<(usize, Option<Vec<(Action, f32)>>)>,
    /// Tasti digitati finora per la carta in corso (es. "10" in attesa del seme).
    keys: String,
    entry_error: Option<String>,
    vision: input::Vision,
    /// Si sta disegnando la finestra compatta.
    small: bool,
}

impl PokerIt {
    pub fn load(_: Option<&dyn eframe::Storage>) -> PokerIt {
        PokerIt {
            engine: Engine::bundled(),
            stage: Stage::Setup,
            me: 0,
            cards: Vec::new(),
            hand: None,
            discard: [false; 5],
            kept: Vec::new(),
            drawn: Vec::new(),
            gone: Vec::new(),
            undo: Vec::new(),
            advice: None,
            keys: String::new(),
            entry_error: None,
            vision: input::Vision::default(),
            small: false,
        }
    }

    pub fn save(&self, _: &mut dyn eframe::Storage) {}

    /// Mano nuova. Chi parla per primo si alterna da una mano all'altra (si corregge con Tab).
    fn reset(&mut self) {
        let played = self.hand.is_some();
        self.me ^= played as u8;
        self.stage = Stage::Setup;
        self.cards.clear();
        self.hand = None;
        self.discard = [false; 5];
        self.kept.clear();
        self.drawn.clear();
        self.gone.clear();
        self.undo.clear();
        self.advice = None;
        self.keys.clear();
        self.entry_error = None;
        self.vision.new_hand();
    }

    fn snapshot(&mut self) {
        self.undo.push((self.stage, self.hand.clone(), self.cards.clone(), self.drawn.clone(), self.gone.clone()));
        self.advice = None;
    }

    fn back(&mut self) {
        if let Some((stage, hand, cards, drawn, gone)) = self.undo.pop() {
            (self.stage, self.hand, self.cards, self.drawn, self.gone) = (stage, hand, cards, drawn, gone);
            self.discard = [false; 5];
            self.advice = None;
            self.vision_undone();
        }
    }

    /// Crea la mano con le tue carte al tuo posto (le carte dell'avversario non contano).
    fn start(&mut self) {
        self.snapshot();
        self.deal();
        self.stage = Stage::Playing;
    }

    fn deal(&mut self) {
        let mut deck = [0 as Card; NUM_CARDS];
        let mut rest = (0..NUM_CARDS as Card).filter(|c| !self.cards.contains(c));
        for i in 0..NUM_CARDS {
            deck[i] = if i < 10 && i % 2 == self.me as usize { self.cards[i / 2] } else { rest.next().unwrap() };
        }
        self.hand = Some(Hand::new(deck));
    }

    fn act(&mut self, a: Action) {
        self.snapshot();
        let me = self.me;
        let h = self.hand.as_mut().unwrap();
        let mine = h.to_act == me;
        if let (Action::Draw(mask), true) = (a, mine) {
            let old = h.hands[me as usize];
            self.kept = (0..5).filter(|i| mask & (1 << i) == 0).map(|i| old[i]).collect();
            self.gone.extend((0..5).filter(|i| mask & (1 << i) != 0).map(|i| old[i]));
            h.apply(a);
            h.hands[me as usize] = old;
            self.drawn.clear();
            if mask != 0 {
                self.stage = Stage::NewCards;
            }
        } else {
            h.apply(a);
        }
        self.discard = [false; 5];
    }

    fn add_drawn(&mut self, c: Card) {
        self.drawn.push(c);
        let need = 5 - self.kept.len();
        if self.drawn.len() == need {
            self.snapshot();
            let me = self.me as usize;
            let h = self.hand.as_mut().unwrap();
            let mut new: Vec<Card> = self.kept.iter().chain(&self.drawn).copied().collect();
            new.sort_unstable();
            h.hands[me] = new.try_into().unwrap();
            self.stage = Stage::Playing;
        }
    }

    pub fn top_controls(&mut self, ui: &mut egui::Ui) {
        if ui.button("Nuova mano").clicked() {
            self.reset();
        }
        if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Annulla")).on_hover_text("Ctrl+Z").clicked() {
            self.back();
        }
    }

    /// Finestra compatta: lettura dello schermo, mano, azioni e consiglio uno sotto l'altro.
    pub fn compact(&mut self, ui: &mut egui::Ui) {
        self.frame_input(ui.ctx());
        self.small = true;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| self.top_controls(ui));
            self.vision_ui(ui);
            egui::Frame::new()
                .fill(cards_ui::FELT_TOP)
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(4, 0))
                .show(ui, |ui| self.table_ui(ui));
            ui.add_space(6.0);
            self.side_ui(ui);
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.frame_input(ui.ctx());
        self.small = false;
        let full = ui.available_rect_before_wrap();
        let side_w = (full.width() * 0.4).clamp(400.0, 520.0);
        let split = full.max.x - side_w;
        let left = egui::Rect::from_min_max(full.min, egui::pos2(split, full.max.y));
        let right =
            egui::Rect::from_min_max(egui::pos2(split + 14.0, full.min.y + 10.0), full.max - egui::vec2(10.0, 0.0));
        cards_ui::paint_felt(ui.painter(), left);
        ui.painter().vline(split, full.y_range(), egui::Stroke::new(1.0, Color32::from_gray(60)));
        ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| self.table_ui(ui));
        ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.vision_ui(ui);
                self.side_ui(ui);
            });
        });
    }

    fn table_ui(&mut self, ui: &mut egui::Ui) {
        let white = |a: u8| Color32::from_white_alpha(a);
        ui.add_space(12.0);
        if self.stage == Stage::Setup {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Nuova mano").size(22.0).strong().color(GOLD));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space((ui.available_width() - 360.0).max(0.0) / 2.0);
                    ui.label(RichText::new("Chi parla per primo? (Tab)").color(white(220)));
                    ui.selectable_value(&mut self.me, 0, "Io");
                    ui.selectable_value(&mut self.me, 1, "L'avversario");
                });
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!(
                        "Le tue 5 carte ({} di 5): clicca nella griglia o scrivi, es. 10c, Aq, Jf",
                        self.cards.len()
                    ))
                    .color(white(220)),
                );
                self.entry_hint(ui);
                self.cards_row(ui, &self.cards.clone(), None);
            });
            let cards = self.cards.clone();
            ui.horizontal(|ui| {
                if let Some(c) = picker(ui, "pk_setup", |c| !cards.contains(&c)) {
                    self.cards.push(c);
                }
            });
            if self.cards.len() == 5 {
                self.start();
            }
            return;
        }
        let h = self.hand.clone().unwrap();
        let me = self.me as usize;
        ui.vertical_centered(|ui| {
            let opp = match h.discards[1 - me].map(|m| m.count_ones()) {
                Some(0) => "Avversario · servito".to_string(),
                Some(1) => "Avversario · ha cambiato 1 carta".to_string(),
                Some(k) => format!("Avversario · ha cambiato {k} carte"),
                None => "Avversario".to_string(),
            };
            ui.label(RichText::new(opp).size(16.0).color(white(210)));
            ui.label(RichText::new(format!("Piatto: {} fiche", h.pot())).size(22.0).strong().color(GOLD));
            let phase = match h.phase {
                Phase::Bet1 => "Primo giro di puntate",
                Phase::Draw => "Cambio",
                Phase::Bet2 => "Secondo giro di puntate",
                Phase::Over => "Mano finita",
            };
            ui.label(RichText::new(phase).color(white(200)));
            ui.label(RichText::new(history_text(&h, self.me)).color(white(170)).small());
        });
        ui.add_space(8.0);
        if self.stage == Stage::NewCards {
            let need = 5 - self.kept.len();
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(format!("Carte pescate: {} di {need}", self.drawn.len()))
                        .size(17.0)
                        .strong()
                        .color(Color32::WHITE),
                );
                let shown: Vec<Card> = self.kept.iter().chain(&self.drawn).copied().collect();
                self.cards_row(ui, &shown, None);
                self.entry_hint(ui);
            });
            let used: Vec<Card> = self.kept.iter().chain(&self.drawn).chain(&self.gone).copied().collect();
            let mut pick = None;
            ui.horizontal(|ui| pick = picker(ui, "pk_draw", |c| !used.contains(&c)));
            if let Some(c) = pick {
                self.add_drawn(c);
            }
            return;
        }
        let my_turn = !h.is_over() && h.to_act == self.me;
        let drawing = my_turn && h.phase == Phase::Draw;
        let suggested = drawing.then(|| self.best_discard()).flatten();
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("La tua mano").size(16.0).strong().color(Color32::WHITE));
            let hand = h.hands[me].to_vec();
            let toggled = self.cards_row(ui, &hand, drawing.then_some(suggested.unwrap_or(0)));
            if let Some(i) = toggled {
                self.discard[i] ^= true;
            }
            ui.label(RichText::new(describe(evaluate(&h.hands[me]))).strong().color(white(230)));
            if drawing {
                ui.label(
                    RichText::new(
                        "Clicca le carte che scarti o premi 1-5 (in arancione quelle da scartare secondo il motore)",
                    )
                    .color(white(170))
                    .small(),
                );
                ui.label(
                    RichText::new("Spazio = fai lo scarto consigliato · Invio = conferma le carte scelte")
                        .color(white(170))
                        .small(),
                );
            }
        });
        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            if h.is_over() {
                ui.label(RichText::new("Mano finita").size(18.0).color(white(220)));
                if ui.button(RichText::new("Nuova mano (Invio)").size(18.0)).clicked() {
                    self.reset();
                }
            } else {
                let who =
                    if my_turn { "Cosa hai fatto? (Spazio = il consiglio)" } else { "Cosa ha fatto l'avversario?" };
                ui.label(RichText::new(who).size(18.0).strong().color(Color32::WHITE));
                self.action_buttons(ui, &h, my_turn);
            }
        });
    }

    /// Fila di carte; con `select` le carte sono cliccabili per il cambio (`select` = scarto consigliato).
    fn cards_row(&self, ui: &mut egui::Ui, cs: &[Card], select: Option<u8>) -> Option<usize> {
        let mut clicked = None;
        ui.horizontal(|ui| {
            // Nella finestra compatta le carte sono più piccole.
            let size = if self.small { CARD * 0.72 } else { CARD };
            let w = 5.0 * (size.x + 6.0);
            ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
            for i in 0..5 {
                match cs.get(i) {
                    Some(&c) => {
                        let glow = select.filter(|m| m & (1 << i) != 0).map(|_| Color32::from_rgb(240, 150, 60));
                        let raised = select.is_some() && self.discard[i];
                        if card(ui, size, Some(c), glow, raised, select.is_some(), false).clicked() {
                            clicked = Some(i);
                        }
                    }
                    None => {
                        let (r, _) = ui.allocate_exact_size(size + egui::vec2(0.0, 18.0), egui::Sense::hover());
                        cards_ui::paint_slot(
                            ui.painter(),
                            egui::Rect::from_min_size(r.min + egui::vec2(0.0, 18.0), size),
                            "?",
                        );
                    }
                }
                ui.add_space(6.0);
            }
        });
        clicked
    }

    fn action_buttons(&mut self, ui: &mut egui::Ui, h: &Hand, mine: bool) {
        let big = |s: String| egui::Button::new(RichText::new(s).size(17.0)).min_size(egui::vec2(140.0, 38.0));
        let mut chosen = None;
        ui.horizontal_wrapped(|ui| {
            if h.phase == Phase::Draw {
                if mine {
                    let mask = self.discard.iter().enumerate().fold(0u8, |m, (i, &d)| m | ((d as u8) << i));
                    let label = match mask.count_ones() {
                        0 => "Servito (non cambio) · Invio".to_string(),
                        1 => "Ho cambiato 1 carta · Invio".to_string(),
                        k => format!("Ho cambiato {k} carte · Invio"),
                    };
                    ui.add_space((ui.available_width() - 300.0).max(0.0) / 2.0);
                    if ui.add(big(label)).clicked() {
                        chosen = Some(Action::Draw(mask));
                    }
                    if let Some(m) = self.best_discard() {
                        if ui.add(big("Scarto consigliato".into())).clicked() {
                            self.discard = core::array::from_fn(|i| m & (1 << i) != 0);
                        }
                    }
                } else {
                    ui.add_space((ui.available_width() - 6.0 * 64.0).max(0.0) / 2.0);
                    for k in 0..6u8 {
                        let l = if k == 0 { "Servito · 0".to_string() } else { format!("{k}") };
                        if ui
                            .add(egui::Button::new(RichText::new(l).size(17.0)).min_size(egui::vec2(58.0, 38.0)))
                            .clicked()
                        {
                            chosen = Some(Action::Draw((1u8 << k) - 1));
                        }
                    }
                }
            } else {
                let to_call = h.to_call();
                let can_raise = h.legal().contains(&Action::Raise);
                let n = 1 + (to_call > 0) as usize + can_raise as usize;
                ui.add_space((ui.available_width() - n as f32 * 148.0).max(0.0) / 2.0);
                let bet = if h.phase == Phase::Bet1 { SMALL_BET } else { BIG_BET };
                if to_call > 0 && ui.add(big("Lascia · L".into())).clicked() {
                    chosen = Some(Action::Fold);
                }
                let call = if to_call > 0 { format!("Vede ({to_call}) · V") } else { "Parola · V".into() };
                if ui.add(big(call)).clicked() {
                    chosen = Some(Action::Call);
                }
                if can_raise {
                    let r = if to_call > 0 { format!("Rilancia (+{bet}) · R") } else { format!("Punta {bet} · R") };
                    if ui.add(big(r)).clicked() {
                        chosen = Some(Action::Raise);
                    }
                }
            }
        });
        if !mine && h.phase == Phase::Draw {
            ui.label(
                RichText::new("Quante carte ha cambiato? (tasti 0-5)").color(Color32::from_white_alpha(170)).small(),
            );
        }
        if let Some(a) = chosen {
            self.act(a);
        }
    }

    /// Consiglio del motore per la situazione attuale (ricalcolato solo quando cambia).
    fn current_advice(&mut self) -> Option<Vec<(Action, f32)>> {
        let h = self.hand.as_ref()?;
        if h.is_over() || h.to_act != self.me || self.stage != Stage::Playing {
            return None;
        }
        let n = h.history.len();
        if self.advice.as_ref().map(|a| a.0) != Some(n) {
            let obs = Observation::of(h, self.me);
            self.advice = Some((n, self.engine.advice(&obs)));
        }
        self.advice.as_ref().unwrap().1.clone()
    }

    fn best_discard(&self) -> Option<u8> {
        let (_, adv) = self.advice.as_ref()?;
        adv.as_ref()?.iter().max_by(|a, b| a.1.total_cmp(&b.1)).and_then(|(a, _)| match a {
            Action::Draw(m) => Some(*m),
            _ => None,
        })
    }

    fn side_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Cosa fare");
        let adv = self.current_advice();
        match (&self.hand, self.stage) {
            (_, Stage::Setup) => {
                ui.label("Scegli chi parla per primo e inserisci le tue 5 carte.");
            }
            (_, Stage::NewCards) => {
                ui.label("Inserisci le carte che hai pescato.");
            }
            (Some(h), _) if h.is_over() => {
                ui.label("Mano finita. Premi «Nuova mano» per la prossima.");
            }
            (Some(h), _) if h.to_act != self.me => {
                ui.label("Tocca all'avversario: inserisci cosa ha fatto.");
            }
            (Some(h), _) => {
                let obs = Observation::of(h, self.me);
                hand_facts(ui, &obs);
                match adv {
                    Some(adv) => advice_list(ui, &obs, &adv),
                    None => {
                        ui.label(RichText::new("Situazione mai vista in allenamento: nessun consiglio.").weak());
                    }
                }
            }
            _ => {}
        }
        ui.separator();
        ui.collapsing("Regole usate dal motore", |ui| {
            ui.label(format!(
                "Testa a testa, mazzo da 32 carte (dal 7 all'Asso). Invito di 1 fiche a testa. Primo giro: \
                 puntata e rilanci da {SMALL_BET}; poi il cambio (da 0 a 5 carte); secondo giro: puntata e \
                 rilanci da {BIG_BET}. Al massimo {CAP} puntate per giro. Il colore batte il full, la scala più \
                 bassa è A-7-8-9-10, a parità vince il seme più alto (cuori, quadri, fiori, picche). Chi parla \
                 per primo nel primo giro parla per primo anche dopo il cambio."
            ));
        });
        ui.label(RichText::new("Rispetta il regolamento delle piattaforme su cui giochi.").weak().small());
    }
}

fn action_name(a: Action, to_call: i32) -> String {
    match a {
        Action::Fold => "Lascia".into(),
        Action::Call if to_call > 0 => "Vedi".into(),
        Action::Call => "Parola".into(),
        Action::Raise if to_call > 0 => "Rilancia".into(),
        Action::Raise => "Punta".into(),
        Action::Draw(m) => match m.count_ones() {
            0 => "Resta servito".into(),
            1 => "Cambia 1 carta".into(),
            k => format!("Cambia {k} carte"),
        },
    }
}

/// Cronologia leggibile delle azioni della mano.
fn history_text(h: &Hand, me: u8) -> String {
    let mut sim = Hand::new(h.deck);
    let mut parts = Vec::new();
    for &a in &h.history {
        let who = if sim.to_act == me { "Tu" } else { "Avversario" };
        let to_call = sim.to_call();
        let real = match a {
            Action::Draw(_) => Action::Draw(h.discards[sim.to_act as usize].unwrap_or(0)),
            a => a,
        };
        parts.push(format!("{who}: {}", action_name(real, to_call).to_lowercase()));
        sim.apply(real);
    }
    if parts.is_empty() {
        "Nessuna azione ancora".into()
    } else {
        parts.join(" · ")
    }
}

fn hand_facts(ui: &mut egui::Ui, obs: &Observation) {
    let d = summary(&obs.hand);
    match obs.phase {
        Phase::Bet1 | Phase::Draw => {
            ui.label(format!(
                "La tua mano è migliore del {:.0}% delle mani di partenza.",
                start_quantile(d.max_ev()) * 100.0
            ));
        }
        _ => {
            ui.label(format!(
                "La tua mano batte il {:.0}% delle mani che si hanno di solito dopo il cambio.",
                strength(&obs.hand) * 100.0
            ));
            if let Some(k) = obs.opp_drawn {
                let hint = match k {
                    0 => "è rimasto servito: o ha una mano fatta o bluffa",
                    1 => "ha cambiato 1 carta: doppia coppia, o un colore o una scala da completare",
                    2 => "ha cambiato 2 carte: spesso un tris",
                    3 => "ha cambiato 3 carte: di solito una coppia",
                    _ => "ha cambiato 4 o 5 carte: partiva da una mano debole",
                };
                ui.label(RichText::new(format!("L'avversario {hint}.")).weak());
            }
        }
    }
    if obs.to_call > 0 {
        let need = obs.to_call as f64 / (obs.pot + obs.to_call) as f64;
        ui.label(
            RichText::new(format!(
                "Per vedere metti {} fiche in un piatto che diventa {}: ti basta vincere il {:.0}% delle volte.",
                obs.to_call,
                obs.pot + obs.to_call,
                need * 100.0
            ))
            .weak(),
        );
    }
    ui.add_space(6.0);
}

fn advice_list(ui: &mut egui::Ui, obs: &Observation, adv: &[(Action, f32)]) {
    let mut sorted: Vec<(Action, f32)> = adv.iter().copied().filter(|x| x.1 >= 0.01).collect();
    sorted.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (i, &(a, p)) in sorted.iter().enumerate() {
        let mut name = action_name(a, obs.to_call);
        if let Action::Draw(m) = a {
            if m != 0 {
                let kept: Vec<Card> = (0..5).filter(|i| m & (1 << i) == 0).map(|i| obs.hand[i]).collect();
                name += &format!(" (tieni {})", if kept.is_empty() { "niente".into() } else { cards_text(&kept) });
            }
        }
        if i == 0 {
            ui.label(RichText::new(name.clone()).size(24.0).strong().color(BEST));
        }
        ui.horizontal(|ui| {
            let col = if i == 0 { BEST } else { Color32::from_gray(150) };
            ui.add(egui::ProgressBar::new(p).desired_width(110.0).fill(col).text(format!("{:.0}%", p * 100.0)));
            ui.label(RichText::new(name).color(if i == 0 { Color32::WHITE } else { Color32::from_gray(200) }));
        });
    }
    if sorted.len() > 1 {
        ui.label(
            RichText::new(
                "Il motore a volte mescola le scelte (per esempio bluffa ogni tanto) per non essere \
                 prevedibile: le percentuali dicono quanto spesso sceglie ciascuna azione qui.",
            )
            .weak()
            .small(),
        );
    }
}

/// "A di picche, 10 di cuori" (il carattere dei semi non c'è nel font dell'app).
fn cards_text(cs: &[Card]) -> String {
    cs.iter()
        .map(|&c| format!("{} di {}", RANK_SHORT[rank(c) as usize], SUIT_NAMES[suit(c) as usize].to_lowercase()))
        .collect::<Vec<_>>()
        .join(", ")
}
