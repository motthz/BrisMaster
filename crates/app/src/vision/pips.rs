//! Conteggio dei simboli (denari, coppe, spade, bastoni) su una carta raddrizzata: un indizio che
//! non dipende dal disegno del mazzo. Un quattro ha quattro simboli in qualunque mazzo; dove i
//! simboli si toccano (le spade incrociate di alcuni mazzi) il conteggio non è sicuro e non si usa.

use super::Img;
use briscola_core::cards;
use std::sync::OnceLock;

/// Valore di rango k, con lo stesso ordinamento di una lista di byte.
pub(super) fn percentile(histogram: [usize; 256], rank: usize) -> u8 {
    let mut total = 0;
    for (value, count) in histogram.into_iter().enumerate() {
        total += count;
        if total > rank { return value as u8; }
    }
    255
}

/// I simboli separati della carta, se il conteggio è sicuro: da due a sette pezzi di disegno della
/// stessa misura, e nient'altro di grande.
pub fn count(card: &Img) -> Option<usize> {
    symbols(card, false).map(|symbols| symbols.len())
}

#[derive(Debug)]
pub(super) struct Symbol {
    pub(super) area: usize,
    // Bounds in the rectified crop, left/top/right/bottom.
    pub(super) bounds: [usize; 4],
}

pub(super) fn visible_symbols(card: &Img) -> Option<Vec<Symbol>> {
    symbols(card, true)
}

fn symbols(card: &Img, measure_bounds: bool) -> Option<Vec<Symbol>> {
    let (w, h) = (card.w, card.h);
    if w < 40 || h < 60 {
        return None;
    }
    // La carta: il colore della carta è il più chiaro (l'85° percentile del canale più scuro).
    let mut histogram = [0usize; 256];
    for &[r, g, b] in &card.px {
        histogram[r.min(g).min(b) as usize] += 1;
    }
    let paper = percentile(histogram, card.px.len() * 85 / 100) as i32;
    if paper < 120 {
        return None;
    }
    let (mx, my) = (w * 6 / 100, h * 4 / 100);
    let ink = |x: usize, y: usize| {
        let p = card.at(x, y);
        let (lo, hi) = (*p.iter().min().unwrap() as i32, *p.iter().max().unwrap() as i32);
        lo < paper * 78 / 100 || hi - lo > 45
    };
    // Pezzi di disegno collegati (vicini di lato).
    let mut label = vec![0u32; w * h];
    let mut symbols = Vec::new();
    let mut stack = Vec::new();
    for y in my..h - my {
        for x in mx..w - mx {
            if label[y * w + x] != 0 || !ink(x, y) {
                continue;
            }
            let id = symbols.len() as u32 + 1;
            let mut area = 0;
            let (mut l, mut t, mut r, mut b) = (x, y, x, y);
            label[y * w + x] = id;
            stack.push((x, y));
            while let Some((cx, cy)) = stack.pop() {
                area += 1;
                if measure_bounds {
                    l = l.min(cx);
                    t = t.min(cy);
                    r = r.max(cx);
                    b = b.max(cy);
                }
                for (nx, ny) in [(cx.wrapping_sub(1), cy), (cx + 1, cy), (cx, cy.wrapping_sub(1)), (cx, cy + 1)] {
                    if nx < mx || ny < my || nx >= w - mx || ny >= h - my {
                        continue;
                    }
                    if label[ny * w + nx] == 0 && ink(nx, ny) {
                        label[ny * w + nx] = id;
                        stack.push((nx, ny));
                    }
                }
            }
            symbols.push(Symbol { area, bounds: [l, t, r, b] });
        }
    }
    let min_area = (w * h) / 600;
    let mut big: Vec<Symbol> = symbols.into_iter().filter(|symbol| symbol.area >= min_area).collect();
    if big.len() < 2 || big.len() > 7 {
        return None;
    }
    big.sort_unstable_by_key(|symbol| symbol.area);
    let med = big[big.len() / 2].area as f32;
    // Tutti della stessa misura (un pezzo molto più grande è una figura o simboli che si toccano).
    if big.iter().any(|s| (s.area as f32) < 0.35 * med || (s.area as f32) > 2.8 * med) {
        return None;
    }
    // Abbastanza disegno: pochi puntini sparsi non sono simboli.
    let total: usize = big.iter().map(|s| s.area).sum();
    if total * 25 < w * h {
        return None;
    }
    Some(big)
}

/// Per ogni carta, i conteggi che dà nei mazzi di riferimento (quelli sicuri): un conteggio letto
/// sullo schermo che non è fra questi esclude la carta.
pub fn expected() -> &'static [Vec<usize>; 40] {
    static EXP: OnceLock<[Vec<usize>; 40]> = OnceLock::new();
    EXP.get_or_init(|| {
        let mut out: [Vec<usize>; 40] = std::array::from_fn(|_| Vec::new());
        for card in super::reference::reference_cards() {
            if let Some(n) = count(&card.1) {
                if !out[card.0 as usize].contains(&n) {
                    out[card.0 as usize].push(n);
                }
            }
        }
        out
    })
}

/// Le carte escluse dal conteggio letto: quelle per cui almeno un mazzo dà un conteggio sicuro e
/// nessuno dà questo. (Le carte che in nessun mazzo si contano non si escludono.)
pub fn excluded(n: usize) -> [bool; 40] {
    let exp = expected();
    std::array::from_fn(|c| !exp[c].is_empty() && !exp[c].contains(&n) && cards::rank(c as u8) <= 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Il conteggio sui fogli: nel mazzo dell'utente e per denari e coppe di tutti i mazzi torna
    /// il numero della carta.
    #[test]
    fn counts_the_symbols() {
        let exp = expected();
        for c in 0..40u8 {
            let r = cards::rank(c) as usize;
            if (1..=6).contains(&r) && cards::suit(c) <= 1 {
                assert!(exp[c as usize].contains(&(r + 1)), "{} {:?}", cards::name(c), exp[c as usize]);
            }
        }
    }
}
