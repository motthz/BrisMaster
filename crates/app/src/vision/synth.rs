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
    /// Scritte sul tavolo: finestrelle, chat, punteggi, nomi, scritte grandi (sotto le carte).
    pub writing: bool,
    /// Loghi e scritte stampati sul tavolo, sotto le carte (opacità 0..1; 0 niente).
    pub logo: f32,
    /// Centro del mazzo (frazioni dello schermo): a sinistra, a destra, dal mio lato (in basso),
    /// dal lato dell'avversario (in alto).
    pub pile_at: [f32; 2],
    /// Briscola in piedi accanto al mazzo invece che di traverso sotto il mazzo.
    pub trump_upright: bool,
    /// Dove stanno le due carte giocate.
    pub table: Table,
    /// Schermo a bassa risoluzione: le carte si disegnano a questa frazione della risoluzione e
    /// poi si riportano alla misura (pixel grossi e sfocati, come un sito rimpicciolito).
    pub lowres: f32,
    /// Raggio degli angoli arrotondati delle carte (frazione della larghezza; 0 angoli vivi).
    pub rounded: f32,
    /// Alone colorato attorno alle carte della mano (carte giocabili evidenziate), in pixel.
    pub glow: f32,
    /// Carte della mano scurite (non è il mio turno): fattore di luminosità (1 niente).
    pub dim: f32,
    /// Ombra morbida sotto le carte: spostamento in pixel (0 solo il filo d'ombra).
    pub shadow: f32,
    /// Colore dei dorsi (None: rosso a quadretti).
    pub back: Option<[u8; 3]>,
    /// Carte senza bordo bianco: il disegno arriva fino al taglio (ritaglio della frazione).
    pub bleed: f32,
}

/// Disposizione delle carte giocate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Table {
    /// Avversario in alto, io in basso un po' a destra.
    Classic,
    /// Una accanto all'altra alla stessa altezza: avversario a sinistra, io a destra.
    SideBySide,
    /// La mia sopra quella dell'avversario, coprendone una parte (come una presa raccolta).
    Overlap,
    /// La mia sopra quella dell'avversario, coprendone solo un angolo.
    LightOverlap,
    /// Avversario in alto e io in basso, tutte e due un po' storte.
    Tilted,
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
            writing: false,
            logo: 0.0,
            pile_at: [0.14, 0.5],
            trump_upright: false,
            table: Table::Classic,
            lowres: 1.0,
            rounded: 0.0,
            glow: 0.0,
            dim: 1.0,
            shadow: 0.0,
            back: None,
            bleed: 0.0,
        }
    }
}

/// Le carte sul tavolo.
pub struct Deal<'a> {
    pub hand: Vec<&'a Img>,
    pub opp: Option<&'a Img>,
    pub mine: Option<&'a Img>,
    pub trump: Option<&'a Img>,
    /// Il mazzo coperto sopra la briscola (finito, resta solo la briscola).
    pub deck: bool,
}

const W: usize = 1400;
const H: usize = 900;

fn screenshot(name: &str) -> Img {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata").join(name);
    super::capture::load_png(&path).unwrap()
}

