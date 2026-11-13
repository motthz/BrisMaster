//! Le 40 carte napoletane di riferimento, in più stili di disegno (i siti non usano tutti lo stesso
//! mazzo): ogni stile è un foglio con quattro righe (denari, coppe, bastoni, spade) e dieci colonne
//! (dall'asso al sette, fante, cavallo, re).
//! - `assets/carte_napoletane_wiki.jpg`: il mazzo napoletano di Wikimedia Commons (pubblico
//!   dominio, <https://commons.wikimedia.org/wiki/Category:Naples_deck>), il più diffuso sui siti;
//!   è anche quello disegnato nell'app.
//! - `assets/carte_napoletane_classiche.jpg`: un secondo disegno, dai colori più scuri (anche
//!   questo di pubblico dominio, "Carte napoletane al completo.jpg" di Wikimedia Commons).
//! - `assets/carte_napoletane_brisva.jpg`: il mazzo del tavolo su cui gioca l'utente (immagini
//!   fornite dall'utente, che ne ha autorizzato l'uso): carte senza bordo bianco, a tutta figura.
//!
//! Una carta raddrizzata si confronta con ciascuna in miniatura: luminosità e due differenze di
//! colore (rosso-verde, giallo-blu), con la correlazione normalizzata, che non bada a quanto lo
//! schermo è chiaro, scuro o saturo; per ogni carta vale lo stile che le somiglia di più. Ogni sito
//! disegna le carte con un bordo bianco diverso: si provano più ritagli e piccoli spostamenti e si
//! tiene il migliore. Della carta coperta in parte si confronta solo la fascia visibile.

use super::{hsv, Img, CARD_H, CARD_W, WHOLE};
use briscola_core::cards;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Un mazzo di riferimento: il foglio e il margine tolto a ogni carta (orizzontale, verticale).
struct Style {
    bytes: &'static [u8],
    format: image::ImageFormat,
    margin: (f32, f32),
}

