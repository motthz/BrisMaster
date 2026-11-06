//! Inseguimento veloce delle carte fra un'analisi e l'altra: l'analisi completa di una schermata
//! è lenta, ma se la finestra del browser si sposta o la pagina scorre le carte devono restare
//! segnate al posto giusto. Si ritaglia la zona delle carte dalla schermata analizzata e la si
//! ritrova in ogni nuova cattura con un confronto a bassa risoluzione: lo spostamento trovato si
//! aggiunge alle posizioni lette.

use super::Img;

/// Immagine in toni di grigio.
#[derive(Clone, Debug)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Gray {
    /// Il ritaglio [x0, x1) x [y0, y1) di `img` rimpicciolito di `k` volte (media dei blocchi).
    pub fn of(img: &Img, k: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> Gray {
        let (x1, y1) = (x1.min(img.w), y1.min(img.h));
        let (w, h) = ((x1.saturating_sub(x0)) / k, (y1.saturating_sub(y0)) / k);
        let mut px = Vec::with_capacity(w * h);
        for j in 0..h {
            for i in 0..w {
                let mut acc = 0u32;
                for dy in 0..k {
                    let row = (y0 + j * k + dy) * img.w;
                    for dx in 0..k {
                        let p = img.px[row + x0 + i * k + dx];
                        acc += (p[0] as u32 * 3 + p[1] as u32 * 6 + p[2] as u32) / 10;
                    }
                }
                px.push((acc / (k * k) as u32) as u8);
            }
        }
        Gray { w, h, px }
    }

    /// Differenza media (0-255) fra `tpl` e la zona di `self` con l'angolo in (x, y); si ferma
    /// appena supera `limit`.
    fn diff(&self, tpl: &Gray, x: usize, y: usize, limit: f32) -> f32 {
        let stop = (limit * (tpl.w * tpl.h) as f32) as u32;
        let mut sum = 0u32;
        for j in 0..tpl.h {
            let a = &self.px[(y + j) * self.w + x..(y + j) * self.w + x + tpl.w];
            let b = &tpl.px[j * tpl.w..(j + 1) * tpl.w];
            sum += a.iter().zip(b).map(|(&p, &q)| (p as i32 - q as i32).unsigned_abs()).sum::<u32>();
            if sum > stop {
                return f32::INFINITY;
            }
        }
        sum as f32 / (tpl.w * tpl.h).max(1) as f32
    }

    fn contrast(&self) -> f32 {
        let n = self.px.len().max(1) as f32;
        let mean = self.px.iter().map(|&p| p as f32).sum::<f32>() / n;
        (self.px.iter().map(|&p| (p as f32 - mean).powi(2)).sum::<f32>() / n).sqrt()
    }
}

/// Risoluzione della ricerca grossolana (su tutta la schermata) e di quella fine.
const COARSE: usize = 8;
const FINE: usize = 4;
/// Differenza media oltre la quale la zona delle carte non è stata ritrovata.
const MATCH: f32 = 16.0;

/// La zona delle carte ritagliata da una schermata analizzata.
#[derive(Clone, Debug)]
pub struct Template {
    coarse: Gray,
    fine: Gray,
    /// Angolo in alto a sinistra della zona, in pixel della schermata.
    x0: usize,
    y0: usize,
    /// Misure della schermata: lo spostamento si cerca solo in schermate grandi uguali.
    frame: (usize, usize),
}

impl Template {
    /// La zona `at` (frazioni della schermata: sinistra, alto, destra, basso) di `img`, un po'
    /// allargata. None se è troppo piccola o tutta di un colore (non si ritroverebbe).
    pub fn new(img: &Img, at: [f32; 4]) -> Option<Template> {
        let (w, h) = (img.w as f32, img.h as f32);
        let pad = 0.01 * w;
        let x0 = ((at[0] * w - pad).max(0.0) as usize) / COARSE * COARSE;
        let y0 = ((at[1] * h - pad).max(0.0) as usize) / COARSE * COARSE;
        let x1 = ((at[2] * w + pad).min(w) as usize).min(img.w);
        let y1 = ((at[3] * h + pad).min(h) as usize).min(img.h);
        if x1 <= x0 + 4 * COARSE || y1 <= y0 + 4 * COARSE {
            return None;
        }
        let coarse = Gray::of(img, COARSE, x0, y0, x1, y1);
        let fine = Gray::of(img, FINE, x0, y0, x0 + coarse.w * COARSE, y0 + coarse.h * COARSE);
        (coarse.contrast() > 12.0).then_some(Template { coarse, fine, x0, y0, frame: (img.w, img.h) })
    }

    /// Vero se è lo stesso ritaglio (della stessa scena).
    pub fn same(&self, o: &Template) -> bool {
        (self.x0, self.y0, self.frame) == (o.x0, o.y0, o.frame) && self.fine.px == o.fine.px
    }