/// Sfondo ripetuto a specchio (senza cuciture) da un pezzo di screenshot senza carte.
fn texture(bg: Bg) -> Option<Img> {
    // Le fixture sono immutabili: decodificare lo stesso screenshot a ogni
    // fotogramma rallenta il banco senza mettere alla prova il riconoscitore.
    static WOOD: std::sync::OnceLock<Img> = std::sync::OnceLock::new();
    static SEA: std::sync::OnceLock<Img> = std::sync::OnceLock::new();
    static SKY: std::sync::OnceLock<Img> = std::sync::OnceLock::new();
    match bg {
        Bg::Felt(_) => None,
        Bg::Wood => Some(WOOD.get_or_init(|| screenshot("solitalian_briscola.png").crop(950, 200, 1450, 950)).clone()),
        Bg::Sea => Some(SEA.get_or_init(|| screenshot("solitalian_mare.jpg").crop(880, 600, 1440, 1000)).clone()),
        Bg::Sky => Some(SKY.get_or_init(|| screenshot("solitalian_mare.jpg").crop(330, 250, 1280, 470)).clone()),
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
/// Fuori dagli angoli arrotondati di raggio `r` pixel (coordinate u, v in 0..1).
fn outside_corner(u: f32, v: f32, w: f32, h: f32, r: f32) -> bool {
    if r <= 0.0 {
        return false;
    }
    let (x, y) = (u * w, v * h);
    let dx = if x < r { r - x } else if x > w - r { x - (w - r) } else { 0.0 };
    let dy = if y < r { r - y } else if y > h - r { y - (h - r) } else { 0.0 };
    dx > 0.0 && dy > 0.0 && dx.hypot(dy) > r
}

/// Un rettangolo ruotato che scurisce (o tinge) quello che c'è sotto: ombre e aloni.
fn shade(img: &mut Img, cx: f32, cy: f32, w: f32, h: f32, angle: f32, r: f32, soft: f32, color: [u8; 3], strength: f32) {
    let (s, c) = angle.to_radians().sin_cos();
    let ext = ((w + 2.0 * soft).hypot(h + 2.0 * soft) / 2.0 + 2.0) as isize;
    for y in (cy as isize - ext).max(0)..(cy as isize + ext).min(H as isize) {
        for x in (cx as isize - ext).max(0)..(cx as isize + ext).min(W as isize) {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let (a, b) = (dx * c + dy * s, -dx * s + dy * c);
            // Distanza dal rettangolo (arrotondato).
            let qx = (a.abs() - (w / 2.0 - r)).max(0.0);
            let qy = (b.abs() - (h / 2.0 - r)).max(0.0);
            let d = (qx.hypot(qy) - r).max(0.0);
            if d > soft {
                continue;
            }
            let k = strength * if soft > 0.0 { 1.0 - d / soft } else { 1.0 };
            let i = y as usize * W + x as usize;
            let q = img.px[i];
            img.px[i] = [0, 1, 2].map(|ch| (q[ch] as f32 * (1.0 - k) + color[ch] as f32 * k).round() as u8);
        }
    }
}

fn draw_round(img: &mut Img, cx: f32, cy: f32, w: f32, h: f32, angle: f32, r: f32, paint: &dyn Fn(f32, f32) -> [u8; 3]) {
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
                if !outside_corner(u, v, w, h, r) {
                    img.px[i] = paint(u, v);
                }
            } else if r <= 0.0 && (-2.0 / w..1.0 + 2.0 / w).contains(&u) && (-2.0 / h..1.0 + 2.0 / h).contains(&v) {
                // Ombra di due pixel.
                img.px[i] = img.px[i].map(|p| (p as f32 * 0.6) as u8);
            }
        }
    }
}

fn draw_card(img: &mut Img, card: &Img, look: &Look, cx: f32, cy: f32, h: f32, angle: f32) {
    draw_card_lit(img, card, look, cx, cy, h, angle, 1.0)
}

/// Una carta con gli effetti del sito: ombra, angoli arrotondati, senza bordo, scurita.
fn draw_card_lit(img: &mut Img, card: &Img, look: &Look, cx: f32, cy: f32, h: f32, angle: f32, light: f32) {
    let w = h * look.aspect.unwrap_or(card.w as f32 / card.h as f32);
    let r = look.rounded * w;
    if look.shadow > 0.0 {
        let s = look.shadow;
        shade(img, cx + s * 0.6, cy + s, w, h, angle, r, s * 1.5, [0, 0, 0], 0.55);
    }
    let b = look.bleed.clamp(0.0, 0.3);
    let paint = |u: f32, v: f32| {
        let (u, v) = (b + u * (1.0 - 2.0 * b), b + v * (1.0 - 2.0 * b));
        let p = tint(card.sample(u * (card.w - 1) as f32, v * (card.h - 1) as f32), look);
        if light < 1.0 { p.map(|c| (c as f32 * light) as u8) } else { p }
    };
    draw_round(img, cx, cy, w, h, angle, r, &paint);
}

