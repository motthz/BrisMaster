//! Le 40 carte napoletane di riferimento, in `assets/carte_napoletane.png`: quattro righe (denari,
//! coppe, bastoni, spade) e dieci colonne (dall'asso al sette, fante, cavallo, re).
//!
//! Una carta raddrizzata si confronta con ciascuna in miniatura: luminosità e due differenze di
//! colore (rosso-verde, giallo-blu), con la correlazione normalizzata, che non bada a quanto lo
//! schermo è chiaro, scuro o saturo. Ogni sito disegna le carte con un bordo bianco diverso: si
//! provano più ritagli e piccoli spostamenti e si tiene il migliore. Della carta coperta in parte si
//! confronta solo la fascia visibile.

use super::{Img, CARD_H, CARD_W};
use briscola_core::cards;
use std::collections::HashMap;
use std::sync::OnceLock;

pub const SHEET: &[u8] = include_bytes!("../../assets/carte_napoletane.png");
/// Seme di ogni riga del foglio.
const SUIT_OF_ROW: [u8; 4] = [0, 1, 3, 2];

/// Colonna e riga della carta nel foglio (10 x 4).
pub fn sheet_cell(c: cards::Card) -> (usize, usize) {
    let row = SUIT_OF_ROW.iter().position(|&s| s == cards::suit(c)).unwrap_or(0);
    (cards::rank(c) as usize, row)
}
/// Margine tolto alle carte del foglio (orizzontale, verticale), dove i bordi sono tagliati male.
const SHEET_MARGIN: (f32, f32) = (0.03, 0.02);

const TW: usize = 16;
const TH: usize = 26;
const N: usize = TW * TH;
/// Peso dei colori rispetto alla luminosità.
const ALPHA: f32 = 2.0;

/// Ritagli della carta vista: margine orizzontale e verticale tolti da ogni lato.
const SX: [f32; 6] = [0.0, 0.04, 0.08, 0.12, 0.16, 0.20];
const SY: [f32; 4] = [0.0, 0.03, 0.06, 0.09];
/// Piccoli spostamenti provati attorno al ritaglio migliore (in orizzontale e in verticale).
const SHIFT: [f32; 5] = [-0.03, -0.015, 0.0, 0.015, 0.03];
/// Carte riesaminate con gli spostamenti, fra le più somiglianti al primo giro.
const REFINE: usize = 4;
/// Somiglianza al primo giro sotto la quale non si cerca oltre: non è una carta.
const HOPELESS: f32 = 0.25;

/// Miniatura: per ogni pixel luminosità e due differenze di colore.
type Thumb = [[f32; 3]; N];

/// Miniatura con le somme per riga già pronte, per la correlazione su una fascia di righe.
struct Stats {
    px: Thumb,
    /// Somma dei valori e dei quadrati delle righe 0..j, per canale.
    sum: [[f64; 3]; TH + 1],
    sq: [[f64; 3]; TH + 1],
}

impl Stats {
    fn new(px: Thumb) -> Stats {
        let (mut sum, mut sq) = ([[0.0; 3]; TH + 1], [[0.0; 3]; TH + 1]);
        for j in 0..TH {
            for c in 0..3 {
                let (mut a, mut b) = (0.0, 0.0);
                for p in &px[j * TW..(j + 1) * TW] {
                    let v = p[c] as f64;
                    a += v;
                    b += v * v;
                }
                sum[j + 1][c] = sum[j][c] + a;
                sq[j + 1][c] = sq[j][c] + b;
            }
        }
        Stats { px, sum, sq }
    }
}

