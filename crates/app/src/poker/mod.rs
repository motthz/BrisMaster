//! Assistente di Texas Hold'em: inserisci le tue carte, la posizione, le azioni dell'avversario
//! e il board man mano, e il motore ti dice cosa fare, con la probabilità di ogni azione.
//!
//! No Limit testa a testa, bui 50/100 e 100 grandi bui a testa (la situazione per cui il motore
//! è allenato).

mod cards;

use crate::cards_ui::{BEST, GOLD};
use eframe::egui::{self, vec2, Align2, Color32, FontId, RichText, Sense, Stroke, StrokeKind};
use poker_core::cards::{describe, parse, rank, suit, Card, CardSet};
use poker_core::engine::{Advice, Engine};
use poker_core::equity::equity_vs_random;
use poker_core::game::{Action, Hand, BB, STACK, STREET_NAMES};
use poker_core::View;
use rand::rngs::SmallRng;
use rand::SeedableRng;

const ME: u8 = 0;
const OPP: u8 = 1;

#[derive(Clone)]
struct Snapshot {
    hole: Vec<Card>,
    i_am_button: bool,
    board: Vec<Card>,
    hand: Option<Hand>,
}

pub struct HoldemAssistant {
    hole: Vec<Card>,
    i_am_button: bool,
    board: Vec<Card>,
    /// La mano in corso (le carte dentro `Hand` sono segnaposto: contano solo le puntate).
    hand: Option<Hand>,
    undo: Vec<Snapshot>,
    coach: Engine,
    rng: SmallRng,
    /// Consiglio calcolato per (azioni, carte del board).
    advice: Option<((usize, usize), Vec<Advice>, f64)>,
    /// Importo del rilancio dell'avversario, in grandi bui.
    opp_amount: f64,
    opp_key: usize,
    my_amount: u32,
    typed: String,
    error: Option<String>,
}

impl Default for HoldemAssistant {
    fn default() -> Self {
        HoldemAssistant {
            hole: Vec::new(),
            i_am_button: true,
            board: Vec::new(),
            hand: None,
            undo: Vec::new(),
            coach: Engine::bundled(),
            rng: SmallRng::from_entropy(),
            advice: None,
            opp_amount: 3.0,
            opp_key: usize::MAX,
            my_amount: 0,
            typed: String::new(),
            error: None,
        }
    }
}

fn chips(x: u32) -> String {
    if x % BB == 0 {
        format!("{} bb", x / BB)
    } else {
        format!("{:.1} bb", x as f64 / BB as f64).replace('.', ",")
    }
}

/// Testo di un'azione; `all_in` è il totale che corrisponde a puntare tutto, se si conosce.
fn action_text_ai(a: Action, to_call: u32, all_in: Option<u32>) -> String {
    match a {
        Action::RaiseTo(x) if Some(x) == all_in => "All-in".into(),
        a => action_text(a, to_call),
    }
}

fn action_text(a: Action, to_call: u32) -> String {
    match a {
        Action::Fold if to_call == 0 => "Check".into(),
        Action::Fold => "Fold".into(),
        Action::Call if to_call == 0 => "Check".into(),
        Action::Call => format!("Call {}", chips(to_call)),
        Action::RaiseTo(x) if x >= STACK => "All-in".into(),
        Action::RaiseTo(x) => format!("Punta/rilancia a {}", chips(x)),
    }
}

impl HoldemAssistant {
    fn snapshot(&self) -> Snapshot {
        Snapshot { hole: self.hole.clone(), i_am_button: self.i_am_button, board: self.board.clone(), hand: self.hand.clone() }
    }

    fn push_undo(&mut self) {
        self.undo.push(self.snapshot());
        self.error = None;
    }

    fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.hole = s.hole;
            self.i_am_button = s.i_am_button;
            self.board = s.board;
            self.hand = s.hand;
            // Il motore tiene traccia della mano: si riparte puliti.
            self.coach = Engine::bundled();
            self.advice = None;
            self.error = None;
        }
    }

    fn new_hand(&mut self) {
        self.push_undo();
        // Testa a testa il bottone passa all'altro a ogni mano.
        let next_button = if self.hand.is_some() { !self.i_am_button } else { self.i_am_button };
        self.hole.clear();
        self.board.clear();
        self.hand = None;
        self.i_am_button = next_button;
        self.advice = None;
        self.coach = Engine::bundled();
    }

    fn used(&self) -> CardSet {
        let mut s = CardSet::of(&self.hole);
        for &c in &self.board {
            s.add(c);
        }
        s
    }

    /// Carte del board che servono prima di continuare.
    fn board_needed(&self) -> usize {
        match &self.hand {
            Some(h) if !(h.over && h.folded.is_some()) => [0, 3, 4, 5][h.street as usize],
            _ => 0,
        }
    }

    fn start(&mut self) {
        self.push_undo();
        let used = self.used();
        // Carte dell'avversario e board fittizi: servono solo a creare la mano.
        let filler: Vec<Card> = (0..52).filter(|&c| !used.has(c)).take(7).collect();
        let hole = [[self.hole[0], self.hole[1]], [filler[0], filler[1]]];
        let button = if self.i_am_button { ME } else { OPP };
        self.hand = Some(Hand::with_cards(hole, [filler[2], filler[3], filler[4], filler[5], filler[6]], button));
    }

    fn add_card(&mut self, c: Card) {
        if self.used().has(c) {
            self.error = Some("Carta già inserita".into());
            return;
        }
        if self.hand.is_none() && self.hole.len() < 2 {
            self.push_undo();
            self.hole.push(c);
            if self.hole.len() == 2 {
                self.start();
            }
        } else if self.board.len() < self.board_needed() {
            self.push_undo();
            self.board.push(c);
        }
    }

    fn apply(&mut self, a: Action) {
        self.push_undo();
        if let Some(h) = &mut self.hand {
            h.apply(a);
        }
    }

    fn view(&self) -> Option<View<'_>> {
        let h = self.hand.as_ref()?;
        if h.over {
            return None;
        }
        let mut v = View::of(h, h.to_act);
        v.board = &self.board[..self.board.len().min(v.board.len())];
        Some(v)
    }

    /// Ricalcola il consiglio quando tocca a te e la situazione è cambiata.
    fn refresh(&mut self) {
        let Some(h) = &self.hand else { return };
        if h.over || h.to_act != ME || self.board.len() < self.board_needed() {
            return;
        }
        let key = (h.log.len(), self.board.len());
        if self.advice.as_ref().is_some_and(|a| a.0 == key) {
            return;
        }
        let mut v = View::of(h, ME);
        v.board = &self.board;
        let adv = self.coach.advice(&v, &mut self.rng).unwrap_or_default();
        let eq = equity_vs_random(v.hole, v.board, 4000, &mut self.rng);
        self.my_amount = adv
            .iter()
            .filter_map(|a| match a.action {
                Action::RaiseTo(x) => Some((x, a.prob)),
                _ => None,
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|x| x.0)
            .or(v.raise_range.map(|r| r.0))
            .unwrap_or(0);
        self.advice = Some((key, adv, eq));
    }

    pub fn table_ui(&mut self, ui: &mut egui::Ui) {
        self.refresh();
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) {
            self.undo();
        }
        let white = |a: u8| Color32::from_white_alpha(a);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.label(RichText::new("Texas Hold'em · assistente").size(18.0).strong().color(GOLD));
            ui.add_space(20.0);
            if ui.button("Nuova mano").clicked() {
                self.new_hand();
            }
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Annulla (Ctrl+Z)")).clicked() {
                self.undo();
            }
        });
        ui.add_space(8.0);

        // Posizione (solo prima di iniziare).
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.label(RichText::new("La tua posizione:").color(white(210)));
            let locked = self.hand.is_some();
            for (b, label) in [(true, "Bottone (piccolo buio, parli per primo preflop)"), (false, "Grande buio")] {
                if ui.add_enabled(!locked, egui::Button::selectable(self.i_am_button == b, label)).clicked() {
                    self.i_am_button = b;
                }
            }
        });
        ui.add_space(8.0);

        // Carte: le tue e il board.
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Le tue carte").color(white(210)));
                ui.horizontal(|ui| {
                    for i in 0..2 {
                        cards::card(ui, self.hole.get(i).copied(), true, cards::SIZE, None);
                        ui.add_space(4.0);
                    }
                });
            });
            ui.add_space(30.0);
            ui.vertical(|ui| {
                let street = self.hand.as_ref().map_or(0, |h| h.street as usize);
                ui.label(RichText::new(format!("Board · {}", STREET_NAMES[street])).color(white(210)));
                ui.horizontal(|ui| {
                    for i in 0..5 {
                        cards::card(ui, self.board.get(i).copied(), true, cards::SIZE, None);
                        ui.add_space(4.0);
                    }
                });
            });
        });
        ui.add_space(8.0);
        if let Some(h) = &self.hand {
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(format!(
                        "Piatto {} · tu: stack {}, puntata {} · avversario: stack {}, puntata {}",
                        chips(h.pot()),
                        chips(h.stack[0]),
                        chips(h.street_bet[0]),
                        chips(h.stack[1]),
                        chips(h.street_bet[1])
                    ))
                    .color(white(220)),
                );
            });
        }
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.vertical(|ui| self.input_ui(ui));
        });
    }

    /// Cosa serve adesso: carte da inserire o azione da registrare.
    fn input_ui(&mut self, ui: &mut egui::Ui) {
        let white = |a: u8| Color32::from_white_alpha(a);
        let need_cards = if self.hand.is_none() {
            Some(format!("Inserisci le tue carte ({} di 2)", self.hole.len() + 1))
        } else if self.board.len() < self.board_needed() {
            let what = ["", "il flop", "il turn", "il river"][self.hand.as_ref().unwrap().street as usize];
            Some(format!("Inserisci {what} (carta {} di {})", self.board.len() + 1, self.board_needed()))
        } else {
            None
        };
        if let Some(msg) = need_cards {
            ui.label(RichText::new(msg).size(17.0).strong().color(Color32::WHITE));
            if let Some(c) = self.picker(ui) {
                self.add_card(c);
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("oppure scrivi (es. As, Td, 7c, 10q) e premi Invio:").color(white(170)));
                let r = ui.add(egui::TextEdit::singleline(&mut self.typed).desired_width(80.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    match parse(&self.typed) {
                        Some(c) => self.add_card(c),
                        None => self.error = Some(format!("Carta non riconosciuta: {}", self.typed)),
                    }
                    self.typed.clear();
                    r.request_focus();
                }
            });
            ui.label(
                RichText::new("Semi: p = picche, c = cuori, q = quadri, f = fiori (vanno bene anche s, h, d). Valori: 2-9, T o 10, J, Q, K, A.")
                    .small()
                    .color(white(140)),
            );
        } else if let Some(h) = self.hand.clone() {
            if h.over {
                let text = match h.folded {
                    Some(ME) => "Hai lasciato: mano finita".to_string(),
                    Some(_) => format!("L'avversario lascia: vinci il piatto di {}", chips(h.pot())),
                    None => {
                        let mut mine: Vec<Card> = self.board.clone();
                        mine.extend_from_slice(&self.hole);
                        format!("Showdown: hai {}. Piatto {}", describe(&mine), chips(h.pot()))
                    }
                };
                ui.label(RichText::new(text).size(17.0).strong().color(BEST));
                if ui.button("Nuova mano").clicked() {
                    self.new_hand();
                }
            } else if h.to_act == OPP {
                self.opponent_ui(ui, &h);
            } else {
                self.my_turn_ui(ui, &h);
            }
        }
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(Color32::from_rgb(240, 120, 110)));
        }
    }

    fn opponent_ui(&mut self, ui: &mut egui::Ui, h: &Hand) {
        let to_call = h.to_call();
        if self.opp_key != h.log.len() {
            // Proposta iniziale: due terzi del piatto, o il rilancio minimo.
            self.opp_key = h.log.len();
            let v = View::of(h, OPP);
            self.opp_amount = match v.pot_raise(0.66) {
                Some(Action::RaiseTo(x)) => (x as f64 / BB as f64 * 2.0).round() / 2.0,
                _ => 0.0,
            };
        }
        ui.label(RichText::new("Cosa ha fatto l'avversario?").size(17.0).strong().color(Color32::WHITE));
        let mut chosen = None;
        ui.horizontal(|ui| {
            if to_call > 0 && ui.button(RichText::new("Fold").size(16.0)).clicked() {
                chosen = Some(Action::Fold);
            }
            let call = if to_call == 0 { "Check".to_string() } else { format!("Call {}", chips(to_call)) };
            if ui.button(RichText::new(call).size(16.0)).clicked() {
                chosen = Some(Action::Call);
            }
            if let Some((lo, hi)) = h.raise_range() {
                let verb = if h.street_bet[0] == h.street_bet[1] && !(h.street == 0) { "Punta" } else { "Rilancia a" };
                ui.label(RichText::new(verb).color(Color32::WHITE));
                let (lo_bb, hi_bb) = (lo as f64 / BB as f64, hi as f64 / BB as f64);
                self.opp_amount = self.opp_amount.clamp(lo_bb, hi_bb);
                ui.add(egui::DragValue::new(&mut self.opp_amount).range(lo_bb..=hi_bb).speed(0.1).fixed_decimals(1).suffix(" bb"));
                if ui.button(RichText::new("Conferma").size(16.0)).clicked() {
                    chosen = Some(Action::RaiseTo((self.opp_amount * BB as f64).round() as u32));
                }
                if ui.button(RichText::new("All-in").size(16.0)).clicked() {
                    chosen = Some(Action::RaiseTo(hi));
                }
            }
        });
        ui.label(
            RichText::new("Per una puntata o un rilancio scrivi il totale messo dall'avversario in questo giro, in grandi bui.")
                .small()
                .color(Color32::from_white_alpha(140)),
        );
        if let Some(a) = chosen {
            self.apply(a);
        }
    }

    fn my_turn_ui(&mut self, ui: &mut egui::Ui, h: &Hand) {
        let to_call = h.to_call();
        ui.label(RichText::new("Tocca a te: cosa hai fatto?").size(17.0).strong().color(Color32::WHITE));
        ui.label(RichText::new("Il consiglio è a destra. Registra l'azione che hai fatto davvero, così l'assistente segue la mano.").small().color(Color32::from_white_alpha(150)));
        let best = self.advice.as_ref().and_then(|(_, adv, _)| adv.iter().max_by(|a, b| a.prob.total_cmp(&b.prob)).map(|a| a.action));
        let mut chosen = None;
        ui.horizontal(|ui| {
            let mut btn = |ui: &mut egui::Ui, text: String, a: Action| {
                let mut b = egui::Button::new(RichText::new(text).size(16.0));
                if best == Some(a) {
                    b = b.stroke(Stroke::new(3.0, BEST));
                }
                if ui.add(b).clicked() {
                    chosen = Some(a);
                }
            };
            if to_call > 0 {
                btn(ui, "Fold".into(), Action::Fold);
            }
            btn(ui, action_text(Action::Call, to_call), Action::Call);
            if let Some((lo, hi)) = h.raise_range() {
                self.my_amount = self.my_amount.clamp(lo, hi);
                let mut bb = self.my_amount as f64 / BB as f64;
                ui.add(egui::DragValue::new(&mut bb).range(lo as f64 / BB as f64..=hi as f64 / BB as f64).speed(0.1).fixed_decimals(1).suffix(" bb"));
                self.my_amount = (bb * BB as f64).round() as u32;
                btn(ui, action_text_ai(Action::RaiseTo(self.my_amount), to_call, Some(hi)), Action::RaiseTo(self.my_amount));
                if self.my_amount != hi {
                    btn(ui, "All-in".into(), Action::RaiseTo(hi));
                }
            }
        });
        if let Some(a) = chosen {
            self.apply(a);
        }
    }

    /// Griglia 4×13 per scegliere una carta: le carte già usate non sono cliccabili.
    fn picker(&self, ui: &mut egui::Ui) -> Option<Card> {
        let used = self.used();
        let mut out = None;
        let size = vec2(34.0, 44.0);
        for s in [0u8, 1, 2, 3] {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for r in (0..13u8).rev() {
                    let c = r * 4 + s;
                    let enabled = !used.has(c);
                    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
                    let p = ui.painter();
                    let mut p = p.clone();
                    if !enabled {
                        p.multiply_opacity(0.25);
                    }
                    let hovered = enabled && resp.hovered();
                    let rect = if hovered { rect.translate(vec2(0.0, -2.0)) } else { rect };
                    p.rect_filled(rect, 5.0, Color32::from_rgb(251, 249, 244));
                    let border = if hovered { Stroke::new(2.5, BEST) } else { Stroke::new(1.0, cards::suit_color(s)) };
                    p.rect_stroke(rect, 5.0, border, StrokeKind::Inside);
                    p.text(rect.center_top() + vec2(0.0, 3.0), Align2::CENTER_TOP, cards::rank_label(c), FontId::proportional(15.0), cards::suit_color(s));
                    cards::paint_suit(&p, rect.center_bottom() - vec2(0.0, 12.0), 14.0, s);
                    if resp.clicked() {
                        out = Some(c);
                    }
                    debug_assert_eq!((rank(c), suit(c)), (r, s));
                }
            });
        }
        out
    }

    pub fn side_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Cosa fare");
            let my_turn = self.hand.as_ref().is_some_and(|h| !h.over && h.to_act == ME) && self.board.len() >= self.board_needed();
            if !my_turn {
                let msg = match &self.hand {
                    None => "Inserisci le tue carte e la posizione.",
                    Some(h) if h.over => "Mano finita.",
                    Some(_) if self.board.len() < self.board_needed() => "Inserisci le carte del board.",
                    _ => "Inserisci l'azione dell'avversario.",
                };
                ui.label(RichText::new(msg).weak());
            } else if let Some((_, adv, eq)) = &self.advice {
                let to_call = self.view().map_or(0, |v| v.to_call);
                let all_in = self.view().and_then(|v| v.raise_range).map(|r| r.1);
                if adv.is_empty() {
                    ui.label(RichText::new("La mano è uscita dalle situazioni studiate dal motore (sequenza di puntate molto insolita): qui non c'è un consiglio affidabile.").color(GOLD));
                } else {
                    let best = adv.iter().max_by(|a, b| a.prob.total_cmp(&b.prob)).unwrap();
                    ui.label(RichText::new(action_text_ai(best.action, to_call, all_in)).size(26.0).strong().color(BEST));
                    ui.add_space(4.0);
                    ui.label(RichText::new("Il motore mescola le scelte per non essere prevedibile: queste sono le probabilità con cui sceglie ogni azione.").small().weak());
                    ui.add_space(4.0);
                    for a in adv {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(vec2(130.0, 16.0), Sense::hover());
                            ui.painter().rect_filled(rect, 4.0, Color32::from_gray(50));
                            let mut bar = rect;
                            bar.set_width(rect.width() * a.prob);
                            ui.painter().rect_filled(bar, 4.0, BEST);
                            ui.label(format!("{:>3.0}%  {}", a.prob * 100.0, action_text_ai(a.action, to_call, all_in)));
                        });
                    }
                }
                ui.add_space(6.0);
                ui.label(format!("Equità contro una mano a caso: {:.0}%", eq * 100.0));
                let mut seen: Vec<Card> = self.board.clone();
                seen.extend_from_slice(&self.hole);
                if seen.len() >= 5 {
                    ui.label(format!("Hai: {}", describe(&seen)));
                }
            }
            ui.separator();
            ui.heading("La mano");
            match &self.hand {
                None => {
                    ui.label(RichText::new("Non ancora iniziata.").weak());
                }
                Some(h) => {
                    for l in &h.log {
                        let who = if l.seat == ME { "Tu" } else { "Avversario" };
                        ui.label(format!("{} · {who}: {}", STREET_NAMES[l.street as usize], action_text(l.action, l.to_call)));
                    }
                }
            }
            ui.separator();
            ui.label(
                RichText::new(
                    "No Limit testa a testa, bui 50/100: il motore è allenato con 100 grandi bui a testa. \
                     Con stack molto diversi o più di due giocatori al tavolo i consigli valgono meno. \
                     Molte sale da poker online vietano nei regolamenti l'uso di programmi di aiuto \
                     durante il gioco: controlla quelle della piattaforma su cui giochi.",
                )
                .small()
                .weak(),
            );
        });
    }
}
