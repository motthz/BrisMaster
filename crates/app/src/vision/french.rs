//! Carte francesi del poker all'italiana (dal 7 all'Asso): le trova sul tavolo e le riconosce
//! senza immagini di riferimento, leggendo l'indice nell'angolo in alto a sinistra:
//! - il valore dalla forma del carattere (buchi, gambe, asta a sinistra o a destra; il 10 ha due
//!   caratteri), confermato dal numero di simboli al centro quando la carta si vede intera;
//! - il seme dal colore (rosso o nero; nei mazzi a quattro colori quadri blu e fiori verdi) e dalla
//!   forma del simbolo sotto il valore, confrontata con le sagome di cuori, quadri, fiori e picche.
//!
//! Basta l'angolo, quindi si riconoscono anche le carte della mano sovrapposte a ventaglio, di cui
//! si vede solo la striscia di sinistra.

use super::detect::{blobs, is_white, view_of, Blob, RotRect};
use super::memory::{self, Memory};
use super::{hsv, CardView, Img, CARD_H, CARD_W};
use poker_it::cards::{make, Card, NUM_CARDS};

/// Proporzione larghezza/altezza delle carte francesi.
pub const RATIO: f32 = 0.714;

/// Quanto pesa un indizio sicuro (come per le carte napoletane).
const SURE: f32 = 2.0;

/// Colore dell'inchiostro: nero, rosso, blu, verde; `OTHER` per i disegni delle figure.
const BLACK: u8 = 0;
const RED: u8 = 1;
const BLUE: u8 = 2;
const GREEN: u8 = 3;
const OTHER: u8 = 4;

fn ink_class(p: [u8; 3]) -> Option<u8> {
    let (h, s, v) = hsv(p);
    if v < 120 && s < 110 {
        return Some(BLACK);
    }
    if s <= 80 && v >= 140 {
        return None;
    }
    Some(if (h <= 20.0 || h >= 330.0) && s > 90 && v > 90 {
        RED
    } else if (190.0..=260.0).contains(&h) && s > 80 {
        BLUE
    } else if (70.0..=170.0).contains(&h) && s > 70 && v > 60 {
        GREEN
    } else if v < 120 {
        BLACK
    } else {
        OTHER
    })
}

/// Componente connessa d'inchiostro dello stesso colore.
#[derive(Clone, Debug)]
struct Comp {
    color: u8,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    pts: Vec<(usize, usize)>,
}

impl Comp {
    fn w(&self) -> usize {
        self.x1 - self.x0 + 1
    }
    fn h(&self) -> usize {
        self.y1 - self.y0 + 1
    }
    fn area(&self) -> usize {
        self.pts.len()
    }
    fn cx(&self) -> f32 {
        (self.x0 + self.x1) as f32 / 2.0
    }
    fn cy(&self) -> f32 {
        (self.y0 + self.y1) as f32 / 2.0
    }
}