const STYLES: [Style; 3] = [
    Style {
        bytes: include_bytes!("../../assets/carte_napoletane_wiki.jpg"),
        format: image::ImageFormat::Jpeg,
        margin: (0.02, 0.015),
    },
    Style {
        bytes: include_bytes!("../../assets/carte_napoletane_classiche.jpg"),
        format: image::ImageFormat::Jpeg,
        margin: (0.03, 0.02),
    },
    Style {
        bytes: include_bytes!("../../assets/carte_napoletane_brisva.jpg"),
        format: image::ImageFormat::Jpeg,
        margin: (0.0, 0.0),
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

/// L'immagine della carta `c` nel mazzo `style` (per i test che compongono tavoli finti).
#[cfg(test)]
pub fn card_image(style: usize, c: cards::Card) -> Img {
    let st = &STYLES[style];
    let sheet = sheet(st);
    let (cw, ch) = (sheet.w / 10, sheet.h / 4);
    let (col, row) = sheet_cell(c);
    sheet.crop(col * cw + 1, row * ch + 1, (col + 1) * cw - 1, (row + 1) * ch - 1)
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
fn corr(o: &Stats, t: &Stats, (r0, r1): (usize, usize), gray: bool) -> f32 {
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
    let (mut num, mut vo, mut vt) = ([0.0; 3], [0.0; 3], [0.0; 3]);
    for (c, d) in dot.iter().enumerate() {
        let (so, st) = (o.sum[r1][c] - o.sum[r0][c], t.sum[r1][c] - t.sum[r0][c]);
        num[c] = d - so * st / n;
        vo[c] = o.sq[r1][c] - o.sq[r0][c] - so * so / n;
        vt[c] = t.sq[r1][c] - t.sq[r0][c] - st * st / n;
    }
    combine(num, vo, vt, n, gray)
}

/// La correlazione dai tre canali. Una carta vista senza colori (schermo in bianco e nero, filtro
/// grigio, carta scolorita) si confronta solo sulla luminosità: i colori delle carte di riferimento
/// non devono contare contro di lei.
fn combine(num: [f64; 3], vo: [f64; 3], vt: [f64; 3], n: f64, gray: bool) -> f32 {
    let used: &[usize] = if gray { &[0] } else { &[0, 1, 2] };
    let (num, vo, vt): (f64, f64, f64) = used.iter().fold((0.0, 0.0, 0.0), |a, &c| (a.0 + num[c], a.1 + vo[c], a.2 + vt[c]));
    // Meno di 6 livelli di scarto medio: niente disegni.
    if vo < 36.0 * n || vt < 1e-3 {
        return 0.0;
    }
    (num / (vo * vt).sqrt()) as f32
}

/// Come `corr_rect`, saltando le righe coperte (`skip`).
fn corr_masked(o: &Stats, t: &Stats, (r0, r1): (usize, usize), (c0, c1): (usize, usize), skip: &[bool; TH], gray: bool) -> f32 {
    let rows: Vec<usize> = (r0..r1).filter(|&j| !skip[j]).collect();
    if rows.len() < 4 || c1 < c0 + 4 {
        return -1.0;
    }
    let n = (rows.len() * (c1 - c0)) as f64;
    let (mut num, mut vo, mut vt) = ([0.0; 3], [0.0; 3], [0.0; 3]);
    for c in 0..3 {
        let (mut so, mut st, mut d, mut qo, mut qt) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for &j in &rows {
            for i in c0..c1 {
                let (a, b) = (o.px[j * TW + i][c] as f64, t.px[j * TW + i][c] as f64);
                so += a;
                st += b;
                d += a * b;
                qo += a * a;
                qt += b * b;
            }
        }
        num[c] = d - so * st / n;
        vo[c] = qo - so * so / n;
        vt[c] = qt - st * st / n;
    }
    combine(num, vo, vt, n, gray)
}

/// Le righe della carta raddrizzata attraversate da qualcosa che la copre (una fascia, una
/// scritta, un adesivo che passa sopra la carta): lì il bordo bianco non si vede né a sinistra né
/// a destra. Vuoto se non c'è niente, o se sembrano coperte troppe righe (la carta è inquadrata
/// male, o non ha il bordo bianco): allora si confronta tutto come sempre.
fn covered_rows(img: &Img) -> Vec<bool> {
    let (w, h) = (CARD_W, CARD_H);
    let light = |x0: f32, x1: f32, y: usize| {
        let (a, b) = ((x0 * w as f32) as usize, ((x1 * w as f32) as usize).max((x0 * w as f32) as usize + 1));
        let mut acc = [0u32; 3];
        for x in a..b.min(w) {
            let p = img.at(x, y);
            (0..3).for_each(|c| acc[c] += p[c] as u32);
        }
        let n = (b.min(w) - a).max(1) as u32;
        let p = acc.map(|v| v / n);
        let (lo, hi) = (*p.iter().min().expect("tre canali"), *p.iter().max().expect("tre canali"));
        (p[0] * 3 + p[1] * 6 + p[2]) / 10 >= 140 && hi - lo <= 70
    };
    let rows: Vec<bool> = (0..h)
        .map(|y| {
            let f = y as f32 / h as f32;
            (0.04..0.96).contains(&f) && !light(0.015, 0.045, y) && !light(0.955, 0.985, y)
        })
        .collect();
    let n = rows.iter().filter(|&&r| r).count();
    if n == 0 || n as f32 > 0.4 * h as f32 {
        return Vec::new();
    }
    rows
}

/// Come `corr`, ma solo sulle colonne c0..c1 (la carta coperta di lato da un'altra).
fn corr_rect(o: &Stats, t: &Stats, (r0, r1): (usize, usize), (c0, c1): (usize, usize), gray: bool) -> f32 {
    if r1 < r0 + 4 || c1 < c0 + 4 {
        return -1.0;
    }
    let n = ((r1 - r0) * (c1 - c0)) as f64;
    let (mut num, mut vo, mut vt) = ([0.0; 3], [0.0; 3], [0.0; 3]);
    for c in 0..3 {
        let (mut so, mut st, mut d, mut qo, mut qt) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for j in r0..r1 {
            for i in c0..c1 {
                let (a, b) = (o.px[j * TW + i][c] as f64, t.px[j * TW + i][c] as f64);
                so += a;
                st += b;
                d += a * b;
                qo += a * a;
                qt += b * b;
            }
        }
        num[c] = d - so * st / n;
        vo[c] = qo - so * so / n;
        vt[c] = qt - st * st / n;
    }
    combine(num, vo, vt, n, gray)
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

thread_local! {
    /// Lo schermo letto adesso è senza colori (filtro in bianco e nero del sistema): le carte si
    /// confrontano sulla sola luminosità.
    static GRAY_SCREEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Dice se lo schermo che si sta leggendo è senza colori (vedi `GRAY_SCREEN`).
pub fn set_gray_screen(gray: bool) {
    GRAY_SCREEN.with(|g| g.set(gray));
}

/// Vero se lo schermo che si sta leggendo è senza colori.
pub fn gray_screen() -> bool {
    GRAY_SCREEN.with(|g| g.get())
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

    /// Colonne della miniatura che cadono nella striscia visibile della carta (frazioni della
    /// larghezza da sinistra).
    fn cols(&self, cols: [f32; 2]) -> (usize, usize) {
        let x = |i: usize| self.x0 + (i as f32 + 0.5) / TW as f32 * self.w;
        let first = (0..TW).find(|&i| x(i) >= cols[0]).unwrap_or(TW);
        let last = (0..TW).rev().find(|&i| x(i) <= cols[1]).map_or(0, |i| i + 1);
        (first, last.max(first))
    }
}

/// La carta vista, con le somme cumulate dei canali: la miniatura di un ritaglio qualsiasi costa
/// poche operazioni per pixel, e si possono provare molti ritagli.
struct Seen {
    vis: [f32; 2],
    /// Striscia visibile in larghezza (tutta, tranne le carte coperte di lato).
    cols: [f32; 2],
    /// Somme dei canali sul rettangolo [0, x) x [0, y), (CARD_W + 1) x (CARD_H + 1).
    sat: Vec<[f64; 3]>,
    /// Le righe della carta coperte da qualcosa che la attraversa (vuoto se nessuna).
    covered: Vec<bool>,
    /// La carta si vede senza colori (schermo in bianco e nero): si confronta la luminosità.
    gray: bool,
    cache: HashMap<[i32; 4], Stats>,
}

impl Seen {
    fn new(img: &Img, vis: [f32; 2], cols: [f32; 2]) -> Seen {
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
        let gray = GRAY_SCREEN.with(|g| g.get());
        Seen { vis, cols, sat, covered: Vec::new(), gray, cache: HashMap::new() }
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
        if !self.covered.is_empty() {
            // Le righe della miniatura che toccano una riga coperta della carta non contano.
            let h = CARD_H as f32;
            let mut skip = [false; TH];
            for (j, s) in skip.iter_mut().enumerate() {
                let a = ((crop.y0 + j as f32 / TH as f32 * crop.h) * h).max(0.0) as usize;
                let b = (((crop.y0 + (j + 1) as f32 / TH as f32 * crop.h) * h).ceil() as usize).min(CARD_H);
                *s = (a..b).any(|y| self.covered[y]);
            }
            let cols = if self.cols == WHOLE { (0, TW) } else { crop.cols(self.cols) };
            let gray = self.gray;
            return corr_masked(self.thumb(crop), &templates()[card], rows, cols, &skip, gray);
        }
        if self.cols == WHOLE {
            let gray = self.gray;
            corr(self.thumb(crop), &templates()[card], rows, gray)
        } else {
            let cols = crop.cols(self.cols);
            let gray = self.gray;
            corr_rect(self.thumb(crop), &templates()[card], rows, cols, gray)
        }
    }
}

/// Ritagli del confronto veloce (per far scorrere una finestra su molte posizioni).
const QX: [f32; 3] = [0.04, 0.08, 0.12];
const QY: [f32; 2] = [0.03, 0.06];

/// Somiglianza migliore della carta raddrizzata con le carte di riferimento, per ciascuna striscia
/// visibile `cols` della larghezza, con pochi ritagli: le miniature si calcolano una volta sola.
pub fn quick_tops(card: &Img, cols_list: &[[f32; 2]]) -> Vec<f32> {
    let mut seen = Seen::new(card, WHOLE, WHOLE);
    let n = templates().len();
    // Prima un ritaglio solo: le posizioni che non somigliano a niente si scartano subito.
    let probe = Crop::new(0.08, 0.045, 0.0, 0.0);
    let hopeful = cols_list.iter().any(|&cols| {
        seen.cols = cols;
        (0..n).any(|k| seen.score(probe, k) >= 0.3)
    });
    if !hopeful {
        return vec![-1.0; cols_list.len()];
    }
    cols_list
        .iter()
        .map(|&cols| {
            seen.cols = cols;
            let mut top = -1.0f32;
            for &sx in &QX {
                for &sy in &QY {
                    let crop = Crop::new(sx, sy, 0.0, 0.0);
                    for k in 0..n {
                        top = top.max(seen.score(crop, k));
                    }
                }
            }
            top
        })
        .collect()
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
    /// Una carta di cui si vede la fascia `vis` dell'altezza e la striscia `cols` della larghezza
    /// (tutta, o una parte se è coperta di lato da un'altra carta, come nelle mani a ventaglio).
    pub fn with_cols(card: &Img, vis: [f32; 2], cols: [f32; 2]) -> Match {
        Match::crops(card, vis, cols, &SX, &SY, false)
    }

    /// Come `with_cols`, senza contare le righe attraversate da qualcosa che copre la carta (una
    /// fascia che passa sopra una fila di carte). Solo dove la posizione della carta è già certa:
    /// altrimenti toglie righe utili a distinguere carte simili.
    pub fn with_cols_uncovered(card: &Img, vis: [f32; 2], cols: [f32; 2]) -> Match {
        Match::crops(card, vis, cols, &SX, &SY, true)
    }

    fn crops(card: &Img, vis: [f32; 2], cols: [f32; 2], xs: &[f32], ys: &[f32], uncover: bool) -> Match {
        let mut seen = Seen::new(card, vis, cols);
        if uncover {
            seen.covered = covered_rows(card);
        }
        let (w, h) = (CARD_W as f32, CARD_H as f32);
        let hues = Hues::new(card, (0.1 * w).max(cols[0] * w), 0.06 * h, (0.9 * w).min(cols[1] * w), 0.94 * h);
        let mut best = vec![(-1.0f32, 0.0f32, 0.0f32); NT];
        for &sx in xs {
            for &sy in ys {
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
    pub fn refine(self) -> [f32; 40] {
        self.refine_styles().0
    }

    /// Come `refine`, con in più la somiglianza con ciascuno stile.
    pub fn refine_styles(mut self) -> ([f32; 40], Vec<[f32; 40]>) {
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
        // (I colori di riferimento sono di tutta la larghezza: per una striscia non si confrontano.)
        let vis = self.seen.vis;
        let whole_width = self.seen.cols == WHOLE;
        if let Some(seen) = self.hues.share(vis).filter(|_| whole_width && !self.seen.gray) {
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
        // Per ogni carta vale lo stile che le somiglia di più (poi la scena intera sceglie lo
        // stile del sito: `site_style`).
        let mut per_style = vec![[f32::MIN; 40]; NT / 40];
        let mut out = [f32::MIN; 40];
        for (k, b) in self.best.iter().enumerate() {
            per_style[k / 40][k % 40] = b.0;
            out[k % 40] = out[k % 40].max(b.0);
        }
        (out, per_style)
    }
}

/// Di quanto pesa meno uno stile diverso da quello del sito, una volta riconosciuto.
const STYLE_PENALTY: f32 = 0.15;

thread_local! {
    /// Gli ultimi voti per stile delle carte viste (vedi `site_style`): solo i più recenti, così
    /// se si cambia sito lo stile si riconosce di nuovo. (Per thread: la lettura dello schermo
    /// avviene sempre nello stesso.)
    static VOTES: std::cell::RefCell<std::collections::VecDeque<usize>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}

/// Quanti voti recenti si tengono.
const RECENT_VOTES: usize = 12;

/// Lo stile del mazzo del sito: ogni carta vista intera e riconosciuta con sicurezza in uno stile
/// (somiglianza alta, ben staccata dalle altre carte dello stesso stile e dagli altri stili) vota;
/// i voti recenti si tengono fra una lettura e l'altra. Vale lo stile con quasi tutti i voti, se
/// ne ha almeno due.
pub fn site_style(seen: &[&[[f32; 40]]]) -> Option<usize> {
    VOTES.with(|v| {
        let mut v = v.borrow_mut();
        for per in seen {
            let tops: Vec<(f32, f32)> = per
                .iter()
                .map(|sims| {
                    let mut x: Vec<f32> = sims.to_vec();
                    x.sort_by(|a, b| b.total_cmp(a));
                    (x[0], x[0] - x[1])
                })
                .collect();
            let Some(b) = (0..tops.len()).max_by(|&a, &b| tops[a].0.total_cmp(&tops[b].0)) else { continue };
            let runner = tops.iter().enumerate().filter(|(i, _)| *i != b).map(|(_, p)| p.0).fold(f32::MIN, f32::max);
            if tops[b].0 >= 0.85 && tops[b].1 >= 0.08 && tops[b].0 - runner >= 0.06 {
                v.push_back(b);
                if v.len() > RECENT_VOTES {
                    v.pop_front();
                }
            }
        }
        let mut count = vec![0usize; NT / 40];
        for &x in v.iter() {
            count[x] += 1;
        }
        let (i, &top) = count.iter().enumerate().max_by_key(|(_, n)| **n)?;
        (top >= 4 && top * 100 >= v.len() * 85).then_some(i)
    })
}

/// Dimentica lo stile del sito riconosciuto finora (un'altra lettura indipendente).
pub fn forget_style() {
    VOTES.with(|v| v.borrow_mut().clear());
}

/// La somiglianza di una carta tenendo conto dello stile del sito: gli altri stili pesano meno.
pub fn with_style(per_style: &[[f32; 40]], style: usize) -> [f32; 40] {
    let mut out = [f32::MIN; 40];
    for (st, sims) in per_style.iter().enumerate() {
        let penalty = if st == style { 0.0 } else { STYLE_PENALTY };
        for c in 0..40 {
            out[c] = out[c].max(sims[c] - penalty);
        }
    }
    out
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
                let s = Match::with_cols(&card, WHOLE, WHOLE).refine();
                let best = (0..40).max_by(|&a, &b| s[a].total_cmp(&s[b])).unwrap();
                assert_eq!(best as u8, cards::make(suit, col as u8), "{}", cards::name(best as u8));
            }
        }
    }
}
