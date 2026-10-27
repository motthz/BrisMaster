//! Trova le carte scoperte sul tavolo verde e le divide fra briscola, carte in tavola e mano.
//!
//! Le carte sono rettangoli quasi bianchi circondati dal panno verde: si cercano le zone chiare,
//! si separano quelle che si toccano (le carte della mano a ventaglio) con un'erosione, e di
//! ciascuna si prende il rettangolo ruotato più piccolo che la contiene. La posizione decide il
//! ruolo: la riga più in basso è la mano, la carta di traverso accanto al mazzo è la briscola,
//! le altre sono in tavola (in alto quella dell'avversario, in basso la tua).

use super::{hsv, CardView, Img, Scene, CARD_H, CARD_W};

/// Rettangolo ruotato: centro, asse "larghezza" `u` e asse "altezza" `v` (versori), semilati.
#[derive(Clone, Copy, Debug)]
pub struct RotRect {
    pub cx: f32,
    pub cy: f32,
    pub u: (f32, f32),
    pub v: (f32, f32),
    pub hw: f32,
    pub hh: f32,
}

impl RotRect {
    pub fn long(&self) -> f32 {
        2.0 * self.hw.max(self.hh)
    }
    pub fn short(&self) -> f32 {
        2.0 * self.hw.min(self.hh)
    }
    pub fn area(&self) -> f32 {
        4.0 * self.hw * self.hh
    }
    /// Vero se il punto è dentro il rettangolo.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let (dx, dy) = (x - self.cx, y - self.cy);
        (dx * self.u.0 + dy * self.u.1).abs() <= self.hw && (dx * self.v.0 + dy * self.v.1).abs() <= self.hh
    }

    /// Vero se il lato lungo è più vicino all'orizzontale che alla verticale.
    pub fn landscape(&self) -> bool {
        let long_axis = if self.hh >= self.hw { self.v } else { self.u };
        long_axis.0.abs() > long_axis.1.abs()
    }
}

/// Una zona chiara trovata sullo schermo.
#[derive(Clone, Copy, Debug)]
pub struct Blob {
    pub rect: RotRect,
    /// Frazione del rettangolo coperta da pixel chiari (bassa per i dorsi, che hanno solo il bordo bianco).
    pub white: f32,
    /// Frazione del contorno appena fuori dal rettangolo che è panno verde.
    pub felt: f32,
}

#[inline]
fn is_white(p: [u8; 3]) -> bool {
    let (_, s, v) = hsv(p);
    s < 50 && v > 190
}

#[inline]
pub fn is_felt(p: [u8; 3]) -> bool {
    let (h, s, v) = hsv(p);
    (100.0..=200.0).contains(&h) && s > 70 && v > 50
}

