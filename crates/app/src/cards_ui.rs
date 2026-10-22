//! Grafica delle carte napoletane: semi disegnati a vettori, dorso, tappeto e selettore di carte.

use briscola_core::cards::{self, Card, CardSet};
use eframe::egui::{
    self, epaint::Shadow, pos2, vec2, Align2, Color32, FontId, Mesh, Painter, Pos2, Rect, Sense,
    Stroke, StrokeKind, Vec2,
};

pub const CARD_SIZE: Vec2 = Vec2::new(96.0, 148.0);
pub const MINI_SIZE: Vec2 = Vec2::new(40.0, 58.0);

const PAPER: Color32 = Color32::from_rgb(251, 247, 236);
const INK: Color32 = Color32::from_rgb(45, 40, 35);
pub const GOLD: Color32 = Color32::from_rgb(222, 178, 70);
pub const BEST: Color32 = Color32::from_rgb(120, 230, 140);
const FELT_TOP: Color32 = Color32::from_rgb(22, 92, 62);
const FELT_BOTTOM: Color32 = Color32::from_rgb(10, 58, 40);

/// Colore di riferimento di ogni seme (testo e bordi).
pub fn suit_color(s: u8) -> Color32 {
    match s {
        0 => Color32::from_rgb(196, 140, 20),
        1 => Color32::from_rgb(196, 52, 46),
        2 => Color32::from_rgb(52, 98, 190),
        _ => Color32::from_rgb(46, 132, 62),
    }
}

/// Versione chiara del colore del seme, leggibile su sfondo scuro.
pub fn suit_color_light(s: u8) -> Color32 {
    match s {
        0 => Color32::from_rgb(240, 196, 80),
        1 => Color32::from_rgb(240, 110, 100),
        2 => Color32::from_rgb(120, 160, 245),
        _ => Color32::from_rgb(110, 200, 120),
    }
}

/// Disegna il simbolo del seme centrato in `c`, alto circa `d`.
pub fn paint_suit(p: &Painter, c: Pos2, d: f32, suit: u8) {
    let at = |x: f32, y: f32| pos2(c.x + x * d, c.y + y * d);
    match suit {
        0 => {
            // Denari: moneta d'oro con il centro rosso.
            p.circle_filled(c, 0.46 * d, Color32::from_rgb(232, 182, 48));
            p.circle_stroke(
                c,
                0.46 * d,
                Stroke::new(0.06 * d, Color32::from_rgb(150, 100, 15)),
            );
            p.circle_stroke(
                c,
                0.30 * d,
                Stroke::new(0.04 * d, Color32::from_rgb(170, 115, 20)),
            );
            p.circle_filled(c, 0.13 * d, Color32::from_rgb(190, 50, 40));
        }
        1 => {
            // Coppe: calice rosso con l'orlo dorato.
            let red = Color32::from_rgb(196, 52, 46);
            let bowl: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = std::f32::consts::PI * i as f32 / 16.0;
                    at(0.38 * a.cos(), -0.36 + 0.40 * a.sin())
                })
                .collect();
            p.add(egui::Shape::convex_polygon(bowl, red, Stroke::NONE));
            p.rect_filled(
                Rect::from_min_max(at(-0.43, -0.42), at(0.43, -0.33)),
                0.03 * d,
                GOLD,
            );
            p.rect_filled(Rect::from_min_max(at(-0.05, 0.0), at(0.05, 0.32)), 0.0, red);
            p.circle_filled(at(0.0, 0.16), 0.08 * d, GOLD);
            p.rect_filled(
                Rect::from_min_max(at(-0.26, 0.30), at(0.26, 0.42)),
                0.05 * d,
                red,
            );
        }
        2 => {
            // Spade: lama d'acciaio, elsa d'oro.
            let blade = vec![
                at(0.0, -0.50),
                at(0.08, -0.36),
                at(0.08, 0.16),
                at(-0.08, 0.16),
                at(-0.08, -0.36),
            ];
            p.add(egui::Shape::convex_polygon(
                blade,
                Color32::from_rgb(70, 112, 200),
                Stroke::NONE,
            ));
            p.line_segment(
                [at(0.0, -0.40), at(0.0, 0.14)],
                Stroke::new(0.025 * d, Color32::from_rgb(190, 210, 245)),
            );
            p.rect_filled(
                Rect::from_min_max(at(-0.27, 0.15), at(0.27, 0.23)),
                0.03 * d,
                GOLD,
            );
            p.rect_filled(
                Rect::from_min_max(at(-0.045, 0.23), at(0.045, 0.40)),
                0.0,
                Color32::from_rgb(110, 70, 40),
            );
            p.circle_filled(at(0.0, 0.44), 0.065 * d, GOLD);
        }
        _ => {
            // Bastoni: randello verde con i nodi.
            let green = Color32::from_rgb(52, 140, 66);
            let (a, b) = (at(-0.13, 0.44), at(0.13, -0.40));
            p.line_segment([a, b], Stroke::new(0.17 * d, green));
            p.circle_filled(a, 0.085 * d, green);
            p.circle_filled(b, 0.14 * d, green);
            for t in [0.25f32, 0.5, 0.75] {
                let k = a + (b - a) * t;
                p.circle_filled(
                    k + vec2(0.06 * d, 0.0),
                    0.04 * d,
                    Color32::from_rgb(25, 80, 35),
                );
            }
        }
    }
}

