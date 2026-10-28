//! Trova le carte scoperte sullo schermo, su qualunque tavolo, e le divide fra briscola, carte in
//! tavola e mano.
//!
//! Le carte sono rettangoli chiari su un fondo più scuro o colorato: si cercano le zone chiare (con
//! una soglia che segue la luminosità dello schermo), si separano quelle che si toccano (la mano a
//! ventaglio) con un'erosione e di ciascuna si prende il rettangolo ruotato più piccolo che la
//! contiene. Una zona diventa una carta solo se somiglia a una delle 40 carte napoletane di
//! riferimento: dorsi, riquadri, scritte e pannelli del sito si scartano da soli. Una zona più corta
//! di una carta è una carta coperta in parte (la briscola sotto il mazzo, una carta tagliata dal
//! bordo o da una scritta): si provano le posizioni possibili della carta intera e si confronta solo
//! la parte che si vede. Restano le carte appoggiate sullo stesso tavolo; la posizione decide il
//! ruolo: la riga più in basso è la mano, la carta di traverso è la briscola, le altre sono in
//! tavola (in alto quella dell'avversario, in basso la tua).

use super::{hsv, reference, CardView, Img, Scene, CARD_H, CARD_W, WHOLE};

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

    /// Distanza fra i centri.
    pub fn dist(&self, o: &RotRect) -> f32 {
        ((self.cx - o.cx).powi(2) + (self.cy - o.cy).powi(2)).sqrt()
    }

    /// Lo stesso rettangolo girato di mezzo giro (la testa della carta dall'altra parte).
    fn flipped(&self) -> RotRect {
        RotRect { u: (-self.u.0, -self.u.1), v: (-self.v.0, -self.v.1), ..*self }
    }

    /// Punto del contorno ingrandito di `k` (più `pad` pixel), per `t` da 0 a 4 (un lato per unità).
    fn outline(&self, t: f32, k: f32, pad: f32) -> (f32, f32) {
        let (hw, hh) = (self.hw * k + pad, self.hh * k + pad);
        let (a, b) = match t as u32 {
            0 => (-hw + 2.0 * hw * t.fract(), -hh),
            1 => (hw, -hh + 2.0 * hh * t.fract()),
            2 => (hw - 2.0 * hw * t.fract(), hh),
            _ => (-hw, hh - 2.0 * hh * t.fract()),
        };
        (self.cx + self.u.0 * a + self.v.0 * b, self.cy + self.u.1 * a + self.v.1 * b)
    }
}

/// Una zona chiara trovata sullo schermo.
#[derive(Clone, Copy, Debug)]
pub struct Blob {
    pub rect: RotRect,
    /// Frazione del rettangolo coperta da pixel chiari (bassa per i dorsi, che hanno solo il bordo bianco).
    pub white: f32,
    /// Frazione del contorno appena fuori dal rettangolo che non è chiara (il tavolo attorno).
    pub bg: f32,
}

impl Blob {
    /// Chiara e staccata dal fondo: può essere una carta scoperta (o un pezzo di carta).
    pub fn light(&self) -> bool {
        self.white > 0.3 && self.bg > 0.3
    }
}

/// Il bianco delle carte. Si prende dai punti più chiari e poco colorati dello schermo e si usa per
/// bilanciare i colori: così le carte si trovano anche con lo schermo scurito o la luce notturna.
#[derive(Clone, Debug)]
pub struct Paper {
    /// Ogni canale bilanciato: valore corretto per ciascuno dei 256 valori.
    lut: [[u8; 256]; 3],
}

