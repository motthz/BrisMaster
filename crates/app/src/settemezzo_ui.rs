//! Sette e mezzo contro il banco: inserisci le carte della mano e l'app dice se chiedere carta o stare.

use crate::cards_ui::{self, BEST, GOLD};
use briscola_core::cards::{self, Card, CardSet};
use eframe::egui::{self, Color32, RichText, Stroke};
use serde::{Deserialize, Serialize};
use settemezzo::{card_value_label, fmt_points, Advice, Banco, Dist, Hand, Rules, Solver};

const SMALL_CARD: egui::Vec2 = egui::Vec2::new(72.0, 111.0);

/// Preferenze salvate tra un avvio e l'altro.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Prefs {
    pub matta: bool,
    pub reale_doppio: bool,
    /// Indice in `Banco::ALL`.
    pub banco: usize,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { matta: true, reale_doppio: true, banco: 3 }
    }
}

impl Prefs {
    fn rules(&self) -> Rules {
        Rules { matta: self.matta, reale_paga: if self.reale_doppio { 2 } else { 1 } }
    }
    fn banco(&self) -> Banco {
        Banco::ALL[self.banco.min(Banco::ALL.len() - 1)]
    }
}

/// Dove va la prossima carta scelta nell'assistente.
#[derive(Clone, Copy, PartialEq)]
enum Target {
    Coperta,
    Scoperta,
    Vista,
}

pub struct SetteMezzo {
    solver: Solver,
    coperta: Option<Card>,
    scoperte: Vec<Card>,
    viste: Vec<Card>,
    target: Target,
    entry: String,
    entry_error: Option<String>,
}

impl SetteMezzo {
    pub fn new(p: &Prefs) -> Self {
        SetteMezzo {
            solver: Solver::new(p.rules(), p.banco()),
            coperta: None,
            scoperte: Vec::new(),
            viste: Vec::new(),
            target: Target::Coperta,
            entry: String::new(),
            entry_error: None,
        }
    }

    /// Il risolutore va rifatto se cambiano regole o banco.
    fn sync(&mut self, p: &Prefs) {
        if self.solver.rules != p.rules() || self.solver.banco() != p.banco() {
            self.solver = Solver::new(p.rules(), p.banco());
        }
    }

    pub fn top_controls(&mut self, ui: &mut egui::Ui, p: &mut Prefs) {
        egui::ComboBox::from_id_salt("sm_banco")
            .selected_text(p.banco().label())
            .show_ui(ui, |ui| {
                for (i, b) in Banco::ALL.iter().enumerate() {
                    ui.selectable_value(&mut p.banco, i, b.label()).on_hover_text(b.description());
                }
            })
            .response
            .on_hover_text("Come gioca il banco: il consiglio è calcolato per questo banco");
        ui.checkbox(&mut p.matta, "Matta").on_hover_text("Il Re di Denari vale quello che conviene: ½ oppure da 1 a 7");
        ui.checkbox(&mut p.reale_doppio, "Reale ×2")
            .on_hover_text("Il sette e mezzo reale (7½ con due carte) è pagato il doppio");
    }

    fn used(&self) -> CardSet {
        self.coperta.iter().chain(&self.scoperte).chain(&self.viste).copied().collect()
    }

    fn enter(&mut self, c: Card) -> bool {
        if self.used().contains(c) {
            return false;
        }
        match self.target {
            Target::Coperta => {
                self.coperta = Some(c);
                self.target = Target::Scoperta;
            }
            Target::Scoperta => self.scoperte.push(c),
            Target::Vista => self.viste.push(c),
        }
        self.entry_error = None;
        true
    }

