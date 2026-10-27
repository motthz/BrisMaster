//! Grafica delle carte francesi per il poker all'italiana: semi disegnati a vettori.

use crate::cards_ui::{paint_back, GOLD};
use eframe::egui::{self, epaint::Shadow, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};
use poker_it::cards::{rank, suit, Card, RANK_SHORT};

pub const CARD: Vec2 = Vec2::new(92.0, 132.0);

const PAPER: Color32 = Color32::from_rgb(251, 249, 243);
const BLACK: Color32 = Color32::from_rgb(30, 30, 34);
const RED: Color32 = Color32::from_rgb(200, 30, 40);

/// Colore del seme: picche e fiori neri, quadri e cuori rossi.
pub fn suit_color(s: u8) -> Color32 {
    if s >= 2 { RED } else { BLACK }
}

/// Simbolo del seme (0 picche, 1 fiori, 2 quadri, 3 cuori) centrato in `c`, alto circa `d`.
pub fn paint_suit(p: &Painter, c: Pos2, d: f32, s: u8) {
    let col = suit_color(s);
    let at = |x: f32, y: f32| c + vec2(x * d, y * d);
    let tri = |a: Pos2, b: Pos2, e: Pos2| Shape::convex_polygon(vec![a, b, e], col, Stroke::NONE);
    match s {
        0 => {
            p.circle_filled(at(-0.22, 0.08), 0.25 * d, col);
            p.circle_filled(at(0.22, 0.08), 0.25 * d, col);
            p.add(tri(at(-0.46, 0.0), at(0.0, -0.5), at(0.46, 0.0)));
            p.add(tri(at(0.0, 0.1), at(0.17, 0.48), at(-0.17, 0.48)));
        }
        1 => {
            for (x, y) in [(0.0, -0.24), (-0.25, 0.08), (0.25, 0.08)] {
                p.circle_filled(at(x, y), 0.21 * d, col);
            }
            p.add(tri(at(0.0, 0.0), at(0.17, 0.48), at(-0.17, 0.48)));
        }
        2 => {
            p.add(Shape::convex_polygon(vec![at(0.0, -0.5), at(0.36, 0.0), at(0.0, 0.5), at(-0.36, 0.0)], col, Stroke::NONE));
        }
        _ => {
            p.circle_filled(at(-0.22, -0.14), 0.25 * d, col);
            p.circle_filled(at(0.22, -0.14), 0.25 * d, col);
            p.add(tri(at(-0.46, -0.06), at(0.46, -0.06), at(0.0, 0.46)));
        }
    }
}

/// Faccia della carta. `raised`: sollevata (scelta per il cambio); `glow`: bordo colorato.
pub fn paint_face(p: &Painter, rect: Rect, c: Card, glow: Option<Color32>, dim: bool) {
    let mut p = p.clone();
    if dim {
        p.multiply_opacity(0.45);
    }
    let h = rect.height();
    let r = 0.07 * h;
    let shadow = Shadow { offset: [2, 4], blur: 10, spread: 0, color: Color32::from_black_alpha(110) };
    p.add(shadow.as_shape(rect, r));
    p.rect_filled(rect, r, PAPER);
    let border = glow.map_or(Stroke::new(1.0, Color32::from_gray(170)), |g| Stroke::new(3.5, g));
    p.rect_stroke(rect, r, border, StrokeKind::Inside);
    let (k, s) = (rank(c) as usize, suit(c));
    let col = suit_color(s);
    let corner = |pos: Pos2, align: Align2| {
        p.text(pos, align, RANK_SHORT[k], FontId::proportional(0.17 * h), col);
    };
    corner(rect.left_top() + vec2(0.07 * h, 0.04 * h), Align2::LEFT_TOP);
    paint_suit(&p, rect.left_top() + vec2(0.12 * h, 0.27 * h), 0.12 * h, s);
    corner(rect.right_bottom() - vec2(0.07 * h, 0.04 * h), Align2::RIGHT_BOTTOM);
    paint_suit(&p, rect.right_bottom() - vec2(0.12 * h, 0.27 * h), 0.12 * h, s);
    if k >= 4 && k <= 6 {
        // Figure: cornice dorata.
        let inner = rect.shrink2(vec2(0.2 * rect.width(), 0.2 * h));
        p.rect_stroke(inner, 0.03 * h, Stroke::new(1.5, GOLD), StrokeKind::Inside);
        p.text(inner.center() - vec2(0.0, 0.1 * h), Align2::CENTER_CENTER, RANK_SHORT[k], FontId::proportional(0.22 * h), col);
        paint_suit(&p, inner.center() + vec2(0.0, 0.15 * h), 0.18 * h, s);
    } else {
        paint_suit(&p, rect.center(), if k == 7 { 0.42 * h } else { 0.3 * h }, s);
    }
}