impl Paper {
    pub fn of(img: &Img) -> Paper {
        // Luminosità dei punti poco colorati, e per ogni luminosità la somma dei loro colori.
        let mut hist = [0usize; 256];
        let mut colors = vec![Vec::new(); 256];
        for &p in img.px.iter().step_by(3) {
            let (_, s, v) = hsv(p);
            if s < 110 {
                hist[v as usize] += 1;
                if v >= 100 {
                    colors[v as usize].push(p);
                }
            }
        }
        let n: usize = hist.iter().sum();
        // Il 99° percentile della luminosità; il colore mediano dei punti chiari quasi come quello
        // è il bianco.
        let (mut acc, mut top) = (0, 255);
        for v in (0..256).rev() {
            acc += hist[v];
            if acc * 100 > n {
                top = v;
                break;
            }
        }
        let bright: Vec<[u8; 3]> = colors[(top * 92 / 100).max(100)..].iter().flatten().copied().collect();
        let mut lut = [[0u8; 256]; 3];
        for (c, l) in lut.iter_mut().enumerate() {
            let mut ch: Vec<u8> = bright.iter().map(|p| p[c]).collect();
            ch.sort_unstable();
            let white = ch.get(ch.len() / 2).copied().unwrap_or(255).max(1) as f32;
            let gain = (255.0 / white).clamp(1.0, 2.5);
            for (v, o) in l.iter_mut().enumerate() {
                *o = (v as f32 * gain).min(255.0) as u8;
            }
        }
        Paper { lut }
    }

    /// Vero se il colore, bilanciato, è bianco: poco saturo e chiaro.
    #[inline]
    pub fn is(&self, p: [u8; 3]) -> bool {
        let q = [self.lut[0][p[0] as usize], self.lut[1][p[1] as usize], self.lut[2][p[2] as usize]];
        let (max, min) = (q[0].max(q[1]).max(q[2]) as u32, q[0].min(q[1]).min(q[2]) as u32);
        max > 190 && (max - min) * 255 < 70 * max
    }
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

/// Orienta gli assi: `v` lungo il lato lungo, verso il basso (verso sinistra per le carte di
/// traverso); `u` a destra di `v`. Quale sia la testa della carta lo decide poi il riconoscimento.
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

/// Zone chiare dopo un'erosione di raggio `r` (i rettangoli vengono poi riallargati di `r`).
fn blobs_r(img: &Img, paper: &[bool], r: usize) -> Vec<Blob> {
    let (w, h) = (img.w, img.h);
    let er = erode(paper, w, h, r);
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
        let white = (count as f32 / rect.area()).min(1.0);
        out.push(Blob { rect, white, bg: background(img, paper, &rect) });
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

/// Punti dell'anello appena fuori dal rettangolo, dentro l'immagine.
fn ring<'a>(img: &'a Img, r: &RotRect) -> impl Iterator<Item = (usize, usize)> + 'a {
    let r = *r;
    (0..64).filter_map(move |k| {
        let (x, y) = r.outline(k as f32 / 16.0, 1.12, 3.0);
        (x >= 0.0 && y >= 0.0 && x < img.w as f32 && y < img.h as f32).then_some((x as usize, y as usize))
    })
}

/// Frazione dell'anello attorno al rettangolo che non è chiara.
fn background(img: &Img, paper: &[bool], r: &RotRect) -> f32 {
    let (mut n, mut ok) = (0, 0);
    for (x, y) in ring(img, r) {
        n += 1;
        ok += !paper[y * img.w + x] as usize;
    }
    if n == 0 {
        0.0
    } else {
        ok as f32 / n as f32
    }
}

/// Colore del tavolo attorno al rettangolo (mediana dei punti non chiari dell'anello).
fn table_color(img: &Img, paper: &[bool], r: &RotRect) -> Option<[u8; 3]> {
    let pts: Vec<[u8; 3]> = ring(img, r).filter(|&(x, y)| !paper[y * img.w + x]).map(|(x, y)| img.at(x, y)).collect();
    if pts.len() < 8 {
        return None;
    }
    let mut out = [0u8; 3];
    for (c, o) in out.iter_mut().enumerate() {
        let mut v: Vec<u8> = pts.iter().map(|p| p[c]).collect();
        v.sort_unstable();
        *o = v[v.len() / 2];
    }
    Some(out)
}

