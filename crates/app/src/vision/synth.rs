//! Tavoli finti per i test: le carte di un mazzo qualsiasi disposte come su un sito di briscola
//! (mano in basso, carte giocate al centro, briscola di traverso sotto il mazzo, dorsi
//! dell'avversario in alto), su tavoli, misure, proporzioni, colori e sfocature diversi. Servono a
//! controllare che il riconoscimento regga su grafiche che non conosce.

use super::Img;

/// Sfondo del tavolo.
#[derive(Clone, Copy, Debug)]
pub enum Bg {
    /// Panno di un colore, più chiaro al centro.
    Felt([u8; 3]),
    /// Il legno chiaro di Solitalian (preso dallo screenshot vero).
    Wood,
    /// La foto del mare di Solitalian, con scogli scuri, acqua e alghe.
    Sea,
    /// La stessa foto, nella parte del cielo: chiaro e poco colorato, quasi come le carte.
    Sky,
}

/// Come appare il tavolo.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub bg: Bg,
    /// Altezza delle carte in pixel.
    pub card_h: f32,
    /// Larghezza / altezza delle carte disegnate (None: quella dell'immagine).
    pub aspect: Option<f32>,
    /// Rotazione della tinta in gradi, saturazione e gamma dei colori delle carte.
    pub hue: f32,
    pub sat: f32,
    pub gamma: f32,
    /// Sfocatura di tutto lo schermo.
    pub blur: bool,
    /// Rotazione delle carte della mano (gradi, a ventaglio).
    pub fan: f32,
    /// Mazzo e briscola rispetto alle altre carte.
    pub pile: f32,
    /// Frazione della briscola coperta dal mazzo.
    pub cover: f32,
}

impl Default for Look {
    fn default() -> Look {
        Look {
            bg: Bg::Felt([20, 105, 70]),
            card_h: 160.0,
            aspect: None,
            hue: 0.0,
            sat: 1.0,
            gamma: 1.0,
            blur: false,
            fan: 0.0,
            pile: 1.0,
            cover: 0.45,
        }
    }
}

/// Le carte sul tavolo.
pub struct Deal<'a> {
    pub hand: Vec<&'a Img>,
    pub opp: Option<&'a Img>,
    pub mine: Option<&'a Img>,
    pub trump: Option<&'a Img>,
}

const W: usize = 1400;
const H: usize = 900;

fn screenshot(name: &str) -> Img {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata").join(name);
    super::capture::load_png(&path).unwrap()
}

/// Sfondo ripetuto a specchio (senza cuciture) da un pezzo di screenshot senza carte.
fn texture(bg: Bg) -> Option<Img> {
    match bg {
        Bg::Felt(_) => None,
        Bg::Wood => Some(screenshot("solitalian_briscola.png").crop(950, 200, 1450, 950)),
        Bg::Sea => Some(screenshot("solitalian_mare.jpg").crop(880, 600, 1440, 1000)),
        Bg::Sky => Some(screenshot("solitalian_mare.jpg").crop(330, 250, 1280, 470)),
    }
}

/// Punto della texture ripetuta a specchio.
fn mirrored(t: &Img, x: usize, y: usize) -> [u8; 3] {
    let fold = |v: usize, n: usize| {
        let k = v % (2 * n);
        if k < n {
            k
        } else {
            2 * n - 1 - k
        }
    };
    t.at(fold(x, t.w), fold(y, t.h))
}

/// Colore della carta dopo tinta, saturazione e gamma.
fn tint(p: [u8; 3], look: &Look) -> [u8; 3] {
    let [r, g, b] = p.map(|v| (v as f32 / 255.0).powf(look.gamma) * 255.0);
    // Rotazione della tinta nello spazio YIQ, poi saturazione attorno alla luminosità.
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    let i = 0.596 * r - 0.274 * g - 0.322 * b;
    let q = 0.211 * r - 0.523 * g + 0.312 * b;
    let (s, c) = look.hue.to_radians().sin_cos();
    let (i, q) = ((i * c - q * s) * look.sat, (i * s + q * c) * look.sat);
    let out = [y + 0.956 * i + 0.621 * q, y - 0.272 * i - 0.647 * q, y - 1.106 * i + 1.703 * q];
    out.map(|v| v.round().clamp(0.0, 255.0) as u8)
}

/// Disegna un'immagine (una carta o un dorso) di `w` x `h` centrata in (cx, cy), girata di
/// `angle` gradi, con un filo d'ombra attorno. `paint` dà il colore di un punto (u, v) in 0..1.
fn draw(img: &mut Img, cx: f32, cy: f32, w: f32, h: f32, angle: f32, paint: &dyn Fn(f32, f32) -> [u8; 3]) {
    let (s, c) = angle.to_radians().sin_cos();
    let ext = (w.hypot(h) / 2.0 + 4.0) as isize;
    for y in (cy as isize - ext).max(0)..(cy as isize + ext).min(H as isize) {
        for x in (cx as isize - ext).max(0)..(cx as isize + ext).min(W as isize) {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            // Coordinate nella carta (prima della rotazione).
            let (a, b) = (dx * c + dy * s, -dx * s + dy * c);
            let (u, v) = (a / w + 0.5, b / h + 0.5);
            let i = y as usize * W + x as usize;
            if (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v) {
                img.px[i] = paint(u, v);
            } else if (-2.0 / w..1.0 + 2.0 / w).contains(&u) && (-2.0 / h..1.0 + 2.0 / h).contains(&v) {
                // Ombra di due pixel.
                img.px[i] = img.px[i].map(|p| (p as f32 * 0.6) as u8);
            }
        }
    }
}

