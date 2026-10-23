//! Blackjack: consigliere. Inserisci la carta scoperta del banco e le tue carte, l'app calcola
//! il valore atteso esatto di ogni azione per le regole del tavolo e ti dice cosa fare.

use crate::cards_ui::{self, BEST, GOLD};
use blackjack_core::cards::{idx_label, Counts, Hand};
use blackjack_core::ev::{ActionEvs, Analyzer, Situation};
use blackjack_core::strategy::Legal;
use blackjack_core::{Action, Card, Rules};
use eframe::egui::{
    self, epaint::Shadow, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Vec2,
};
use serde::{Deserialize, Serialize};

const CARD: Vec2 = Vec2::new(92.0, 132.0);
const OVERLAP: f32 = 40.0;
const PAPER: Color32 = Color32::from_rgb(252, 250, 245);
const RED: Color32 = Color32::from_rgb(200, 36, 44);
const BLACK: Color32 = Color32::from_rgb(28, 28, 32);

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct BjSettings {
    /// Regole predefinite scelte; `None` = personalizzate.
    pub preset: Option<usize>,
    pub rules: Rules,
}

impl Default for BjSettings {
    fn default() -> Self {
        BjSettings { preset: Some(0), rules: Rules::preset(0) }
    }
}

#[derive(Clone, Default)]
struct AHand {
    /// Indici di valore: 0 = asso, 9 = dieci o figura.
    cards: Vec<usize>,
    from_split: bool,
    doubled: bool,
    /// Il giocatore ha finito questa mano (stai, resa, raddoppio completato, sballato).
    done: bool,
}

impl AHand {
    fn hand(&self) -> Hand {
        Hand::of(&self.cards)
    }
}

#[derive(Clone, Default)]
struct Table {
    up: Option<usize>,
    hands: Vec<AHand>,
    active: usize,
}

/// Cosa dire al giocatore nella situazione corrente.
enum Advice {
    NeedDealer,
    NeedCards,
    Bust,
    Blackjack,
    TwentyOne,
    /// Mano finita, si passa alla prossima.
    Done,
    Play {
        evs: ActionEvs,
        legal: Legal,
        cards: Vec<usize>,
    },
}

pub struct BlackjackUi {
    pub settings: BjSettings,
    table: Table,
    undo: Vec<Table>,
    /// Le prossime carte vanno al banco invece che alla mano.
    to_dealer: bool,
    /// Consiglio memorizzato: si ricalcola solo quando cambia il tavolo.
    cached: Option<(u64, Advice)>,
    version: u64,
}

impl BlackjackUi {
    pub fn new(settings: BjSettings) -> BlackjackUi {
        BlackjackUi {
            settings,
            table: Table { hands: vec![AHand::default()], ..Default::default() },
            undo: Vec::new(),
            to_dealer: true,
            cached: None,
            version: 0,
        }
    }

    fn changed(&mut self) {
        self.version += 1;
    }

    fn snapshot(&mut self) {
        self.undo.push(self.table.clone());
        self.undo.truncate(200);
    }

    fn new_hand(&mut self) {
        self.snapshot();
        self.table = Table { hands: vec![AHand::default()], ..Default::default() };
        self.to_dealer = true;
        self.changed();
    }

    fn undo(&mut self) {
        if let Some(t) = self.undo.pop() {
            self.table = t;
            self.to_dealer = self.table.up.is_none();
            self.changed();
        }
    }

    fn add_card(&mut self, i: usize) {
        self.snapshot();
        if self.to_dealer || self.table.up.is_none() {
            self.table.up = Some(i);
            self.to_dealer = false;
        } else {
            let rules = self.settings.rules;
            let t = &mut self.table;
            let n = t.hands.len() as u8;
            let h = &mut t.hands[t.active];
            if h.done {
                return;
            }
            h.cards.push(i);
            let hand = h.hand();
            let split_aces = h.from_split && h.cards[0] == 0 && rules.one_card_split_aces && h.cards.len() == 2;
            let pair_can_resplit = Legal::of(&rules, &h.cards, Situation { hands: n, from_split: h.from_split }).split;
            if hand.total() >= 21 || h.doubled || (split_aces && !pair_can_resplit) {
                h.done = true;
            }
            self.next_if_done();
        }
        self.changed();
    }