/// Vero se due colori sono dello stesso tavolo: stessa tinta, luminosità non troppo diversa (il
/// panno è spesso più chiaro al centro).
pub fn same_table(a: [u8; 3], b: [u8; 3]) -> bool {
    let sum = |p: [u8; 3]| p.iter().map(|&c| c as f32).sum::<f32>();
    let (sa, sb) = (sum(a), sum(b));
    if sa < 90.0 || sb < 90.0 {
        // Fondi quasi neri: la tinta non si vede.
        return sa < 90.0 && sb < 90.0;
    }
    let chroma = |p: [u8; 3], s: f32| (p[0] as f32 / s, p[1] as f32 / s);
    let ((ra, ga), (rb, gb)) = (chroma(a, sa), chroma(b, sb));
    let ratio = sa / sb;
    ((ra - rb).powi(2) + (ga - gb).powi(2)).sqrt() < 0.08 && (0.5..=2.0).contains(&ratio)
}

/// Raddrizza la carta: il rettangolo diventa un'immagine CARD_W x CARD_H con il lato -v in alto.
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

/// Somiglianza minima con una carta di riferimento per credere che sia una carta.
const ACCEPT: f32 = 0.45;
/// Lo stesso per una carta vista in parte: con meno disegno da confrontare serve più somiglianza.
const ACCEPT_PARTIAL: f32 = 0.50;

/// Fascia (frazione dell'altezza della carta) tolta al confronto dalla parte coperta di una carta
/// vista in parte.
const HIDDEN_EDGE: f32 = 0.07;

/// Proporzione tipica larghezza/altezza di una carta sullo schermo.
const RATIO: f32 = 0.62;

/// Proporzioni (lato corto / lato lungo) accettate per una carta intera.
fn card_shaped(r: &RotRect) -> bool {
    (0.52..=0.76).contains(&(r.short() / r.long()))
}

/// Una carta trovata e riconosciuta.
#[derive(Clone, Debug)]
pub struct Found {
    /// La carta intera, con la testa dalla parte di -v (anche dove è coperta).
    pub rect: RotRect,
    pub view: CardView,
    /// Somiglianza con la carta di riferimento più vicina.
    pub score: f32,
}

impl Found {
    pub fn partial(&self) -> bool {
        self.view.partial
    }
}

/// Una posizione possibile della carta, confrontata al primo giro con le carte di riferimento.
struct Try {
    rect: RotRect,
    vis: [f32; 2],
    card: Img,
    m: reference::Match,
}

impl Try {
    fn new(img: &Img, rect: RotRect, vis: [f32; 2]) -> Try {
        let card = warp(img, &rect);
        let m = reference::Match::new(&card, vis);
        Try { rect, vis, card, m }
    }

    /// Confronto completo.
    fn finish(self) -> Found {
        let sim = self.m.refine();
        let score = sim.iter().copied().fold(f32::MIN, f32::max);
        Found { rect: self.rect, view: CardView::new(self.card, self.vis, sim), score }
    }
}

/// La zona come carta intera, se somiglia abbastanza a una carta. Si prova anche girata di mezzo
/// giro (la carta dell'avversario su alcuni siti), che vince solo se somiglia chiaramente di più:
/// le carte quasi simmetriche restano dritte, sempre nello stesso verso.
fn as_full(img: &Img, r: &RotRect) -> Option<Found> {
    let up = Try::new(img, *r, WHOLE);
    let down = Try::new(img, r.flipped(), WHOLE);
    let t = if down.m.top() > up.m.top() + 0.03 { down } else { up };
    let f = t.finish();
    (f.score >= ACCEPT).then_some(f)
}