/// Componenti d'inchiostro della carta raddrizzata, senza quelle che toccano il bordo (il panno
/// negli angoli arrotondati, la cornice).
fn components(img: &Img) -> Vec<Comp> {
    let (w, h) = (img.w, img.h);
    let m = 3;
    let class: Vec<Option<u8>> = img.px.iter().map(|&p| ink_class(p)).collect();
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for s in 0..w * h {
        let Some(col) = class[s] else { continue };
        if seen[s] {
            continue;
        }
        seen[s] = true;
        stack.push(s);
        let mut c = Comp { color: col, x0: usize::MAX, y0: usize::MAX, x1: 0, y1: 0, pts: Vec::new() };
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            c.pts.push((x, y));
            c.x0 = c.x0.min(x);
            c.x1 = c.x1.max(x);
            c.y0 = c.y0.min(y);
            c.y1 = c.y1.max(y);
            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                // I colori non si mescolano: un simbolo rosso non si attacca al disegno giallo.
                if !seen[j] && class[j] == Some(col) {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        let touches = c.x0 < m || c.y0 < m || c.x1 + m >= w || c.y1 + m >= h;
        if !touches && c.area() >= 6 {
            out.push(c);
        }
    }
    out
}

/// Maschera della componente in una griglia `n` x `n` sul suo riquadro (frazione coperta, con 3x3
/// campioni per casella: funziona anche quando il simbolo è più piccolo della griglia).
fn grid(c: &Comp, n: usize) -> Vec<f32> {
    let (w, h) = (c.w(), c.h());
    let mut m = vec![false; w * h];
    for &(x, y) in &c.pts {
        m[(y - c.y0) * w + x - c.x0] = true;
    }
    let mut g = vec![0.0f32; n * n];
    for j in 0..n {
        for i in 0..n {
            let mut k = 0;
            for sj in 0..3 {
                for si in 0..3 {
                    let x = ((i as f32 + (si as f32 + 0.5) / 3.0) / n as f32 * w as f32) as usize;
                    let y = ((j as f32 + (sj as f32 + 0.5) / 3.0) / n as f32 * h as f32) as usize;
                    k += m[y.min(h - 1) * w + x.min(w - 1)] as u32;
                }
            }
            g[j * n + i] = k as f32 / 9.0;
        }
    }
    g
}

/// Sagome dei quattro semi (picche, fiori, quadri, cuori) sul riquadro unitario.
fn suit_shape(s: u8, u: f32, v: f32) -> bool {
    let circle = |cx: f32, cy: f32, r: f32| (u - cx).powi(2) + (v - cy).powi(2) <= r * r;
    let tri = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        let d = |p: (f32, f32), q: (f32, f32)| (u - q.0) * (p.1 - q.1) - (p.0 - q.0) * (v - q.1);
        let (d1, d2, d3) = (d(a, b), d(b, c), d(c, a));
        !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
    };
    match s {
        0 => {
            circle(0.27, 0.6, 0.26)
                || circle(0.73, 0.6, 0.26)
                || tri((0.02, 0.56), (0.98, 0.56), (0.5, 0.0))
                || tri((0.5, 0.6), (0.7, 1.0), (0.3, 1.0))
        }
        1 => {
            circle(0.5, 0.24, 0.24)
                || circle(0.24, 0.56, 0.24)
                || circle(0.76, 0.56, 0.24)
                || circle(0.5, 0.5, 0.14)
                || tri((0.5, 0.5), (0.68, 1.0), (0.32, 1.0))
        }
        2 => (u - 0.5).abs() / 0.5 + (v - 0.5).abs() / 0.5 <= 1.0,
        _ => circle(0.27, 0.28, 0.27) || circle(0.73, 0.28, 0.27) || tri((0.02, 0.36), (0.98, 0.36), (0.5, 1.0)),
    }
}

/// Larghezza della punta rispetto alla larghezza poco più in basso: bassa per picche e quadri
/// (finiscono a punta), alta per fiori e cuori (lobi tondi). Si misura sull'inchiostro sfumato
/// riga per riga, così regge anche con simboli alti dieci pixel.
fn tip_ratio(img: &Img, c: &Comp) -> f32 {
    let ink = |x: usize, y: usize| {
        let p = img.at(x, y);
        let (_, s, v) = hsv(p);
        // Quanto il pixel si allontana dal bianco della carta (0 = carta, 1 = inchiostro pieno).
        ((255.0 - v as f32).max(s as f32) / 160.0).min(1.0)
    };
    let rows: Vec<f32> = (c.y0..=c.y1).map(|y| (c.x0..=c.x1).map(|x| ink(x, y)).sum()).collect();
    let at = |v: f32| {
        let f = v * rows.len() as f32 - 0.5;
        let i = f.floor().max(0.0) as usize;
        let j = (i + 1).min(rows.len() - 1);
        let t = (f - i as f32).clamp(0.0, 1.0);
        rows[i.min(rows.len() - 1)] * (1.0 - t) + rows[j] * t
    };
    at(0.14) / at(0.34).max(1e-3)
}

/// Quanto la componente somiglia a ciascun seme: soprattutto la punta (vedi `tip_ratio`), poi la sagoma.
fn suit_votes(img: &Img, c: &Comp) -> [f32; 4] {
    let r = tip_ratio(img, c);
    let iou = suit_match(c);
    const TIP: [f32; 4] = [0.55, 0.95, 0.55, 0.9];
    core::array::from_fn(|s| 0.5 * iou[s] - 3.0 * (r - TIP[s]).abs())
}