    fn assist_table(&mut self, ui: &mut egui::Ui, p: &Prefs) {
        let rules = p.rules();
        let white = |a: u8| Color32::from_white_alpha(a);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            if ui.button("Nuova mano").on_hover_text("Le carte di questa mano passano tra quelle viste").clicked() {
                self.viste.extend(self.coperta.take());
                self.viste.append(&mut self.scoperte);
                self.target = Target::Coperta;
            }
            if ui
                .button("Mazzo nuovo")
                .on_hover_text("Il mazzo è stato rimescolato: dimentica tutte le carte")
                .clicked()
            {
                self.coperta = None;
                self.scoperte.clear();
                self.viste.clear();
                self.target = Target::Coperta;
            }
            let any = self.coperta.is_some() || !self.viste.is_empty();
            if ui.add_enabled(any, egui::Button::new("Annulla ultima carta")).clicked() {
                if self.target == Target::Vista && !self.viste.is_empty() {
                    self.viste.pop();
                } else if self.scoperte.pop().is_none() {
                    self.coperta = None;
                    self.target = Target::Coperta;
                }
            }
        });
        ui.add_space(14.0);
        ui.vertical_centered(|ui| {
            let me = self.coperta.map(|c| Hand::of(&rules, std::iter::once(c).chain(self.scoperte.iter().copied())));
            let caption = me.map_or("Le tue carte".to_string(), |h| format!("Le tue carte · {}", total_label(h)));
            ui.label(RichText::new(caption).size(17.0).strong().color(Color32::WHITE));
            let mine: Vec<Card> = self.coperta.iter().chain(&self.scoperte).copied().collect();
            if mine.is_empty() {
                let (rect, _) = ui.allocate_exact_size(SMALL_CARD, egui::Sense::hover());
                cards_ui::paint_slot(ui.painter(), rect, "coperta");
            } else {
                cards_row(ui, &rules, &mine);
            }
            ui.add_space(14.0);
            ui.label(
                RichText::new(format!("Altre carte viste in questo mazzo: {}", self.viste.len()))
                    .size(15.0)
                    .color(white(200)),
            );
            ui.label(
                RichText::new(
                    "Carte scoperte degli altri giocatori e delle mani precedenti, se il mazzo non è stato rimescolato",
                )
                .small()
                .color(white(150)),
            );
            if !self.viste.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for &c in &self.viste {
                        let (rect, _) = ui.allocate_exact_size(cards_ui::MINI_SIZE, egui::Sense::hover());
                        cards_ui::paint_mini(ui.painter(), rect, c, true, false);
                    }
                });
            }
        });
    }

    fn assist_side(&mut self, ui: &mut egui::Ui, p: &Prefs) {
        let rules = p.rules();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if let Some(c) = self.coperta {
                let d = rules.unseen(std::iter::once(c).chain(self.scoperte.iter().chain(&self.viste).copied()));
                let hc = rules.class_of(c);
                let up = Hand::of(&rules, self.scoperte.iter().copied());
                if up.add(hc).bust() {
                    ui.label(RichText::new("Hai sballato: la mano è persa.").size(20.0).color(Color32::LIGHT_RED));
                } else {
                    let a = self.solver.advise(&d, hc, up);
                    advice_ui(ui, &rules, &a, up.add(hc));
                }
                ui.add_space(10.0);
                ui.separator();
            }
            ui.label(RichText::new("Inserisci").strong());
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.target, Target::Coperta, "La tua carta coperta");
                ui.add_enabled_ui(self.coperta.is_some(), |ui| {
                    ui.selectable_value(&mut self.target, Target::Scoperta, "Carte che ricevi");
                });
                ui.selectable_value(&mut self.target, Target::Vista, "Altre carte viste");
            });
            if self.target == Target::Coperta && self.coperta.is_some() {
                ui.label(RichText::new("Scegli una carta per sostituire la coperta").small().weak());
            }
            ui.add_space(4.0);
            let mut allowed = CardSet::FULL.minus(self.used());
            if self.target == Target::Coperta {
                if let Some(c) = self.coperta {
                    allowed.insert(c);
                }
            }
            if let Some(c) = cards_ui::picker(ui, "sm_picker", allowed) {
                if self.target == Target::Coperta {
                    self.coperta = None;
                }
                self.enter(c);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.entry)
                        .hint_text("da tastiera: 7c, Fs, Rd…")
                        .desired_width(200.0),
                );
                let submit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.button("Inserisci").clicked() || submit) && !self.entry.trim().is_empty() {
                    match cards_ui::parse_card(&self.entry) {
                        Some(c) => {
                            if self.target == Target::Coperta {
                                self.coperta = None;
                            }
                            if self.enter(c) {
                                self.entry.clear();
                            } else {
                                self.entry_error = Some(format!("{} è già stata inserita", cards::name(c)));
                            }
                        }
                        None => self.entry_error = Some("Carta non riconosciuta: scrivi valore e seme, es. 7c".into()),
                    }
                    resp.request_focus();
                }
            });
            if let Some(e) = &self.entry_error {
                ui.label(RichText::new(e).color(Color32::LIGHT_RED));
            }
            ui.add_space(8.0);
            footer(ui, &mut self.solver, &rules);
        });
    }

    pub fn table_ui(&mut self, ui: &mut egui::Ui, p: &mut Prefs) {
        self.sync(p);
        self.assist_table(ui, p);
    }

    pub fn side_ui(&mut self, ui: &mut egui::Ui, p: &mut Prefs) {
        self.sync(p);
        self.assist_side(ui, p);
    }
}

fn total_label(h: Hand) -> String {
    match h.total() {
        None => "sballato".into(),
        Some(_) if h.reale() => "sette e mezzo reale".into(),
        Some(t) => fmt_points(t),
    }
}

fn cards_row(ui: &mut egui::Ui, rules: &Rules, cs: &[Card]) {
    ui.horizontal(|ui| {
        ui.set_min_height(SMALL_CARD.y);
        let w = cs.len() as f32 * (SMALL_CARD.x + 8.0);
        ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
        for (i, &c) in cs.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(SMALL_CARD, egui::Sense::hover());
            cards_ui::paint_face(ui.painter(), rect, c, cards_ui::Look { no_points: true, ..Default::default() });
            // Valore nel Sette e mezzo al posto dei punti della briscola.
            let h = rect.height();
            let pill = egui::Rect::from_center_size(
                rect.right_top() + egui::vec2(-0.16 * h, 0.11 * h),
                egui::vec2(0.24 * h, 0.13 * h),
            );
            ui.painter().rect_filled(pill, 0.06 * h, GOLD);
            ui.painter().text(
                pill.center(),
                egui::Align2::CENTER_CENTER,
                card_value_label(rules, c),
                egui::FontId::proportional(0.085 * h),
                Color32::from_rgb(45, 40, 35),
            );
            let tip = if i == 0 { format!("{} (coperta)", cards::name(c)) } else { cards::name(c) };
            resp.on_hover_text(tip);
        }
    });
}