/// Posizioni dei semi sulle carte numeriche (da 2 a 7), in un riquadro [-1, 1]².
fn pips(n: u8) -> &'static [(f32, f32)] {
    match n {
        2 => &[(0.0, -0.6), (0.0, 0.6)],
        3 => &[(0.0, -0.72), (0.0, 0.0), (0.0, 0.72)],
        4 => &[(-0.5, -0.6), (0.5, -0.6), (-0.5, 0.6), (0.5, 0.6)],
        5 => &[
            (-0.5, -0.65),
            (0.5, -0.65),
            (0.0, 0.0),
            (-0.5, 0.65),
            (0.5, 0.65),
        ],
        6 => &[
            (-0.5, -0.72),
            (0.5, -0.72),
            (-0.5, 0.0),
            (0.5, 0.0),
            (-0.5, 0.72),
            (0.5, 0.72),
        ],
        _ => &[
            (-0.5, -0.72),
            (0.5, -0.72),
            (0.0, -0.36),
            (-0.5, 0.0),
            (0.5, 0.0),
            (-0.5, 0.72),
            (0.5, 0.72),
        ],
    }
}

/// Aspetto di una carta disegnata.
#[derive(Clone, Copy, Default)]
pub struct Look {
    /// Bordo luminoso (carta consigliata).
    pub glow: Option<Color32>,
    /// Opacità (carte non disponibili o briscola già pescata).
    pub dim: bool,
    /// Sollevata (passaggio del mouse).
    pub lifted: bool,
}