/// Somiglianza (intersezione su unione) fra la componente e la sagoma di ciascun seme.
fn suit_match(c: &Comp) -> [f32; 4] {
    const N: usize = 16;
    let g = grid(c, N);
    let mut out = [0.0; 4];
    for (s, o) in out.iter_mut().enumerate() {
        let (mut inter, mut union) = (0.0, 0.0);
        for j in 0..N {
            for i in 0..N {
                let a = g[j * N + i];
                let b = suit_shape(s as u8, (i as f32 + 0.5) / N as f32, (j as f32 + 0.5) / N as f32) as u8 as f32;
                inter += a.min(b);
                union += a.max(b);
            }
        }
        *o = if union > 0.0 { inter / union } else { 0.0 };
    }
    out
}

/// Caratteristiche del valore scritto nell'angolo (uno o due caratteri).
#[derive(Debug, Default)]
struct Glyph {
    parts: usize,
    /// Per ogni buco: altezza del centro e altezza del buco, rispetto al carattere.
    holes: Vec<(f32, f32)>,
    /// Pezzi d'inchiostro nella riga più bassa (le due gambe della A).
    legs: usize,
    /// Larghezza dell'inchiostro nella riga più alta, rispetto al carattere.
    top: f32,
    /// Frazione delle righe con inchiostro nella colonna di sinistra e di destra.
    left: f32,
    right: f32,
    /// Larghezza / altezza.
    aspect: f32,
}

fn glyph(img: &Img, parts: &[&Comp]) -> Glyph {
    let c = parts[0];
    let (w, h) = (c.w(), c.h());
    let mut m = vec![false; w * h];
    for &(x, y) in &c.pts {
        m[(y - c.y0) * w + x - c.x0] = true;
    }
    // Anche i pixel sfumati del bordo dei tratti: chiudono i buchi dei caratteri sottili.
    for y in 0..h {
        for x in 0..w {
            let (_, s, v) = hsv(img.at(c.x0 + x, c.y0 + y));
            m[y * w + x] |= v < 200 || s > 50;
        }
    }
    // Buchi: zone vuote che non toccano il bordo del riquadro.
    let mut out = vec![false; w * h];
    let mut stack: Vec<usize> = (0..w * h).filter(|&i| !m[i] && (i % w == 0 || i % w == w - 1 || i / w == 0 || i / w == h - 1)).collect();
    for &i in &stack {
        out[i] = true;
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
            if nx < w && ny < h && !m[ny * w + nx] && !out[ny * w + nx] {
                out[ny * w + nx] = true;
                stack.push(ny * w + nx);
            }
        }
    }
    let mut holes = Vec::new();
    let mut seen = out.clone();
    let min_hole = ((w * h) / 60).max(2);
    for s in 0..w * h {
        if m[s] || seen[s] {
            continue;
        }
        let (mut n, mut sy, mut ya, mut yb) = (0, 0, h, 0);
        seen[s] = true;
        stack.push(s);
        while let Some(i) = stack.pop() {
            n += 1;
            sy += i / w;
            ya = ya.min(i / w);
            yb = yb.max(i / w);
            let (x, y) = (i % w, i / w);
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < w && ny < h && !m[ny * w + nx] && !seen[ny * w + nx] {
                    seen[ny * w + nx] = true;
                    stack.push(ny * w + nx);
                }
            }
        }
        if n >= min_hole {
            holes.push(((sy as f32 / n as f32 + 0.5) / h as f32, (yb - ya + 1) as f32 / h as f32));
        }
    }
    let runs = |y: usize| {
        let mut r = 0;
        let mut prev = false;
        for x in 0..w {
            if m[y * w + x] && !prev {
                r += 1;
            }
            prev = m[y * w + x];
        }
        r
    };
    let row_ink = |y: usize| (0..w).filter(|&x| m[y * w + x]).count();
    let band = |x0: usize, x1: usize| (0..h).filter(|&y| (x0..x1).any(|x| m[y * w + x])).count() as f32 / h as f32;
    let col_w = (w / 4).max(1);
    let low = h - 1 - h / 12;
    Glyph {
        parts: parts.len(),
        holes,
        legs: runs(low).max(runs(h - 1)),
        top: (0..=h / 10).map(row_ink).max().unwrap_or(0) as f32 / w as f32,
        left: band(0, col_w),
        right: band(w - col_w, w),
        aspect: w as f32 / h as f32,
    }
}