/// La zona come pezzo di una carta di misura `card_l` x `card_w`: un lato è largo quanto la carta,
/// l'altro è la parte che si vede. Si prova la carta intera prolungata da una parte o dall'altra,
/// con la testa da una parte o dall'altra, e si confronta solo la fascia visibile.
fn as_partial(img: &Img, r: &RotRect, card_l: f32, card_w: f32) -> Option<Found> {
    let mut tries = Vec::new();
    for (width, axis, seen) in [(2.0 * r.hw, r.v, 2.0 * r.hh), (2.0 * r.hh, r.u, 2.0 * r.hw)] {
        // Un terzo di carta almeno: meno non basta a dire quale sia.
        if (width - card_w).abs() > 0.15 * card_w || seen < 0.35 * card_l || seen > 0.95 * card_l {
            continue;
        }
        let missing = card_l - seen;
        for side in [1.0, -1.0] {
            // La parte nascosta prosegue verso side * axis.
            let (cx, cy) = (r.cx + side * axis.0 * missing / 2.0, r.cy + side * axis.1 * missing / 2.0);
            for head in [1.0, -1.0] {
                let v = (head * axis.0, head * axis.1);
                let rect = RotRect { cx, cy, u: (v.1, -v.0), v, hw: width / 2.0, hh: card_l / 2.0 };
                // Centro del pezzo visibile lungo v, rispetto al centro della carta.
                let off = -side * head * missing / 2.0;
                let mut vis = [
                    ((off - seen / 2.0) / card_l + 0.5).clamp(0.0, 1.0),
                    ((off + seen / 2.0) / card_l + 0.5).clamp(0.0, 1.0),
                ];
                // Dalla parte coperta il pezzo chiaro comprende spesso il bordo bianco di quello che
                // copre (il mazzo): quella fascia non si confronta.
                if side * head > 0.0 {
                    vis[1] -= HIDDEN_EDGE;
                } else {
                    vis[0] += HIDDEN_EDGE;
                }
                tries.push(Try::new(img, rect, vis));
            }
        }
    }
    // Si affinano solo le due posizioni più promettenti.
    tries.sort_by(|a, b| b.m.top().total_cmp(&a.m.top()));
    tries
        .into_iter()
        .take(2)
        .map(Try::finish)
        .filter(|f| f.score >= ACCEPT_PARTIAL)
        .max_by(|a, b| a.score.total_cmp(&b.score))
}