/// Disegna la faccia di una carta nel rettangolo dato.
pub fn paint_face(p: &Painter, rect: Rect, c: Card, look: Look) {
    let mut p = p.clone();
    if look.dim {
        p.multiply_opacity(0.45);
    }
    let rect = if look.lifted {
        rect.translate(vec2(0.0, -6.0))
    } else {
        rect
    };
    let h = rect.height();
    let r = 0.07 * h;
    let shadow = Shadow {
        offset: [2, 4],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    p.add(shadow.as_shape(rect, r));
    p.rect_filled(rect, r, PAPER);
    let (s, k) = (cards::suit(c), cards::rank(c));
    let col = suit_color(s);
    p.rect_stroke(
        rect.shrink(0.035 * h),
        0.05 * h,
        Stroke::new(1.0, col.gamma_multiply(0.6)),
        StrokeKind::Inside,
    );

    // Indici agli angoli.
    let rank = cards::RANK_SHORT[k as usize];
    let font = FontId::proportional(0.14 * h);
    p.text(
        rect.left_top() + vec2(0.1 * h, 0.035 * h),
        Align2::CENTER_TOP,
        rank,
        font.clone(),
        col,
    );
    p.text(
        rect.right_bottom() - vec2(0.1 * h, 0.035 * h),
        Align2::CENTER_BOTTOM,
        rank,
        font,
        col,
    );

    // Punti della carta in alto a destra.
    let pts = cards::points(c);
    if pts > 0 {
        let pill = Rect::from_center_size(
            rect.right_top() + vec2(-0.16 * h, 0.11 * h),
            vec2(0.2 * h, 0.12 * h),
        );
        p.rect_filled(pill, 0.06 * h, GOLD);
        p.text(
            pill.center(),
            Align2::CENTER_CENTER,
            pts.to_string(),
            FontId::proportional(0.085 * h),
            INK,
        );
    }

    // Centro della carta.
    let center = rect.center() + vec2(0.0, 0.02 * h);
    match k {
        0 => {
            p.circle_filled(center, 0.27 * h, col.gamma_multiply(0.12));
            paint_suit(&p, center, 0.42 * h, s);
        }
        1..=6 => {
            let half = vec2(0.17 * h, 0.27 * h);
            let size = if k >= 5 { 0.12 * h } else { 0.14 * h };
            for &(x, y) in pips(k + 1) {
                paint_suit(&p, center + vec2(x * half.x, y * half.y), size, s);
            }
        }
        _ => {
            // Figure: riquadro colorato con lettera, seme e nome.
            let panel = Rect::from_center_size(center, vec2(0.40 * h, 0.62 * h));
            p.rect_filled(panel, 0.04 * h, col.gamma_multiply(0.14));
            p.rect_stroke(panel, 0.04 * h, Stroke::new(1.5, col), StrokeKind::Inside);
            p.text(
                panel.center_top() + vec2(0.0, 0.03 * h),
                Align2::CENTER_TOP,
                rank,
                FontId::proportional(0.17 * h),
                col,
            );
            paint_suit(&p, panel.center() + vec2(0.0, 0.04 * h), 0.22 * h, s);
            p.text(
                panel.center_bottom() - vec2(0.0, 0.03 * h),
                Align2::CENTER_BOTTOM,
                cards::RANK_NAMES[k as usize],
                FontId::proportional(0.075 * h),
                INK,
            );
        }
    }
    if let Some(g) = look.glow {
        p.rect_stroke(
            rect.expand(2.0),
            r + 2.0,
            Stroke::new(4.0, g),
            StrokeKind::Outside,
        );
    }
}

/// Disegna il dorso di una carta.
pub fn paint_back(p: &Painter, rect: Rect) {
    let h = rect.height();
    let r = 0.07 * h;
    let shadow = Shadow {
        offset: [2, 4],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    p.add(shadow.as_shape(rect, r));
    p.rect_filled(rect, r, Color32::from_rgb(128, 28, 40));
    let inner = rect.shrink(0.06 * h);
    let clip = p.with_clip_rect(inner);
    let line = Stroke::new(1.0, Color32::from_rgb(160, 55, 62));
    let step = 0.07 * h;
    let mut x = inner.left() - inner.height();
    while x < inner.right() {
        clip.line_segment(
            [
                pos2(x, inner.bottom()),
                pos2(x + inner.height(), inner.top()),
            ],
            line,
        );
        clip.line_segment(
            [
                pos2(x, inner.top()),
                pos2(x + inner.height(), inner.bottom()),
            ],
            line,
        );
        x += step;
    }
    p.rect_stroke(inner, 0.04 * h, Stroke::new(1.5, GOLD), StrokeKind::Inside);
    p.circle_filled(rect.center(), 0.16 * h, Color32::from_rgb(128, 28, 40));
    p.circle_stroke(rect.center(), 0.16 * h, Stroke::new(1.5, GOLD));
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "BM",
        FontId::proportional(0.12 * h),
        GOLD,
    );
}

/// Spazio vuoto dove andrà una carta.
pub fn paint_slot(p: &Painter, rect: Rect, label: &str) {
    let h = rect.height();
    p.rect_filled(rect, 0.07 * h, Color32::from_black_alpha(40));
    p.rect_stroke(
        rect,
        0.07 * h,
        Stroke::new(1.5, Color32::from_white_alpha(50)),
        StrokeKind::Inside,
    );
    if !label.is_empty() {
        p.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(13.0),
            Color32::from_white_alpha(90),
        );
    }
}

