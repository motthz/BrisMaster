//! Scala 40: partita contro il motore con i consigli e assistente con inserimento manuale.

use crate::cards_ui::{self, BEST, GOLD};
use eframe::egui::{
    self, epaint::Shadow, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    Vec2,
};
use scala40_core::ai::{self, EnginePlayer, Turn, Weights};
use scala40_core::cards::{self, is_joker, Card, JOKER};
use scala40_core::{Action, Deal, Meld, Phase};

const ME: u8 = 0;
const OPP: u8 = 1;
const HAND_CARD: Vec2 = Vec2::new(64.0, 94.0);
const TABLE_CARD: Vec2 = Vec2::new(42.0, 61.0);
const PAPER: Color32 = Color32::from_rgb(251, 249, 243);
const RED: Color32 = Color32::from_rgb(200, 36, 42);
const INK: Color32 = Color32::from_rgb(28, 28, 34);

// ---------------------------------------------------------------------------------------
// Carte francesi
// ---------------------------------------------------------------------------------------

fn suit_ink(s: u8) -> Color32 {
    if s < 2 { RED } else { INK }
}

/// Simbolo del seme (cuori, quadri, fiori, picche) centrato in `c`, alto circa `d`.
pub fn paint_suit(p: &Painter, c: Pos2, d: f32, s: u8) {
    let col = suit_ink(s);
    let at = |x: f32, y: f32| pos2(c.x + x * d, c.y + y * d);
    let heart = |p: &Painter, flip: f32| {
        p.circle_filled(at(-0.22, -0.12 * flip), 0.25 * d, col);
        p.circle_filled(at(0.22, -0.12 * flip), 0.25 * d, col);
        let tri = vec![at(-0.46, -0.03 * flip), at(0.46, -0.03 * flip), at(0.0, 0.45 * flip)];
        p.add(egui::Shape::convex_polygon(tri, col, Stroke::NONE));
    };
    match s {
        0 => heart(p, 1.0),
        1 => {
            let rh = vec![at(0.0, -0.5), at(0.36, 0.0), at(0.0, 0.5), at(-0.36, 0.0)];
            p.add(egui::Shape::convex_polygon(rh, col, Stroke::NONE));
        }
        2 => {
            p.circle_filled(at(0.0, -0.24), 0.21 * d, col);
            p.circle_filled(at(-0.23, 0.08), 0.21 * d, col);
            p.circle_filled(at(0.23, 0.08), 0.21 * d, col);
            let stem = vec![at(0.0, 0.0), at(0.14, 0.48), at(-0.14, 0.48)];
            p.add(egui::Shape::convex_polygon(stem, col, Stroke::NONE));
        }
        _ => {
            heart(p, -1.0);
            let stem = vec![at(0.0, 0.05), at(0.15, 0.5), at(-0.15, 0.5)];
            p.add(egui::Shape::convex_polygon(stem, col, Stroke::NONE));
        }
    }
}

fn paint_star(p: &Painter, c: Pos2, r: f32, col: Color32) {
    let pts: Vec<Pos2> = (0..10)
        .map(|i| {
            let a = std::f32::consts::PI * (i as f32 / 5.0 - 0.5);
            let rr = if i % 2 == 0 { r } else { r * 0.45 };
            pos2(c.x + rr * a.cos(), c.y + rr * a.sin())
        })
        .collect();
    // La stella non è convessa: si disegna come ventaglio di triangoli dal centro.
    for i in 0..10 {
        let tri = vec![c, pts[i], pts[(i + 1) % 10]];
        p.add(egui::Shape::convex_polygon(tri, col, Stroke::NONE));
    }
}

#[derive(Clone, Copy, Default)]
struct Look {
    glow: Option<Color32>,
    lifted: bool,
    dim: bool,
}