/// Più carte di misura `card_l` x `card_w` affiancate in una zona sola (la mano a ventaglio), quando
/// l'erosione non basta a dividerle: si cerca una carta a ciascuna estremità della fila, provando
/// qualche piccola rotazione (le carte a ventaglio sono un po' girate). Le carte in mezzo si
/// ritrovano dopo, togliendo quelle delle estremità.
fn ends(img: &Img, r: &RotRect, card_l: f32, card_w: f32) -> Vec<Found> {
    let mut out = Vec::new();
    // La fila corre lungo l'asse più lungo di una carta; l'altro lato è alto quanto una carta.
    for (row, across, len, height) in [(r.u, r.v, 2.0 * r.hw, 2.0 * r.hh), (r.v, r.u, 2.0 * r.hh, 2.0 * r.hw)] {
        if len < 1.2 * card_w || !(0.8..=1.25).contains(&(height / card_l)) {
            continue;
        }
        for side in [-1.0f32, 1.0] {
            let off = side * (len - card_w) / 2.0;
            let (cx, cy) = (r.cx + row.0 * off, r.cy + row.1 * off);
            let mut tries = Vec::new();
            for deg in [-8.0f32, -4.0, 0.0, 4.0, 8.0] {
                let (sin, cos) = deg.to_radians().sin_cos();
                // L'asse verticale della carta, girato; la testa dalle due parti.
                let v = (across.0 * cos - across.1 * sin, across.0 * sin + across.1 * cos);
                for v in [v, (-v.0, -v.1)] {
                    let rect = RotRect { cx, cy, u: (v.1, -v.0), v, hw: card_w / 2.0, hh: height.min(card_l) / 2.0 };
                    tries.push(Try::new(img, rect, WHOLE));
                }
            }
            if let Some(t) = tries.into_iter().max_by(|a, b| a.m.top().total_cmp(&b.m.top())) {
                let f = t.finish();
                if f.score >= ACCEPT {
                    out.push(f);
                }
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

/// Toglie dalla maschera il rettangolo (allargato di due pixel).
fn clear(mask: &mut [bool], w: usize, h: usize, r: &RotRect) {
    let big = RotRect { hw: r.hw + 2.0, hh: r.hh + 2.0, ..*r };
    let ext = big.hw.max(big.hh);
    let (x0, x1) = ((r.cx - ext).max(0.0) as usize, ((r.cx + ext) as usize + 1).min(w));
    let (y0, y1) = ((r.cy - ext).max(0.0) as usize, ((r.cy + ext) as usize + 1).min(h));
    for y in y0..y1 {
        for x in x0..x1 {
            if big.contains(x as f32, y as f32) {
                mask[y * w + x] = false;
            }
        }
    }
}

/// Applica `f` a ogni elemento, in parallelo (un thread per elemento: le zone sono poche).
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let f = &f;
    std::thread::scope(|s| {
        let handles: Vec<_> = items.iter().map(|it| s.spawn(move || f(it))).collect();
        handles.into_iter().map(|h| h.join().expect("lettura di una carta")).collect()
    })
}

/// Tutto quello che si è visto sullo schermo.
pub struct Analysis {
    /// Le carte riconosciute, appoggiate sul tavolo.
    pub cards: Vec<Found>,
    /// Misura di una carta intera (lato lungo), se se ne è vista almeno una.
    pub card_l: f32,
    /// Colore del tavolo attorno alle carte.
    pub table: Option<[u8; 3]>,
    /// Tutte le zone chiare (per la diagnosi e per i dorsi).
    pub blobs: Vec<Blob>,
}

/// Trova e riconosce le carte scoperte.
pub fn analyze(img: &Img) -> Analysis {
    let p = Paper::of(img);
    let paper: Vec<bool> = img.px.iter().map(|&px| p.is(px)).collect();
    let base = blobs_r(img, &paper, 1);
    let mut cards: Vec<Found> = Vec::new();
    let mut rest: Vec<Blob> = Vec::new();
    let light: Vec<Blob> = base.iter().copied().filter(|b| b.light()).collect();
    let found = par_map(&light, |b| card_shaped(&b.rect).then(|| as_full(img, &b.rect)).flatten());
    for (b, f) in light.into_iter().zip(found) {
        match f {
            Some(f) => cards.push(f),
            None => rest.push(b),
        }
    }
    let median = |mut v: Vec<f32>| {
        v.sort_by(|a, b| a.total_cmp(b));
        v[v.len() / 2]
    };
    // Zone che non sono una carta intera: forse più carte che si toccano (la mano a ventaglio).
    // Si erode di più, finché la zona si divide in pezzi della misura delle carte già trovate (o,
    // se non ce ne sono ancora, in pezzi lunghi almeno metà della zona) che sono carte.
    let known_l = (!cards.is_empty()).then(|| median(cards.iter().map(|c| c.rect.long()).collect()));
    let known_w = (!cards.is_empty()).then(|| median(cards.iter().map(|c| c.rect.short()).collect()));
    let mut levels: Vec<(usize, Vec<Blob>)> = Vec::new();
    let mut leftover = Vec::new();
    let mut was_split = false;
    for b in rest {
        // Le carte hanno i disegni: le zone quasi tutte bianche sono riquadri del sito.
        let merged = b.white < 0.85 && known_l.is_none_or(|l| (1.35..4.5).contains(&(b.rect.area() / (l * l * RATIO))));
        let piece = |q: &Blob| match known_l {
            Some(l) => (0.8..=1.2).contains(&(q.rect.long() / l)),
            None => q.rect.long() >= 0.5 * b.rect.long() && q.rect.area() < 0.85 * b.rect.area(),
        };
        let mut split = Vec::new();
        for r in [3, 5, 8, 12].into_iter().filter(|_| merged) {
            if !levels.iter().any(|(k, _)| *k == r) {
                levels.push((r, blobs_r(img, &paper, r)));
            }
            let level = &levels.iter().find(|(k, _)| *k == r).unwrap().1;
            split = level
                .iter()
                .filter(|q| b.rect.contains(q.rect.cx, q.rect.cy) && card_shaped(&q.rect) && piece(q))
                .filter_map(|q| as_full(img, &q.rect))
                .collect();
            if !split.is_empty() {
                break;
            }
        }
        if split.is_empty() && merged {
            if let (Some(l), Some(w)) = (known_l, known_w) {
                split = ends(img, &b.rect, l, w);
            }
        }
        if split.is_empty() {
            leftover.push(b);
        } else {
            was_split = true;
            cards.extend(split);
        }
    }
    if cards.is_empty() {
        return Analysis { cards, card_l: 0.0, table: None, blobs: base };
    }
    // Quello che resta togliendo le carte trovate: la carta che ne toccava un'altra (e che
    // l'erosione ha spezzato perché il disegno arriva ai bordi) torna intera. Serve solo se una
    // zona è stata divisa o ne tocca una riconosciuta.
    let touching = |b: &Blob| cards.iter().any(|c| b.rect.dist(&c.rect) < (b.rect.long() + c.rect.long()) / 2.0);
    if was_split || leftover.iter().any(touching) {
        let mut rest_paper = paper.clone();
        for c in &cards {
            clear(&mut rest_paper, img.w, img.h, &c.rect);
        }
        leftover.clear();
        for b in blobs_r(img, &rest_paper, 1).into_iter().filter(|b| b.light()) {
            if cards.iter().any(|c| c.rect.contains(b.rect.cx, b.rect.cy)) {
                continue;
            }
            match card_shaped(&b.rect).then(|| as_full(img, &b.rect)).flatten() {
                Some(f) => cards.push(f),
                None => leftover.push(b),
            }
        }
    }
    // Un pezzo di carta attaccato al bordo bianco di qualcos'altro (la briscola contro il bordo
    // del mazzo, quando lo schermo è un po' sfocato) forma una zona poco bianca che si scarta:
    // con un'erosione leggera i bordi sottili spariscono e il pezzo torna da solo.
    if !levels.iter().any(|(k, _)| *k == 3) {
        levels.push((3, blobs_r(img, &paper, 3)));
    }
    let thin = &levels.iter().find(|(k, _)| *k == 3).unwrap().1;
    for q in thin.iter().filter(|q| q.light()) {
        let known = base.iter().filter(|b| b.light()).any(|b| b.rect.contains(q.rect.cx, q.rect.cy))
            || cards.iter().any(|c| c.rect.contains(q.rect.cx, q.rect.cy));
        if !known {
            leftover.push(*q);
        }
    }
    let card_l = median(cards.iter().map(|c| c.rect.long()).collect());
    let card_w = median(cards.iter().map(|c| c.rect.short()).collect());
    // Una "carta" molto più corta delle altre è il pezzo di una carta coperta in parte (la briscola
    // sotto il mazzo ha quasi le proporzioni di una carta): si tiene la lettura che somiglia di più.
    for c in cards.iter_mut() {
        if c.rect.long() < 0.85 * card_l {
            if let Some(f) = as_partial(img, &c.rect, card_l, card_w) {
                if f.score > c.score {
                    *c = f;
                }
            }
        }
    }
    // Carte coperte in parte.
    leftover.retain(|b| !cards.iter().any(|c| c.rect.contains(b.rect.cx, b.rect.cy)));
    cards.extend(par_map(&leftover, |b| as_partial(img, &b.rect, card_l, card_w)).into_iter().flatten());
    // La stessa carta trovata due volte (per esempio divisa da una scritta): si tiene la migliore.
    cards.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Found> = Vec::new();
    for c in cards {
        if !kept.iter().any(|k| k.rect.dist(&c.rect) < 0.5 * card_w) {
            kept.push(c);
        }
    }
    // Restano le carte sullo stesso tavolo della maggior parte, e quelle vicine (il sito può avere
    // una fascia di un altro colore sotto la mano): non quelle disegnate altrove sullo schermo.
    let colors: Vec<Option<[u8; 3]>> = kept.iter().map(|c| table_color(img, &paper, &c.rect)).collect();
    let agree = |a: [u8; 3]| colors.iter().flatten().filter(|&&b| same_table(a, b)).count();
    let table = colors.iter().flatten().copied().max_by_key(|&a| agree(a));
    let main: Vec<bool> = colors
        .iter()
        .map(|c| match (c, table) {
            (Some(c), Some(t)) => same_table(*c, t),
            _ => true,
        })
        .collect();
    let near = |c: &Found| kept.iter().zip(&main).any(|(k, &m)| m && k.rect.dist(&c.rect) < 3.0 * card_l);
    let keep: Vec<bool> = kept.iter().zip(&main).map(|(c, &m)| m || near(c)).collect();
    let cards = kept.iter().zip(keep).filter(|(_, k)| *k).map(|(c, _)| c.clone()).collect();
    Analysis { cards, card_l, table, blobs: base }
}

/// La carta riconosciuta con più sicurezza.
fn best<'a>(cards: impl Iterator<Item = &'a Found>) -> Option<&'a Found> {
    cards.max_by(|x, y| x.score.total_cmp(&y.score))
}

/// Divide le carte scoperte trovate sullo schermo fra briscola, tavolo e mano.
pub fn scene(img: &Img) -> Scene {
    let a = analyze(img);
    let card_l = a.card_l;
    let mut scene = Scene::default();
    let upright = |f: &&Found| !f.rect.landscape();
    let bottom =
        a.cards.iter().filter(upright).filter(|f| !f.partial()).map(|f| f.rect.cy).fold(f32::MIN, f32::max);
    if bottom == f32::MIN {
        // Nessuna carta in piedi: al più la briscola.
        if let Some(t) = best(a.cards.iter()) {
            scene.trump = Some(t.view.clone());
        }
        return scene;
    }
    // La mano: la riga più in basso.
    let in_hand = |f: &Found| !f.rect.landscape() && f.rect.cy >= bottom - 0.35 * card_l;
    let mut hand: Vec<&Found> = a.cards.iter().filter(|f| in_hand(f)).collect();
    hand.sort_by(|x, y| x.rect.cx.total_cmp(&y.rect.cx));
    // La briscola: di traverso, o coperta in parte vicino alla mano (sotto il mazzo).
    let others: Vec<&Found> = a.cards.iter().filter(|f| !in_hand(f)).collect();
    let trump = best(others.iter().copied().filter(|f| f.rect.landscape()))
        .or_else(|| best(others.iter().copied().filter(|f| f.partial() && f.rect.cy > bottom - 0.9 * card_l)));
    if let Some(t) = trump {
        scene.trump = Some(t.view.clone());
    }
    // Le carte dell'avversario coperte, in alto: servono a dividere il tavolo a metà.
    let backs_y = a
        .blobs
        .iter()
        .filter(|b| b.white <= 0.3 && b.rect.long() > 0.8 * card_l && b.rect.cy < bottom - 2.5 * card_l)
        .map(|b| b.rect.cy)
        .fold(f32::MAX, f32::min);
    let mid = if backs_y < f32::MAX { (backs_y + bottom) / 2.0 } else { bottom - 2.1 * card_l };
    let mut table: Vec<&Found> = others
        .into_iter()
        .filter(|f| upright(f) && !trump.is_some_and(|t| std::ptr::eq(t, *f)))
        .collect();
    table.sort_by(|x, y| x.rect.cy.total_cmp(&y.rect.cy));
    match table.as_slice() {
        [] => {}
        [one] => {
            if one.rect.cy < mid {
                scene.opp_slot = Some(one.view.clone());
            } else {
                scene.my_slot = Some(one.view.clone());
            }
        }
        [top, .., low] => {
            scene.opp_slot = Some(top.view.clone());
            scene.my_slot = Some(low.view.clone());
        }
    }
    scene.hand = hand.iter().map(|f| f.view.clone()).collect();
    scene
}

/// Quello che si vede sul tavolo della scopa in un istante.
#[derive(Clone, Debug, Default)]
pub struct ScopaScene {
    /// Le tue carte, da sinistra a destra.
    pub hand: Vec<CardView>,
    /// Le carte scoperte sul tavolo (in tavola o appena giocate), per righe dall'alto.
    pub table: Vec<CardView>,
}

/// Colore del tavolo senza carte: il colore più comune al centro dello schermo, se è un panno
/// colorato che copre buona parte dello schermo.
fn bare_table(img: &Img) -> Option<[u8; 3]> {
    let p = Paper::of(img);
    let center: Vec<[u8; 3]> = (img.h / 4..img.h * 3 / 4)
        .step_by(8)
        .flat_map(|y| (img.w / 4..img.w * 3 / 4).step_by(8).map(move |x| (x, y)))
        .map(|(x, y)| img.at(x, y))
        .filter(|&px| !p.is(px))
        .collect();
    let color = center.iter().copied().max_by_key(|&a| center.iter().filter(|&&b| same_table(a, b)).count())?;
    let (_, s, v) = hsv(color);
    let (mut n, mut ok) = (0, 0);
    for y in (0..img.h).step_by(8) {
        for x in (0..img.w).step_by(8) {
            n += 1;
            ok += same_table(img.at(x, y), color) as usize;
        }
    }
    (s > 40 && v > 40 && ok * 10 > n * 3).then_some(color)
}

/// Riga più in basso dello schermo in cui il tavolo occupa almeno un quarto della larghezza.
fn table_bottom(img: &Img, table: [u8; 3]) -> f32 {
    for y in (0..img.h).rev().step_by(4) {
        let n = (0..img.w).step_by(6).filter(|&x| same_table(img.at(x, y), table)).count();
        if n * 4 >= img.w / 6 {
            return y as f32;
        }
    }
    img.h as f32
}

/// Divide le carte scoperte della scopa fra la tua mano (in basso) e il tavolo (tutte le altre in
/// piedi). Le carte di traverso (le scope segnate sul mucchio delle prese) si ignorano.
/// None se sullo schermo non c'è il tavolo; una scena vuota se c'è il tavolo ma nessuna carta.
pub fn scopa(img: &Img) -> Option<ScopaScene> {
    let a = analyze(img);
    if a.cards.is_empty() {
        return bare_table(img).map(|_| ScopaScene::default());
    }
    let card_l = a.card_l;
    let upright: Vec<&Found> = a.cards.iter().filter(|f| !f.rect.landscape()).collect();
    // La mano sta sul bordo basso del tavolo: entro una carta e un quarto dal fondo.
    let bottom = a.table.map_or(img.h as f32, |c| table_bottom(img, c));
    let hand_from = bottom - 1.25 * card_l;
    let (mut hand, table): (Vec<&Found>, Vec<&Found>) = upright.iter().partition(|f| f.rect.cy >= hand_from);
    hand.sort_by(|x, y| x.rect.cx.total_cmp(&y.rect.cx));
    // Per righe: due carte sono nella stessa riga se i centri distano meno di mezza carta.
    let mut table = table;
    table.sort_by(|x, y| x.rect.cy.total_cmp(&y.rect.cy));
    let mut rows: Vec<Vec<&Found>> = Vec::new();
    for f in table {
        match rows.last_mut() {
            Some(r) if (f.rect.cy - r[0].rect.cy).abs() < 0.5 * card_l => r.push(f),
            _ => rows.push(vec![f]),
        }
    }
    let mut out = Vec::new();
    for mut r in rows {
        r.sort_by(|x, y| x.rect.cx.total_cmp(&y.rect.cx));
        out.extend(r.iter().map(|f| f.view.clone()));
    }
    Some(ScopaScene { hand: hand.iter().map(|f| f.view.clone()).collect(), table: out })
}