    /// Passa alla prima mano non finita.
    fn next_if_done(&mut self) {
        let t = &mut self.table;
        if t.hands[t.active].done && t.active + 1 < t.hands.len() {
            t.active += 1;
        }
    }

    fn split(&mut self) {
        self.snapshot();
        let t = &mut self.table;
        let h = &mut t.hands[t.active];
        let c = h.cards.pop().unwrap();
        h.from_split = true;
        t.hands.insert(t.active + 1, AHand { cards: vec![c], from_split: true, ..Default::default() });
        self.changed();
    }

    fn double(&mut self) {
        self.snapshot();
        let t = &mut self.table;
        t.hands[t.active].doubled = true;
        self.changed();
    }

    /// Stai o arrenditi: la mano è finita.
    fn finish_hand(&mut self) {
        self.snapshot();
        let t = &mut self.table;
        t.hands[t.active].done = true;
        self.next_if_done();
        self.changed();
    }

    fn advice(&mut self) -> &Advice {
        if self.cached.as_ref().is_none_or(|c| c.0 != self.version) {
            let a = self.compute_advice();
            self.cached = Some((self.version, a));
        }
        &self.cached.as_ref().unwrap().1
    }

    fn compute_advice(&self) -> Advice {
        let rules = self.settings.rules;
        let t = &self.table;
        let Some(up) = t.up else { return Advice::NeedDealer };
        let h = &t.hands[t.active];
        if h.cards.len() < 2 {
            return Advice::NeedCards;
        }
        let hand = h.hand();
        if hand.busted() {
            return Advice::Bust;
        }
        if hand.is_blackjack() && t.hands.len() == 1 {
            return Advice::Blackjack;
        }
        if h.done {
            return Advice::Done;
        }
        if hand.total() == 21 {
            return Advice::TwentyOne;
        }
        if h.doubled {
            return Advice::NeedCards;
        }
        // Carte non viste: sabot pieno meno tutte le carte sul tavolo.
        let mut c = Counts::full(rules.decks);
        let mut take = |i: usize| {
            if c.0[i] > 0 {
                c.remove(i);
            }
        };
        take(up);
        for h in &t.hands {
            h.cards.iter().for_each(|&i| take(i));
        }
        let sit = Situation { hands: t.hands.len() as u8, from_split: h.from_split };
        let legal = Legal::of(&rules, &h.cards, sit);
        let mut a = Analyzer::new(rules, up);
        let evs = a.analyze(&c, &h.cards, sit);
        Advice::Play { evs, legal, cards: h.cards.clone() }
    }

    // ------------------------------------------------------------------ interfaccia