/// Carta della mano: faccia o dorso. Restituisce la risposta per i clic.
pub fn card(ui: &mut egui::Ui, c: Option<Card>, glow: Option<Color32>, raised: bool, clickable: bool, dim: bool) -> egui::Response {
    let (slot, resp) = ui.allocate_exact_size(CARD + vec2(0.0, 18.0), if clickable { Sense::click() } else { Sense::hover() });
    let lift = if raised { 0.0 } else if clickable && resp.hovered() { 12.0 } else { 18.0 };
    let rect = Rect::from_min_size(pos2(slot.min.x, slot.min.y + lift), CARD);
    match c {
        Some(c) => paint_face(ui.painter(), rect, c, glow, dim),
        None => paint_back(ui.painter(), rect),
    }
    if clickable {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

pub const MINI: Vec2 = Vec2::new(40.0, 56.0);

/// Carta in miniatura per il selettore: valore e seme.
pub fn paint_mini(p: &Painter, rect: Rect, c: Card, enabled: bool, hovered: bool) {
    let mut p = p.clone();
    if !enabled {
        p.multiply_opacity(0.2);
    }
    let h = rect.height();
    let rect = if hovered { rect.translate(vec2(0.0, -3.0)) } else { rect };
    p.rect_filled(rect, 0.1 * h, PAPER);
    let border = if hovered { Stroke::new(2.5, crate::cards_ui::BEST) } else { Stroke::new(1.0, Color32::from_gray(150)) };
    p.rect_stroke(rect, 0.1 * h, border, StrokeKind::Inside);
    let col = suit_color(suit(c));
    p.text(rect.center_top() + vec2(0.0, 0.05 * h), Align2::CENTER_TOP, RANK_SHORT[rank(c) as usize], FontId::proportional(0.32 * h), col);
    paint_suit(&p, rect.center_bottom() - vec2(0.0, 0.26 * h), 0.34 * h, suit(c));
}

/// Griglia 4×8 per scegliere una carta; `allowed(c)` dice quali sono cliccabili.
pub fn picker(ui: &mut egui::Ui, id: &str, allowed: impl Fn(Card) -> bool) -> Option<Card> {
    let mut chosen = None;
    let w = 24.0 + 8.0 * (MINI.x + 5.0) + 5.0;
    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
    egui::Grid::new(id).spacing([5.0, 6.0]).show(ui, |ui| {
        for s in (0..4u8).rev() {
            let (r, _) = ui.allocate_exact_size(vec2(24.0, MINI.y), Sense::hover());
            ui.painter().rect_filled(r.shrink(1.0), 4.0, PAPER);
            paint_suit(ui.painter(), r.center(), 18.0, s);
            for k in (0..8u8).rev() {
                let c = poker_it::cards::make(k, s);
                let ok = allowed(c);
                let (rect, resp) = ui.allocate_exact_size(MINI, if ok { Sense::click() } else { Sense::hover() });
                paint_mini(ui.painter(), rect, c, ok, ok && resp.hovered());
                if ok && resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(poker_it::cards::name(c)).clicked() {
                    chosen = Some(c);
                }
            }
            ui.end_row();
        }
    });
    chosen
}