/// Correlazione normalizzata fra due miniature sulle righe r0..r1 (0 se la prima è quasi uniforme:
/// un riquadro bianco non somiglia a nessuna carta).
fn corr(o: &Stats, t: &Stats, (r0, r1): (usize, usize)) -> f32 {
    if r1 < r0 + 4 {
        return -1.0;
    }
    let n = ((r1 - r0) * TW) as f64;
    let mut dot = [0.0f64; 3];
    for (a, b) in o.px[r0 * TW..r1 * TW].iter().zip(&t.px[r0 * TW..r1 * TW]) {
        for c in 0..3 {
            dot[c] += (a[c] * b[c]) as f64;
        }
    }
    let (mut num, mut vo, mut vt) = (0.0, 0.0, 0.0);
    for (c, d) in dot.iter().enumerate() {
        let (so, st) = (o.sum[r1][c] - o.sum[r0][c], t.sum[r1][c] - t.sum[r0][c]);
        num += d - so * st / n;
        vo += o.sq[r1][c] - o.sq[r0][c] - so * so / n;
        vt += t.sq[r1][c] - t.sq[r0][c] - st * st / n;
    }
    // Meno di 6 livelli di scarto medio: niente disegni.
    if vo < 36.0 * n || vt < 1e-3 {
        return 0.0;
    }
    (num / (vo * vt).sqrt()) as f32
}

/// Le 40 carte di riferimento in miniatura.
fn templates() -> &'static Vec<Stats> {
    static T: OnceLock<Vec<Stats>> = OnceLock::new();
    T.get_or_init(|| {
        let im = image::load_from_memory_with_format(SHEET, image::ImageFormat::Png)
            .expect("foglio delle carte")
            .to_rgba8();
        let sheet = Img::from_rgba(im.width() as usize, im.height() as usize, im.as_raw());
        let (cw, ch) = (sheet.w as f32 / 10.0, sheet.h as f32 / 4.0);
        let mut thumbs = vec![[[0.0; 3]; N]; 40];
        for (row, &suit) in SUIT_OF_ROW.iter().enumerate() {
            for col in 0..10 {
                let (mx, my) = (SHEET_MARGIN.0 * cw, SHEET_MARGIN.1 * ch);
                let (x0, y0) = (col as f32 * cw + mx, row as f32 * ch + my);
                let t = sample_thumb(&sheet, x0, y0, x0 + cw - 2.0 * mx, y0 + ch - 2.0 * my);
                thumbs[cards::make(suit, col as u8) as usize] = t;
            }
        }
        thumbs.into_iter().map(Stats::new).collect()
    })
}

/// I tre canali confrontati di un colore.
#[inline]
fn channels([r, g, b]: [f32; 3]) -> [f32; 3] {
    [(r + g + b) / 3.0, ALPHA * (r - g), ALPHA * ((r + g) / 2.0 - b)]
}

/// Miniatura del rettangolo [x0, x1] x [y0, y1] dell'immagine (media di 4x4 punti per pixel).
fn sample_thumb(img: &Img, x0: f32, y0: f32, x1: f32, y1: f32) -> Thumb {
    let (pw, ph) = ((x1 - x0) / TW as f32, (y1 - y0) / TH as f32);
    let mut out = [[0.0; 3]; N];
    for j in 0..TH {
        for i in 0..TW {
            let mut acc = [0.0f32; 3];
            for sy in 0..4 {
                for sx in 0..4 {
                    let x = x0 + (i as f32 + (sx as f32 + 0.5) / 4.0) * pw - 0.5;
                    let y = y0 + (j as f32 + (sy as f32 + 0.5) / 4.0) * ph - 0.5;
                    let p = img.sample(x, y);
                    for c in 0..3 {
                        acc[c] += p[c] as f32;
                    }
                }
            }
            out[j * TW + i] = channels(acc.map(|v| v / 16.0));
        }
    }
    out
}

/// Un ritaglio della carta vista: frazioni tolte a sinistra e in alto, larghezza e altezza.
#[derive(Clone, Copy)]
struct Crop {
    x0: f32,
    y0: f32,
    w: f32,
    h: f32,
}

impl Crop {
    fn new(sx: f32, sy: f32, dx: f32, dy: f32) -> Crop {
        Crop { x0: sx + dx, y0: sy + dy, w: 1.0 - 2.0 * sx, h: 1.0 - 2.0 * sy }
    }