/// Punteggio dei valori (0 = Sette ... 7 = Asso) dalla forma del carattere.
fn rank_from_glyph(g: &Glyph, narrow: bool) -> [f32; 8] {
    let mut r = [0.0f32; 8];
    let mut pick = |k: usize, alt: &[usize]| {
        r[k] += SURE;
        for &a in alt {
            r[a] += 0.5;
        }
    };
    if g.parts >= 2 {
        pick(3, &[]);
    } else if g.holes.len() >= 2 {
        pick(1, &[]);
    } else if g.holes.len() == 1 {
        let (_, hh) = g.holes[0];
        if g.legs >= 2 && g.top < 0.6 {
            pick(7, &[5]);
        } else if hh < 0.5 {
            pick(2, &[5]);
        } else {
            pick(5, &[2, 7]);
        }
    } else if g.left > 0.75 && !narrow {
        pick(6, &[4]);
    } else if g.right > 0.65 || g.aspect < 0.55 {
        pick(4, &[0]);
    } else if narrow && g.left > 0.75 {
        // Un'asta sola: l'"1" del 10 staccato dallo "0".
        pick(3, &[4]);
    } else if g.top < 0.45 {
        // Una punta in alto senza buco: la A col buco chiuso dalle grazie o dalla sfumatura.
        pick(7, &[0]);
    } else {
        pick(0, &[4]);
    }
    r
}