/// Un dorso col bordo bianco: rosso a quadretti, o del colore del sito.
fn draw_back_of(img: &mut Img, cx: f32, cy: f32, w: f32, h: f32, color: Option<[u8; 3]>, r: f32) {
    let paint = |u: f32, v: f32| {
        let edge = u.min(1.0 - u) * w < 4.0 || v.min(1.0 - v) * h < 4.0;
        let check = ((u * 12.0) as i32 + (v * 18.0) as i32) % 2 == 0;
        match color {
            _ if edge => [250, 250, 250],
            None if check => [170, 30, 40],
            None => [140, 20, 32],
            Some(c) if check => c,
            Some(c) => c.map(|v| (v as f32 * 0.8) as u8),
        }
    };
    draw_round(img, cx, cy, w, h, 0.0, r, &paint);
}

/// Le scritte di testdata/scritte, con dove metterle (frazioni dello schermo). Il magenta è
/// trasparente.
const WRITING: [(&str, f32, f32); 11] = [
    ("popup", 0.63, 0.33),
    ("badge", 0.30, 0.62),
    ("chat", 0.77, 0.69),
    ("punti", 0.84, 0.04),
    ("tocca", 0.37, 0.23),
    ("vinto", 0.61, 0.52),
    ("briscola", 0.08, 0.64),
    ("nome", 0.41, 0.77),
    ("numero", 0.03, 0.04),
    ("java", 0.66, 0.77),
    ("banner", 0.02, 0.27),
];

/// Una scritta di testdata/scritte.
pub fn writing(name: &str) -> Img {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Img>>> = std::sync::OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    cache.entry(name.to_owned()).or_insert_with(|| screenshot(&format!("scritte/{name}.png"))).clone()
}

/// I loghi stampati sul tavolo, col centro in (frazioni dello schermo): lo stemma sotto le carte
/// giocate, la scritta fra le carte giocate e la mano, il marchio del sito vicino al mazzo.
const LOGOS: [(&str, f32, f32); 3] = [("stemma", 0.5, 0.5), ("scritta_tavolo", 0.5, 0.71), ("marchio", 0.2, 0.3)];

/// Incolla una scritta con l'angolo in alto a sinistra in (x, y), senza il magenta, con opacità
/// `alpha`.
fn stamp(img: &mut Img, s: &Img, x: usize, y: usize, alpha: f32) {
    for j in 0..s.h.min(img.h.saturating_sub(y)) {
        for i in 0..s.w.min(img.w.saturating_sub(x)) {
            let p = s.at(i, j);
            if !(p[0] > 200 && p[1] < 80 && p[2] > 200) {
                let q = &mut img.px[(y + j) * img.w + x + i];
                for c in 0..3 {
                    q[c] = (alpha * p[c] as f32 + (1.0 - alpha) * q[c] as f32).round() as u8;
                }
            }
        }
    }
}

/// Solo le scritte e i loghi, su un tavolo senza carte.
pub fn writing_only(look: &Look) -> Img {
    table(&Deal { hand: vec![], opp: None, mine: None, trump: None, deck: true }, &Look { writing: true, logo: 0.9, ..*look })
}

/// Il tavolo con le carte, come lo vedrebbe la lettura dello schermo.
fn play_positions(look: &Look) -> (([f32; 2], f32), ([f32; 2], f32)) {
    let h = look.card_h;
    let w = h * look.aspect.unwrap_or(0.62);
    let (mx, my) = (W as f32 / 2.0, H as f32 / 2.0);
    match look.table {
        Table::Classic => (([mx, my - h * 0.45], 0.0), ([mx + w * 0.3, my + h * 0.5], 0.0)),
        Table::SideBySide => (([mx - w * 0.6, my], 0.0), ([mx + w * 0.6, my], 0.0)),
        Table::Overlap => (([mx - w * 0.2, my - h * 0.2], 0.0), ([mx + w * 0.2, my + h * 0.25], 0.0)),
        Table::Tilted => (([mx - w * 0.1, my - h * 0.45], -9.0), ([mx + w * 0.3, my + h * 0.5], 8.0)),
        Table::LightOverlap => (([mx - w * 0.4, my - h * 0.35], 0.0), ([mx + w * 0.4, my + h * 0.4], 0.0)),
    }
}