    /// Di quanto si è spostata la zona in `img` (frazioni della schermata), partendo dallo
    /// spostamento di prima `hint`. None se `img` ha altre misure o la zona non c'è più.
    pub fn find(&self, img: &Img, hint: [f32; 2]) -> Option<[f32; 2]> {
        if (img.w, img.h) != self.frame {
            return None;
        }
        let fine_img = Gray::of(img, FINE, 0, 0, img.w, img.h);
        let max_x = fine_img.w.checked_sub(self.fine.w)?;
        let max_y = fine_img.h.checked_sub(self.fine.h)?;
        // Prima vicino a dove era l'ultima volta (quasi sempre lì: si fa in un attimo).
        let around = |cx: i64, cy: i64, r: i64| {
            let mut best = (f32::INFINITY, 0usize, 0usize);
            for y in (cy - r).max(0)..=(cy + r).min(max_y as i64) {
                for x in (cx - r).max(0)..=(cx + r).min(max_x as i64) {
                    let d = fine_img.diff(&self.fine, x as usize, y as usize, best.0.min(MATCH * 2.0));
                    if d < best.0 {
                        best = (d, x as usize, y as usize);
                    }
                }
            }
            best
        };
        let fx = ((self.x0 as f32 + hint[0] * img.w as f32) / FINE as f32).round() as i64;
        let fy = ((self.y0 as f32 + hint[1] * img.h as f32) / FINE as f32).round() as i64;
        let mut best = around(fx, fy, 3);
        if best.0 > MATCH * 0.6 {
            // Si è spostata parecchio: ricerca grossolana su tutta la schermata, poi fine.
            let coarse_img = Gray::of(img, COARSE, 0, 0, img.w, img.h);
            let mut c = (f32::INFINITY, 0usize, 0usize);
            for y in 0..=coarse_img.h.saturating_sub(self.coarse.h) {
                for x in 0..=coarse_img.w.saturating_sub(self.coarse.w) {
                    let d = coarse_img.diff(&self.coarse, x, y, c.0.min(MATCH * 2.0));
                    if d < c.0 {
                        c = (d, x, y);
                    }
                }
            }
            if c.0.is_finite() {
                let k = (COARSE / FINE) as i64;
                let refined = around(c.1 as i64 * k, c.2 as i64 * k, k + 1);
                if refined.0 < best.0 {
                    best = refined;
                }
            }
        }
        (best.0 <= MATCH).then(|| {
            [
                (best.1 as f32 * FINE as f32 - self.x0 as f32) / img.w as f32,
                (best.2 as f32 * FINE as f32 - self.y0 as f32) / img.h as f32,
            ]
        })
    }
}

/// Riquadro che contiene tutti quelli dati (frazioni), None se non ce n'è nessuno.
pub fn union(spots: &[[f32; 4]]) -> Option<[f32; 4]> {
    spots.iter().copied().reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un tavolo verde con tre "carte" disegnate, spostate di (dx, dy) pixel.
    fn table(dx: usize, dy: usize) -> Img {
        let mut img = Img::new(1600, 900);
        for p in img.px.iter_mut() {
            *p = [20, 110, 50];
        }
        for (k, x) in [500usize, 640, 780].iter().enumerate() {
            for y in 600..800 {
                for xx in *x..x + 120 {
                    let stripe = ((xx + y * (k + 1)) / 9) % 3 == 0;
                    img.px[(y + dy) * img.w + xx + dx] = if stripe { [200, 30, 30] } else { [250, 250, 245] };
                }
            }
        }
        img
    }

    /// La zona delle carte si ritrova sia vicina sia lontana, e non si inventa se sparisce.
    #[test]
    fn follows_the_cards() {
        let start = table(0, 0);
        let at = [500.0 / 1600.0, 600.0 / 900.0, 900.0 / 1600.0, 800.0 / 900.0];
        let tpl = Template::new(&start, at).expect("zona con contrasto");
        let close = tpl.find(&table(8, 4), [0.0, 0.0]).expect("ritrovata vicina");
        assert!((close[0] * 1600.0 - 8.0).abs() <= 4.0 && (close[1] * 900.0 - 4.0).abs() <= 4.0, "{close:?}");
        let far = tpl.find(&table(400, 0), [0.0, 0.0]).expect("ritrovata lontana");
        assert!((far[0] * 1600.0 - 400.0).abs() <= 4.0 && (far[1] * 900.0).abs() <= 4.0, "{far:?}");
        let mut empty = Img::new(1600, 900);
        for p in empty.px.iter_mut() {
            *p = [20, 110, 50];
        }
        assert!(tpl.find(&empty, [0.0, 0.0]).is_none());
        assert!(tpl.find(&Img::new(800, 600), [0.0, 0.0]).is_none());
    }
}
