//! Le 40 carte napoletane di riferimento, in più stili di disegno (i siti non usano tutti lo stesso
//! mazzo): ogni stile è un foglio con quattro righe (denari, coppe, bastoni, spade) e dieci colonne
//! (dall'asso al sette, fante, cavallo, re).
//! - `assets/carte_napoletane_wiki.jpg`: il mazzo napoletano di Wikimedia Commons (pubblico
//!   dominio, <https://commons.wikimedia.org/wiki/Category:Naples_deck>), il più diffuso sui siti;
//!   è anche quello disegnato nell'app.
//! - `assets/carte_napoletane.png`: un secondo disegno, con colori più scuri.
//!
//! Una carta raddrizzata si confronta con ciascuna in miniatura: luminosità e due differenze di
//! colore (rosso-verde, giallo-blu), con la correlazione normalizzata, che non bada a quanto lo
//! schermo è chiaro, scuro o saturo; per ogni carta vale lo stile che le somiglia di più. Ogni sito
//! disegna le carte con un bordo bianco diverso: si provano più ritagli e piccoli spostamenti e si
//! tiene il migliore. Della carta coperta in parte si confronta solo la fascia visibile.

use super::{hsv, Img, CARD_H, CARD_W};
use briscola_core::cards;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Un mazzo di riferimento: il foglio e il margine tolto a ogni carta (orizzontale, verticale).
struct Style {
    bytes: &'static [u8],
    format: image::ImageFormat,
    margin: (f32, f32),
}

const STYLES: [Style; 2] = [
    Style {
        bytes: include_bytes!("../../assets/carte_napoletane_wiki.jpg"),
        format: image::ImageFormat::Jpeg,
        margin: (0.02, 0.015),
    },
    Style {
        bytes: include_bytes!("../../assets/carte_napoletane.png"),
        format: image::ImageFormat::Png,
        margin: (0.03, 0.02),
    },
];
/// Quante carte di riferimento ci sono in tutto (40 per stile).
const NT: usize = 40 * STYLES.len();

/// Il foglio delle carte disegnate nell'app (lo stile di Wikimedia) e le proporzioni di una carta.
pub const DISPLAY_SHEET: &[u8] = STYLES[0].bytes;
pub const DISPLAY_RATIO: f32 = 160.0 / 264.0;

/// Seme di ogni riga del foglio.
const SUIT_OF_ROW: [u8; 4] = [0, 1, 3, 2];

/// Colonna e riga della carta nel foglio (10 x 4).
pub fn sheet_cell(c: cards::Card) -> (usize, usize) {
    let row = SUIT_OF_ROW.iter().position(|&s| s == cards::suit(c)).unwrap_or(0);
    (cards::rank(c) as usize, row)
}

/// Il foglio di uno stile, decodificato.
fn sheet(style: &Style) -> Img {
    let im = image::load_from_memory_with_format(style.bytes, style.format).expect("foglio delle carte").to_rgba8();
    Img::from_rgba(im.width() as usize, im.height() as usize, im.as_raw())
}

/// Il rettangolo (x0, y0, x1, y1) della carta `c` nel foglio, senza il margine dello stile.
fn cell_rect(sheet: &Img, style: &Style, c: cards::Card) -> (f32, f32, f32, f32) {
    let (cw, ch) = (sheet.w as f32 / 10.0, sheet.h as f32 / 4.0);
    let (col, row) = sheet_cell(c);
    let (mx, my) = (style.margin.0 * cw, style.margin.1 * ch);
    let (x0, y0) = (col as f32 * cw + mx, row as f32 * ch + my);
    (x0, y0, x0 + cw - 2.0 * mx, y0 + ch - 2.0 * my)
}

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

/// Le carte di riferimento in miniatura: per ogni stile le 40 carte (indice stile * 40 + carta).
fn templates() -> &'static Vec<Stats> {
    static T: OnceLock<Vec<Stats>> = OnceLock::new();
    T.get_or_init(|| {
        let mut out = Vec::with_capacity(NT);
        for style in &STYLES {
            let sheet = sheet(style);
            for c in 0..40u8 {
                let (x0, y0, x1, y1) = cell_rect(&sheet, style, c);
                out.push(Stats::new(sample_thumb(&sheet, x0, y0, x1, y1)));
            }
        }
        out
    })
}

/// Peso della differenza fra le firme di colore (vedi `Hues`) nella somiglianza.
const COLOR: f32 = 0.2;
/// Righe in cui si conta il colore dei disegni.
const HR: usize = 32;
/// Colonne in cui si conta il colore dei disegni.
const HC: usize = 20;

/// Colore dei disegni di una carta, riga per riga (cumulato): quanti punti rossi, gialli, verdi e
/// blu. Distingue i semi anche quando la disposizione dei simboli è la stessa (il sette di denari,
/// monete gialle, e il sette di coppe, coppe rosse e verdi) e il disegno del sito è diverso da
/// quello di riferimento: conta il colore, non la forma.
struct Hues {
    rows: [[f32; 4]; HR + 1],
}