    pub fn top_controls(&mut self, ui: &mut egui::Ui) {
        let before = self.settings.rules;
        egui::ComboBox::from_id_salt("bj_preset")
            .selected_text(self.settings.preset.map_or("Regole personalizzate", |p| Rules::PRESETS[p]))
            .show_ui(ui, |ui| {
                for (i, name) in Rules::PRESETS.iter().enumerate() {
                    if ui.selectable_label(self.settings.preset == Some(i), *name).clicked() {
                        self.settings.preset = Some(i);
                        self.settings.rules = Rules::preset(i);
                    }
                }
            })
            .response
            .on_hover_text("Le regole del tavolo su cui giochi: la mossa giusta dipende da queste");
        if ui.button("Nuova mano").on_hover_text("Invio").clicked() {
            self.new_hand();
        }
        if ui
            .add_enabled(!self.undo.is_empty(), egui::Button::new("Annulla"))
            .on_hover_text("Ctrl+Z o Backspace")
            .clicked()
        {
            self.undo();
        }
        if self.settings.rules != before {
            self.changed();
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, _now: f64) {
        self.keyboard(ui);
        let full = ui.available_rect_before_wrap();
        let side_w = (full.width() * 0.36).clamp(380.0, 480.0);
        let split = full.max.x - side_w;
        let left = Rect::from_min_max(full.min, pos2(split, full.max.y));
        let right = Rect::from_min_max(pos2(split + 14.0, full.min.y + 10.0), full.max - vec2(10.0, 0.0));
        cards_ui::paint_felt(ui.painter(), left);
        ui.painter().vline(split, full.y_range(), Stroke::new(1.0, Color32::from_gray(60)));
        ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| self.table_ui(ui));
        ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| self.side_ui(ui));
    }

    fn keyboard(&mut self, ui: &mut egui::Ui) {
        use egui::Key;
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        // Un pulsante cliccato prende il fuoco e reagirebbe a Invio e spazio: lo si libera,
        // così la tastiera serve solo a inserire le carte.
        ui.ctx().memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        let keys = [
            (Key::A, 0),
            (Key::Num1, 0),
            (Key::Num2, 1),
            (Key::Num3, 2),
            (Key::Num4, 3),
            (Key::Num5, 4),
            (Key::Num6, 5),
            (Key::Num7, 6),
            (Key::Num8, 7),
            (Key::Num9, 8),
            (Key::Num0, 9),
            (Key::T, 9),
            (Key::J, 9),
            (Key::Q, 9),
            (Key::K, 9),
        ];
        let (card, undo, new) = ui.input(|i| {
            let card = keys.iter().find(|(k, _)| i.key_pressed(*k) && !i.modifiers.command).map(|k| k.1);
            let undo = (i.modifiers.command && i.key_pressed(Key::Z)) || i.key_pressed(Key::Backspace);
            (card, undo, i.key_pressed(Key::Enter))
        });
        if let Some(c) = card {
            self.add_card(c);
        }
        if undo {
            self.undo();
        }
        if new {
            self.new_hand();
        }
    }

    fn table_ui(&mut self, ui: &mut egui::Ui) {
        let white = |a: u8| Color32::from_white_alpha(a);
        let rules = self.settings.rules;
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!(
                    "Il banco {} sul 17 morbido · Blackjack paga {}",
                    if rules.hit_soft17 { "chiede carta" } else { "sta" },
                    if rules.blackjack_pays < 1.4 { "6 a 5" } else { "3 a 2" }
                ))
                .color(GOLD),
            );
        });
        ui.add_space(10.0);

        // Banco: clic per correggere la carta scoperta.
        let dealer_target = self.to_dealer || self.table.up.is_none();
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Carta scoperta del banco").size(17.0).color(white(220)));
            ui.add_space(6.0);
            let resp = match self.table.up {
                Some(i) => cards_row(ui, &[i], dealer_target.then_some(GOLD), 0),
                None => slot(ui, dealer_target),
            };
            if resp.on_hover_text("Clic per cambiarla").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.to_dealer = true;
            }
        });
        ui.add_space(26.0);

        // Mani del giocatore.
        let hand_target = !dealer_target;
        let mut select = None;
        ui.horizontal(|ui| {
            let w: f32 = self.table.hands.iter().map(|h| hand_width(h.cards.len()) + 30.0).sum();
            ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
            for (i, h) in self.table.hands.iter().enumerate() {
                let active = i == self.table.active;
                ui.vertical(|ui| {
                    let mut title =
                        if self.table.hands.len() > 1 { format!("Mano {}", i + 1) } else { "Le tue carte".into() };
                    if !h.cards.is_empty() {
                        title += &format!(" · {}", hand_label(h.hand()));
                    }
                    if h.doubled {
                        title += " · raddoppiata";
                    }
                    let col = if active && hand_target { GOLD } else { white(220) };
                    ui.label(RichText::new(title).size(16.0).color(col));
                    ui.add_space(6.0);
                    let resp = if h.cards.is_empty() {
                        slot(ui, active && hand_target)
                    } else {
                        cards_row(ui, &h.cards, (active && hand_target).then_some(GOLD), i)
                    };
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        select = Some(i);
                    }
                });
                ui.add_space(30.0);
            }
        });
        if let Some(i) = select {
            self.table.active = i;
            self.to_dealer = false;
            self.changed();
        }
        ui.add_space(26.0);

        // Tastierino delle carte.
        ui.vertical_centered(|ui| {
            let target = if dealer_target {
                "Clicca la carta scoperta del banco".to_string()
            } else if self.table.hands.len() > 1 {
                format!("Clicca le carte della mano {}", self.table.active + 1)
            } else {
                "Clicca le tue carte, anche quelle che peschi dopo".to_string()
            };
            ui.label(RichText::new(target).size(16.0).color(Color32::WHITE));
            ui.add_space(6.0);
            let mut clicked = None;
            ui.horizontal(|ui| {
                let w = 10.0 * 58.0;
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                for i in [0usize, 1, 2, 3, 4, 5, 6, 7, 8, 9] {
                    let label = if i == 9 { "10 J Q K".to_string() } else { idx_label(i).to_string() };
                    let size = if i == 9 { vec2(84.0, 50.0) } else { vec2(46.0, 50.0) };
                    let b = egui::Button::new(RichText::new(label).size(if i == 9 { 15.0 } else { 20.0 }).strong())
                        .min_size(size);
                    if ui.add(b).clicked() {
                        clicked = Some(i);
                    }
                }
            });
            if let Some(i) = clicked {
                self.add_card(i);
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new("Tastiera: A o 1 asso, 2-9, 0 T J Q K per il 10 · Backspace annulla · Invio nuova mano")
                    .small()
                    .color(white(170)),
            );
        });
    }

    fn side_ui(&mut self, ui: &mut egui::Ui) {
        let rules = self.settings.rules;
        let n_hands = self.table.hands.len();
        let more_hands = self.table.active + 1 < n_hands;
        let up = self.table.up;
        let mut act: Option<&str> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Cosa fare");
            ui.add_space(6.0);
            match self.advice() {
                Advice::NeedDealer => {
                    ui.label(RichText::new("Inserisci la carta scoperta del banco.").size(17.0));
                }
                Advice::NeedCards => {
                    ui.label(RichText::new("Inserisci le tue carte.").size(17.0));
                }
                Advice::Bust => {
                    big(ui, "Sballato", Color32::from_rgb(255, 130, 120));
                }
                Advice::Blackjack => {
                    big(ui, "Blackjack!", BEST);
                    ui.label("Si paga subito, a meno che il banco non abbia blackjack anche lui.");
                }
                Advice::TwentyOne => big(ui, "Stai: hai 21", BEST),
                Advice::Done => {
                    ui.label(RichText::new("Mano finita.").size(17.0));
                }
                Advice::Play { evs, legal, cards } => {
                    let ranked = evs.ranked();
                    let (best, best_ev) = ranked[0];
                    big(ui, best.label(), action_color(best));
                    ui.label(RichText::new(explain(best, best_ev)).color(Color32::from_white_alpha(200)));
                    // Se l'azione migliore non è sempre possibile (raddoppio, resa), cosa fare altrimenti.
                    if matches!(best, Action::Double | Action::Surrender) {
                        if let Some(&(alt, _)) = ranked.iter().find(|r| matches!(r.0, Action::Hit | Action::Stand)) {
                            ui.label(
                                RichText::new(format!("Se il tavolo non lo permette: {}", alt.label().to_lowercase()))
                                    .color(Color32::from_white_alpha(200)),
                            );
                        }
                    }
                    ui.add_space(10.0);
                    ui.label(RichText::new("Valore atteso di ogni scelta").strong());
                    egui::Grid::new("bj_evs").num_columns(3).spacing([14.0, 4.0]).show(ui, |ui| {
                        for (a, ev) in &ranked {
                            let col = if *a == best { BEST } else { Color32::from_white_alpha(210) };
                            ui.label(RichText::new(a.label()).color(col));
                            ui.label(RichText::new(signed(*ev, 3)).color(col).monospace());
                            ui.label(RichText::new(per_euro(*ev)).small().weak());
                            ui.end_row();
                        }
                    });
                    if let Some(up) = up {
                        if up == 0 && rules.peek && cards.len() == 2 && n_hands == 1 {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("Assicurazione: no, non conviene mai senza contare le carte.")
                                    .color(Color32::from_white_alpha(200)),
                            );
                        }
                    }
                    ui.add_space(10.0);
                    // Pulsanti per dire all'app cosa hai fatto quando cambia il tavolo.
                    ui.horizontal_wrapped(|ui| {
                        if legal.split && ui.button("Ho diviso").clicked() {
                            act = Some("split");
                        }
                        if legal.double && ui.button("Ho raddoppiato").clicked() {
                            act = Some("double");
                        }
                        if more_hands
                            && ui
                                .button("Ho finito questa mano")
                                .on_hover_text("Stai o arrenditi: passa alla mano successiva")
                                .clicked()
                        {
                            act = Some("finish");
                        }
                    });
                    ui.label(
                        RichText::new("Se chiedi carta, cliccala sul tastierino: il consiglio si aggiorna.")
                            .small()
                            .weak(),
                    );
                }
            }
            let show_next =
                matches!(self.advice(), Advice::Done | Advice::TwentyOne | Advice::Blackjack | Advice::Bust);
            if more_hands && show_next && ui.button("Passa alla mano successiva").clicked() {
                act = Some("finish");
            }
            ui.add_space(14.0);
            ui.separator();
            egui::CollapsingHeader::new(RichText::new("Regole del tavolo").strong()).show(ui, |ui| self.rules_ui(ui));
            ui.label(RichText::new(rules.short()).small().weak());
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "Il consiglio è la scelta con il valore atteso più alto, calcolato in modo esatto per le carte \
                     sul tavolo e le regole scelte. Nel blackjack il banco ha sempre un piccolo vantaggio: giocando \
                     così lo riduci al minimo (circa 0,5%), ma nel lungo periodo non lo elimini. \
                     Rispetta il regolamento delle piattaforme su cui giochi.",
                )
                .small()
                .weak(),
            );
        });
        match act {
            Some("split") => self.split(),
            Some("double") => self.double(),
            Some("finish") => self.finish_hand(),
            _ => {}
        }
    }

    fn rules_ui(&mut self, ui: &mut egui::Ui) {
        let mut r = self.settings.rules;
        egui::Grid::new("bj_rules").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Mazzi");
            ui.add(egui::Slider::new(&mut r.decks, 1..=8));
            ui.end_row();
            ui.label("Banco sul 17 morbido");
            ui.horizontal(|ui| {
                ui.radio_value(&mut r.hit_soft17, false, "sta");
                ui.radio_value(&mut r.hit_soft17, true, "chiede carta");
            });
            ui.end_row();
            ui.label("Carta coperta del banco");
            ui.horizontal(|ui| {
                ui.radio_value(&mut r.peek, true, "sì");
                ui.radio_value(&mut r.peek, false, "no (europeo)");
            })
            .response
            .on_hover_text("Sì: con A o 10 scoperto il banco controlla subito se ha blackjack (regola americana)");
            ui.end_row();
            ui.label("Raddoppio");
            egui::ComboBox::from_id_salt("bj_double").selected_text(r.double.label()).show_ui(ui, |ui| {
                use blackjack_core::DoubleRule::*;
                for d in [Any, NineToEleven, TenEleven] {
                    ui.selectable_value(&mut r.double, d, d.label());
                }
            });
            ui.end_row();
            ui.label("Raddoppio dopo divisione");
            ui.checkbox(&mut r.double_after_split, "");
            ui.end_row();
            ui.label("Mani dopo le divisioni");
            ui.add(egui::Slider::new(&mut r.max_hands, 2..=4));
            ui.end_row();
            ui.label("Assi ridivisibili");
            ui.checkbox(&mut r.resplit_aces, "");
            ui.end_row();
            ui.label("Resa");
            ui.checkbox(&mut r.surrender, "");
            ui.end_row();
            ui.label("Il blackjack paga");
            ui.horizontal(|ui| {
                ui.radio_value(&mut r.blackjack_pays, 1.5, "3 a 2");
                ui.radio_value(&mut r.blackjack_pays, 1.2, "6 a 5");
            });
            ui.end_row();
        });
        if r != self.settings.rules {
            self.settings.rules = r;
            self.settings.preset = (0..Rules::PRESETS.len()).find(|&p| Rules::preset(p) == r);
            self.changed();
        }
    }
}