/// Faccia di una carta francese.
fn paint_face(p: &Painter, rect: Rect, c: Card, look: Look) {
    let mut p = p.clone();
    if look.dim {
        p.multiply_opacity(0.45);
    }
    let rect = if look.lifted { rect.translate(vec2(0.0, -12.0)) } else { rect };
    let h = rect.height();
    let r = 0.07 * h;
    let shadow = Shadow { offset: [1, 3], blur: 8, spread: 0, color: Color32::from_black_alpha(100) };
    p.add(shadow.as_shape(rect, r));
    p.rect_filled(rect, r, PAPER);
    p.rect_stroke(rect, r, Stroke::new(1.0, Color32::from_gray(170)), StrokeKind::Inside);
    if is_joker(c) {
        p.text(rect.left_top() + vec2(0.13 * h, 0.05 * h), Align2::CENTER_TOP, "J", FontId::proportional(0.17 * h), RED);
        paint_star(&p, rect.center() - vec2(0.0, 0.06 * h), 0.2 * h, GOLD);
        p.text(
            rect.center_bottom() - vec2(0.0, 0.1 * h),
            Align2::CENTER_BOTTOM,
            "JOLLY",
            FontId::proportional(0.12 * h),
            RED,
        );
    } else {
        let (s, r) = (cards::suit(c), cards::rank(c));
        let col = suit_ink(s);
        let label = cards::rank_label(r);
        p.text(rect.left_top() + vec2(0.14 * h, 0.03 * h), Align2::CENTER_TOP, label, FontId::proportional(0.2 * h), col);
        paint_suit(&p, rect.left_top() + vec2(0.14 * h, 0.32 * h), 0.14 * h, s);
        if h >= 80.0 {
            paint_suit(&p, rect.center() + vec2(0.06 * h, 0.12 * h), 0.36 * h, s);
            if r >= 11 {
                p.text(
                    rect.center() + vec2(0.06 * h, -0.2 * h),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(0.2 * h),
                    col,
                );
            }
        } else {
            paint_suit(&p, rect.center() + vec2(0.1 * h, 0.2 * h), 0.3 * h, s);
        }
    }
    if let Some(g) = look.glow {
        p.rect_stroke(rect.expand(2.0), r + 2.0, Stroke::new(3.5, g), StrokeKind::Outside);
    }
}

fn sorted(hand: &[Card]) -> Vec<Card> {
    let mut h = hand.to_vec();
    h.sort_by_key(|&c| if is_joker(c) { (9, 0, c) } else { (cards::suit(c), cards::rank(c), c) });
    h
}

fn labels(cs: &[Card]) -> String {
    match Meld::new(cs, 0) {
        Some(m) => m.label(),
        None => cs.iter().map(|&c| cards::short(c)).collect::<Vec<_>>().join(", "),
    }
}

/// Le azioni di un turno a parole.
fn describe(t: &Turn, d: &Deal) -> Vec<String> {
    let mut g = d.clone();
    let mut out = Vec::new();
    for a in &t.actions {
        match a {
            Action::Lay(groups) => {
                let v: u32 = groups.iter().map(|m| Meld::new(m, 0).map_or(0, |m| m.value())).sum();
                let what = groups.iter().map(|m| labels(m)).collect::<Vec<_>>().join(" + ");
                if g.opened[g.to_move as usize] {
                    out.push(format!("Cala {what}"));
                } else {
                    out.push(format!("Apri con {what} ({v} punti)"));
                }
            }
            Action::Attach(c, i) => out.push(format!("Attacca {} a {}", cards::short(*c), g.melds[*i].label())),
            Action::Swap(c, i) => {
                out.push(format!("Metti {} al posto del jolly in {} e prendi il jolly", cards::short(*c), g.melds[*i].label()))
            }
        }
        let _ = g.apply(a);
    }
    let left = g.hands[g.to_move as usize].len();
    if left == 1 {
        out.push(format!("Scarta {} e chiudi!", cards::short(t.discard)));
    } else {
        out.push(format!("Scarta {}", cards::short(t.discard)));
    }
    out
}

fn engine() -> EnginePlayer {
    EnginePlayer::new(Weights::default())
}

// ---------------------------------------------------------------------------------------
// Assistente
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Hand,
    Top,
    Pile,
    Builder,
}