/// Inviluppo convesso (catena monotona di Andrew).
fn hull(mut pts: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: (f32, f32), a: (f32, f32), b: (f32, f32)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut lower: Vec<(f32, f32)> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<(f32, f32)> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Rettangolo di area minima che contiene i punti (calibri rotanti sui lati dell'inviluppo).
pub fn min_area_rect(pts: Vec<(f32, f32)>) -> Option<RotRect> {
    let h = hull(pts);
    if h.len() < 3 {
        return None;
    }
    let mut best: Option<(f32, RotRect)> = None;
    for i in 0..h.len() {
        let (a, b) = (h[i], h[(i + 1) % h.len()]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if len < 1e-3 {
            continue;
        }
        let e = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let n = (-e.1, e.0);
        let (mut e0, mut e1, mut n0, mut n1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in &h {
            let pe = p.0 * e.0 + p.1 * e.1;
            let pn = p.0 * n.0 + p.1 * n.1;
            e0 = e0.min(pe);
            e1 = e1.max(pe);
            n0 = n0.min(pn);
            n1 = n1.max(pn);
        }
        let area = (e1 - e0) * (n1 - n0);
        if best.as_ref().is_none_or(|(ba, _)| area < *ba) {
            let (me, mn) = ((e0 + e1) / 2.0, (n0 + n1) / 2.0);
            let rect = RotRect {
                cx: e.0 * me + n.0 * mn,
                cy: e.1 * me + n.1 * mn,
                u: e,
                v: n,
                hw: (e1 - e0) / 2.0,
                hh: (n1 - n0) / 2.0,
            };
            best = Some((area, rect));
        }
    }
    best.map(|(_, r)| canonical(r))
}

/// Orienta gli assi: `v` lungo il lato lungo; per le carte in piedi `v` punta in basso, per quelle
/// di traverso (la briscola) la testa della carta è a destra, quindi `v` punta a sinistra.
fn canonical(mut r: RotRect) -> RotRect {
    if r.hw > r.hh {
        std::mem::swap(&mut r.hw, &mut r.hh);
        std::mem::swap(&mut r.u, &mut r.v);
    }
    let down = if r.landscape() { r.v.0 < 0.0 } else { r.v.1 > 0.0 };
    if !down {
        r.v = (-r.v.0, -r.v.1);
    }
    // u a destra di v (sistema destrorso come quello dello schermo).
    r.u = (r.v.1, -r.v.0);
    r
}

/// Zone chiare abbastanza grandi da essere carte (o dorsi col bordo bianco). Le carte della mano
/// a ventaglio che si toccano diventano una zona sola: in quel caso si erode di più, finché la
/// zona si divide in pezzi della misura di una carta (serve a qualunque zoom del sito).
pub fn blobs(img: &Img) -> Vec<Blob> {
    let base = blobs_r(img, 1);
    let card = |b: &Blob| is_face(b) && (0.52..=0.74).contains(&(b.rect.short() / b.rect.long()));
    let mut longs: Vec<f32> = base.iter().filter(|b| card(b)).map(|b| b.rect.long()).collect();
    if longs.is_empty() {
        return base;
    }
    longs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let card_l = longs[longs.len() / 2];
    let card_area = card_l * card_l * RATIO;
    let mut deeper: Vec<(usize, Vec<Blob>)> = Vec::new();
    let mut out = Vec::new();
    for b in base {
        let merged = b.white > 0.3 && b.rect.area() > 1.35 * card_area && b.rect.long() < 4.0 * card_l;
        if !merged {
            out.push(b);
            continue;
        }
        let mut split = None;
        for r in [3, 5, 8, 12] {
            if !deeper.iter().any(|(k, _)| *k == r) {
                deeper.push((r, blobs_r(img, r)));
            }
            let level = &deeper.iter().find(|(k, _)| *k == r).unwrap().1;
            let pieces: Vec<Blob> = level
                .iter()
                .filter(|p| b.rect.contains(p.rect.cx, p.rect.cy))
                .filter(|p| (0.8..=1.2).contains(&(p.rect.long() / card_l)))
                .filter(|p| (0.52..=0.74).contains(&(p.rect.short() / p.rect.long())))
                .map(|p| Blob { felt: b.felt, white: b.white.max(p.white), ..*p })
                .collect();
            if pieces.len() >= 2 {
                split = Some(pieces);
                break;
            }
        }
        match split {
            Some(p) => out.extend(p),
            None => out.push(b),
        }
    }
    out
}

/// Zone chiare dopo un'erosione di raggio `r` (i rettangoli vengono poi riallargati di `r`).
fn blobs_r(img: &Img, r: usize) -> Vec<Blob> {
    let (w, h) = (img.w, img.h);
    let white: Vec<bool> = img.px.iter().map(|&p| is_white(p)).collect();
    let er = erode(&white, w, h, r);
    let min_area = ((w * h) as f32 * 0.0006) as usize;
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if !er[start] || seen[start] {
            continue;
        }
        // Riempimento a 4 vicini; dei pixel servono solo quelli di bordo per l'inviluppo.
        let mut count = 0usize;
        let mut edge: Vec<(f32, f32)> = Vec::new();
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            count += 1;
            let (x, y) = (i % w, i / w);
            let mut border = false;
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx >= w || ny >= h {
                    border = true;
                    continue;
                }
                let j = ny * w + nx;
                if !er[j] {
                    border = true;
                } else if !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
            if border {
                edge.push((x as f32, y as f32));
            }
        }
        if count < min_area {
            continue;
        }
        let Some(mut rect) = min_area_rect(edge) else {
            continue;
        };
        // Restituisce i pixel tolti dall'erosione (e mezzo pixel di bordo).
        rect.hw += r as f32 + 0.5;
        rect.hh += r as f32 + 0.5;
        let white_frac = (count as f32 / rect.area()).min(1.0);
        out.push(Blob { rect, white: white_frac, felt: felt_ring(img, &rect) });
    }
    out
}

/// Erosione quadrata di raggio `r`, separabile: prima le righe, poi le colonne.
fn erode(mask: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    let run = |get: &dyn Fn(usize) -> bool, n: usize, out: &mut dyn FnMut(usize, bool)| {
        // Pixel bianchi consecutivi a sinistra e a destra di ognuno.
        let mut left = vec![0usize; n];
        let mut c = 0;
        for i in 0..n {
            c = if get(i) { c + 1 } else { 0 };
            left[i] = c;
        }
        let mut right = 0;
        for i in (0..n).rev() {
            right = if get(i) { right + 1 } else { 0 };
            out(i, left[i] > r && right > r);
        }
    };
    let mut rows = vec![false; w * h];
    for y in 0..h {
        run(&|x| mask[y * w + x], w, &mut |x, v| rows[y * w + x] = v);
    }
    let mut out = vec![false; w * h];
    for x in 0..w {
        run(&|y| rows[y * w + x], h, &mut |y, v| out[y * w + x] = v);
    }
    out
}

/// Frazione di panno verde su un anello appena fuori dal rettangolo.
fn felt_ring(img: &Img, r: &RotRect) -> f32 {
    let (mut n, mut ok) = (0, 0);
    let (hw, hh) = (r.hw * 1.12 + 3.0, r.hh * 1.12 + 3.0);
    for k in 0..64 {
        let t = k as f32 / 64.0 * 4.0;
        let (a, b) = match t as u32 {
            0 => (-hw + 2.0 * hw * t.fract(), -hh),
            1 => (hw, -hh + 2.0 * hh * t.fract()),
            2 => (hw - 2.0 * hw * t.fract(), hh),
            _ => (-hw, hh - 2.0 * hh * t.fract()),
        };
        let x = r.cx + r.u.0 * a + r.v.0 * b;
        let y = r.cy + r.u.1 * a + r.v.1 * b;
        if x < 0.0 || y < 0.0 || x >= img.w as f32 || y >= img.h as f32 {
            continue;
        }
        n += 1;
        if is_felt(img.at(x as usize, y as usize)) {
            ok += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        ok as f32 / n as f32
    }
}

/// Raddrizza la carta: il rettangolo diventa un'immagine CARD_W x CARD_H con la testa in alto.
pub fn warp(img: &Img, r: &RotRect) -> Img {
    let mut out = Img::new(CARD_W, CARD_H);
    for j in 0..CARD_H {
        for i in 0..CARD_W {
            let a = ((i as f32 + 0.5) / CARD_W as f32 * 2.0 - 1.0) * r.hw;
            let b = ((j as f32 + 0.5) / CARD_H as f32 * 2.0 - 1.0) * r.hh;
            let x = r.cx + r.u.0 * a + r.v.0 * b;
            let y = r.cy + r.u.1 * a + r.v.1 * b;
            out.px[j * CARD_W + i] = img.sample(x, y);
        }
    }
    out
}

/// Proporzione larghezza/altezza delle carte napoletane.
const RATIO: f32 = 0.62;

/// Faccia di una carta scoperta: chiara, appoggiata sul panno e con dei disegni dentro. I riquadri
/// tutti bianchi dei pannelli del sito accanto al tavolo hanno poco panno attorno e niente disegni.
pub fn is_face(b: &Blob) -> bool {
    b.white > 0.3 && b.felt > 0.3 && (b.felt > 0.75 || b.white < 0.8)
}

/// Divide le carte scoperte trovate sullo schermo fra briscola, tavolo e mano (senza riconoscerle).
pub fn scene(img: &Img) -> Scene {
    let all = blobs(img);
    // Facce scoperte: chiare e appoggiate sul panno.
    let faces: Vec<&Blob> = all.iter().filter(|b| is_face(b)).collect();
    let full = |b: &Blob| (0.52..=0.74).contains(&(b.rect.short() / b.rect.long()));
    let mut longs: Vec<f32> = faces.iter().filter(|b| full(b)).map(|b| b.rect.long()).collect();
    if longs.is_empty() {
        return Scene::default();
    }
    longs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let card_l = longs[longs.len() / 2];
    let sized = |b: &Blob| (0.8..=1.2).contains(&(b.rect.long() / card_l));
    let upright: Vec<&Blob> = faces.iter().filter(|b| full(b) && sized(b) && !b.rect.landscape()).copied().collect();
    let mut scene = Scene::default();
    let bottom = upright.iter().map(|b| b.rect.cy).fold(f32::MIN, f32::max);
    if bottom == f32::MIN {
        return scene;
    }
    // La mano: la riga più in basso.
    let mut hand: Vec<&Blob> = upright.iter().filter(|b| b.rect.cy >= bottom - 0.35 * card_l).copied().collect();
    hand.sort_by(|a, b| a.rect.cx.partial_cmp(&b.rect.cx).unwrap());
    let hand_x = hand.first().map_or(f32::MAX, |b| b.rect.cx - 0.5 * card_l);
    // Le carte dell'avversario coperte, in alto: servono a dividere il tavolo a metà.
    let backs_y = all
        .iter()
        .filter(|b| b.white <= 0.3 && b.rect.long() > 0.8 * card_l && b.rect.cy < bottom - 2.5 * card_l)
        .map(|b| b.rect.cy)
        .fold(f32::MAX, f32::min);
    let mid = if backs_y < f32::MAX { (backs_y + bottom) / 2.0 } else { bottom - 2.1 * card_l };
    let mut table: Vec<&Blob> = upright.iter().filter(|b| b.rect.cy < bottom - 0.6 * card_l).copied().collect();
    table.sort_by(|a, b| a.rect.cy.partial_cmp(&b.rect.cy).unwrap());
    match table.as_slice() {
        [] => {}
        [one] => {
            let view = Some(view_of(img, &one.rect, false));
            if one.rect.cy < mid {
                scene.opp_slot = view;
            } else {
                scene.my_slot = view;
            }
        }
        [top, .., low] => {
            scene.opp_slot = Some(view_of(img, &top.rect, false));
            scene.my_slot = Some(view_of(img, &low.rect, false));
        }
    }
    // La briscola: in basso a sinistra della mano, di traverso o coperta in parte dal mazzo.
    let trump = faces
        .iter()
        .filter(|b| b.rect.cy > bottom - 0.9 * card_l && b.rect.cx < hand_x)
        .filter(|b| b.rect.landscape() || (b.rect.long() < 0.85 * card_l && b.rect.long() > 0.4 * card_l))
        .max_by(|a, b| a.rect.area().partial_cmp(&b.rect.area()).unwrap());
    if let Some(t) = trump {
        let (rect, partial) = full_trump(&t.rect, card_l);
        scene.trump = Some(view_of(img, &rect, partial));
    }
    scene.hand = hand.iter().map(|b| view_of(img, &b.rect, false)).collect();
    scene
}

/// Rettangolo della carta intera per la briscola: se se ne vede solo un pezzo, la parte coperta
/// è verso il mazzo, cioè verso il piede della carta.
fn full_trump(r: &RotRect, card_l: f32) -> (RotRect, bool) {
    let card_w = card_l * RATIO;
    if r.long() > 0.9 * card_l && r.landscape() {
        return (*r, false);
    }
    // Il lato più vicino alla larghezza della carta è la larghezza; l'altro è il pezzo visibile.
    let (width, seen_axis, seen) = if (2.0 * r.hw - card_w).abs() <= (2.0 * r.hh - card_w).abs() {
        (2.0 * r.hw, r.v, 2.0 * r.hh)
    } else {
        (2.0 * r.hh, r.u, 2.0 * r.hw)
    };
    // La testa è dalla parte opposta al mazzo, che sta a sinistra: il piede punta a sinistra.
    let foot = if seen_axis.0 < 0.0 { seen_axis } else { (-seen_axis.0, -seen_axis.1) };
    let missing = (card_l - seen).max(0.0);
    let rect = RotRect {
        cx: r.cx + foot.0 * missing / 2.0,
        cy: r.cy + foot.1 * missing / 2.0,
        u: (foot.1, -foot.0),
        v: foot,
        hw: width.max(card_w) / 2.0,
        hh: card_l / 2.0,
    };
    (rect, missing > 0.1 * card_l)
}

fn view_of(img: &Img, r: &RotRect, partial: bool) -> CardView {
    CardView { img: warp(img, r), partial, scores: [0.0; 40] }
}

/// Quello che si vede sul tavolo della scopa in un istante.
#[derive(Clone, Debug, Default)]
pub struct ScopaScene {
    /// Le tue carte, da sinistra a destra.
    pub hand: Vec<CardView>,
    /// Le carte scoperte sul panno (in tavola o appena giocate), per righe dall'alto.
    pub table: Vec<CardView>,
}

/// Vero se buona parte dello schermo è panno verde (il tavolo è aperto anche se non ci sono carte).
fn felt_visible(img: &Img) -> bool {
    let (mut n, mut ok) = (0, 0);
    for y in (0..img.h).step_by(8) {
        for x in (0..img.w).step_by(8) {
            n += 1;
            ok += is_felt(img.at(x, y)) as usize;
        }
    }
    n > 0 && ok * 5 > n
}

/// Riga più in basso dello schermo in cui il panno verde occupa almeno un quarto della larghezza.
fn felt_bottom(img: &Img) -> f32 {
    for y in (0..img.h).rev().step_by(4) {
        let felt = (0..img.w).step_by(6).filter(|&x| is_felt(img.at(x, y))).count();
        if felt * 4 >= img.w / 6 {
            return y as f32;
        }
    }
    img.h as f32
}

/// Divide le carte scoperte della scopa fra la tua mano (in basso) e il tavolo (tutte le altre in
/// piedi). Le carte di traverso (le scope segnate sul mucchio delle prese) e i dorsi si ignorano.
/// None se sullo schermo non c'è il tavolo; una scena vuota se c'è il tavolo ma nessuna carta.
pub fn scopa(img: &Img) -> Option<ScopaScene> {
    let all = blobs(img);
    let faces: Vec<&Blob> = all.iter().filter(|b| is_face(b)).collect();
    let full = |b: &Blob| (0.52..=0.74).contains(&(b.rect.short() / b.rect.long()));
    let mut longs: Vec<f32> = faces.iter().filter(|b| full(b)).map(|b| b.rect.long()).collect();
    if longs.is_empty() {
        return felt_visible(img).then(ScopaScene::default);
    }
    longs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let card_l = longs[longs.len() / 2];
    let sized = |b: &Blob| (0.8..=1.2).contains(&(b.rect.long() / card_l));
    let upright: Vec<&Blob> = faces.iter().filter(|b| full(b) && sized(b) && !b.rect.landscape()).copied().collect();
    // La mano sta sul bordo basso del panno: entro una carta e un quarto dal fondo.
    let hand_from = felt_bottom(img) - 1.25 * card_l;
    let (mut hand, mut table): (Vec<&Blob>, Vec<&Blob>) = upright.iter().partition(|b| b.rect.cy >= hand_from);
    hand.sort_by(|a, b| a.rect.cx.partial_cmp(&b.rect.cx).unwrap());
    // Per righe: due carte sono nella stessa riga se i centri distano meno di mezza carta.
    table.sort_by(|a, b| a.rect.cy.partial_cmp(&b.rect.cy).unwrap());
    let mut rows: Vec<Vec<&Blob>> = Vec::new();
    for b in table {
        match rows.last_mut() {
            Some(r) if (b.rect.cy - r[0].rect.cy).abs() < 0.5 * card_l => r.push(b),
            _ => rows.push(vec![b]),
        }
    }
    let mut table = Vec::new();
    for mut r in rows {
        r.sort_by(|a, b| a.rect.cx.partial_cmp(&b.rect.cx).unwrap());
        table.extend(r.iter().map(|b| view_of(img, &b.rect, false)));
    }
    Some(ScopaScene { hand: hand.iter().map(|b| view_of(img, &b.rect, false)).collect(), table })
}