    /// Chiave per riusare la miniatura dello stesso ritaglio.
    fn key(&self) -> [i32; 4] {
        [self.x0, self.y0, self.w, self.h].map(|v| (v * 1000.0).round() as i32)
    }

    /// Righe della miniatura che cadono nella fascia visibile della carta.
    fn rows(&self, vis: [f32; 2]) -> (usize, usize) {
        let y = |j: usize| self.y0 + (j as f32 + 0.5) / TH as f32 * self.h;
        let first = (0..TH).find(|&j| y(j) >= vis[0]).unwrap_or(TH);
        let last = (0..TH).rev().find(|&j| y(j) <= vis[1]).map_or(0, |j| j + 1);
        (first, last.max(first))
    }
}

/// La carta vista, con le somme cumulate dei canali: la miniatura di un ritaglio qualsiasi costa
/// poche operazioni per pixel, e si possono provare molti ritagli.
struct Seen {
    vis: [f32; 2],
    /// Somme dei canali sul rettangolo [0, x) x [0, y), (CARD_W + 1) x (CARD_H + 1).
    sat: Vec<[f64; 3]>,
    cache: HashMap<[i32; 4], Stats>,
}

impl Seen {
    fn new(img: &Img, vis: [f32; 2]) -> Seen {
        let w = CARD_W + 1;
        let mut sat = vec![[0.0f64; 3]; w * (CARD_H + 1)];
        for y in 0..CARD_H {
            let mut row = [0.0f64; 3];
            for x in 0..CARD_W {
                let f = channels(img.at(x, y).map(|v| v as f32));
                for c in 0..3 {
                    row[c] += f[c] as f64;
                    sat[(y + 1) * w + x + 1][c] = sat[y * w + x + 1][c] + row[c];
                }
            }
        }
        Seen { vis, sat, cache: HashMap::new() }
    }

    /// Somma cumulata in un punto qualsiasi (interpolata; fuori dalla carta si ferma al bordo).
    fn at(&self, x: f32, y: f32) -> [f64; 3] {
        let (x, y) = (x.clamp(0.0, CARD_W as f32), y.clamp(0.0, CARD_H as f32));
        let (x0, y0) = ((x as usize).min(CARD_W - 1), (y as usize).min(CARD_H - 1));
        let (fx, fy) = ((x - x0 as f32) as f64, (y - y0 as f32) as f64);
        let w = CARD_W + 1;
        let p = |xx: usize, yy: usize| self.sat[yy * w + xx];
        let (a, b, c, d) = (p(x0, y0), p(x0 + 1, y0), p(x0, y0 + 1), p(x0 + 1, y0 + 1));
        let mut out = [0.0; 3];
        for k in 0..3 {
            out[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
        }
        out
    }

    fn thumb(&mut self, crop: Crop) -> &Stats {
        let key = crop.key();
        if !self.cache.contains_key(&key) {
            let (w, h) = (CARD_W as f32, CARD_H as f32);
            let (pw, ph) = (crop.w * w / TW as f32, crop.h * h / TH as f32);
            // Somme cumulate negli angoli della griglia della miniatura.
            let mut grid = [[0.0f64; 3]; (TW + 1) * (TH + 1)];
            for j in 0..=TH {
                for i in 0..=TW {
                    grid[j * (TW + 1) + i] = self.at(crop.x0 * w + i as f32 * pw, crop.y0 * h + j as f32 * ph);
                }
            }
            let area = (pw * ph) as f64;
            let mut px = [[0.0; 3]; N];
            for j in 0..TH {
                for i in 0..TW {
                    let g = |di: usize, dj: usize| grid[(j + dj) * (TW + 1) + i + di];
                    let (a, b, c, d) = (g(0, 0), g(1, 0), g(0, 1), g(1, 1));
                    for k in 0..3 {
                        px[j * TW + i][k] = ((d[k] - b[k] - c[k] + a[k]) / area) as f32;
                    }
                }
            }
            self.cache.insert(key, Stats::new(px));
        }
        &self.cache[&key]
    }

    fn score(&mut self, crop: Crop, card: usize) -> f32 {
        let rows = crop.rows(self.vis);
        corr(self.thumb(crop), &templates()[card], rows)
    }
}

/// Confronto di una carta raddrizzata (CARD_W x CARD_H, testa in alto) con le 40 carte, guardando
/// solo la fascia visibile `vis` (frazioni dell'altezza dalla testa). Il primo giro prova pochi
/// ritagli e basta a scartare quello che non è una carta; `refine` affina le più somiglianti.
pub struct Match {
    seen: Seen,
    /// Per carta: somiglianza migliore e ritaglio (margini) che l'ha data.
    best: [(f32, f32, f32); 40],
}

impl Match {
    pub fn new(card: &Img, vis: [f32; 2]) -> Match {
        let mut seen = Seen::new(card, vis);
        let mut best = [(-1.0f32, 0.0f32, 0.0f32); 40];
        for &sx in &SX {
            for &sy in &SY {
                let crop = Crop::new(sx, sy, 0.0, 0.0);
                for (k, slot) in best.iter_mut().enumerate() {
                    let s = seen.score(crop, k);
                    if s > slot.0 {
                        *slot = (s, sx, sy);
                    }
                }
            }
        }
        Match { seen, best }
    }