fn pct(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

fn ev_label(x: f64) -> String {
    format!("{:+.2} puntate", x).replace('.', ",")
}

/// Riquadro del consiglio: cosa fare, quanto vale ogni scelta e perché.
fn advice_ui(ui: &mut egui::Ui, rules: &Rules, a: &Advice, me: Hand) {
    let hit = a.should_hit(rules);
    let best = a.best(rules);
    egui::Frame::new()
        .fill(Color32::from_rgb(24, 52, 36))
        .stroke(Stroke::new(1.5, BEST))
        .corner_radius(10.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Consiglio").color(Color32::from_white_alpha(180)));
            let t = if hit { "Chiedi carta" } else { "Stai" };
            ui.label(RichText::new(t).size(28.0).strong().color(Color32::WHITE));
            ui.label(
                RichText::new(format!(
                    "Vinci la mano il {} delle volte · {}",
                    pct(best.p_win()),
                    ev_label(best.ev(rules))
                ))
                .size(16.0)
                .color(BEST),
            );
        });
    ui.add_space(8.0);
    let row = |ui: &mut egui::Ui, name: &str, d: Option<Dist>, chosen: bool| {
        ui.horizontal(|ui| {
            let n = RichText::new(format!("{name:<13}")).monospace();
            ui.label(if chosen { n.color(BEST) } else { n });
            match d {
                Some(d) => {
                    ui.label(format!("vinci {}", pct(d.p_win())));
                    ui.label(RichText::new(ev_label(d.ev(rules))).strong());
                }
                None => {
                    ui.label(RichText::new("mazzo finito").weak());
                }
            }
        });
    };
    row(ui, "Stai", Some(a.stand), !hit);
    row(ui, "Chiedi carta", a.hit, hit);
    ui.add_space(6.0);
    let tot = me.total().unwrap_or(0);
    let mut reasons = vec![format!("Hai {}.", total_label(me))];
    if a.p_bust_next > 0.0 {
        reasons.push(format!("Con la prossima carta sballi nel {} dei casi.", pct(a.p_bust_next)));
    } else {
        reasons.push("La prossima carta non può farti sballare.".into());
    }
    if let Some(h) = a.hit {
        let d = h.ev(rules) - a.stand.ev(rules);
        if d.abs() < 0.01 {
            reasons.push("Le due scelte si equivalgono quasi: la differenza è sotto un centesimo di puntata.".into());
        } else if hit && tot >= 11 {
            reasons.push(format!(
                "Fermandoti vinceresti solo il {}: rischiare conviene, in media guadagni {} di puntata in più.",
                pct(a.stand.p_win()),
                format!("{:.2}", d).replace('.', ",")
            ));
        } else if !hit {
            reasons.push(format!(
                "Chiedendo carta perderesti in media {} di puntata.",
                format!("{:.2}", -d).replace('.', ",")
            ));
        }
    }
    for r in reasons {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("•").color(GOLD));
            ui.label(r);
        });
    }
}

fn footer(ui: &mut egui::Ui, solver: &mut Solver, rules: &Rules) {
    let ev = solver.optimal_value(&rules.full_deck()).ev(rules);
    ui.label(
        RichText::new(format!(
            "Calcolo esatto contro il {} (nessuna stima): giocando sempre come consigliato la vincita media è {} \
             della puntata a mano. Il banco vince i pareggi, quindi nel lungo periodo è quasi sempre in vantaggio.",
            solver.banco().label().to_lowercase(),
            format!("{:+.1}%", ev * 100.0).replace('.', ",")
        ))
        .small()
        .weak(),
    );
    ui.label(
        RichText::new(
            "Rispetta il regolamento delle piattaforme su cui giochi; il gioco d'azzardo può creare dipendenza.",
        )
        .small()
        .weak(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserimento_e_consiglio() {
        let p = Prefs::default();
        let mut sm = SetteMezzo::new(&p);
        // Coperta: Sette di Coppe; poi arriva il Fante di Spade: sette e mezzo reale.
        assert!(sm.enter(cards::make(1, 6)));
        assert!(sm.target == Target::Scoperta);
        assert!(!sm.enter(cards::make(1, 6)), "carta già inserita");
        assert!(sm.enter(cards::make(2, 7)));
        let rules = p.rules();
        let me = Hand::of(&rules, sm.coperta.iter().chain(&sm.scoperte).copied());
        assert!(me.reale());
        sm.target = Target::Vista;
        assert!(sm.enter(cards::make(3, 0)));
        assert_eq!(sm.viste.len(), 1);
    }
}