struct Assist {
    hand: Vec<Card>,
    top: Option<Card>,
    pile: Vec<Card>,
    melds: Vec<Meld>,
    builder: Vec<Card>,
    opened: [bool; 2],
    opp_len: usize,
    target: Target,
    msg: Option<String>,
    advice: Option<Vec<String>>,
    ranking: Vec<(Card, f32)>,
    /// Ho già pescato in questo turno.
    drawn: bool,
    /// Dopo aver applicato un consiglio, la prossima carta aggiunta alla mano è la pescata.
    awaiting_draw: bool,
    /// Posizione e turno consigliato, per applicarlo con un clic.
    plan: Option<(Deal, Turn)>,
}

impl Default for Assist {
    fn default() -> Self {
        Assist {
            hand: Vec::new(),
            top: None,
            pile: Vec::new(),
            melds: Vec::new(),
            builder: Vec::new(),
            opened: [false; 2],
            opp_len: 13,
            target: Target::Hand,
            msg: None,
            advice: None,
            ranking: Vec::new(),
            drawn: false,
            awaiting_draw: false,
            plan: None,
        }
    }
}

impl Assist {
    fn used(&self) -> Vec<Card> {
        let mut u = self.hand.clone();
        u.extend(self.top);
        u.extend(&self.pile);
        u.extend(&self.builder);
        for m in &self.melds {
            u.extend(&m.cards);
        }
        u
    }

    fn add(&mut self, k: u8) {
        let Some(c) = cards::free_copy(k, &self.used()) else {
            self.msg = Some("Di questa carta ci sono già tutte le copie".into());
            return;
        };
        self.msg = None;
        match self.target {
            Target::Hand => {
                if self.hand.len() >= 21 {
                    self.msg = Some("Troppe carte in mano".into());
                    return;
                }
                self.hand.push(c);
                if self.awaiting_draw {
                    self.awaiting_draw = false;
                    self.drawn = true;
                }
            }
            Target::Top => {
                if let Some(old) = self.top.replace(c) {
                    self.pile.push(old);
                }
            }
            Target::Pile => self.pile.push(c),
            Target::Builder => self.builder.push(c),
        }
        self.refresh();
    }

    fn refresh(&mut self) {
        self.advice = None;
        self.ranking.clear();
        self.plan = None;
        if self.hand.is_empty() {
            return;
        }
        let mut discard = self.pile.clone();
        discard.extend(self.top);
        let e = engine();
        let d = Deal::from_position(ME, self.hand.clone(), self.opp_len, discard, self.melds.clone(), self.opened, self.drawn);
        if d.phase == Phase::Draw {
            let mut lines = Vec::new();
            if self.top.is_some() && ai::discard_usable(&d, ME) {
                let mut g = d.clone();
                let c = g.take_discard().unwrap();
                lines.push(format!("Prendi {} dal pozzo, poi:", cards::short(c)));
                let (t, ranking) = e.advise(&g, ME);
                lines.extend(describe(&t, &g).into_iter().map(|l| format!("  {l}")));
                self.ranking = ranking;
                self.plan = Some((g, t));
            } else {
                lines.push("Pesca dal tallone, poi aggiungi la carta pescata alla mano.".into());
            }
            self.advice = Some(lines);
        } else {
            let (t, ranking) = e.advise(&d, ME);
            self.advice = Some(describe(&t, &d));
            self.ranking = ranking;
            self.plan = Some((d, t));
        }
    }

    /// Esegue il turno consigliato: calate e attacchi vanno in tavola, lo scarto nel pozzo.
    fn apply_plan(&mut self) {
        let Some((mut g, t)) = self.plan.take() else { return };
        for a in &t.actions {
            if g.apply(a).is_err() {
                return;
            }
        }
        if g.discard_card(t.discard).is_err() {
            return;
        }
        self.hand = g.hands[ME as usize].clone();
        self.melds = g.melds.clone();
        self.opened = g.opened;
        self.top = g.discard.last().copied();
        self.pile = g.discard[..g.discard.len().saturating_sub(1)].to_vec();
        self.drawn = false;
        self.awaiting_draw = true;
        self.refresh();
        self.msg = Some(if self.hand.is_empty() {
            "Hai chiuso! Premi «Nuova smazzata» per ricominciare.".into()
        } else {
            "Fatto. Al turno dell'avversario metti il suo scarto in cima al pozzo e le sue combinazioni in tavola; \
             poi aggiungi alla mano la carta che peschi."
                .into()
        });
    }
}

