//! Riconosce una carta napoletana raddrizzata senza immagini di riferimento:
//! - il seme dal colore dei simboli (spade blu, bastoni verdi e allungati, coppe con rosso e blu,
//!   denari tondi);
//! - il valore, dall'asso al sette, contando i simboli separati;
//! - le figure dalla presenza di volti e mani; il cavallo dal colore del cavallo.
//!
//! Il risultato è un punteggio per ciascuna delle 40 carte: chi lo usa sceglie la migliore fra
//! quelle ancora possibili e chiede conferma all'utente quando il distacco è piccolo.

use super::{hsv, Img, CARD_H, CARD_W};
use briscola_core::cards;

/// Quanto pesa un indizio sicuro: con due indizi sicuri (seme e valore) la carta vince di 4 punti.
const SURE: f32 = 2.0;

#[derive(Default, Debug, Clone, Copy)]
struct Colors {
    ink: f32,
    blue: f32,
    green: f32,
    red: f32,
    yellow: f32,
    skin: f32,
    brown: f32,
}

/// Componente connessa di pixel colorati.
#[derive(Debug, Clone, Copy)]
struct Comp {
    area: usize,
    w: usize,
    h: usize,
}

fn components(mask: &[bool], w: usize, h: usize, min_area: usize) -> Vec<Comp> {
    let mut seen = vec![false; mask.len()];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for s in 0..mask.len() {
        if !mask[s] || seen[s] {
            continue;
        }
        seen[s] = true;
        stack.push(s);
        let (mut area, mut x0, mut x1, mut y0, mut y1) = (0, usize::MAX, 0, usize::MAX, 0);
        while let Some(i) = stack.pop() {
            area += 1;
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if mask[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        if area >= min_area {
            out.push(Comp { area, w: x1 - x0 + 1, h: y1 - y0 + 1 });
        }
    }
    out.sort_by(|a, b| b.area.cmp(&a.area));
    out
}

/// Simboli "veri": scarta i frammenti molto più piccoli del simbolo più grande.
fn symbols(comps: &[Comp]) -> Vec<Comp> {
    let Some(big) = comps.first() else { return Vec::new() };
    comps.iter().copied().filter(|c| c.area * 4 >= big.area).collect()
}

/// Punteggio delle 40 carte per una carta raddrizzata. `partial`: si vede solo la metà alta.
pub fn structural(img: &Img, partial: bool) -> [f32; 40] {
    let m = CARD_W / 12; // margine: salta il bordo (anche quello arancione delle carte in tavola)
    let w = CARD_W - 2 * m;
    let h_full = CARD_H - 2 * m;
    let h = if partial { h_full * 55 / 100 } else { h_full };
    let n = (w * h) as f32;
    let mut c = Colors::default();
    let (mut ink, mut blue) = (vec![false; w * h], vec![false; w * h]);
    for y in 0..h {
        for x in 0..w {
            let p = img.at(x + m, y + m);
            let (hu, s, v) = hsv(p);
            let i = y * w + x;
            ink[i] = s > 70 || v < 150;
            blue[i] = (190.0..=260.0).contains(&hu) && s > 80;
            c.ink += ink[i] as u8 as f32;
            c.blue += blue[i] as u8 as f32;
            c.green += ((70.0..=180.0).contains(&hu) && s > 60 && v > 40) as u8 as f32;
            c.red += ((hu <= 16.0 || hu >= 330.0) && s > 100) as u8 as f32;
            c.yellow += ((30.0..=68.0).contains(&hu) && s > 90) as u8 as f32;
            c.skin += ((6.0..=44.0).contains(&hu) && (25..=120).contains(&s) && v >= 170) as u8 as f32;
            c.brown += ((6.0..=44.0).contains(&hu) && s >= 50 && (60..170).contains(&v)) as u8 as f32;
        }
    }
    for f in [&mut c.ink, &mut c.blue, &mut c.green, &mut c.red, &mut c.yellow, &mut c.skin, &mut c.brown] {
        *f /= n;
    }
    let min_area = w * h / 400;
    let inks = components(&ink, w, h, min_area);
    let blues = components(&blue, w, h, min_area / 2);
    let tall = inks.first().map_or(0.0, |b| b.h as f32 / h as f32);

    let mut suit = [0.0f32; 4];
    let mut rank = [0.0f32; 10];
    let figure = c.skin > 0.012 && tall > 0.55 && c.blue < 0.08;
    if figure {
        // Figure: il cavallo si vede; fante e re si somigliano. Il seme è in mano alla figura:
        // indizi deboli dai colori.
        if c.brown > 0.12 {
            rank[8] = SURE;
        } else {
            rank[7] = SURE * 0.75;
            rank[9] = SURE * 0.75;
            rank[8] = SURE * 0.25;
        }
        // Il simbolo tenuto in mano, quando è staccato dal disegno: una moneta tonda per i denari.
        let big = inks.first().map_or(0, |b| b.area);
        let held = inks.iter().skip(1).filter(|s| s.area * 60 >= w * h && s.area * 4 < big);
        if held.clone().any(|s| (s.h as f32 / s.w as f32 - 1.0).abs() < 0.25) {
            suit[0] += 0.8;
        }
        if held.clone().any(|s| s.h as f32 > 2.5 * s.w as f32 || s.w as f32 > 2.5 * s.h as f32) {
            suit[2] += 0.4;
            suit[3] += 0.4;
        }
        suit[2] += 0.3 * (c.blue > 0.01) as u8 as f32;
        suit[1] += 0.2 * (c.blue > 0.01 && c.red > 0.03) as u8 as f32;
        return combine(suit, rank);
    }
    let syms = symbols(&inks);
    let elongated = syms.iter().filter(|s| s.h as f32 > 2.2 * s.w as f32).count();
    let round = syms.iter().filter(|s| (s.h as f32 / s.w as f32 - 1.0).abs() < 0.3).count();
    // Seme.
    let count = if c.blue > 0.04 && c.red < 0.02 {
        suit[2] = SURE;
        symbols(&blues).len()
    } else if c.blue > 0.015 && c.red > 0.03 {
        suit[1] = SURE;
        syms.len()
    } else if c.green > 0.04 && elongated * 2 >= syms.len().max(1) {
        suit[3] = SURE;
        syms.len()
    } else if round * 2 >= syms.len().max(1) {
        suit[0] = SURE;
        syms.len()
    } else {
        suit = [0.5; 4];
        syms.len()
    };
    if partial {
        // Della briscola si vede metà carta: i simboli contati sono circa la metà.
        for r in 0..7 {
            rank[r] = 0.5;
        }
        if count >= 1 {
            let est = (count * 2).min(7);
            rank[est - 1] = 1.0;
        }
    } else if (1..=7).contains(&count) {
        rank[count - 1] = SURE;
        if count > 1 {
            rank[count - 2] = SURE * 0.25;
        }
        if count < 7 {
            rank[count] = SURE * 0.25;
        }
    }
    combine(suit, rank)
}

fn combine(suit: [f32; 4], rank: [f32; 10]) -> [f32; 40] {
    let mut out = [0.0; 40];
    for s in 0..4u8 {
        for r in 0..10u8 {
            out[cards::make(s, r) as usize] = suit[s as usize] + rank[r as usize];
        }
    }
    out
}