    /// La somiglianza migliore trovata finora.
    pub fn top(&self) -> f32 {
        self.best.iter().map(|b| b.0).fold(f32::MIN, f32::max)
    }

    /// Le carte più somiglianti, con ritagli vicini e spostati. Non serve se nessuna somiglia
    /// abbastanza: non è una carta.
    pub fn refine(mut self) -> [f32; 40] {
        if self.top() >= HOPELESS {
            let mut order: Vec<usize> = (0..40).collect();
            order.sort_by(|&x, &y| self.best[y].0.total_cmp(&self.best[x].0));
            for &k in order.iter().take(REFINE) {
                let (_, sx0, sy0) = self.best[k];
                for sx in [sx0 - 0.02, sx0, sx0 + 0.02] {
                    for sy in [sy0 - 0.015, sy0, sy0 + 0.015] {
                        for dx in SHIFT {
                            for dy in SHIFT {
                                let s = self.seen.score(Crop::new(sx, sy, dx, dy), k);
                                self.best[k].0 = self.best[k].0.max(s);
                            }
                        }
                    }
                }
            }
        }
        self.best.map(|b| b.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ogni carta del foglio, raddrizzata come quelle dello schermo, riconosce sé stessa.
    #[test]
    fn sheet_cards_recognize_themselves() {
        let im = image::load_from_memory_with_format(SHEET, image::ImageFormat::Png).unwrap().to_rgba8();
        let sheet = Img::from_rgba(im.width() as usize, im.height() as usize, im.as_raw());
        let (cw, ch) = (sheet.w as f32 / 10.0, sheet.h as f32 / 4.0);
        for (row, &suit) in SUIT_OF_ROW.iter().enumerate() {
            for col in [0usize, 4, 8] {
                // La carta con un bordo bianco attorno, come sul sito.
                let mut card = Img::new(CARD_W, CARD_H);
                for y in 0..CARD_H {
                    for x in 0..CARD_W {
                        let fx = (x as f32 / CARD_W as f32 - 0.1) / 0.8;
                        let fy = (y as f32 / CARD_H as f32 - 0.05) / 0.9;
                        if (0.0..1.0).contains(&fx) && (0.0..1.0).contains(&fy) {
                            let (sx, sy) = ((col as f32 + fx) * cw, (row as f32 + fy) * ch);
                            card.px[y * CARD_W + x] = sheet.sample(sx, sy);
                        }
                    }
                }
                let s = Match::new(&card, [0.0, 1.0]).refine();
                let best = (0..40).max_by(|&a, &b| s[a].total_cmp(&s[b])).unwrap();
                assert_eq!(best as u8, cards::make(suit, col as u8), "{}", cards::name(best as u8));
            }
        }
    }
}
