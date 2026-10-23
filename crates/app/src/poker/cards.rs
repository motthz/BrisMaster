//! Carte francesi per il Texas Hold'em: semi disegnati a vettori, così non dipendono dai font.

use crate::cards_ui::{paint_back, paint_slot};
use eframe::egui::{self, epaint::Shadow, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};
use poker_core::cards::{rank, suit, Card, RANKS};

pub const SIZE: Vec2 = Vec2::new(84.0, 120.0);
const PAPER: Color32 = Color32::from_rgb(251, 249, 244);
const RED: Color32 = Color32::from_rgb(200, 36, 44);
const BLACK: Color32 = Color32::from_rgb(30, 30, 34);

/// Semi nell'ordine di `poker_core::cards`: picche, cuori, quadri, fiori.
pub fn suit_color(s: u8) -> Color32 {
    if s == 1 || s == 2 { RED } else { BLACK }
}

pub fn rank_label(c: Card) -> String {
    match RANKS[rank(c) as usize] {
        b'T' => "10".into(),
        r => (r as char).to_string(),
    }
}

/// Disegna il seme centrato in `c`, alto circa `d`.
pub fn paint_suit(p: &Painter, c: Pos2, d: f32, s: u8) {
    let col = suit_color(s);
    let r = d * 0.27;
    match s {
        // Cuori: due cerchi e un triangolo verso il basso.
        1 => {
            p.circle_filled(c + vec2(-r * 0.9, -r * 0.5), r, col);
            p.circle_filled(c + vec2(r * 0.9, -r * 0.5), r, col);
            p.add(Shape::convex_polygon(
                vec![c + vec2(-r * 1.85, -r * 0.2), c + vec2(r * 1.85, -r * 0.2), c + vec2(0.0, d * 0.5)],
                col,
                Stroke::NONE,
            ));
        }
        // Quadri: rombo.
        2 => {
            p.add(Shape::convex_polygon(
                vec![c + vec2(0.0, -d * 0.5), c + vec2(d * 0.36, 0.0), c + vec2(0.0, d * 0.5), c + vec2(-d * 0.36, 0.0)],
                col,
                Stroke::NONE,
            ));
        }
        // Picche: cuore rovesciato con gambo.
        0 => {
            p.circle_filled(c + vec2(-r * 0.9, r * 0.35), r, col);
            p.circle_filled(c + vec2(r * 0.9, r * 0.35), r, col);
            p.add(Shape::convex_polygon(
                vec![c + vec2(0.0, -d * 0.5), c + vec2(r * 1.85, r * 0.1), c + vec2(-r * 1.85, r * 0.1)],
                col,
                Stroke::NONE,
            ));
            stem(p, c, d, col);
        }
        // Fiori: tre cerchi con gambo.
        _ => {
            p.circle_filled(c + vec2(0.0, -r * 0.95), r, col);
            p.circle_filled(c + vec2(-r * 1.05, r * 0.45), r, col);
            p.circle_filled(c + vec2(r * 1.05, r * 0.45), r, col);
            p.circle_filled(c + vec2(0.0, r * 0.1), r * 0.6, col);
            stem(p, c, d, col);
        }
    }
}

fn stem(p: &Painter, c: Pos2, d: f32, col: Color32) {
    p.add(Shape::convex_polygon(
        vec![c + vec2(-d * 0.05, d * 0.15), c + vec2(d * 0.05, d * 0.15), c + vec2(d * 0.17, d * 0.5), c + vec2(-d * 0.17, d * 0.5)],
        col,
        Stroke::NONE,
    ));
}

pub fn paint_face(p: &Painter, rect: Rect, c: Card, glow: Option<Color32>, dim: bool) {
    let mut p = p.clone();
    if dim {
        p.multiply_opacity(0.4);
    }
    let h = rect.height();
    let r = 0.07 * h;
    let shadow = Shadow { offset: [2, 4], blur: 10, spread: 0, color: Color32::from_black_alpha(110) };
    p.add(shadow.as_shape(rect, r));
    p.rect_filled(rect, r, PAPER);
    let s = suit(c);
    let col = suit_color(s);
    let label = rank_label(c);
    p.text(rect.left_top() + vec2(0.11 * h, 0.03 * h), Align2::CENTER_TOP, &label, FontId::proportional(0.17 * h), col);
    paint_suit(&p, rect.left_top() + vec2(0.11 * h, 0.27 * h), 0.12 * h, s);
    paint_suit(&p, rect.center() + vec2(0.06 * h, 0.08 * h), 0.4 * h, s);
    if let Some(g) = glow {
        p.rect_stroke(rect.expand(2.0), r + 2.0, Stroke::new(4.0, g), StrokeKind::Outside);
    }
}

/// Carta (faccia, dorso o spazio vuoto) di dimensione `size`.
pub fn card(ui: &mut egui::Ui, c: Option<Card>, face_up: bool, size: Vec2, glow: Option<Color32>) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    match (c, face_up) {
        (Some(c), true) => paint_face(ui.painter(), rect, c, glow, false),
        (Some(_), false) => paint_back(ui.painter(), rect),
        (None, _) => paint_slot(ui.painter(), rect, ""),
    }
}