// ---------------------------------------------------------------------- funzioni di supporto

fn signed(x: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, x.abs()).replace('.', ",");
    if x >= 0.0 {
        format!("+{s}")
    } else {
        format!("−{s}")
    }
}

/// "perdi in media 25 cent. ogni 1 € puntato"
fn per_euro(ev: f64) -> String {
    let cents = (ev.abs() * 100.0).round();
    if cents < 0.5 {
        "in pari in media".into()
    } else if ev < 0.0 {
        format!("perdi {cents:.0} cent. ogni euro")
    } else {
        format!("guadagni {cents:.0} cent. ogni euro")
    }
}

fn explain(a: Action, ev: f64) -> String {
    let base = match a {
        Action::Stand => "Fermati con le carte che hai.",
        Action::Hit => "Chiedi un'altra carta.",
        Action::Double => "Raddoppia la puntata: ricevi una sola carta.",
        Action::Split => "Dividi la coppia in due mani, ognuna con la puntata iniziale.",
        Action::Surrender => "Arrenditi: recuperi metà della puntata.",
    };
    if ev < 0.0 {
        format!("{base} È la scelta che perde meno.")
    } else {
        base.into()
    }
}

fn action_color(a: Action) -> Color32 {
    match a {
        Action::Hit => Color32::from_rgb(255, 150, 130),
        Action::Stand => Color32::from_rgb(250, 220, 110),
        Action::Double => BEST,
        Action::Split => Color32::from_rgb(140, 185, 255),
        Action::Surrender => Color32::from_rgb(200, 200, 200),
    }
}