/// Zone fisse ricavate dalla geometria del tavolo, indipendenti dalle carte.
pub fn play_zones(look: &Look) -> [Option<[f32; 4]>; 4] {
    let ((opp, _), (mine, _)) = play_positions(look);
    let radius = look.card_h * 0.42;
    let (mut left, mut right) = ([opp[0]-radius, opp[1]-radius, opp[0]+radius, opp[1]+radius],
        [mine[0]-radius, mine[1]-radius, mine[0]+radius, mine[1]+radius]);
    let axis = if mine[1] - opp[1] > look.card_h * 0.2 { 1 } else { 0 };
    let middle = (opp[axis] + mine[axis]) / 2.0;
    left[axis + 2] = left[axis + 2].min(middle - 1.0);
    right[axis] = right[axis].max(middle + 1.0);
    let normalize = |[x0, y0, x1, y1]: [f32; 4]| [x0 / W as f32, y0 / H as f32, x1 / W as f32, y1 / H as f32];
    [None, None, Some(normalize(right)), Some(normalize(left))]
}

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
    if look.logo > 0.0 {
        for (name, fx, fy) in LOGOS {
            let l = writing(name);
            let (x, y) = (fx * W as f32 - l.w as f32 / 2.0, fy * H as f32 - l.h as f32 / 2.0);
            stamp(&mut img, &l, x.max(0.0) as usize, y.max(0.0) as usize, look.logo);
        }
    }
    if look.writing {
        for (name, fx, fy) in WRITING {
            stamp(&mut img, &writing(name), (fx * W as f32) as usize, (fy * H as f32) as usize, 1.0);
        }
    }
    let h = look.card_h;
    let w = h * look.aspect.unwrap_or(0.62);
    // I dorsi dell'avversario in alto.
    for k in 0..3 {
        draw_back_of(&mut img, W as f32 / 2.0 + (k as f32 - 1.0) * (w + 8.0), h * 0.55 + 10.0, w, h, look.back, look.rounded * w);
    }
    // Il mazzo e la briscola di traverso, a sinistra: la briscola sporge a destra del mazzo.
    let (ph, pw) = (h * look.pile, w * look.pile);
    let (deck_x, deck_y) = (W as f32 * look.pile_at[0], H as f32 * look.pile_at[1]);
    if let Some(t) = deal.trump {
        let b = look.bleed.clamp(0.0, 0.3);
        let paint = |u: f32, v: f32| {
            let (u, v) = (b + u * (1.0 - 2.0 * b), b + v * (1.0 - 2.0 * b));
            tint(t.sample(u * (t.w - 1) as f32, v * (t.h - 1) as f32), look)
        };
        if look.trump_upright {
            // In piedi accanto al mazzo, verso il centro dello schermo, coperta in parte.
            let tw = ph * look.aspect.unwrap_or(t.w as f32 / t.h as f32);
            let side = if deck_x < W as f32 / 2.0 { 1.0 } else { -1.0 };
            let cx = deck_x + side * (pw / 2.0 + tw / 2.0 - look.cover * tw);
            draw_round(&mut img, cx, deck_y, tw, ph, 0.0, look.rounded * tw, &paint);
        } else {
            // La briscola è lunga ph: ne resta coperta la frazione `cover`, sotto il mazzo.
            let left = deck_x + pw / 2.0 - look.cover * ph;
            let paint_h = ph;
            let tw = paint_h * look.aspect.unwrap_or(t.w as f32 / t.h as f32);
            // Girata di 90 gradi: la testa a destra.
            draw_round(&mut img, left + paint_h / 2.0, deck_y, tw, paint_h, 90.0, look.rounded * tw, &paint);
        }
    }
    if deal.deck {
        draw_back_of(&mut img, deck_x, deck_y, pw, ph, look.back, look.rounded * pw);
    }
    let (opp_at, mine_at) = play_positions(look);
    if let Some(c) = deal.opp {
        draw_card(&mut img, c, look, opp_at.0[0], opp_at.0[1], h, opp_at.1);
    }
    if let Some(c) = deal.mine {
        draw_card(&mut img, c, look, mine_at.0[0], mine_at.0[1], h, mine_at.1);
    }
    // La mano in basso, a ventaglio.
    let n = deal.hand.len() as f32;
    for (k, c) in deal.hand.iter().enumerate() {
        let off = k as f32 - (n - 1.0) / 2.0;
        let (cx, cy) = (W as f32 / 2.0 + off * (w + 12.0), H as f32 - h * 0.55 - 10.0);
        if look.glow > 0.0 {
            let cw = h * look.aspect.unwrap_or(c.w as f32 / c.h as f32);
            shade(&mut img, cx, cy, cw, h, off * look.fan, look.rounded * cw, look.glow, [255, 215, 40], 0.9);
        }
        draw_card_lit(&mut img, c, look, cx, cy, h, off * look.fan, look.dim);
    }
    if look.lowres < 0.99 {
        // Giù a bassa risoluzione (media dei punti) e di nuovo su (punto più vicino).
        let (lw, lh) = ((W as f32 * look.lowres) as usize, (H as f32 * look.lowres) as usize);
        let mut small = Img::new(lw, lh);
        for y in 0..lh {
            for x in 0..lw {
                let (x0, x1) = (x * W / lw, ((x + 1) * W / lw).max(x * W / lw + 1));
                let (y0, y1) = (y * H / lh, ((y + 1) * H / lh).max(y * H / lh + 1));
                let mut acc = [0u32; 3];
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let p = img.at(xx, yy);
                        (0..3).for_each(|c| acc[c] += p[c] as u32);
                    }
                }
                let n = ((x1 - x0) * (y1 - y0)) as u32;
                small.px[y * lw + x] = acc.map(|a| (a / n) as u8);
            }
        }
        for y in 0..H {
            for x in 0..W {
                img.px[y * W + x] = small.at((x * lw / W).min(lw - 1), (y * lh / H).min(lh - 1));
            }
        }
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

