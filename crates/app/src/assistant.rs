//! Assistente: inserisci a mano briscola, carte in mano, pescate e giocate; il motore consiglia la mossa.

use crate::cards_ui::{self, BEST, CARD_SIZE, GOLD};
use briscola_core::ai::{evaluate, EngineConfig, MoveEval};
use briscola_core::cards::{self, Card, CardSet};
use briscola_core::{Observation, Trick};
use eframe::egui::{
    self, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use std::sync::mpsc::{channel, Receiver};

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
                if self.trump_card_owner == Some(OPP) {
                    u.insert(self.trump_card.unwrap());
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
        let winner = if cards::beats(c, lead, cards::suit(trump_card)) {
            p
        } else {
            self.leader
        };
        let pts = cards::points(lead) + cards::points(c);
        self.points[winner as usize] += pts;
        self.played.insert(lead);
        self.played.insert(c);
        self.tricks += 1;
        let trick = Trick {
            leader: self.leader,
            lead,
            follow: c,
            winner,
            points: pts,
        };
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
                    out.push(format!(
                        "Tagli con la briscola: prendi {} e porti a casa {pts} punti.",
                        cards::name(t)
                    ));
                } else {
                    out.push(format!(
                        "Prendi la presa con {}: incassi {pts} punti.",
                        cards::name(c)
                    ));
                }
            } else if pts == 0 {
                out.push(
                    "Lasci la presa: sul tavolo non ci sono punti, scarti senza perdere nulla."
                        .into(),
                );
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
                "Apri con un carico: secondo le simulazioni l'avversario difficilmente lo prende o lo taglia.".to_string()
            } else {
                format!("Apri con una figura ({p} punti): rischio contenuto, tieni i lisci per dopo.")
            });
        }
    }
    if let Some(second) = evals.get(1) {
        let d = best.win_prob - second.win_prob;
        if d < 0.015 {
            out.push(format!(
                "Scelta quasi indifferente: {} vale praticamente lo stesso.",
                cards::name(second.card)
            ));
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
    let trumps: Vec<Card> = out_trumps
        .iter()
        .filter(|&x| cards::suit(x) == trump)
        .collect();
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
        out.push(format!(
            "Hai {mine} punti: te ne mancano {} per arrivare a 61.",
            61 - mine
        ));
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
    history: Vec<(Position, usize)>,
    tricks: Vec<Trick>,
    generation: u64,
    analysis: Option<Analysis>,
    pending: Option<Receiver<Analysis>>,
    entry: String,
    entry_error: Option<String>,
    focus_entry: bool,
}

impl Default for Assistant {
    fn default() -> Self {
        Assistant {
            pos: Position::default(),
            history: Vec::new(),
            tricks: Vec::new(),
            generation: 0,
            analysis: None,
            pending: None,
            entry: String::new(),
            entry_error: None,
            focus_entry: true,
        }
    }
}

impl Assistant {
    fn changed(&mut self) {
        self.generation += 1;
        self.analysis = None;
        self.pending = None;
        self.entry.clear();
        self.entry_error = None;
        self.focus_entry = true;
    }

    fn save(&mut self) {
        self.history.push((self.pos, self.tricks.len()));
    }

    fn undo(&mut self) {
        if let Some((p, n)) = self.history.pop() {
            self.pos = p;
            self.tricks.truncate(n);
            self.changed();
        }
    }

    fn reset(&mut self) {
        *self = Assistant::default();
    }

    fn enter(&mut self, c: Card) -> bool {
        let before = self.pos;
        match self.pos.enter(c) {
            Some(trick) => {
                self.history.push((before, self.tricks.len()));
                self.tricks.extend(trick);
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
                self.analysis = Some(a);
                self.pending = None;
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
                let evals = evaluate(&obs, &EngineConfig::strength(samples), &mut rng);
                let reasons = explain(&obs, &evals);
                let _ = tx.send(Analysis {
                    evals,
                    reasons,
                    samples,
                });
            });
            self.pending = Some(rx);
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    fn eval_of(&self, c: Card) -> Option<(usize, MoveEval)> {
        let a = self.analysis.as_ref()?;
        a.evals
            .iter()
            .position(|e| e.card == c)
            .map(|i| (i, a.evals[i]))
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
                    .add_enabled(
                        !self.history.is_empty(),
                        egui::Button::new("Annulla ultima azione"),
                    )
                    .on_hover_text("Ctrl+Z");
                if undo.clicked() {
                    self.undo();
                }
            });
        });
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            self.undo();
        }
        ui.add_space(10.0);

        // Avversario.
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!(
                    "Avversario · {} carte in mano · {} punti",
                    pos.opp_hand_len, pos.points[1]
                ))
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
                        let look = cards_ui::Look {
                            dim: pos.deck_len == 0,
                            ..Default::default()
                        };
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
            let (opp_c, my_c) = if pos.leader == ME {
                (None, pos.table)
            } else {
                (pos.table, None)
            };
            for (label, c, waiting) in [
                ("Avversario", opp_c, step == Step::OppMove),
                ("Tu", my_c, step == Step::MyMove),
            ] {
                ui.vertical(|ui| {
                    ui.label(RichText::new(label).color(Color32::from_white_alpha(200)));
                    let (rect, _) = ui.allocate_exact_size(CARD_SIZE, Sense::hover());
                    match c {
                        Some(c) => cards_ui::paint_face(ui.painter(), rect, c, Default::default()),
                        None => {
                            cards_ui::paint_slot(
                                ui.painter(),
                                rect,
                                if waiting { "tocca a te" } else { "" },
                            );
                            if waiting {
                                let t = ui.input(|i| i.time) as f32;
                                let a = 0.5 + 0.5 * (t * 3.0).sin();
                                ui.painter().rect_stroke(
                                    rect,
                                    10.0,
                                    Stroke::new(2.0, GOLD.gamma_multiply(0.4 + 0.6 * a)),
                                    StrokeKind::Inside,
                                );
                                ui.ctx()
                                    .request_repaint_after(std::time::Duration::from_millis(40));
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
            let (mine, theirs) = if t.leader == ME {
                (t.lead, t.follow)
            } else {
                (t.follow, t.lead)
            };
            let who = if t.winner == ME {
                "hai preso tu"
            } else {
                "ha preso l'avversario"
            };
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
            ui.label(
                RichText::new("La tua mano")
                    .size(16.0)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.horizontal(|ui| {
                ui.set_min_height(CARD_SIZE.y + 40.0);
                let hand = pos.hand.to_vec();
                center(ui, hand.len() as f32 * (CARD_SIZE.x + 14.0));
                for c in hand {
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
                    });
                    ui.add_space(8.0);
                }
            });
            if step == Step::MyMove {
                ui.label(
                    RichText::new("Clicca la carta che giochi (anche se non è quella consigliata)")
                        .color(Color32::from_white_alpha(170)),
                );
            }
        });
        if let Some(c) = clicked {
            self.enter(c);
        }
    }

    /// Pannello di destra: richiesta di input oppure consiglio del motore.
    pub fn side_ui(&mut self, ui: &mut egui::Ui, samples: &mut u32) {
        self.poll(*samples, &ui.ctx().clone());
        let step = self.pos.step();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                match step {
                    Step::Trump => {
                        title(
                            ui,
                            "Qual è la briscola?",
                            "Indica la carta scoperta sotto il mazzo.",
                        );
                        self.picker_and_entry(ui);
                    }
                    Step::First => {
                        title(
                            ui,
                            "Chi gioca per primo?",
                            "Chi apre la prima presa della partita.",
                        );
                        ui.horizontal(|ui| {
                            let big = |t: &str| {
                                egui::Button::new(RichText::new(t).size(17.0))
                                    .min_size(vec2(200.0, 44.0))
                            };
                            if ui.add(big("Comincio io")).clicked() {
                                self.save();
                                self.pos.set_first(ME);
                                self.changed();
                            }
                            if ui.add(big("Comincia l'avversario")).clicked() {
                                self.save();
                                self.pos.set_first(OPP);
                                self.changed();
                            }
                        });
                    }
                    Step::Draw(n) => {
                        if self.pos.tricks == 0 {
                            title(
                                ui,
                                "Le tue tre carte",
                                &format!("Indica le carte che hai in mano: ne mancano {n}."),
                            );
                        } else {
                            title(
                                ui,
                                "Che carta hai pescato?",
                                "Il mazzo l'ha data a te dopo l'ultima presa.",
                            );
                        }
                        self.picker_and_entry(ui);
                    }
                    Step::OppMove => {
                        let sub = if self.pos.table.is_some() {
                            "Ha risposto alla tua carta."
                        } else {
                            "Apre lui questa presa."
                        };
                        title(ui, "Che carta ha giocato l'avversario?", sub);
                        self.picker_and_entry(ui);
                    }
                    Step::MyMove => {
                        self.advice(ui, samples);
                        ui.add_space(8.0);
                        ui.label(RichText::new("Oppure scrivi la carta che hai giocato:").weak());
                        self.entry_field(ui);
                    }
                    Step::Over => {
                        let mine = self.pos.points[ME as usize];
                        let (t, col) = match mine {
                            p if p > 60 => (format!("Hai vinto {mine} a {}!", 120 - mine), BEST),
                            60 => ("Pareggio 60 a 60".to_string(), Color32::YELLOW),
                            _ => (
                                format!("Hai perso {mine} a {}", 120 - mine),
                                Color32::LIGHT_RED,
                            ),
                        };
                        ui.add_space(10.0);
                        ui.label(RichText::new(t).size(28.0).strong().color(col));
                        ui.add_space(8.0);
                        if ui
                            .button(RichText::new("Nuova partita").size(17.0))
                            .clicked()
                        {
                            self.reset();
                        }
                    }
                }
                ui.add_space(14.0);
                ui.separator();
                ui.heading("Carte uscite");
                let mut gone = self.pos.played;
                if let Some(t) = self.pos.table {
                    gone.insert(t);
                }
                tracker(ui, gone, self.pos.hand);
            });
    }

    fn picker_and_entry(&mut self, ui: &mut egui::Ui) {
        if let Some(c) = cards_ui::picker(ui, "assist_picker", self.pos.allowed()) {
            self.enter(c);
        }
        ui.add_space(8.0);
        self.entry_field(ui);
    }

    fn entry_field(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.entry)
                    .hint_text("da tastiera: 3c, AD, Rs, cavallo bastoni…")
                    .desired_width(260.0),
            );
            if self.focus_entry {
                resp.request_focus();
                self.focus_entry = false;
            }
            let submit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Inserisci").clicked() || submit) && !self.entry.trim().is_empty() {
                match cards_ui::parse_card(&self.entry) {
                    Some(c) if self.enter(c) => {}
                    Some(c) => {
                        self.entry_error =
                            Some(format!("{} non è possibile adesso", cards::name(c)));
                        self.focus_entry = true;
                    }
                    None => {
                        self.entry_error =
                            Some("Carta non riconosciuta: scrivi valore e seme, es. 3c".into());
                        self.focus_entry = true;
                    }
                }
            }
        });
        if let Some(e) = &self.entry_error {
            ui.label(RichText::new(e).color(Color32::LIGHT_RED));
        }
        ui.label(
            RichText::new("Valori: A 2 3 4 5 6 7 F C R · Semi: d c s b · Ctrl+Z annulla")
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
                        for s in [500, 2000, 6000] {
                            ui.selectable_value(samples, s, precision_label(s));
                        }
                    })
                    .response
                    .on_hover_text("Quante distribuzioni delle carte coperte simula il motore");
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
                        ui.label(
                            RichText::new(cards::name(best.card))
                                .size(24.0)
                                .strong()
                                .color(Color32::WHITE),
                        );
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
                ui.painter().rect_filled(
                    fill,
                    4.0,
                    if i == 0 {
                        BEST
                    } else {
                        Color32::from_gray(150)
                    },
                );
                ui.label(format!("{:>3.0}%", e.win_prob * 100.0));
                ui.label(RichText::new(format!("{:.0} pt", e.exp_points)).weak());
            });
        }
        ui.label(
            RichText::new(format!(
                "Stime su {} distribuzioni delle carte che non vedi, usando solo ciò che sai.",
                a.samples
            ))
            .small()
            .weak(),
        );
    }
}