fn big(ui: &mut egui::Ui, text: &str, col: Color32) {
    ui.label(RichText::new(text).size(34.0).strong().color(col));
}

fn hand_label(h: Hand) -> String {
    if h.is_blackjack() {
        "Blackjack".into()
    } else if h.busted() {
        format!("{} sballato", h.hard)
    } else if h.is_soft() && h.total() < 21 {
        format!("{} o {}", h.hard, h.total())
    } else {
        h.total().to_string()
    }
}

fn hand_width(n: usize) -> f32 {
    CARD.x + (n.max(1) - 1) as f32 * OVERLAP
}

/// Spazio vuoto dove andrà una carta; bordato d'oro se è lì che va la prossima carta.
fn slot(ui: &mut egui::Ui, target: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(CARD, Sense::click());
    cards_ui::paint_slot(ui.painter(), rect, "");
    if target {
        ui.painter().rect_stroke(rect.expand(3.0), 9.0, Stroke::new(2.5, GOLD), StrokeKind::Outside);
    }
    resp
}

/// Carte sovrapposte in fila. Il seme è solo grafico (al blackjack non conta): varia con la posizione.
fn cards_row(ui: &mut egui::Ui, cards: &[usize], glow: Option<Color32>, seed: usize) -> egui::Response {
    let w = hand_width(cards.len());
    let (rect, resp) = ui.allocate_exact_size(vec2(w, CARD.y), Sense::click());
    for (i, &c) in cards.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(i as f32 * OVERLAP, 0.0), CARD);
        let card = Card::new(if c == 9 { 9 } else { c as u8 }, ((i + seed + c) % 4) as u8);
        paint_card(ui.painter(), r, card);
    }
    if let Some(g) = glow {
        ui.painter().rect_stroke(rect.expand(4.0), 9.0, Stroke::new(2.5, g), StrokeKind::Outside);
    }
    resp
}