#[test]
fn distinguishes_two_rows_from_a_nearby_play() {
    let cards: Vec<_> = (0..3).map(|id| super::reference::card_image(1, id)).collect();
    let look = Look { card_h: 205.0, aspect: Some(0.7), ..Look::default() };
    let mut deal = Deal {
        hand: vec![&cards[0], &cards[1]], opp: None, mine: Some(&cards[2]), trump: None, deck: false,
    };
    super::detect::forget_reading_state();
    let scene = super::detect::scene(&table(&deal, &look));
    assert_eq!(scene.hand.len(), 2);
    assert!(scene.my_slot.is_some());

    deal.mine = None;
    let mut img = table(&deal, &look);
    let h = look.card_h;
    let w = h * look.aspect.unwrap();
    draw_card(&mut img, &cards[2], &look, W as f32 / 2.0 - (w + 12.0)/2.0,
              H as f32 - h*0.55 - 10.0 - h - 8.0, h, 0.0);
    super::detect::forget_reading_state();
    assert_eq!(super::detect::scene(&img).hand.len(), 3);
}

#[test]
fn identifies_small_faces_in_a_large_viewport() {
    use briscola_core::cards::CardSet;
    let ids = [0, 12, 25];
    let images = ids.map(|id| super::reference::card_image(0, id));
    let look = Look { card_h: 32.0, ..Look::default() };
    let deal = Deal { hand: images.iter().collect(), mine: None, opp: None, trump: None, deck: false };
    super::detect::forget_reading_state();
    let scene = super::read_briscola(&table(&deal, &look), &std::sync::Mutex::new(super::memory::Memory::in_memory())).unwrap();
    let actual: Vec<_> = scene.hand.iter().filter_map(|v| v.best(CardSet::FULL).map(|b| b.0)).collect();
    assert_eq!(actual, ids);
}