/// Punteggio delle 32 carte (nelle prime 32 posizioni) per una carta francese raddrizzata.
/// `partial`: della carta si vede solo la striscia di sinistra (mano a ventaglio).
pub fn structural(img: &Img, partial: bool) -> [f32; 40] {
    let mut out = [0.0f32; 40];
    let comps = components(img);
    let (w, h) = (img.w as f32, img.h as f32);
    let colored = |c: &Comp| c.color != OTHER && c.h() as f32 >= 0.05 * h && (c.h() as f32) < 0.45 * h && (c.w() as f32) < 0.45 * w;
    // Il primo carattere dell'indice: il segno colorato più vicino all'angolo in alto a sinistra.
    let Some(first) = comps
        .iter()
        .filter(|c| colored(c) && c.cx() < 0.5 * w && c.cy() < 0.4 * h)
        .min_by(|a, b| (a.x0 + a.y0).cmp(&(b.x0 + b.y0)))
    else {
        return out;
    };
    // Gli altri caratteri sulla stessa riga (lo "0" del 10).
    let mut parts = vec![first];
    for c in comps.iter().filter(|c| colored(c) && c.color == first.color && !std::ptr::eq(*c, first)) {
        let last = parts.last().unwrap();
        let overlap = c.y1.min(last.y1) as f32 - c.y0.max(last.y0) as f32;
        let similar = (c.h() as f32 / first.h() as f32 - 1.0).abs() < 0.3;
        let next = c.x0 > last.x1 && ((c.x0 - last.x1) as f32) < 0.5 * first.h() as f32;
        if overlap > 0.6 * first.h() as f32 && similar && next {
            parts.push(c);
        }
    }
    let g = glyph(img, &parts);
    let narrow = (first.w() as f32) < 0.4 * first.h() as f32;
    let mut rank = rank_from_glyph(&g, narrow);
    let color = first.color;
    // Il simbolo del seme: il segno dello stesso colore più vicino al valore, sotto o a destra.
    let in_parts = |c: &Comp| parts.iter().any(|p| std::ptr::eq(*p, c));
    let suit_sym = comps
        .iter()
        .filter(|c| c.color == color && !in_parts(c) && c.area() >= first.area() / 6 && c.y0 + 2 >= first.y1)
        // Sotto il valore (indice classico) o, se più grande del valore, anche spostato a destra (indice grande).
        .filter(|c| (c.x0 <= parts.last().unwrap().x1 + 2 && c.x1 + 2 >= first.x0) || c.area() * 2 >= first.area())
        .min_by(|a, b| {
            let d = |c: &Comp| (c.cx() - first.cx()).powi(2) + (c.cy() - first.cy()).powi(2);
            d(a).partial_cmp(&d(b)).unwrap()
        });
    // I simboli al centro (quando la carta si vede intera) votano insieme al simbolo dell'indice.
    let in_corner = |c: &Comp| {
        (c.cx() < 0.3 * w && c.cy() < 0.4 * h) || (c.cx() > 0.7 * w && c.cy() > 0.6 * h) || in_parts(c)
    };
    let is_sym = |c: &Comp| suit_sym.is_some_and(|s| std::ptr::eq(s, c));
    // Le figure hanno al centro un disegno, non simboli da contare.
    let card_area = w * h;
    let other: usize = comps.iter().filter(|c| c.color == OTHER).map(|c| c.area()).sum();
    let figure = other as f32 > 0.02 * card_area;
    let pips: Vec<&Comp> = if partial || figure {
        Vec::new()
    } else {
        comps.iter().filter(|c| c.color == color && !in_corner(c) && !is_sym(c)).collect()
    };
    let big = pips.iter().map(|c| c.area()).max().unwrap_or(0);
    // I simboli veri sono grandi almeno quanto quello dell'indice: i pezzetti staccati non contano.
    let min_pip = suit_sym.map_or(first.area() / 4, |s| s.area() * 6 / 10);
    let pips: Vec<&Comp> = pips.into_iter().filter(|c| c.area() * 3 >= big && c.area() >= min_pip).collect();
    let mut suit = [0.0f32; 4];
    match color {
        BLUE => suit[2] = SURE,
        GREEN => suit[1] = SURE,
        _ => {
            let pair: [usize; 2] = if color == RED { [2, 3] } else { [0, 1] };
            for &p in &pair {
                suit[p] = 0.5;
            }
            let mut votes = [0.0f32; 4];
            let mut n = 0.0;
            for c in suit_sym.into_iter().chain(pips.iter().copied()) {
                let v = suit_votes(img, c);
                for s in 0..4 {
                    votes[s] += v[s];
                }
                n += 1.0;
            }
            if n > 0.0 {
                let (a, b) = (votes[pair[0]] / n, votes[pair[1]] / n);
                let (win, lose) = if a >= b { (pair[0], pair[1]) } else { (pair[1], pair[0]) };
                let gap = (a - b).abs();
                // Col solo simbolo dell'indice (figure, carte a ventaglio) serve un distacco più netto.
                let sure = if pips.is_empty() { 0.5 } else { 0.3 };
                suit[win] = if gap > sure { SURE } else { 1.0 + gap * 2.0 };
                suit[lose] = 0.5;
            }
        }
    }
    if !partial {
        // Il centro della carta: simboli da contare, oppure il disegno di una figura.
        if figure {
            for k in 4..=6 {
                rank[k] += 1.0;
            }
        } else if pips.len() == 1 && big as f32 > 0.06 * card_area {
            rank[7] += 1.5;
        } else if (6..=10).contains(&pips.len()) {
            // Due simboli che si toccano contano uno: anche il valore appena sotto è possibile.
            let n = pips.len().max(7);
            rank[n - 7] += 1.5;
            if n > 7 {
                rank[n - 8] += 0.3;
            }
        }
    }
    for r in 0..8u8 {
        for s in 0..4u8 {
            out[make(r, s) as usize] = rank[r as usize] + suit[s as usize];
        }
    }
    out
}

/// L'angolo in alto a sinistra ingrandito alle dimensioni di una carta: è quello che si ricorda e
/// si confronta in memoria (l'unica parte che si vede sempre, anche a ventaglio).
pub fn corner(img: &Img) -> Img {
    let mut out = Img::new(CARD_W, CARD_H);
    let (cw, ch) = (img.w as f32 * 0.42, img.h as f32 * 0.42);
    for j in 0..CARD_H {
        for i in 0..CARD_W {
            let x = (i as f32 + 0.5) / CARD_W as f32 * cw;
            let y = (j as f32 + 0.5) / CARD_H as f32 * ch;
            out.px[j * CARD_W + i] = img.sample(x, y);
        }
    }
    out
}