fn precision_label(s: u32) -> String {
    match s {
        0..=999 => format!("Veloce ({s})"),
        1000..=2999 => format!("Normale ({s})"),
        _ => format!("Massima ({s})"),
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
    for (i, name) in ["Briscola", "Chi inizia", "Mano iniziale", "Partita"]
        .iter()
        .enumerate()
    {
        let done = i < current;
        let active = i == current;
        let (fill, text) = if active {
            (GOLD, Color32::from_rgb(40, 30, 10))
        } else if done {
            (Color32::from_rgb(40, 110, 70), Color32::WHITE)
        } else {
            (
                Color32::from_black_alpha(60),
                Color32::from_white_alpha(150),
            )
        };
        let label = format!("{} {}", i + 1, name);
        egui::Frame::new()
            .fill(fill)
            .corner_radius(12.0)
            .inner_margin(vec2(10.0, 4.0))
            .show(ui, |ui| {
                ui.label(RichText::new(label).color(text).strong());
            });
    }
}

pub fn score_box(ui: &mut egui::Ui, points: [u8; 2], tricks: u8, opp: &str) {
    egui::Frame::new()
        .fill(Color32::from_black_alpha(70))
        .corner_radius(10.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(160.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(format!("Tu  {}", points[0]))
                        .size(24.0)
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.label(
                    RichText::new(format!("{opp}  {}", points[1]))
                        .size(18.0)
                        .color(Color32::from_white_alpha(200)),
                );
                ui.label(
                    RichText::new(format!("Presa {} di 20", (tricks + 1).min(20)))
                        .color(Color32::from_white_alpha(150)),
                );
                let (rect, _) = ui.allocate_exact_size(vec2(150.0, 8.0), Sense::hover());
                ui.painter().rect_filled(rect, 4.0, Color32::from_gray(60));
                let mut mine = rect;
                mine.set_width(rect.width() * points[0] as f32 / 120.0);
                ui.painter().rect_filled(mine, 4.0, BEST);
                let x = rect.left() + rect.width() * 61.0 / 120.0;
                ui.painter()
                    .vline(x, rect.expand(3.0).y_range(), Stroke::new(1.5, GOLD));
            });
        });
}