/// Carta francese: indici agli angoli e seme grande al centro.
fn paint_card(p: &Painter, rect: Rect, c: Card) {
    let h = rect.height();
    let r = 0.07 * h;
    let shadow = Shadow { offset: [2, 4], blur: 10, spread: 0, color: Color32::from_black_alpha(110) };
    p.add(shadow.as_shape(rect, r));
    p.rect_filled(rect, r, PAPER);
    p.rect_stroke(rect, r, Stroke::new(1.0, Color32::from_gray(170)), StrokeKind::Inside);
    let col = if c.is_red() { RED } else { BLACK };
    let rank = c.rank_label();
    let font = FontId::proportional(0.17 * h);
    p.text(rect.left_top() + vec2(0.11 * h, 0.03 * h), Align2::CENTER_TOP, rank, font.clone(), col);
    paint_suit(p, rect.left_top() + vec2(0.11 * h, 0.27 * h), 0.11 * h, c.suit(), col);
    p.text(rect.right_bottom() - vec2(0.11 * h, 0.03 * h), Align2::CENTER_BOTTOM, rank, font, col);
    paint_suit(p, rect.right_bottom() - vec2(0.11 * h, 0.27 * h), 0.11 * h, c.suit(), col);
    let center = rect.center();
    if c.rank() >= 10 {
        let panel = Rect::from_center_size(center, vec2(0.38 * h, 0.58 * h));
        p.rect_filled(panel, 0.04 * h, col.gamma_multiply(0.10));
        p.rect_stroke(panel, 0.04 * h, Stroke::new(1.5, GOLD), StrokeKind::Inside);
        p.text(panel.center() - vec2(0.0, 0.1 * h), Align2::CENTER_CENTER, rank, FontId::proportional(0.2 * h), col);
        paint_suit(p, panel.center() + vec2(0.0, 0.13 * h), 0.16 * h, c.suit(), col);
    } else {
        paint_suit(p, center, if c.rank() == 0 { 0.42 * h } else { 0.3 * h }, c.suit(), col);
    }
}