// ---------------------------------------------------------------------------------------
// Interfaccia
// ---------------------------------------------------------------------------------------

#[derive(Default)]
pub struct Scala40Ui {
    assist: Assist,
}

impl Scala40Ui {
    /// Controlli nella barra in alto.
    pub fn top_controls(&mut self, ui: &mut egui::Ui) {
        if ui.button("Nuova smazzata").on_hover_text("Svuota mano, pozzo e tavolo").clicked() {
            self.assist = Assist::default();
        }
    }

    /// Tavolo (`left`) e pannello laterale (`right`).
    pub fn show(&mut self, ui: &mut egui::Ui, left: Rect, right: Rect) {
        ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| self.assist_side(ui));
        ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| self.assist_table(ui));
    }

    fn assist_side(&mut self, ui: &mut egui::Ui) {
        let a = &mut self.assist;
        ui.heading("Assistente Scala 40");
        ui.label(
            RichText::new(
                "Inserisci le tue carte, la carta in cima al pozzo e le combinazioni in tavola. \
                 Con 13 carte il motore ti dice se prendere lo scarto; con 14 (dopo aver pescato) \
                 cosa calare e cosa scartare.",
            )
            .weak()
            .small(),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (t, label) in [
                (Target::Hand, "La mia mano"),
                (Target::Top, "Cima del pozzo"),
                (Target::Pile, "Carte già scartate"),
                (Target::Builder, "Combinazione in tavola"),
            ] {
                ui.selectable_value(&mut a.target, t, label);
            }
        });
        ui.add_space(4.0);
        let mut picked = None;
        egui::Grid::new("picker40").spacing([3.0, 4.0]).min_col_width(28.0).show(ui, |ui| {
            for s in 0..4u8 {
                for r in 1..=13u8 {
                    let k = cards::kind_of(s, r);
                    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 40.0), Sense::click());
                    let hovered = resp.hovered();
                    let p = ui.painter();
                    let rr = if hovered { rect.translate(vec2(0.0, -2.0)) } else { rect };
                    p.rect_filled(rr, 4.0, PAPER);
                    p.rect_stroke(rr, 4.0, Stroke::new(if hovered { 2.0 } else { 1.0 }, if hovered { BEST } else { suit_ink(s) }), StrokeKind::Inside);
                    p.text(rr.center_top() + vec2(0.0, 2.0), Align2::CENTER_TOP, cards::rank_label(r), FontId::proportional(13.0), suit_ink(s));
                    paint_suit(p, rr.center_bottom() - vec2(0.0, 11.0), 13.0, s);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        picked = Some(k);
                    }
                }
                ui.end_row();
            }
        });
        if ui.button("Jolly").clicked() {
            picked = Some(JOKER);
        }
        if let Some(k) = picked {
            a.add(k);
        }
        if let Some(m) = &a.msg {
            ui.label(RichText::new(m).color(Color32::YELLOW));
        }
        ui.separator();
        let mut changed = false;
        changed |= ui.checkbox(&mut a.drawn, "Ho già pescato in questo turno").changed();
        changed |= ui.checkbox(&mut a.opened[0], "Ho già aperto").changed();
        changed |= ui.checkbox(&mut a.opened[1], "L'avversario ha aperto").changed();
        ui.horizontal(|ui| {
            ui.label("Carte dell'avversario");
            changed |= ui.add(egui::DragValue::new(&mut a.opp_len).range(1..=21)).changed();
        });
        if changed {
            a.refresh();
        }
        ui.separator();
        ui.heading("Consiglio");
        match &a.advice {
            None => {
                ui.label(RichText::new("Inserisci la tua mano").weak());
            }
            Some(lines) => {
                for l in lines {
                    ui.label(RichText::new(l).size(16.0).color(BEST));
                }
            }
        }
        if a.plan.is_some() && ui.button("Fatto: applica il consiglio").on_hover_text("Aggiorna mano, tavolo e pozzo come se avessi giocato la mossa consigliata").clicked() {
            a.apply_plan();
        }
        if a.ranking.len() > 1 {
            ui.add_space(4.0);
            ui.label(RichText::new("Scarti a confronto (0 = il migliore):").weak().small());
            let best = a.ranking[0].1;
            for &(c, v) in a.ranking.iter().take(5) {
                ui.label(RichText::new(format!("{:+6.1}   {}", v - best, cards::short(c))).monospace());
            }
        }
    }

    fn assist_table(&mut self, ui: &mut egui::Ui) {
        let white = |a: u8| Color32::from_white_alpha(a);
        let a = &mut self.assist;
        let mut changed = false;
        ui.add_space(10.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("La mia mano ({} carte) · clic per togliere", a.hand.len())).color(white(220)).strong());
                    ui.horizontal_wrapped(|ui| {
                        let hand = sorted(&a.hand);
                        let best = a.ranking.first().map(|x| x.0);
                        for c in hand {
                            let (rect, resp) = ui.allocate_exact_size(HAND_CARD, Sense::click());
                            let glow = (Some(c) == best).then_some(BEST);
                            paint_face(ui.painter(), rect, c, Look { glow, lifted: resp.hovered(), dim: false });
                            if resp.clicked() {
                                a.hand.retain(|&x| x != c);
                                changed = true;
                            }
                        }
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Cima del pozzo").color(white(200)));
                            let (rect, resp) = ui.allocate_exact_size(HAND_CARD, Sense::click());
                            match a.top {
                                Some(c) => paint_face(ui.painter(), rect, c, Look::default()),
                                None => cards_ui::paint_slot(ui.painter(), rect, "—"),
                            }
                            if resp.clicked() && a.top.is_some() {
                                a.top = None;
                                changed = true;
                            }
                        });
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("Carte già scartate ({})", a.pile.len())).color(white(200)));
                            ui.horizontal_wrapped(|ui| {
                                ui.set_max_width(420.0);
                                let pile = a.pile.clone();
                                for c in pile {
                                    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 40.0), Sense::click());
                                    paint_face(ui.painter(), rect, c, Look { dim: resp.hovered(), ..Default::default() });
                                    if resp.clicked() {
                                        a.pile.retain(|&x| x != c);
                                        changed = true;
                                    }
                                }
                            });
                        });
                    });
                    ui.add_space(10.0);
                    ui.label(RichText::new("Combinazioni in tavola · clic per togliere").color(white(220)).strong());
                    ui.horizontal_wrapped(|ui| {
                        let mut remove = None;
                        for (i, m) in a.melds.iter().enumerate() {
                            if let Some(r) = meld_widget(ui, m, false) {
                                if r.clicked() {
                                    remove = Some(i);
                                }
                            }
                            ui.add_space(6.0);
                        }
                        if let Some(i) = remove {
                            a.melds.remove(i);
                            changed = true;
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Nuova combinazione: {}", if a.builder.is_empty() { "—".into() } else { labels(&a.builder) })).color(white(200)));
                        if !a.builder.is_empty() {
                            if ui.button("Metti in tavola").clicked() {
                                match Meld::new(&a.builder, OPP) {
                                    Some(m) => {
                                        a.melds.push(m);
                                        a.builder.clear();
                                        changed = true;
                                    }
                                    None => a.msg = Some("Non è una combinazione valida".into()),
                                }
                            }
                            if ui.button("Annulla").clicked() {
                                a.builder.clear();
                            }
                        }
                    });
                });
            });
        });
        if changed {
            a.refresh();
        }
    }
}

/// Una combinazione in tavola come fila di carte sovrapposte; cliccabile se `active`.
fn meld_widget(ui: &mut egui::Ui, m: &Meld, active: bool) -> Option<egui::Response> {
    let step = 22.0;
    let w = step * (m.cards.len() - 1) as f32 + TABLE_CARD.x;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, TABLE_CARD.y), Sense::click());
    for (i, &c) in m.cards.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(i as f32 * step, 0.0), TABLE_CARD);
        paint_face(ui.painter(), r, c, Look::default());
    }
    if active {
        ui.painter().rect_stroke(rect.expand(3.0), 6.0, Stroke::new(3.0, BEST), StrokeKind::Outside);
    }
    Some(resp.on_hover_text(format!("{} · {} punti", m.label(), m.value())))
}