/// Carta interattiva (faccia o dorso) di dimensione `size`.
pub fn card(
    ui: &mut egui::Ui,
    c: Option<Card>,
    size: Vec2,
    glow: Option<Color32>,
    clickable: bool,
) -> egui::Response {
    let sense = if clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    match c {
        Some(c) => {
            let look = Look {
                glow,
                dim: false,
                lifted: clickable && resp.hovered(),
            };
            paint_face(ui.painter(), rect, c, look);
        }
        None => paint_back(ui.painter(), rect),
    }
    if clickable {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Carta in miniatura per selettori e conteggi: valore e seme.
pub fn paint_mini(p: &Painter, rect: Rect, c: Card, enabled: bool, hovered: bool) {
    let h = rect.height();
    let (s, k) = (cards::suit(c), cards::rank(c));
    let mut p = p.clone();
    if !enabled {
        p.multiply_opacity(0.22);
    }
    let rect = if hovered {
        rect.translate(vec2(0.0, -3.0))
    } else {
        rect
    };
    p.rect_filled(rect, 0.1 * h, PAPER);
    let border = if hovered {
        Stroke::new(2.5, BEST)
    } else {
        Stroke::new(1.0, suit_color(s))
    };
    p.rect_stroke(rect, 0.1 * h, border, StrokeKind::Inside);
    p.text(
        rect.center_top() + vec2(0.0, 0.05 * h),
        Align2::CENTER_TOP,
        cards::RANK_SHORT[k as usize],
        FontId::proportional(0.34 * h),
        suit_color(s),
    );
    paint_suit(&p, rect.center_bottom() - vec2(0.0, 0.27 * h), 0.36 * h, s);
}

/// Ordine di presentazione dei valori: dal più forte al più debole.
pub const RANK_ORDER: [u8; 10] = [0, 2, 9, 8, 7, 6, 5, 4, 3, 1];

/// Griglia 4×10 per scegliere una carta. Solo le carte in `allowed` sono cliccabili.
pub fn picker(ui: &mut egui::Ui, id: &str, allowed: CardSet) -> Option<Card> {
    let mut chosen = None;
    egui::Grid::new(id).spacing([5.0, 6.0]).show(ui, |ui| {
        for s in 0..4u8 {
            let (r, _) = ui.allocate_exact_size(vec2(26.0, MINI_SIZE.y), Sense::hover());
            paint_suit(ui.painter(), r.center(), 24.0, s);
            for k in RANK_ORDER {
                let c = cards::make(s, k);
                let ok = allowed.contains(c);
                let (rect, resp) = ui.allocate_exact_size(
                    MINI_SIZE,
                    if ok { Sense::click() } else { Sense::hover() },
                );
                paint_mini(ui.painter(), rect, c, ok, ok && resp.hovered());
                if ok
                    && resp
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(cards::name(c))
                        .clicked()
                {
                    chosen = Some(c);
                }
            }
            ui.end_row();
        }
    });
    chosen
}

/// Tappeto verde con una leggera sfumatura verticale.
pub fn paint_felt(p: &Painter, rect: Rect) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), FELT_TOP);
    mesh.colored_vertex(rect.right_top(), FELT_TOP);
    mesh.colored_vertex(rect.left_bottom(), FELT_BOTTOM);
    mesh.colored_vertex(rect.right_bottom(), FELT_BOTTOM);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    p.add(mesh);
    p.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::from_rgb(40, 120, 85)),
        StrokeKind::Inside,
    );
}

/// Riconosce una carta scritta a tastiera: "3c", "AD", "R s", "asso di coppe", "cavallo bastoni".
pub fn parse_card(text: &str) -> Option<Card> {
    let t = text.trim().to_lowercase();
    let words: Vec<&str> = t
        .split_whitespace()
        .filter(|w| *w != "di" && *w != "de")
        .collect();
    let (rank_s, suit_s): (String, String) = match words.as_slice() {
        [one] => {
            let mut chars: Vec<char> = one.chars().collect();
            let s = chars.pop()?;
            (chars.into_iter().collect(), s.to_string())
        }
        [r, s] => (r.to_string(), s.to_string()),
        _ => return None,
    };
    let rank = match rank_s.as_str() {
        "a" | "1" | "asso" => 0,
        "2" | "due" => 1,
        "3" | "tre" => 2,
        "4" | "quattro" => 3,
        "5" | "cinque" => 4,
        "6" | "sei" => 5,
        "7" | "sette" => 6,
        "f" | "j" | "8" | "fante" => 7,
        "c" | "q" | "9" | "cavallo" | "donna" => 8,
        "r" | "k" | "10" | "re" => 9,
        _ => return None,
    };
    let suit = match suit_s.chars().next()? {
        'd' | 'o' => 0,
        'c' => 1,
        's' => 2,
        'b' => 3,
        _ => return None,
    };
    Some(cards::make(suit, rank))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cards() {
        assert_eq!(parse_card("3c"), Some(cards::make(1, 2)));
        assert_eq!(parse_card("AD"), Some(cards::make(0, 0)));
        assert_eq!(parse_card("cc"), Some(cards::make(1, 8)));
        assert_eq!(parse_card(" R s "), Some(cards::make(2, 9)));
        assert_eq!(parse_card("asso di coppe"), Some(cards::make(1, 0)));
        assert_eq!(parse_card("10b"), Some(cards::make(3, 9)));
        assert_eq!(parse_card("fante bastoni"), Some(cards::make(3, 7)));
        assert_eq!(parse_card("x"), None);
        assert_eq!(parse_card("3x"), None);
    }
}