/// Etichetta sotto una carta della mano con la probabilità di vittoria.
pub fn badge(ui: &mut egui::Ui, best: bool, e: MoveEval) {
    let text = format!("{:.0}%", e.win_prob * 100.0);
    let (fill, col) = if best {
        (BEST, Color32::from_rgb(10, 40, 20))
    } else {
        (Color32::from_black_alpha(90), Color32::WHITE)
    };
    let w = CARD_SIZE.x;
    let (rect, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
    let pill = Rect::from_center_size(rect.center(), vec2(64.0, 20.0));
    ui.painter().rect_filled(pill, 10.0, fill);
    ui.painter().text(
        pill.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(14.0),
        col,
    );
    ui.label(
        RichText::new(format!("{:.0} punti attesi", e.exp_points))
            .small()
            .color(Color32::from_white_alpha(170)),
    );
}

/// Griglia delle carte uscite: barrate se uscite, evidenziate se in mano.
pub fn tracker(ui: &mut egui::Ui, gone: CardSet, mine: CardSet) {
    egui::Grid::new("tracker")
        .min_col_width(20.0)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            for s in 0..4u8 {
                let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                cards_ui::paint_suit(ui.painter(), r.center(), 18.0, s);
                for k in cards_ui::RANK_ORDER {
                    let c = cards::make(s, k);
                    let txt = RichText::new(cards::RANK_SHORT[k as usize])
                        .monospace()
                        .size(15.0);
                    let txt = if gone.contains(c) {
                        txt.strikethrough().color(Color32::from_gray(90))
                    } else if mine.contains(c) {
                        txt.color(Color32::BLACK)
                            .background_color(Color32::LIGHT_BLUE)
                    } else {
                        txt.color(cards_ui::suit_color_light(s)).strong()
                    };
                    ui.label(txt);
                }
                ui.end_row();
            }
        });
    let remaining: u32 = CardSet::FULL
        .minus(gone)
        .iter()
        .map(|c| cards::points(c) as u32)
        .sum();
    ui.label(format!("Punti ancora in gioco: {remaining}"));
    ui.label(
        RichText::new("Barrate = uscite, evidenziate = in mano a te")
            .weak()
            .small(),
    );
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

    #[test]
    fn rejects_impossible_cards() {
        let mut a = Position::default();
        a.enter(cards::make(0, 0));
        a.set_first(ME);
        assert!(
            a.enter(cards::make(0, 0)).is_none(),
            "la briscola scoperta non si pesca"
        );
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
        assert_eq!(
            evals[0].card,
            cards::make(1, 0),
            "l'Asso di Coppe prende il Tre"
        );
        let r = explain(&obs, &evals);
        assert!(r[0].starts_with("Prendi la presa"), "{r:?}");
    }
}