/// Seme francese disegnato a vettori, alto circa `d`, centrato in `c`.
fn paint_suit(p: &Painter, c: Pos2, d: f32, suit: u8, col: Color32) {
    let at = |x: f32, y: f32| pos2(c.x + x * d, c.y + y * d);
    let poly = |pts: Vec<Pos2>| egui::Shape::convex_polygon(pts, col, Stroke::NONE);
    match suit {
        // Picche: cuore rovesciato con il gambo.
        0 => {
            p.circle_filled(at(-0.22, 0.08), 0.24 * d, col);
            p.circle_filled(at(0.22, 0.08), 0.24 * d, col);
            p.add(poly(vec![at(0.0, -0.48), at(0.44, 0.02), at(-0.44, 0.02)]));
            p.add(poly(vec![at(0.0, 0.1), at(0.14, 0.48), at(-0.14, 0.48)]));
        }
        // Cuori.
        1 => {
            p.circle_filled(at(-0.22, -0.14), 0.25 * d, col);
            p.circle_filled(at(0.22, -0.14), 0.25 * d, col);
            p.add(poly(vec![at(-0.46, -0.06), at(0.46, -0.06), at(0.0, 0.46)]));
        }
        // Quadri.
        2 => {
            p.add(poly(vec![at(0.0, -0.5), at(0.36, 0.0), at(0.0, 0.5), at(-0.36, 0.0)]));
        }
        // Fiori.
        _ => {
            p.circle_filled(at(0.0, -0.24), 0.21 * d, col);
            p.circle_filled(at(-0.23, 0.08), 0.21 * d, col);
            p.circle_filled(at(0.23, 0.08), 0.21 * d, col);
            p.add(poly(vec![at(0.0, 0.0), at(0.14, 0.48), at(-0.14, 0.48)]));
        }
    }
}