fn draw_card(img: &mut Img, card: &Img, look: &Look, cx: f32, cy: f32, h: f32, angle: f32) {
    let w = h * look.aspect.unwrap_or(card.w as f32 / card.h as f32);
    let paint = |u: f32, v: f32| tint(card.sample(u * (card.w - 1) as f32, v * (card.h - 1) as f32), look);
    draw(img, cx, cy, w, h, angle, &paint);
}

/// Un dorso rosso col bordo bianco.
fn draw_back(img: &mut Img, cx: f32, cy: f32, w: f32, h: f32) {
    let paint = |u: f32, v: f32| {
        let edge = u.min(1.0 - u) * w < 4.0 || v.min(1.0 - v) * h < 4.0;
        if edge {
            [250, 250, 250]
        } else if ((u * 12.0) as i32 + (v * 18.0) as i32) % 2 == 0 {
            [170, 30, 40]
        } else {
            [140, 20, 32]
        }
    };
    draw(img, cx, cy, w, h, 0.0, &paint);
}

/// Il tavolo con le carte, come lo vedrebbe la lettura dello schermo.
pub fn table(deal: &Deal, look: &Look) -> Img {
    let mut img = Img::new(W, H);
    let tex = texture(look.bg);
    for y in 0..H {
        for x in 0..W {
            img.px[y * W + x] = match (&look.bg, &tex) {
                (Bg::Felt(c), _) => {
                    // Più chiaro al centro.
                    let d = ((x as f32 / W as f32 - 0.5).powi(2) + (y as f32 / H as f32 - 0.5).powi(2)).sqrt();
                    c.map(|v| (v as f32 * (1.25 - 0.6 * d)).min(255.0) as u8)
                }
                (_, Some(t)) => mirrored(t, x, y),
                _ => [128, 128, 128],
            };
        }
    }
    let h = look.card_h;
    let w = h * look.aspect.unwrap_or(0.62);
    // I dorsi dell'avversario in alto.
    for k in 0..3 {
        draw_back(&mut img, W as f32 / 2.0 + (k as f32 - 1.0) * (w + 8.0), h * 0.55 + 10.0, w, h);
    }
    // Il mazzo e la briscola di traverso, a sinistra: la briscola sporge a destra del mazzo.
    let (ph, pw) = (h * look.pile, w * look.pile);
    let (deck_x, deck_y) = (W as f32 * 0.14, H as f32 * 0.5);
    if let Some(t) = deal.trump {
        // La briscola è lunga ph: ne resta coperta la frazione `cover`, sotto il mazzo.
        let left = deck_x + pw / 2.0 - look.cover * ph;
        let paint_h = ph;
        let tw = paint_h * look.aspect.unwrap_or(t.w as f32 / t.h as f32);
        // Girata di 90 gradi: la testa a destra.
        let paint = |u: f32, v: f32| tint(t.sample(u * (t.w - 1) as f32, v * (t.h - 1) as f32), look);
        draw(&mut img, left + paint_h / 2.0, deck_y, tw, paint_h, 90.0, &paint);
    }
    draw_back(&mut img, deck_x, deck_y, pw, ph);
    // Le carte giocate al centro: quella dell'avversario più in alto, la tua più in basso.
    if let Some(c) = deal.opp {
        draw_card(&mut img, c, look, W as f32 / 2.0, H as f32 / 2.0 - h * 0.45, h, 0.0);
    }
    if let Some(c) = deal.mine {
        draw_card(&mut img, c, look, W as f32 / 2.0 + w * 0.3, H as f32 / 2.0 + h * 0.5, h, 0.0);
    }
    // La mano in basso, a ventaglio.
    let n = deal.hand.len() as f32;
    for (k, c) in deal.hand.iter().enumerate() {
        let off = k as f32 - (n - 1.0) / 2.0;
        draw_card(&mut img, c, look, W as f32 / 2.0 + off * (w + 12.0), H as f32 - h * 0.55 - 10.0, h, off * look.fan);
    }
    if look.blur {
        let src = img.clone();
        for y in 1..H - 1 {
            for x in 1..W - 1 {
                let mut acc = [0u32; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1), (-1, 0), (0, -1), (-1, -1), (1, -1), (-1, 1)] {
                    let p = src.at((x as isize + dx) as usize, (y as isize + dy) as usize);
                    for c in 0..3 {
                        acc[c] += p[c] as u32;
                    }
                }
                img.px[y * W + x] = acc.map(|a| (a / 9) as u8);
            }
        }
    }
    img
}