/// Riconosce la carta: forme e colori, più la somiglianza con le carte già viste.
pub fn recognize(view: &mut CardView, mem: &Memory) {
    let mut s = structural(&view.img, view.partial);
    for (c, sim) in mem.similarity(&corner(&view.img), false).iter().enumerate().take(NUM_CARDS) {
        if let Some(v) = *sim {
            if v >= memory::SAME {
                s[c] += 6.0 + (v - memory::SAME) * 20.0;
            }
        }
    }
    view.scores = s;
}

/// La carta più probabile fra quelle per cui vale `allowed`, e quanto stacca la seconda.
pub fn best(view: &CardView, allowed: impl Fn(Card) -> bool) -> Option<(Card, f32)> {
    let mut v: Vec<(Card, f32)> = (0..NUM_CARDS as Card).filter(|&c| allowed(c)).map(|c| (c, view.scores[c as usize])).collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let first = *v.first()?;
    let margin = v.get(1).map_or(f32::INFINITY, |s| first.1 - s.1);
    Some((first.0, margin))
}

/// Quello che si vede del tavolo del poker: le tue carte, da sinistra a destra.
#[derive(Clone, Debug, Default)]
pub struct PokerScene {
    pub hand: Vec<CardView>,
}

/// Altezza e larghezza sullo schermo di una zona chiara (le carte stanno dritte).
fn extent(r: &RotRect) -> (f32, f32) {
    let hw = (r.u.0 * r.hw).abs() + (r.v.0 * r.hh).abs();
    let hh = (r.u.1 * r.hw).abs() + (r.v.1 * r.hh).abs();
    (2.0 * hw, 2.0 * hh)
}

/// Trova le tue carte: la fila più in basso di carte scoperte. Una zona chiara più larga di una
/// carta è una mano a ventaglio: si divide dove si vedono i bordi verticali delle carte.
/// None se sullo schermo non c'è niente che somigli a un tavolo con carte.
pub fn scene(img: &Img) -> Option<PokerScene> {
    let all = blobs(img);
    let faces: Vec<&Blob> = all
        .iter()
        .filter(|b| b.white > 0.3 && b.framed > 0.5)
        .filter(|b| {
            // Anche le strisce delle carte a ventaglio separate dall'ombra della carta sopra.
            let (w, h) = extent(&b.rect);
            w / h >= 0.12 && h >= 30.0
        })
        .collect();
    let backs = all.iter().filter(|b| b.white <= 0.3 && b.framed > 0.5).count();
    let mut heights: Vec<f32> = faces
        .iter()
        .filter(|b| {
            let (w, h) = extent(&b.rect);
            (0.6..=0.82).contains(&(w / h)) || w / h > 1.0
        })
        .map(|b| extent(&b.rect).1)
        .collect();
    if heights.is_empty() {
        return (backs > 0 || felt_visible(img)).then(PokerScene::default);
    }
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let card_h = heights[heights.len() / 2];
    let card_w = card_h * RATIO;
    let row: Vec<&Blob> = faces
        .iter()
        .filter(|b| (0.8..=1.2).contains(&(extent(&b.rect).1 / card_h)))
        .copied()
        .collect();
    let bottom = row.iter().map(|b| b.rect.cy).fold(f32::MIN, f32::max);
    let mut hand: Vec<&Blob> = row.into_iter().filter(|b| b.rect.cy >= bottom - 0.5 * card_h).collect();
    hand.sort_by(|a, b| a.rect.cx.partial_cmp(&b.rect.cx).unwrap());
    let mut views = Vec::new();
    for b in hand {
        let (w, h) = extent(&b.rect);
        if w < 0.85 * card_w {
            // Striscia di una carta coperta in parte da quella alla sua destra.
            let x0 = b.rect.cx - w / 2.0;
            let r = RotRect { cx: x0 + card_w / 2.0, cy: b.rect.cy, u: (1.0, 0.0), v: (0.0, 1.0), hw: card_w / 2.0, hh: h / 2.0 };
            views.push(view_of(img, &r, true));
            continue;
        }
        if w <= 1.2 * card_w {
            views.push(view_of(img, &b.rect, false));
            continue;
        }
        let (x0, y0) = (b.rect.cx - w / 2.0, b.rect.cy - h / 2.0);
        let mut edges = vec![x0];
        edges.extend(fan_edges(img, x0, y0, w, h, card_w));
        for (i, &e) in edges.iter().enumerate() {
            let last = i + 1 == edges.len();
            let right = if last { x0 + w } else { e + card_w };
            // Le carte coperte in parte si raddrizzano intere: dell'angolo in alto a sinistra basta la striscia visibile.
            let cw = if last { right - e } else { card_w };
            let r = RotRect { cx: e + cw / 2.0, cy: b.rect.cy, u: (1.0, 0.0), v: (0.0, 1.0), hw: cw / 2.0, hh: h / 2.0 };
            views.push(view_of(img, &r, !last));
        }
    }
    Some(PokerScene { hand: views })
}