impl Hues {
    /// Dal rettangolo [x0, x1] x [y0, y1] dell'immagine.
    fn new(img: &Img, x0: f32, y0: f32, x1: f32, y1: f32) -> Hues {
        let mut rows = [[0.0; 4]; HR + 1];
        for j in 0..HR {
            let mut row = [0.0; 4];
            for i in 0..HC {
                for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                    let x = x0 + (i as f32 + dx) / HC as f32 * (x1 - x0);
                    let y = y0 + (j as f32 + dy) / HR as f32 * (y1 - y0);
                    let (h, s, v) = hsv(img.sample(x, y));
                    if s < 70 || v < 70 {
                        continue;
                    }
                    let bin = match h {
                        h if !(20.0..330.0).contains(&h) => 0,
                        h if h < 70.0 => 1,
                        h if h < 170.0 => 2,
                        h if h < 260.0 => 3,
                        _ => 0,
                    };
                    row[bin] += 1.0;
                }
            }
            for c in 0..4 {
                rows[j + 1][c] = rows[j][c] + row[c];
            }
        }
        Hues { rows }
    }

    /// Proporzione di ciascun colore nella fascia `vis` (None se quasi senza colore).
    fn share(&self, vis: [f32; 2]) -> Option<[f32; 4]> {
        let r0 = ((vis[0] * HR as f32).round() as usize).min(HR);
        let r1 = ((vis[1] * HR as f32).round() as usize).clamp(r0, HR);
        let mut n = [0.0; 4];
        for c in 0..4 {
            n[c] = self.rows[r1][c] - self.rows[r0][c];
        }
        let total: f32 = n.iter().sum();
        // Almeno un punto colorato su cento.
        (total >= 0.01 * ((r1 - r0) * HC * 4) as f32 && total > 4.0).then(|| n.map(|v| v / total))
    }
}

/// Firme di colore delle carte di riferimento (come `templates`).
fn template_hues() -> &'static Vec<Hues> {
    static T: OnceLock<Vec<Hues>> = OnceLock::new();
    T.get_or_init(|| {
        let mut out = Vec::with_capacity(NT);
        for style in &STYLES {
            let sheet = sheet(style);
            for c in 0..40u8 {
                let (x0, y0, x1, y1) = cell_rect(&sheet, style, c);
                out.push(Hues::new(&sheet, x0, y0, x1, y1));
            }
        }
        out
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
    /// Colore dei disegni della carta vista, senza il bordo.
    hues: Hues,
    /// Per carta di riferimento (stile * 40 + carta): somiglianza migliore e ritaglio (margini)
    /// che l'ha data.
    best: Vec<(f32, f32, f32)>,
}

impl Match {
    pub fn new(card: &Img, vis: [f32; 2]) -> Match {
        let mut seen = Seen::new(card, vis);
        let (w, h) = (CARD_W as f32, CARD_H as f32);
        let hues = Hues::new(card, 0.1 * w, 0.06 * h, 0.9 * w, 0.94 * h);
        let mut best = vec![(-1.0f32, 0.0f32, 0.0f32); NT];
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
        Match { seen, hues, best }
    }

    /// La somiglianza migliore trovata finora.
    pub fn top(&self) -> f32 {
        self.best.iter().map(|b| b.0).fold(f32::MIN, f32::max)
    }

    /// Le carte più somiglianti, con ritagli vicini e spostati. Non serve se nessuna somiglia
    /// abbastanza: non è una carta.
    pub fn refine(mut self) -> [f32; 40] {
        if self.top() >= HOPELESS {
            let mut order: Vec<usize> = (0..NT).collect();
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
        // Seme dal colore: si toglie la differenza fra le firme di colore, rispetto alla carta
        // col colore più vicino (che non perde niente: la somiglianza resta confrontabile con le
        // soglie per dire se è una carta).
        let vis = self.seen.vis;
        if let Some(seen) = self.hues.share(vis) {
            let dist: Vec<Option<f32>> = template_hues()
                .iter()
                .map(|t| t.share(vis).map(|t| seen.iter().zip(&t).map(|(a, b)| (a - b).abs()).sum()))
                .collect();
            let closest = dist.iter().flatten().copied().fold(f32::MAX, f32::min);
            for (slot, d) in self.best.iter_mut().zip(dist) {
                if let Some(d) = d {
                    slot.0 -= COLOR * (d - closest);
                }
            }
        }
        // Per ogni carta vale lo stile che le somiglia di più.
        let mut out = [f32::MIN; 40];
        for (k, b) in self.best.iter().enumerate() {
            out[k % 40] = out[k % 40].max(b.0);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ogni carta del foglio, raddrizzata come quelle dello schermo, riconosce sé stessa.
    #[test]
    fn sheet_cards_recognize_themselves() {
        for style in &STYLES {
            check_style(&sheet(style));
        }
    }

    fn check_style(sheet: &Img) {
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