/// Bordi sinistri delle carte sovrapposte dentro una zona chiara: colonne in cui quasi tutta
/// l'altezza ha una riga scura (il bordo o l'ombra della carta sopra).
fn fan_edges(img: &Img, x0: f32, y0: f32, w: f32, h: f32, card_w: f32) -> Vec<f32> {
    let lum = |x: usize, y: usize| {
        let p = img.at(x.min(img.w - 1), y.min(img.h - 1));
        (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0
    };
    let (xa, xb) = ((x0 + 0.12 * card_w).max(2.0) as usize, (x0 + w - 0.12 * card_w) as usize);
    let (ya, yb) = ((y0 + 0.15 * h).max(0.0) as usize, (y0 + 0.85 * h) as usize);
    let mut score = Vec::new();
    for x in xa..xb.min(img.w - 3) {
        let rows = (ya..yb).filter(|&y| {
            let p = img.at(x, y);
            let side = lum(x - 2, y).max(lum(x + 2, y));
            !is_white(p) && lum(x, y) + 25.0 < side
        });
        score.push((x, rows.count() as f32 / (yb - ya).max(1) as f32));
    }
    // Picchi sopra soglia, uno ogni almeno 0.15 carte.
    let mut peaks: Vec<(usize, f32)> = Vec::new();
    for &(x, s) in &score {
        if s < 0.6 {
            continue;
        }
        match peaks.last_mut() {
            Some(p) if (x - p.0) as f32 <= 0.15 * card_w => {
                // Si tiene la colonna più a destra: il bordo della carta viene dopo l'ombra.
                if s >= p.1 * 0.8 {
                    *p = (x, s.max(p.1));
                }
            }
            _ => peaks.push((x, s)),
        }
    }
    peaks.into_iter().map(|(x, _)| x as f32 + 1.0).collect()
}

/// Vero se un quinto dello schermo è di un colore acceso (il panno del tavolo, di qualsiasi colore).
fn felt_visible(img: &Img) -> bool {
    let (mut n, mut ok) = (0, 0);
    for y in (0..img.h).step_by(8) {
        for x in (0..img.w).step_by(8) {
            let (_, s, v) = hsv(img.at(x, y));
            n += 1;
            ok += (s > 70 && v > 50) as usize;
        }
    }
    n > 0 && ok * 5 > n
}

/// Lettura per il poker: trova le tue carte e le riconosce.
pub fn read(img: &Img, mem: &Memory) -> Option<PokerScene> {
    let mut scene = scene(img)?;
    for v in &mut scene.hand {
        recognize(v, mem);
    }
    Some(scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    use poker_it::cards::{name, rank, suit};

    /// Distacco con cui l'assistente inserisce la carta senza chiedere.
    const AUTO: f32 = 1.4;

    fn load(f: &str) -> Img {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata").join(f);
        super::super::capture::load_png(&path).unwrap()
    }

    /// Un foglio con le 32 carte: righe = semi (picche, fiori, quadri, cuori), colonne = valori.
    fn sheet(f: &str) -> Vec<(Card, Card, f32)> {
        let img = load(f);
        let all = blobs(&img);
        let top = all.iter().map(|b| b.rect.long()).fold(0.0, f32::max);
        let mut bs: Vec<Blob> = all.into_iter().filter(|b| b.white > 0.3 && b.rect.long() > 0.85 * top).collect();
        assert_eq!(bs.len(), 32, "{f}");
        let ch = bs[0].rect.long();
        bs.sort_by_key(|b| ((b.rect.cy / ch) as i32, b.rect.cx as i32));
        let mem = Memory::default();
        bs.iter()
            .enumerate()
            .map(|(i, b)| {
                let mut v = view_of(&img, &b.rect, false);
                recognize(&mut v, &mem);
                let (c, m) = best(&v, |_| true).unwrap();
                (make((i % 8) as u8, (i / 8) as u8), c, m)
            })
            .collect()
    }

    /// Nessuna carta sbagliata con sicurezza (quelle incerte si fanno confermare) e al più
    /// `max_unsure` carte incerte.
    fn check(f: &str, max_unsure: usize) {
        let res = sheet(f);
        let sure_wrong: Vec<String> = res
            .iter()
            .filter(|r| r.0 != r.1 && r.2 >= AUTO)
            .map(|r| format!("{} letto {} ({:.1})", name(r.0), name(r.1), r.2))
            .collect();
        assert!(sure_wrong.is_empty(), "{f}: {sure_wrong:#?}");
        let unsure: Vec<String> =
            res.iter().filter(|r| r.0 != r.1 || r.2 < AUTO).map(|r| format!("{} ({:.1})", name(r.0), r.2)).collect();
        assert!(unsure.len() <= max_unsure, "{f}: {unsure:#?}");
    }

    #[test]
    fn reads_classic_deck() {
        check("mazzo_classico.png", 4);
    }

    #[test]
    fn reads_serif_deck() {
        check("mazzo_serif.png", 4);
    }

    #[test]
    fn reads_small_deck() {
        check("mazzo_piccolo.png", 4);
    }

    #[test]
    fn reads_four_color_deck() {
        check("mazzo_jumbo.png", 0);
    }

    fn hand(f: &str) -> Vec<Card> {
        let s = read(&load(f), &Memory::default()).expect("tavolo");
        s.hand.iter().map(|v| best(v, |_| true).unwrap().0).collect()
    }

    fn cards(v: &[(u8, u8)]) -> Vec<Card> {
        v.iter().map(|&(r, s)| make(r, s)).collect()
    }

    #[test]
    fn reads_hands_on_table() {
        assert_eq!(hand("poker_classico.png"), cards(&[(7, 0), (3, 3), (4, 2), (0, 1), (2, 0)]));
        // Due carte sollevate (scelte per il cambio) e caratteri con le grazie.
        assert_eq!(hand("poker_classico2.png"), cards(&[(1, 3), (5, 1), (6, 2), (7, 3), (3, 0)]));
        assert_eq!(hand("poker_jumbo.png"), cards(&[(0, 2), (4, 1), (3, 0), (6, 3), (7, 1)]));
    }

    #[test]
    fn reads_fanned_hand() {
        let h = hand("poker_ventaglio.png");
        let names: Vec<String> = h.iter().map(|&c| format!("{}{}", rank(c), suit(c))).collect();
        assert_eq!(h, cards(&[(6, 0), (2, 3), (7, 2), (1, 1), (5, 3)]), "{names:?}");
    }

    /// Una carta ricordata si riconosce dalla memoria anche quando le forme sbagliano.
    #[test]
    fn memory_wins() {
        let img = load("poker_classico.png");
        let s = scene(&img).unwrap();
        let mut mem = Memory::default();
        let wrong = make(5, 3);
        mem.remember(&corner(&s.hand[0].img), wrong);
        let mut v = s.hand[0].clone();
        recognize(&mut v, &mem);
        assert_eq!(best(&v, |_| true).unwrap().0, wrong);
        let mut other = s.hand[1].clone();
        recognize(&mut other, &mem);
        assert_ne!(best(&other, |_| true).unwrap().0, wrong);
    }

    /// Tavolo senza carte scoperte ma con i dorsi dell'avversario: scena vuota.
    #[test]
    fn no_cards_is_empty_scene() {
        let mut img = load("poker_classico.png");
        let felt = img.at(200, 400);
        for y in 500..img.h {
            for x in 0..img.w {
                img.px[y * img.w + x] = felt;
            }
        }
        let s = scene(&img).expect("tavolo");
        assert!(s.hand.is_empty());
    }
}

