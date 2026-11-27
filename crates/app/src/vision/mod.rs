//! Lettura della partita dallo schermo: trova le carte scoperte su qualunque tavolo, le riconosce
//! confrontandole con le 40 carte napoletane di riferimento (anche quelle coperte in parte) e le
//! passa all'assistente. Legge solo i pixel dello schermo: non tocca il sito e non gioca al posto
//! dell'utente.

pub mod capture;
pub mod detect;
mod deck_import;
mod exposure;
pub mod live;
pub mod memory;
pub mod reference;
pub mod pips;
pub mod nn;
pub mod layout;
pub mod screen;
pub mod zones;
#[cfg(test)]
pub(crate) mod synth;
#[cfg(test)]
mod low_resolution_tests;
pub mod track;

/// Immagine RGB.
#[derive(Clone, Debug)]
pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[u8; 3]>,
}

impl Img {
    pub fn new(w: usize, h: usize) -> Img {
        Img { w, h, px: vec![[255, 255, 255]; w * h] }
    }

    pub fn from_rgba(w: usize, h: usize, rgba: &[u8]) -> Img {
        Img { w, h, px: rgba.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect() }
    }

    #[inline]
    pub fn at(&self, x: usize, y: usize) -> [u8; 3] {
        self.px[y * self.w + x]
    }

    /// Colore interpolato in un punto qualsiasi (bianco fuori dall'immagine).
    pub fn sample(&self, x: f32, y: f32) -> [u8; 3] {
        if x < 0.0 || y < 0.0 || x > (self.w - 1) as f32 || y > (self.h - 1) as f32 {
            return [255, 255, 255];
        }
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let mut out = [0u8; 3];
        for (c, o) in out.iter_mut().enumerate() {
            let a = self.at(x0, y0)[c] as f32 * (1.0 - fx) + self.at(x1, y0)[c] as f32 * fx;
            let b = self.at(x0, y1)[c] as f32 * (1.0 - fx) + self.at(x1, y1)[c] as f32 * fx;
            *o = (a * (1.0 - fy) + b * fy).round() as u8;
        }
        out
    }
}

/// Tonalità (0-360), saturazione e luminosità (0-255), come in un selettore di colori.
#[inline]
pub fn hsv(p: [u8; 3]) -> (f32, u8, u8) {
    let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let s = if max == 0.0 { 0.0 } else { d / max * 255.0 };
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s as u8, max as u8)
}

/// Quello che si vede sul tavolo in un istante.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    /// La briscola scoperta sotto il mazzo (spesso coperta in parte dal mazzo).
    pub trump: Option<CardView>,
    /// Carta giocata dall'avversario (in alto al centro).
    pub opp_slot: Option<CardView>,
    /// Carta giocata da te (in basso al centro).
    pub my_slot: Option<CardView>,
    /// Le tue carte, da sinistra a destra.
    pub hand: Vec<CardView>,
}

/// Una carta scoperta raddrizzata e riconosciuta.
#[derive(Clone, Debug)]
pub struct CardView {
    /// La sagoma è stata recuperata su un tavolo altrimenti vuoto.
    pub recovered_outline: bool,
    /// Carta raddrizzata, CARD_W x CARD_H, con la testa in alto.
    pub img: Img,
    /// Vista solo in parte (la briscola sotto il mazzo, una carta tagliata o coperta).
    pub partial: bool,
    /// Fascia visibile della carta: frazioni dell'altezza, dalla testa.
    pub vis: [f32; 2],
    pub vis_cols: [f32; 2],
    /// Somiglianza (-1..1) con ciascuna delle 40 carte di riferimento.
    pub sim: [f32; 40],
    /// Punteggio di ciascuna delle 40 carte (più alto = più probabile).
    pub scores: [f32; 40],
    /// Dove sta la carta intera nella schermata letta: sinistra, alto, destra, basso, in frazioni
    /// della schermata (serve alla Psy Fly per volarci sopra).
    pub at: [f32; 4],
    /// Somiglianza con le 40 carte di ciascuno stile di riferimento (vuota se non si sa).
    pub styles: Vec<[f32; 40]>,
    /// Lato lungo e lato corto della carta sullo schermo, in pixel (0 se non si sa).
    pub px: [f32; 2],
}

/// Dettaglio locale della parte visibile, usato nel confronto fra fotogrammi
/// della stessa sagoma. Il bordo della carta e il tavolo non partecipano.
pub(crate) fn visible_detail(view: &CardView) -> f32 {
    let lum = |[r, g, b]: [u8; 3]| (i32::from(r) + 2 * i32::from(g) + i32::from(b)) / 4;
    let (w, h) = (view.img.w, view.img.h);
    let bounds = |[from, to]: [f32; 2], length: usize| {
        (((from.max(0.10) * length as f32).ceil() as usize).min(length),
         ((to.min(0.90) * length as f32).floor() as usize).min(length))
    };
    let (x0, x1) = bounds(view.vis_cols, w);
    let (y0, y1) = bounds(view.vis, h);
    let mut total = 0u64;
    let mut samples = 0u32;
    for y in (y0..y1.saturating_sub(1)).step_by(3) {
        for x in (x0..x1.saturating_sub(1)).step_by(3) {
            let p = lum(view.img.at(x, y));
            total += (p - lum(view.img.at(x + 1, y))).unsigned_abs() as u64;
            total += (p - lum(view.img.at(x, y + 1))).unsigned_abs() as u64;
            samples += 2;
        }
    }
    total as f32 / samples.max(1) as f32
}

pub const CARD_W: usize = 100;
pub const CARD_H: usize = 160;
/// La carta vista tutta.
pub const WHOLE: [f32; 2] = [0.0, 1.0];
/// Punti per unità di somiglianza: un distacco di 0.175 basta per inserire la carta senza conferma.
const SCALE: f32 = 8.0;
/// Una carta quasi identica a quella di riferimento (stesso disegno del sito) con la seconda
/// lontana è sicura anche se il distacco è moderato (il cinque e il quattro di coppe, quando del
/// cinque si vede solo metà).
const EXACT: f32 = 0.95;
const EXACT_GAP: f32 = 0.88;
const EXACT_BONUS: f32 = 1.0;

/// Quanto scende il punteggio di una carta esclusa dal numero di simboli contati.
const PIP_PENALTY: f32 = 3.0;

/// Punteggi delle 40 carte dalla somiglianza con le carte di riferimento.
/// Da quale somiglianza in giù conta la rete, e quanto (nelle prove si cambiano con NN_DA e NN_K).
fn nn_params() -> (f32, f32) {
    if cfg!(test) {
        let get = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        return (get("NN_DA", NN_FROM), get("NN_K", NN_WEIGHT));
    }
    (NN_FROM, NN_WEIGHT)
}
const NN_FROM: f32 = 0.65;
/// La memoria decide la carta solo se la carta somiglia poco ai mazzi noti (un mazzo che l'app non
/// conosce): su un mazzo noto la lettura è già sicura, e un'immagine ricordata sotto il nome
/// sbagliato non deve ripetere l'errore.
const DECISIVE_BELOW: f32 = 0.75;
const NN_WEIGHT: f32 = 1.0;
/// Accordo fra confronto e rete (vedi `recognize`): da quale somiglianza, sotto quale distacco, con
/// quale sicurezza della rete (probabilità in logaritmo: 0.9 e 0.05) e quanto si aggiunge.
const AGREE_FROM: f32 = 0.70;
const AGREE_BELOW: f32 = 1.4;
const AGREE_NN_SURE: f32 = -0.105;
const AGREE_NN_OTHER: f32 = -3.0;
const AGREE_BONUS: f32 = 0.8;

fn reference_scores(sim: &[f32; 40]) -> [f32; 40] {
    let mut s = sim.map(|v| v * SCALE);
    let mut order: Vec<usize> = (0..40).collect();
    order.sort_by(|&a, &b| sim[b].total_cmp(&sim[a]));
    if sim[order[0]] >= EXACT && sim[order[1]] <= EXACT_GAP {
        s[order[0]] += EXACT_BONUS;
    }
    s
}

use briscola_core::cards::{Card, CardSet};
pub use detect::ScopaScene;
use memory::Memory;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, TrySendError};
use std::sync::{Arc, Condvar, Mutex};

thread_local! {
    /// Dove stanno le carte sullo schermo (vedi `focused_scene`).
    static FOCUS: std::cell::RefCell<Focus> = std::cell::RefCell::new(Focus::default());
}

/// La zona dello schermo dove si sono viste le carte nelle ultime letture complete (frazioni:
/// sinistra, alto, destra, basso), e quante letture sono passate dall'ultima lettura completa.
#[derive(Default)]
struct Focus {
    boxes: std::collections::VecDeque<[f32; 4]>,
    since_full: u32,
    size: (usize, usize),
    /// Carte viste nell'ultima lettura completa.
    last_count: usize,
    /// Se le letture complete hanno già visto una carta in ciascuna delle due caselle del tavolo:
    /// solo allora la zona comprende tutto il tavolo e si può ritagliare.
    seen_opp: bool,
    seen_mine: bool,
    /// Centro della casella dell'avversario e della mia, imparati dalle prese con due carte.
    opp: Option<[f32; 2]>,
    mine: Option<[f32; 2]>,
    samples: u32,
    /// La disposizione salvata è già stata letta, e quando si è salvata l'ultima volta.
    loaded: bool,
    /// Letture di fila in cui la zona non bastava (un altro sito, la finestra spostata).
    misses: u32,
    saved: Option<std::time::Instant>,
    /// Letture da aspettare prima di un'altra lettura completa per una carta sparita dal tavolo.
    wait_full: u32,
    /// Dove si sono viste carte in tavola (frazioni dello schermo): la zona le comprende sempre,
    /// anche se le ultime letture complete sono capitate a tavolo vuoto.
    table_box: Option<[f32; 4]>,
}

impl Focus {
    fn layout(&self) -> layout::Layout {
        layout::Layout {
            size: self.size,
            boxes: self.boxes.iter().copied().collect(),
            seen_opp: self.seen_opp,
            seen_mine: self.seen_mine,
            opp: self.opp,
            mine: self.mine,
            samples: self.samples,
        }
    }
}

fn center(v: &CardView) -> [f32; 2] {
    [(v.at[0] + v.at[2]) / 2.0, (v.at[1] + v.at[3]) / 2.0]
}

/// La carta sola in tavola va nella casella più vicina fra quelle imparate (su molti siti la metà
/// del tavolo non dice di chi è); con due carte si impara dove sta ciascuna casella.
fn place_table(scene: &mut Scene) {
    FOCUS.with(|f| {
        let mut f = f.borrow_mut();
        match (&scene.opp_slot, &scene.my_slot) {
            (Some(o), Some(m)) => {
                let (co, cm) = (center(o), center(m));
                let ema = |old: Option<[f32; 2]>, new: [f32; 2]| match old {
                    Some(p) => [0.8 * p[0] + 0.2 * new[0], 0.8 * p[1] + 0.2 * new[1]],
                    None => new,
                };
                f.opp = Some(ema(f.opp, co));
                f.mine = Some(ema(f.mine, cm));
                f.samples = f.samples.saturating_add(1);
            }
            (one_opp, one_mine) if one_opp.is_some() != one_mine.is_some() => {
                let (Some(o), Some(m)) = (f.opp, f.mine) else { return };
                if f.samples < 5 || ((o[0] - m[0]).abs() + (o[1] - m[1]).abs()) < 0.02 {
                    return;
                }
                let v = one_opp.as_ref().or(one_mine.as_ref()).unwrap();
                let c = center(v);
                let d = |p: [f32; 2]| (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2);
                let mine_is_nearer = d(m) < d(o);
                let nearest = if mine_is_nearer { m } else { o };
                let width = v.at[2] - v.at[0];
                let height = v.at[3] - v.at[1];
                // Le posizioni apprese valgono solo vicino alle caselle. Un altro tavolo alla
                // stessa risoluzione può avere una carta lontana da entrambe.
                if width <= 0.0 || height <= 0.0
                    || ((nearest[0] - c[0]) / width).powi(2)
                        + ((nearest[1] - c[1]) / height).powi(2) > 1.0
                {
                    return;
                }
                if mine_is_nearer && scene.opp_slot.is_some() {
                    scene.my_slot = scene.opp_slot.take();
                } else if !mine_is_nearer && scene.my_slot.is_some() {
                    scene.opp_slot = scene.my_slot.take();
                }
            }
            _ => {}
        }
    });
}

/// Quante letture al più fra una lettura di tutto lo schermo e l'altra.
const FULL_EVERY: u32 = 24;

/// Dimentica la zona delle carte (una lettura indipendente, come una partita nuova del banco).
pub fn forget_focus() {
    detect::forget_light_hand();
    FOCUS.with(|f| *f.borrow_mut() = Focus::default());
}

/// La zona delle carte di questa lettura, in frazioni dello schermo (`None` se non ci sono carte).
fn scene_box(scene: &Scene) -> Option<[f32; 4]> {
    let mut b: Option<[f32; 4]> = None;
    for v in scene.trump.iter().chain(&scene.opp_slot).chain(&scene.my_slot).chain(&scene.hand) {
        if v.at[2] <= v.at[0] {
            continue;
        }
        b = Some(match b {
            None => v.at,
            Some(x) => [x[0].min(v.at[0]), x[1].min(v.at[1]), x[2].max(v.at[2]), x[3].max(v.at[3])],
        });
    }
    b
}

/// Allarga la zona delle carte in tavola viste con le carte in tavola di questa lettura.
fn add_table_box(f: &mut Focus, scene: &Scene) {
    for v in scene.opp_slot.iter().chain(&scene.my_slot).filter(|v| v.at[2] > v.at[0]) {
        f.table_box = Some(match f.table_box {
            None => v.at,
            Some(x) => [x[0].min(v.at[0]), x[1].min(v.at[1]), x[2].max(v.at[2]), x[3].max(v.at[3])],
        });
    }
}

fn count(scene: &Scene) -> usize {
    scene.trump.is_some() as usize + scene.opp_slot.is_some() as usize + scene.my_slot.is_some() as usize + scene.hand.len()
}

/// Si legge solo la zona dove stanno le carte (con un margine di una carta tutto intorno): il
/// resto dello schermo (chat, punteggi, pubblicità) non si analizza. Ogni tanto, e se nella zona
/// si trova meno di quello che c'era, si rilegge tutto lo schermo e la zona si aggiorna.
fn focused_scene(img: &Img) -> Scene {
    let mut scene = focused_scene_inner(img);
    place_table(&mut scene);
    scene
}

fn focused_scene_inner(img: &Img) -> Scene {
    // La disposizione imparata le altre volte su questo schermo (solo dal file sul computer).
    FOCUS.with(|f| {
        let mut f = f.borrow_mut();
        if !f.loaded || f.size != (img.w, img.h) {
            *f = Focus { size: (img.w, img.h), loaded: true, ..Focus::default() };
            if let Some(l) = layout::load((img.w, img.h)) {
                f.boxes = l.boxes.into_iter().collect();
                f.seen_opp = l.seen_opp;
                f.seen_mine = l.seen_mine;
                f.opp = l.opp;
                f.mine = l.mine;
                f.samples = l.samples;
            }
        }
    });
    let plan = FOCUS.with(|f| {
        let f = f.borrow();
        if f.size != (img.w, img.h) || f.boxes.len() < 3 || f.since_full >= FULL_EVERY || !(f.seen_opp && f.seen_mine)
            || (cfg!(test) && std::env::var_os("SENZA_ZONA").is_some())
        {
            return None;
        }
        let mut u = f.boxes[0];
        for b in f.boxes.iter().chain(f.table_box.iter()) {
            u = [u[0].min(b[0]), u[1].min(b[1]), u[2].max(b[2]), u[3].max(b[3])];
        }
        // Margine: una carta (la lunghezza delle carte è circa l'altezza della mano).
        let card = f.boxes.iter().map(|b| b[3] - b[1]).fold(0.0f32, f32::max).min(0.3) * img.h as f32 / img.w.max(1) as f32;
        let (mx, my) = (card.max(0.04) * 0.8, (card * img.w as f32 / img.h.max(1) as f32).max(0.04) * 0.5);
        // (Sulla griglia di 1/64 dello schermo, così il ritaglio non cambia a ogni lettura.)
        let q = |v: f32, up: bool| if up { (v * 64.0).ceil() / 64.0 } else { (v * 64.0).floor() / 64.0 };
        let r = [q((u[0] - mx).max(0.0), false), q((u[1] - my).max(0.0), false), q((u[2] + mx).min(1.0), true), q((u[3] + 1.6 * my).min(1.0), true)];
        // Se la zona è quasi tutto lo schermo non serve ritagliare.
        ((r[2] - r[0]) * (r[3] - r[1]) < 0.7).then_some((r, f.last_count))
    });
    if let Some((r, expected)) = plan {
        let (x0, y0) = ((r[0] * img.w as f32) as usize, (r[1] * img.h as f32) as usize);
        let (x1, y1) = ((r[2] * img.w as f32) as usize, (r[3] * img.h as f32) as usize);
        let crop = img.crop(x0, y0, x1, y1);
        detect::set_offset(x0 as f32, y0 as f32);
        let mut scene = detect::scene(&crop);
        detect::set_offset(0.0, 0.0);
        // Nella zona c'è almeno quello che c'era all'ultima lettura completa: vale. (Una carta in
        // meno può essere uscita davvero, o tagliata dal bordo della zona: si rilegge tutto.)
        // (E nessuna carta tocca il bordo della zona: una carta tagliata dal ritaglio, o una zona
        // che non è più quella giusta, si rilegge su tutto lo schermo.)
        let inside = |v: &CardView| v.at[0] > 0.01 && v.at[1] > 0.01 && v.at[2] < 0.99 && v.at[3] < 0.99;
        let clear = scene.trump.iter().chain(&scene.opp_slot).chain(&scene.my_slot).chain(&scene.hand).all(|v| v.at[2] <= v.at[0] || inside(v));
        // (Una carta appena sparita dal tavolo: la presa si sta chiudendo e la carta dell'avversario
        // può essere comparsa, sbiadita, fuori dalla zona. Si legge tutto lo schermo.)
        let vanished = detect::take_vanished() && FOCUS.with(|f| {
            let mut f = f.borrow_mut();
            if f.wait_full > 0 {
                f.wait_full -= 1;
                false
            } else {
                f.wait_full = 3;
                true
            }
        });
        let spans_slots = FOCUS.with(|focus| {
            let focus = focus.borrow();
            let interior = |p: [f32; 2]| {
                p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3]
            };
            focus.samples < 5 || focus.opp.into_iter().chain(focus.mine).all(interior)
        });
        if clear && !vanished && spans_slots && count(&scene) >= expected && count(&scene) > 0 {
            let (sx, sy) = ((x1 - x0) as f32 / img.w as f32, (y1 - y0) as f32 / img.h as f32);
            let (ox, oy) = (x0 as f32 / img.w as f32, y0 as f32 / img.h as f32);
            for v in scene.trump.iter_mut().chain(&mut scene.opp_slot).chain(&mut scene.my_slot).chain(&mut scene.hand) {
                if v.at[2] > v.at[0] {
                    v.at = [ox + v.at[0] * sx, oy + v.at[1] * sy, ox + v.at[2] * sx, oy + v.at[3] * sy];
                }
            }
            FOCUS.with(|f| {
                let mut f = f.borrow_mut();
                f.since_full += 1;
                f.misses = 0;
                add_table_box(&mut f, &scene);
            });
            if std::env::var_os("TEMPI").is_some() {
                eprintln!("  ritaglio {}x{} di {}x{} da {},{}", x1 - x0, y1 - y0, img.w, img.h, x0, y0);
            }
            return scene;
        }
    }
    let scene = detect::scene(img);
    let cropped = plan.is_some();
    FOCUS.with(|f| {
        let mut f = f.borrow_mut();
        // La zona non basta più per tre letture di fila: si dimentica e si impara di nuovo.
        if cropped {
            f.misses += 1;
            if f.misses >= 3 {
                f.boxes.clear();
                f.table_box = None;
                f.seen_opp = false;
                f.seen_mine = false;
                f.misses = 0;
            }
        }
        f.since_full = 0;
        f.last_count = count(&scene);
        f.seen_opp |= scene.opp_slot.is_some();
        f.seen_mine |= scene.my_slot.is_some();
        add_table_box(&mut f, &scene);
        if let Some(b) = scene_box(&scene) {
            f.boxes.push_back(b);
            if f.boxes.len() > 30 {
                f.boxes.pop_front();
            }
        }
        // Si salva sul computer al più ogni mezzo minuto.
        if f.saved.is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(30)) && f.boxes.len() >= 3 {
            f.saved = Some(std::time::Instant::now());
            layout::save(&f.layout());
        }
    });
    scene
}

/// Punteggi della carta: somiglianza con le carte di riferimento, più quella con le immagini delle
/// carte già viste su questo sito (che quando c'è decide).
pub fn recognize(view: &mut CardView, mem: &Memory) {
    let mut s = reference_scores(&view.sim);
    // Un mazzo che l'app non conosce somiglia poco a tutte le carte di riferimento: allora conta
    // anche la rete (vedi `nn`), che riconosce seme e valore anche di un mazzo mai visto. Più la
    // carta è sconosciuta, più conta la rete; sui mazzi noti non cambia niente.
    let top = view.sim.iter().copied().fold(f32::MIN, f32::max);
    let (mut from, k) = nn_params();
    // Esperimento E2c (solo nelle prove, `NN_DA_INTERE=0.85`): la rete conta da una somiglianza
    // più alta, ma solo sulle carte viste per intero (non coperte né tagliate).
    if cfg!(test) && !view.partial && view.vis[0] <= 0.0 && view.vis[1] >= 1.0 {
        if let Some(v) = std::env::var("NN_DA_INTERE").ok().and_then(|v| v.parse::<f32>().ok()) {
            from = from.max(v);
        }
    }
    let w = ((from - top) / 0.25).clamp(0.0, 1.0);
    if cfg!(test) && std::env::var_os("NN_LOG").is_some() {
        let lp = nn::log_probs(&view.img).unwrap_or([0.0; 40]);
        let am = |v: &[f32; 40]| (0..40).max_by(|&a, &b| v[a].total_cmp(&v[b])).unwrap();
        eprintln!("RETE top {top:.2} peso {w:.2} parziale {} rif {} rete {}", view.partial, am(&view.sim), am(&lp));
    }
    if w > 0.0 {
        let neural = if view.partial {
            nn::partial_log_probs(&view.img, view.vis, view.vis_cols)
        } else {
            nn::log_probs(&view.img)
        };
        if let Some(lp) = neural {
            let weight = if view.partial { 0.10 * w } else { w };
            for c in 0..40 {
                s[c] += weight * k * lp[c].max(-12.0);
            }
        }
    }
    // Due carte di un mazzo noto molto simili (il fante di bastoni e quello di denari): il
    // confronto le separa di poco. Se la rete, che guarda la carta in un altro modo, dà con
    // sicurezza la stessa carta e quasi niente alla seconda, i due giudizi insieme bastano.
    if !view.partial && top >= AGREE_FROM && !(cfg!(test) && std::env::var_os("SENZA_ACCORDO").is_some()) {
        let mut order: Vec<usize> = (0..40).collect();
        order.sort_by(|&a, &b| s[b].total_cmp(&s[a]));
        let (first, second) = (order[0], order[1]);
        if s[first] - s[second] < AGREE_BELOW {
            if let Some(lp) = nn::log_probs(&view.img) {
                let nn_best = (0..40).max_by(|&a, &b| lp[a].total_cmp(&lp[b])).unwrap();
                if nn_best == first && lp[first] >= AGREE_NN_SURE && lp[second] <= AGREE_NN_OTHER {
                    s[first] += AGREE_BONUS;
                }
            }
        }
    }
    // I simboli contati sulla carta (vista intera) escludono le carte con un altro numero di
    // simboli, in qualunque mazzo: un quattro di coppe un po' storto non diventa un due.
    // Il conteggio decide solo fra le carte che la lettura dà già per possibili: se nessuna delle
    // più somiglianti ha quel numero di simboli, a sbagliare è il conteggio (simboli che si toccano
    // su carte piccole o strette), e non vale.
    // (Solo su carte abbastanza grandi e di proporzioni normali: su carte piccole o stirate i
    // simboli si toccano o si spezzano e il conteggio non è sicuro.)
    let [long, short] = view.px;
    let countable = long >= 110.0 && short >= 66.0 && (0.52..=0.95).contains(&(short / long));
    let excluded = match (!view.partial && countable && !(cfg!(test) && std::env::var_os("SENZA_SIMBOLI").is_some())).then(|| pips::count(&view.img)).flatten() {
        Some(n) => {
            let ex = pips::excluded(n);
            let mut order: Vec<usize> = (0..40).collect();
            order.sort_by(|&a, &b| s[b].total_cmp(&s[a]));
            let top = s[order[0]];
            let fits = order.iter().take(4).any(|&c| s[c] >= top - 4.0 && !ex[c] && pips::expected()[c].contains(&n));
            if fits { ex } else { [false; 40] }
        }
        None => [false; 40],
    };
    for (c, out) in excluded.iter().enumerate() {
        if *out {
            s[c] -= PIP_PENALTY;
        }
    }
    // (Nemmeno la memoria delle carte viste può dare una carta esclusa: un'immagine ricordata
    // sotto il nome sbagliato non deve ripetere l'errore.)
    // Il confronto già affidabile per i mazzi noti può includere una striscia
    // molto stretta. La maschera è una prova di riserva sulle letture deboli.
    let narrow_evidence = view.vis_cols != WHOLE && (view.recovered_outline || top < 0.55);
    let columns = if narrow_evidence { view.vis_cols } else { WHOLE };
    let matches = mem.visible_matches(&view.img, view.vis, columns);
    let mut memory_conflict = matches.conflict();
    let mut sims = matches.combined();
    if !view.partial && top < DECISIVE_BELOW && sims.iter().any(Option::is_some)
        && !sims.iter().flatten().any(|&s| s >= memory::SURE) {
        let mut flipped = view.img.clone();
        flipped.px.reverse();
        let reversed = mem.visible_matches(&flipped, [1.0 - view.vis[1], 1.0 - view.vis[0]], WHOLE);
        memory_conflict |= reversed.conflict();
        let reverse = reversed.combined();
        if reverse.iter().flatten().any(|&s| s >= memory::SURE) {
            for (score, rotated) in sims.iter_mut().zip(reverse) {
                if let Some(b) = rotated { *score = Some(score.map_or(b, |a| a.max(b))); }
            }
        }
    }
    for (c, sim) in sims.iter().enumerate() {
        if let Some(v) = *sim {
            if v >= memory::SAME && !excluded[c] {
                s[c] += 6.0 + (v - memory::SAME) * 20.0;
            }
        }
    }
    // La stessa immagine, quasi pixel per pixel, già vista su questo sito e nessun'altra carta
    // ricordata che le somigli altrettanto: è quella carta, qualunque cosa dicano il confronto coi
    // mazzi di riferimento, la rete o il conteggio dei simboli (che su un mazzo nuovo sbagliano).
    let mut order: Vec<(usize, f32)> = sims.iter().enumerate().filter_map(|(c, v)| v.map(|v| (c, v))).collect();
    order.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut verified_memory = false;
    if let Some(&(c, v)) = order.first() {
        let second = order.get(1).map_or(f32::MIN, |o| o.1);
        if top < DECISIVE_BELOW && v >= memory::SURE && v - second >= memory::SURE_GAP && !(cfg!(test) && std::env::var_os("SENZA_DECISIVA").is_some()) {
            memory_conflict |= excluded[c];
            if !memory_conflict && !narrow_evidence {
                let top = s.iter().copied().fold(f32::MIN, f32::max);
                s[c] = s[c].max(top + 6.0);
                verified_memory = true;
            }
        }
    }
    // La concordanza tra stampe aumenta la fiducia solo se conferma
    // l'identità già in testa dopo simboli e memoria.
    if !view.partial && !verified_memory && !view.recovered_outline
        && (0.65..0.95).contains(&top)
        && !(cfg!(test) && std::env::var_os("BRIS_SENZA_CONCORDANZA").is_some()) {
        if let Some(card) = reference::agreeing_card(&view.styles) {
            let strongest = s.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            if s[card] == strongest {
                s[card] += 0.75;
            }
        }
    }
    // Solo il confronto incerto di una stampa intera, senza riscontro in memoria.
    if !view.partial && !view.recovered_outline && !memory_conflict && !verified_memory
        && (0.60..0.79).contains(&top) {
        let mut ordered = s;
        ordered.sort_by(|a, b| b.total_cmp(a));
        if ordered[0] - ordered[1] < 1.0 {
            if let Some(card) = reference::luminance_alignment(&view.img) {
                if !excluded[card] && ordered[0] - s[card] <= 1.0 {
                    s[card] = s[card].max(ordered[0] + 0.2);
                }
            }
        }
    }
    if memory_conflict || ((view.recovered_outline || narrow_evidence) && !verified_memory) {
        // La sagoma dà la posizione, ma non dimostra seme e valore. Finché
        // non si ritrova in memoria, conserva l'ordine dei candidati e limita
        // il distacco anche dopo aver escluso carte impossibili.
        let lo = s.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = s.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let scale = (hi - lo).max(1.0);
        for score in &mut s { *score = hi + (*score - hi) / scale; }
    }
    view.scores = s;
}

impl CardView {
    pub fn new(img: Img, vis: [f32; 2], sim: [f32; 40]) -> CardView {
        CardView { recovered_outline: false, vis_cols: WHOLE, img, partial: vis[1] - vis[0] < 0.9, vis, sim, scores: reference_scores(&sim), at: [0.0; 4], styles: Vec::new(), px: [0.0; 2] }
    }

    /// La carta più probabile fra `among`, e quanto stacca la seconda.
    pub fn best(&self, among: CardSet) -> Option<(Card, f32)> {
        let mut v: Vec<(Card, f32)> = among.iter().map(|c| (c, self.scores[c as usize])).collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let first = *v.first()?;
        let margin = v.get(1).map_or(f32::INFINITY, |s| first.1 - s.1);
        Some((first.0, margin))
    }
}

/// La carta `c` fra quelle viste sullo schermo (`among` sono le carte che possono essere lì):
/// quella che le somiglia di più, purché le somigli davvero e non sia solo la meno peggio fra
/// `among` (un pezzo di tavolo o una scritta presi per carta non devono essere segnati).
pub fn locate(views: &[CardView], among: CardSet, c: Card) -> Option<&CardView> {
    views
        .iter()
        .filter(|v| v.best(among).is_some_and(|b| b.0 == c))
        .filter(|v| {
            let top = v.scores.iter().copied().fold(f32::MIN, f32::max);
            v.scores[c as usize] >= top - LOCATE_GAP
        })
        .max_by(|a, b| a.scores[c as usize].total_cmp(&b.scores[c as usize]))
}

/// Quanto può staccare la carta più somigliante (fra tutte le 40) quella che si cerca.
const LOCATE_GAP: f32 = 1.5;

/// Esito di una lettura dello schermo.
pub enum Reading<S> {
    Scene(S, u64),
    /// Nessun tavolo con carte sullo schermo.
    NoTable,
    Error(String),
}

/// Lettura per la briscola: trova le carte e le riconosce (None se non c'è nessuna carta scoperta).
/// La memoria resta bloccata solo mentre si confrontano le carte trovate, non durante la ricerca.
/// Esperimento E6 (`SPER_SBIADITE=1`, solo nelle prove): a fine presa alcuni siti sbiadiscono le
/// carte giocate nel colore del tavolo (una carta al 30-50%), e la carta giocata per seconda può
/// comparire già sbiadita. Se in tavola non si trova niente ma c'è la mano, si ricostruisce la
/// schermata togliendo la fusione col colore del tavolo, `p' = T + (p - T) / a`, e si cercano lì
/// le carte giocate (solo carte che somigliano chiaramente a un mazzo noto).
/// Esperimento E6: quanto vicino alla casella imparata e quanto simile a un mazzo noto deve essere
/// una carta ritrovata togliendo il colore del tavolo.
const FADED_NEAR: f32 = 0.05;
const FADED_SIM: f32 = 0.85;

fn faded_table(img: &Img, scene: &mut Scene) {
    if std::env::var_os("SPER_SBIADITE_LOG").is_some() {
        eprintln!("sbiadite? A {} T {} mano {}", scene.opp_slot.is_some(), scene.my_slot.is_some(), scene.hand.len());
    }
    if scene.opp_slot.is_some() || scene.my_slot.is_some() || scene.hand.is_empty() {
        return;
    }
    // Il colore del tavolo: il più frequente (a passi di 8).
    let mut count = std::collections::HashMap::<[u8; 3], usize>::new();
    // (Né i pannelli bianchi né le bande scure: il panno.)
    for p in img.px.iter().step_by(7) {
        let sum = p.iter().map(|&v| v as u32).sum::<u32>();
        if (90..600).contains(&sum) {
            *count.entry(p.map(|v| v / 8)).or_default() += 1;
        }
    }
    let Some((&t, _)) = count.iter().max_by_key(|(_, n)| **n) else { return };
    let t = t.map(|v| v as f32 * 8.0 + 4.0);
    // (Un tavolo chiaro non sbiadisce niente di visibile.)
    if t.iter().sum::<f32>() > 3.0 * 170.0 {
        return;
    }
    let top_hand = scene.hand.iter().map(|v| v.at[1]).fold(1.0f32, f32::min);
    // Le caselle dell'avversario e mia, imparate dalle prese con due carte.
    let slots = FOCUS.with(|f| {
        let f = f.borrow();
        match (f.opp, f.mine) {
            (Some(o), Some(m)) if f.samples >= 5 => Some((o, m)),
            _ => None,
        }
    });
    if slots.is_none() {
        return;
    }
    for a in [0.35f32, 0.5] {
        let mut out = img.clone();
        for p in out.px.iter_mut() {
            *p = std::array::from_fn(|c| (t[c] + (p[c] as f32 - t[c]) / a).clamp(0.0, 255.0) as u8);
        }
        let s = detect::scene(&out);
        if std::env::var_os("SPER_SBIADITE_LOG").is_some() {
            if std::env::var_os("SPER_SBIADITE_SALVA").is_some() {
                let _ = out.save_png(std::path::Path::new(&format!("{}/sbiadita_{a}.png", std::env::var("SPER_SBIADITE_SALVA").unwrap())));
            }
            let show = |v: &Option<CardView>| v.as_ref().map(|v| (v.best(CardSet::FULL), v.sim.iter().copied().fold(f32::MIN, f32::max), v.at));
            eprintln!("prova sbiadite a={a} tavolo {t:?}: A {:?} T {:?} mano {}", show(&s.opp_slot), show(&s.my_slot), s.hand.len());
        }
        // Solo una carta che cade in una casella del tavolo già imparata (centro entro `NEAR`),
        // assegnata a quella casella: ombre e resti dell'animazione stanno altrove.
        let near = |v: &CardView, slot: Option<[f32; 2]>| {
            slot.is_some_and(|p| {
                let c = center(v);
                (c[0] - p[0]).abs() < FADED_NEAR && (c[1] - p[1]).abs() < FADED_NEAR
            })
        };
        let good = |v: &CardView| v.at[3] < top_hand + 0.02 && v.sim.iter().copied().fold(f32::MIN, f32::max) >= FADED_SIM;
        let found: Vec<CardView> = [s.opp_slot.clone(), s.my_slot.clone()].into_iter().flatten().filter(|v| good(v)).collect();
        let pick = |slot: Option<[f32; 2]>| found.iter().find(|v| near(v, slot)).cloned();
        let (o, m) = (pick(slots.map(|s| s.0)), pick(slots.map(|s| s.1)));
        if o.is_some() || m.is_some() {
            if std::env::var_os("SPER_SBIADITE_LOG").is_some() {
                eprintln!("sbiadite a={a}: {:?} {:?}", o.as_ref().and_then(|v| v.best(CardSet::FULL)), m.as_ref().and_then(|v| v.best(CardSet::FULL)));
            }
            scene.opp_slot = o;
            scene.my_slot = m;
            return;
        }
    }
}

pub fn read_briscola(img: &Img, mem: &Mutex<Memory>) -> Option<Scene> {
    let recovered = exposure::recover(img);
    let img = recovered.as_ref().unwrap_or(img);
    let rectified = detect::rectify_aspect(img);
    let original_width = img.w;
    let img = rectified.as_ref().unwrap_or(img);
    let profile = std::env::var_os("BRIS_PROFILO_CARTE").is_some();
    let started = std::time::Instant::now();
    let mut scene = focused_scene(img);
    if cfg!(test) && std::env::var_os("SPER_SBIADITE").is_some() {
        faded_table(img, &mut scene);
    }
    let detected = started.elapsed();
    if profile { eprintln!("profilo briscola rilevamento {} us", detected.as_micros()); }
    let classify_at = std::time::Instant::now();
    let found = scene.trump.is_some() || !scene.hand.is_empty() || scene.opp_slot.is_some() || scene.my_slot.is_some();
    if !found {
        return None;
    }
    // Il confronto è costoso: lavora su una fotografia della memoria, senza
    // trattenere il mutex usato anche dall'interfaccia per apprendere le carte.
    let mem = mem.lock().unwrap_or_else(|e| e.into_inner()).isolated();
    // Il mazzo del sito, dalle carte viste intere di questa lettura (e da quelle prima): le carte
    // si confrontano soprattutto con quello.
    let whole: Vec<&[[f32; 40]]> = scene
        .opp_slot
        .iter()
        .chain(&scene.my_slot)
        .chain(&scene.hand)
        .filter(|v| !v.partial && !v.styles.is_empty())
        .map(|v| v.styles.as_slice())
        .collect();
    let st = reference::site_style(&whole);
    if let Some(st) = st {
        for v in scene.trump.iter_mut().chain(&mut scene.opp_slot).chain(&mut scene.my_slot).chain(&mut scene.hand) {
            if !v.styles.is_empty() {
                v.sim = reference::with_style(&v.styles, st);
            }
        }
    }
    for v in scene.trump.iter_mut().chain(&mut scene.opp_slot).chain(&mut scene.my_slot).chain(&mut scene.hand) {
        recognize(v, &mem);
    }
    if rectified.is_some() {
        // px contiene i lati lungo e corto; la briscola è disposta di traverso.
        let scale = original_width as f32 / img.w as f32;
        for v in scene.hand.iter_mut().chain(&mut scene.opp_slot).chain(&mut scene.my_slot) {
            v.px[1] *= scale;
            if v.px[1] > v.px[0] { v.px.swap(0, 1); }
        }
        if let Some(v) = &mut scene.trump {
            v.px[0] *= scale;
            if v.px[1] > v.px[0] { v.px.swap(0, 1); }
        }
    }
    resolve_hand_duplicates(&mut scene.hand);
    resolve_table_duplicates(&mut scene);
    if profile { eprintln!("profilo briscola classificazione {} us", classify_at.elapsed().as_micros()); }
    Some(scene)
}

/// Due posizioni in mano non possono avere la stessa carta. Modifica solo
/// una lettura già incerta e conserva il margine da confermare.
fn resolve_hand_duplicates(hand: &mut [CardView]) {
    if hand.len() < 2 { return; }
    let mut certain = CardSet::EMPTY;
    for card in hand.iter().filter(|c| !c.partial) {
        if let Some((id, margin)) = card.best(CardSet::FULL) {
            if margin >= 3.0 { certain.insert(id); }
        }
    }
    if certain.is_empty() { return; }
    for card in hand.iter_mut().filter(|c| !c.partial) {
        let Some((id, margin)) = card.best(CardSet::FULL) else { continue };
        if margin >= 1.4 || !certain.contains(id) { continue; }
        let alternative = CardSet::FULL.minus(certain);
        let Some((next, _)) = card.best(alternative) else { continue };
        let plausible = card.scores[next as usize];
        if card.scores[id as usize] - plausible <= 1.4 {
            for taken in certain.iter() {
                let score = &mut card.scores[taken as usize];
                *score = score.min(plausible - 0.01);
            }
        }
    }
}

/// Una briscola scoperta e due carte in tavola distanti dalla mano sono ruoli
/// diversi. Si corregge soltanto la proposta incerta di un contorno recuperato.
fn resolve_table_duplicates(scene: &mut Scene) {
    let mut confirmed = CardSet::EMPTY;
    for view in scene.hand.iter().filter(|v| !v.partial && !v.recovered_outline) {
        if let Some((card, _)) = view.best(CardSet::FULL).filter(|&(_, margin)| margin >= 4.0) {
            confirmed.insert(card);
        }
    }
    for view in scene.trump.iter_mut().chain(&mut scene.opp_slot).chain(&mut scene.my_slot) {
        if !view.recovered_outline || view.partial { continue; }
        let Some((card, margin)) = view.best(CardSet::FULL) else { continue };
        if !confirmed.contains(card) || margin >= 1.0 { continue; }
        let Some((alternative, _)) = view.best(CardSet::FULL.minus(confirmed)) else { continue };
        let ceiling = view.scores[alternative as usize] - 0.01;
        if view.scores[card as usize] - ceiling > 1.0 { continue; }
        for id in confirmed.iter() {
            view.scores[id as usize] = view.scores[id as usize].min(ceiling);
        }
    }
}

/// Lettura per la scopa: mano e carte in tavola, riconosciute (None se il tavolo non si vede).
pub fn read_scopa(img: &Img, mem: &Mutex<Memory>) -> Option<ScopaScene> {
    let profile = std::env::var_os("BRIS_PROFILO_CARTE").is_some();
    let started = std::time::Instant::now();
    let detected = detect::scopa(img);
    if profile { eprintln!("profilo scopa rilevamento {} us", started.elapsed().as_micros()); }
    let classify_at = std::time::Instant::now();
    let mut scene = detected?;
    let mem = mem.lock().unwrap_or_else(|e| e.into_inner()).isolated();
    for v in scene.hand.iter_mut().chain(&mut scene.table) {
        recognize(v, &mem);
    }
    if profile { eprintln!("profilo scopa classificazione {} us", classify_at.elapsed().as_micros()); }
    Some(scene)
}

/// Le carte di una scena: dove stanno nella schermata (frazioni), le tue per prime.
pub trait Located {
    /// Tutte le carte visibili, inseguite singolarmente fra i riconoscimenti.
    fn spots(&self) -> Vec<[f32; 4]>;
    fn refine_native(&mut self, _img: &Img) {}
    fn apply_zones(&mut self, _area: screen::Area, _geom: capture::Geom) {}

}

impl Located for Scene {
    fn apply_zones(&mut self, area: screen::Area, geom: capture::Geom) {
        zones::apply(self, area, geom);
    }
    fn spots(&self) -> Vec<[f32; 4]> {
        self.hand
            .iter()
            .chain(self.trump.iter())
            .chain(self.opp_slot.iter())
            .chain(self.my_slot.iter())
            .map(|v| v.at)
            .collect()
    }
    fn refine_native(&mut self, img: &Img) {
        for card in self.hand.iter_mut().chain(&mut self.opp_slot).chain(&mut self.my_slot) {
            refine_small_crop(card, img);
        }
    }
}

impl Located for ScopaScene {
    fn spots(&self) -> Vec<[f32; 4]> {
        self.hand.iter().chain(self.table.iter()).map(|v| v.at).collect()
    }
    fn refine_native(&mut self, img: &Img) {
        for card in self.hand.iter_mut().chain(&mut self.table) {
            refine_small_crop(card, img);
        }
    }
}

fn refine_small_crop(card: &mut CardView, frame: &Img) {
    if card.partial || card.px[1] > 80.0
        || card.best(CardSet::FULL).is_none_or(|(_, margin)| margin >= 1.4) {
        return;
    }
    let [l, t, r, b] = card.at;
    if l < 0.0 || t < 0.0 || r > 1.0 || b > 1.0 || r <= l || b <= t { return; }
    let w = frame.w as f32;
    let h = frame.h as f32;
    let ratio = (r - l) * w / ((b - t) * h);
    if !(0.52..=0.76).contains(&ratio) { return; }
    let rect = detect::RotRect { cx: (l+r)*w/2.0, cy: (t+b)*h/2.0,
        u: (1.0, 0.0), v: (0.0, 1.0), hw: (r-l)*w/2.0, hh: (b-t)*h/2.0 };
    let img = detect::warp(frame, &rect);
    let (sim, styles) = reference::Match::with_cols(&img, WHOLE, WHOLE).refine_styles();
    let Some(id) = reference::agreeing_card(&styles) else { return };
    let (old, _) = card.best(CardSet::FULL).unwrap();
    let top = card.scores[old as usize];
    if sim[id] >= 0.88 && top - card.scores[id] < 0.40 {
        if id != old as usize { card.scores[id] = top + 0.15; }
        card.img = img;
    }
}

/// Dove è stata presa una schermata: il monitor e l'area letta.
type Place = (capture::Geom, screen::Area);

/// Dove stanno le carte lette: il posto della schermata analizzata (il monitor dove sono state
/// viste davvero, non quello che si sta provando) e di quanto si sono spostate da allora.
#[derive(Default)]
struct Spot {
    capture_ok: bool,
    frame_id: u64,
    captured: Option<std::time::Instant>,
    tracked: Option<std::time::Instant>,
    place: Option<Place>,
    template: Option<track::Template>,
    shift: [f32; 2],
    cards: Vec<live::CardTrack>,
    /// Catture di fila in cui la zona delle carte non si ritrova: le carte non ci sono più (o lo
    /// schermo è cambiato) e non vanno più segnate finché un'analisi non le ritrova.
    misses: u32,
}

/// Dopo quante catture senza ritrovare le carte non si segnano più.
const MISSES: u32 = 2;
const SIGHT_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(2);

/// One immutable coordinate system for all cards in a displayed scene.
#[derive(Clone)]
pub struct Placement {
    pub monitor: Option<capture::Geom>,
    pub hidden: bool,
    place: Option<Place>,
    shift: [f32; 2],
    cards: Vec<([f32; 4], Option<[f32; 4]>)>,
}

impl Placement {
    pub fn on_desktop(&self, at: [f32; 4]) -> Option<[f32; 4]> {
        if self.hidden {
            return None;
        }
        let (g, area) = self.place?;
        if !self.cards.is_empty() {
            let (_, current) =
                self.cards.iter().find(|(source, _)| source.iter().zip(at).all(|(a, b)| (*a - b).abs() < 0.00001))?;
            return Some(place_on_desktop((*current)?, g, area));
        }
        let [dx, dy] = self.shift;
        Some(place_on_desktop([at[0] + dx, at[1] + dy, at[2] + dx, at[3] + dy], g, area))
    }
}

impl Spot {
    fn placement(&self, frame_id: u64, now: std::time::Instant) -> Placement {
        let fresh =
            |at: Option<std::time::Instant>| at.is_some_and(|t| now.saturating_duration_since(t) < SIGHT_MAX_AGE);
        Placement {
            monitor: self.place.map(|p| p.0),
            hidden: !self.capture_ok
                || self.frame_id != frame_id
                || self.misses >= MISSES
                || !fresh(self.captured)
                || !fresh(self.tracked),
            place: self.place,
            shift: self.shift,
            cards: self.cards.iter().map(|c| (c.source, c.current)).collect(),
        }
    }
}

/// Legge lo schermo in un thread a parte, qualche volta al secondo, finché non viene fermato.
pub struct Reader<S> {
    pub rx: Receiver<Reading<S>>,
    stop: Arc<AtomicBool>,
    lag_ms: Arc<AtomicU64>,
    last_result: Arc<Mutex<std::time::Instant>>,
    /// Quando è stata catturata la più vecchia schermata cambiata non ancora letta (in coda o in
    /// analisi): lo schermo fermo non rende vecchia la lettura.
    work: Arc<Mutex<Option<std::time::Instant>>>,
    last: Arc<Mutex<Option<Img>>>,
    /// Dove stanno le carte dell'ultima scena sul desktop.
    spot: Arc<Mutex<Spot>>,
}

/// Ogni quanto si cattura lo schermo: spesso, perché costa poco. Le schermate si analizzano poi
/// tutte, in ordine, anche se l'analisi è più lenta: un sito che chiude una presa in mezzo secondo
/// non deve far perdere nessuna carta.
const CAPTURE_EVERY: std::time::Duration = std::time::Duration::from_millis(16);

impl<S: Located + Send + Clone + 'static> Reader<S> {
    /// `read` trova e riconosce le carte di un gioco; `every` è l'intervallo massimo fra due letture
    /// quando lo schermo non cambia.
    pub fn start(
        mem: Arc<Mutex<Memory>>,
        ctx: eframe::egui::Context,
        every: std::time::Duration,
        read: fn(&Img, &Mutex<Memory>) -> Option<S>,
    ) -> Reader<S> {
        Self::start_with_capture(mem, ctx, every, read, capture::grab_geom)
    }

    fn start_with_capture(
        mem: Arc<Mutex<Memory>>,
        ctx: eframe::egui::Context,
        every: std::time::Duration,
        read: fn(&Img, &Mutex<Memory>) -> Option<S>,
        capture: fn(usize) -> Result<(Img, usize, capture::Geom), String>,
    ) -> Reader<S> {
        let (tx, rx) = sync_channel(16);
        let stop = Arc::new(AtomicBool::new(false));
        let lag_ms = Arc::new(AtomicU64::new(0));
        let last_result = Arc::new(Mutex::new(std::time::Instant::now()));
        let work = Arc::new(Mutex::new(None::<std::time::Instant>));
        let last = Arc::new(Mutex::new(None::<Img>));
        type FrameInfo = (f32, Option<Place>, u64, Option<Arc<Img>>);
        let queue: Arc<(Mutex<Frames<FrameInfo>>, Condvar)> =
            Arc::new((Mutex::new(Frames::new(every)), Condvar::new()));
        // Quanto sono lunghe le carte sullo schermo, in pixel veri (0 se non si sa ancora).
        let card_px = Arc::new(std::sync::atomic::AtomicU32::new(0));
        // Monitor da catturare (l'analisi passa al successivo se non trova il tavolo) e errori.
        let monitor = Arc::new(AtomicUsize::new(screen::area().lock().unwrap().monitor));
        let monitors = Arc::new(AtomicUsize::new(1));
        let (err_tx, err_rx) = sync_channel::<String>(1);
        let spot: Arc<Mutex<Spot>> = Arc::new(Mutex::new(Spot::default()));
        let latest = Arc::new(Mutex::new(None::<(Arc<Img>, Place, std::time::Instant)>));

        // Cattura: ogni CAPTURE_EVERY, e tiene le schermate in cui qualcosa è cambiato.
        {
            let (flag, queue, monitor, monitors, card_px) =
                (stop.clone(), queue.clone(), monitor.clone(), monitors.clone(), card_px.clone());
            let spot = spot.clone();
            let ctx = ctx.clone();
            let latest = latest.clone();
            let work = work.clone();
            std::thread::spawn(move || {
                let mut chosen = None;
                let mut frame_id = 0u64;
                let mut scale = 1.0f32;
                while !flag.load(Ordering::Relaxed) {
                    let t0 = std::time::Instant::now();
                    let area = *screen::area().lock().unwrap_or_else(|e| e.into_inner());
                    if chosen != Some(area) {
                        // Area appena scelta: si legge il suo monitor.
                        monitor.store(area.monitor, Ordering::Relaxed);
                        chosen = Some(area);
                    }
                    match capture(monitor.load(Ordering::Relaxed)) {
                        Ok((img, count, geom)) => {
                            let captured = std::time::Instant::now();
                            if std::env::var_os("BRIS_PROFILO_CARTE").is_some() {
                                eprintln!("profilo cattura {} us", captured.duration_since(t0).as_micros());
                            }
                            spot.lock().unwrap_or_else(|e| e.into_inner()).capture_ok = true;
                            monitors.store(count.max(1), Ordering::Relaxed);
                            let table = Arc::new(screen::table(&img, &area));
                            *latest.lock().unwrap_or_else(|e| e.into_inner()) =
                                Some((table.clone(), (geom, area), captured));
                            follow(&spot, &table, (geom, area), captured);
                            ctx.request_repaint();
                            let (reduced, k) = for_reading((*table).clone(), card_px.load(Ordering::Relaxed), scale);
                            scale = k;
                            let (lock, cv) = &*queue;
                            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
                            // Conserva l'originale solo entro il budget della coda. Una
                            // cattura più recente non è un sostituto per questa.
                            let native = if k > 1.0 {
                                let buffered: usize = q.frames.iter()
                                    .filter_map(|(_, _, info, _, _)| info.3.as_ref())
                                    .map(|image| image.px.len()).sum();
                                (buffered + table.px.len() <= 24_000_000).then_some(table)
                            } else { None };
                            frame_id += 1;
                            if q.offer_with(reduced, (k, Some((geom, area)), frame_id, native), captured) {
                                let mut w = work.lock().unwrap_or_else(|e| e.into_inner());
                                if w.is_none() {
                                    *w = q.oldest_changed();
                                }
                                drop(w);
                                cv.notify_one();
                            }
                        }
                        Err(e) => {
                            {
                                let mut sp = spot.lock().unwrap_or_else(|e| e.into_inner());
                                sp.capture_ok = false;
                                sp.misses = MISSES;
                            }
                            let _ = err_tx.try_send(e);
                            queue.1.notify_one();
                            ctx.request_repaint();
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                    }
                    std::thread::sleep(CAPTURE_EVERY.saturating_sub(t0.elapsed()));
                }
                // Sveglia l'analisi perché si fermi anche lei.
                queue.1.notify_all();
            });
        }

        // Analisi: le schermate tenute, una dopo l'altra.
        let (flag, frame) = (stop.clone(), last.clone());
        let found = spot.clone();
        let age = lag_ms.clone();
        let completed = last_result.clone();
        let working = work.clone();
        std::thread::spawn(move || {
            // L'ultima schermata analizzata e cosa ci si era letto: una schermata
            // uguale non si analizza di nuovo (al più otto volte di fila: quello che l'app impara
            // dalle conferme può cambiare la lettura).
            let mut seen: Option<(Option<S>, u32)> = None;
            let mut last_place = None;
            // Le zone delle carte delle ultime letture: la cattura guarda i cambi solo lì.
            let mut boxes: std::collections::VecDeque<[f32; 4]> = Default::default();
            let mut empty = 0u32;
            while !flag.load(Ordering::Relaxed) {
                let reading = match wait_frame(&queue, &flag, &err_rx) {
                    Err(error) => Reading::Error(error),
                    Ok(None) => { flag.store(true, Ordering::Relaxed); break; }
                    Ok(Some((table, (k, at, frame_id, native), captured, _))) => {
                        // Un errore nella lettura di una schermata non deve fermare le letture
                        // dopo: si mostra e si continua.
                        if last_place != Some((at, k)) {
                            last_place = Some((at, k));
                            seen = None;
                            boxes.clear();
                            forget_focus();
                        }
                        // Due facce possono avere la stessa luminosità media in ogni cella.
                        // Riusa una lettura solo per pixel identici alla stessa risoluzione.
                        let same = k == 1.0 && seen.as_ref().is_some_and(|(_, n)| *n < 8)
                            && frame.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
                                .is_some_and(|old| old.w == table.w && old.h == table.h && old.px == table.px);
                        let scene = if same {
                            let (result, repeats) = seen.as_mut().unwrap();
                            *repeats += 1;
                            Ok(result.clone())
                        } else {
                            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                let mut scene = read(&table, &mem)?;
                                if let Some(native) = native {
                                    scene.refine_native(&native);
                                }
                                Some(scene)
                            }));
                            seen = r.as_ref().ok().map(|s| (s.clone(), 0));
                            r
                        };
                        // La misura delle carte decide quanto rimpicciolire le schermate dopo.
                        card_px.store((detect::last_card_l() * k).round() as u32, Ordering::Relaxed);
                        *frame.lock().unwrap_or_else(|e| e.into_inner()) = Some(table.clone());
                        age.store(captured.elapsed().as_millis().min(u64::MAX as u128) as u64, Ordering::Relaxed);
                        match scene {
                            Err(e) => {
                                found.lock().unwrap_or_else(|e| e.into_inner()).misses = MISSES;
                                let why = e
                                    .downcast_ref::<&str>()
                                    .map(|s| s.to_string())
                                    .or_else(|| e.downcast_ref::<String>().cloned())
                                    .unwrap_or_default();
                                Reading::Error(format!("errore interno ({why})"))
                            }
                            Ok(Some(mut s)) => {
                                if let Some((geom, area)) = at {
                                    s.apply_zones(area, geom);
                                }
                                // Le carte sono su questo monitor: la mosca va qui, e da qui si
                                // insegue la zona delle carte nelle catture che seguono.
                                let mut sp = found.lock().unwrap_or_else(|e| e.into_inner());
                                if at.is_some() {
                                    sp.place = at;
                                }
                                sp.frame_id = frame_id;
                                sp.captured = Some(captured);
                                // Analysis may lag capture; never revive an old frame as fresh.
                                sp.tracked = Some(captured);
                                let spots = s.spots();
                                if let Some(b) = spots.iter().copied().reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]) {
                                    empty = 0;
                                    boxes.push_back(b);
                                    if boxes.len() > 40 {
                                        boxes.pop_front();
                                    }
                                    if boxes.len() >= 10 {
                                        let u = boxes.iter().copied().reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]).unwrap();
                                        let card = spots.iter().map(|r| r[3] - r[1]).fold(0.0f32, f32::max).max(0.05);
                                        let focus = [(u[0] - card).max(0.0), (u[1] - card).max(0.0), (u[2] + card).min(1.0), (u[3] + card).min(1.0)];
                                        let q = |v: f32| (v * 32.0).round() / 32.0;
                                        queue.0.lock().unwrap_or_else(|e| e.into_inner()).set_focus(Some(focus.map(q)));
                                    }
                                }
                                sp.cards = spots.iter().filter_map(|&r| live::CardTrack::new(&table, r)).collect();
                                sp.template = track::union(&spots).and_then(|u| track::Template::new(&table, u));
                                sp.shift = [0.0, 0.0];
                                sp.misses = 0;
                                drop(sp);
                                // Catch up before publishing: a slow recognition result must not
                                // rewind the contour to where the card was captured earlier.
                                let live = latest.lock().unwrap_or_else(|e| e.into_inner()).clone();
                                if let Some((image, place, captured)) = live {
                                    follow(&found, &image, place, captured);
                                }
                                Reading::Scene(s, frame_id)
                            }
                            Ok(None) => {
                                // Niente carte per un po': si torna a guardare tutto lo schermo.
                                empty += 1;
                                if empty >= 5 {
                                    boxes.clear();
                                    queue.0.lock().unwrap_or_else(|e| e.into_inner()).set_focus(None);
                                }
                                found.lock().unwrap_or_else(|e| e.into_inner()).misses = MISSES;
                                // Senza un'area scelta, prova il monitor successivo.
                                if screen::area().lock().unwrap_or_else(|e| e.into_inner()).rect.is_none() {
                                    let n = monitors.load(Ordering::Relaxed).max(1);
                                    monitor.store((monitor.load(Ordering::Relaxed) + 1) % n, Ordering::Relaxed);
                                }
                                Reading::NoTable
                            }
                        }
                    }
                };
                *completed.lock().unwrap_or_else(|e| e.into_inner()) = std::time::Instant::now();
                {
                    // Letta: resta indietro solo quello che è ancora in coda.
                    let q = queue.0.lock().unwrap_or_else(|e| e.into_inner());
                    *working.lock().unwrap_or_else(|e| e.into_inner()) = q.oldest_changed();
                }
                // Bounded backpressure preserves event order without unbounded memory.
                let mut message = reading;
                loop {
                    match tx.try_send(message) {
                        Ok(()) => break,
                        Err(TrySendError::Full(m)) if !flag.load(Ordering::Relaxed) => {
                            message = m;
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(_) => {
                            flag.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }
                ctx.request_repaint();
            }
        });
        Reader { rx, stop, lag_ms, last_result, work, last, spot }
    }
}

impl<S> Reader<S> {
    /// Età della schermata quando termina il riconoscimento, compresa l'attesa in coda.
    /// Quanto la lettura è indietro rispetto allo schermo: da quanto aspetta la più vecchia
    /// schermata cambiata non ancora letta. Lo schermo fermo non è un ritardo; un'analisi che non
    /// dà più nessun risultato sì.
    pub fn lag(&self) -> std::time::Duration {
        let waiting = self.work.lock().unwrap_or_else(|e| e.into_inner()).map_or(std::time::Duration::ZERO, |t| t.elapsed());
        let silent = self.last_result.lock().unwrap_or_else(|e| e.into_inner()).elapsed();
        waiting.max(if silent >= JAMMED { silent } else { std::time::Duration::ZERO })
    }

    /// L'età della schermata dell'ultima lettura quando è stata letta.
    #[allow(dead_code)]
    pub fn read_age(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.lag_ms.load(Ordering::Relaxed))
    }

    pub fn placement(&self, frame_id: u64) -> Placement {
        self.spot.lock().unwrap_or_else(|e| e.into_inner()).placement(frame_id, std::time::Instant::now())
    }

    /// L'ultima schermata letta (già ritagliata sul tavolo), per la diagnosi.
    pub fn last_frame(&self) -> Option<Img> {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Nella cattura appena fatta ritrova la zona delle carte dell'ultima scena, se è dello stesso
/// monitor e della stessa area: così le carte segnate seguono lo schermo che si sposta.
fn follow(spot: &Mutex<Spot>, table: &Img, at: Place, captured: std::time::Instant) {
    let (frame_id, template, hint, mut cards) = {
        let sp = spot.lock().unwrap_or_else(|e| e.into_inner());
        if sp.place != Some(at) {
            drop(sp);
            spot.lock().unwrap_or_else(|e| e.into_inner()).misses = MISSES;
            return;
        }
        if sp.tracked.is_some_and(|at| at > captured) {
            return;
        }
        (sp.frame_id, sp.template.clone(), sp.shift, sp.cards.clone())
    };
    for card in &mut cards {
        card.follow(table, hint);
    }
    let mut shift = hint;
    let any = cards.iter().any(|c| c.current.is_some());
    if !any {
        // A large whole-window jump can exceed the per-card local search radius.
        if let Some(found) = template.as_ref().and_then(|t| t.find(table, hint)) {
            shift = found;
            for card in &mut cards {
                card.follow(table, shift);
            }
        }
    }
    let mut sp = spot.lock().unwrap_or_else(|e| e.into_inner());
    if sp.frame_id == frame_id && sp.place == Some(at) && !sp.tracked.is_some_and(|at| at > captured) {
        let found = cards.iter().any(|c| c.current.is_some());
        sp.cards = cards;
        sp.shift = shift;
        if found {
            sp.misses = 0;
            sp.tracked = Some(captured);
        } else {
            sp.misses += 1;
        }
    }
}

/// Da frazioni della schermata letta (ritagliata sull'area) a pixel del desktop.
pub fn place_on_desktop(at: [f32; 4], g: capture::Geom, area: screen::Area) -> [f32; 4] {
    let [l, t, r, b] = area.rect.unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let x = |f: f32| g.x + (l + f * (r - l)) * g.w;
    let y = |f: f32| g.y + (t + f * (b - t)) * g.h;
    [x(at[0]), y(at[1]), x(at[2]), y(at[3])]
}

/// Le schermate in attesa di analisi: si tiene una schermata quando è cambiata rispetto all'ultima
/// tenuta (o ogni tanto anche se non cambia). Se l'analisi resta indietro, si scartano prima
/// le schermate che differiscono poco da una vicina nella coda.
pub struct Frames<T = ()> {
    /// Schermata, impronta, dato attaccato, quando è stata catturata e se era cambiata.
    frames: std::collections::VecDeque<(Img, Vec<u8>, T, std::time::Instant, bool)>,
    last: Option<Vec<u8>>,
    pushed: Option<std::time::Instant>,
    every: std::time::Duration,
    /// La zona delle carte (frazioni): i cambi fuori (orologi, chat, pubblicità) non contano.
    focus: Option<[f32; 4]>,
}

/// Quante schermate al più restano in attesa.
const FRAMES: usize = 8;
/// Senza nessun risultato per tanto, l'analisi è bloccata.
const JAMMED: std::time::Duration = std::time::Duration::from_secs(6);

#[cfg(test)]
impl Frames {
    /// Offre una schermata appena catturata; vero se è stata tenuta.
    #[cfg(test)]
    pub fn offer(&mut self, img: Img, now: std::time::Instant) -> bool {
        self.offer_with(img, (), now)
    }

    /// La prossima schermata da analizzare.
    pub fn next(&mut self) -> Option<Img> {
        self.next_with().map(|f| f.0)
    }
}

impl<T> Frames<T> {
    pub fn new(every: std::time::Duration) -> Frames<T> {
        Frames { frames: Default::default(), last: None, pushed: None, every, focus: None }
    }

    /// Come `offer`, con un dato attaccato alla schermata.
    /// Guarda i cambi solo nella zona `focus` (frazioni dello schermo; `None`: tutto). Una
    /// schermata ferma si offre comunque a intervalli regolari, quindi niente si perde del tutto.
    pub fn set_focus(&mut self, focus: Option<[f32; 4]>) {
        if self.focus != focus {
            self.focus = focus;
            self.last = None;
        }
    }

    pub fn offer_with(&mut self, img: Img, tag: T, now: std::time::Instant) -> bool {
        let sig = match self.focus {
            Some(r) => {
                let (w, h) = (img.w as f32, img.h as f32);
                signature(&img.crop((r[0] * w) as usize, (r[1] * h) as usize, (r[2] * w).ceil() as usize, (r[3] * h).ceil() as usize))
            }
            None => signature(&img),
        };
        // Cambiata: nel complesso, o anche solo in un pezzo piccolo (una carta su un monitor 4K
        // con il gioco in finestra cambia poche celle dell'impronta, ma tanto).
        let changed = self.last.as_ref().is_none_or(|l| difference(l, &sig) >= 1.5 || changed_cells(l, &sig) >= 2);
        let stale = self.pushed.is_none_or(|t| now.duration_since(t) >= self.every);
        if !changed && !stale {
            return false;
        }
        if self.frames.len() >= FRAMES {
            // Il primo e l'ultimo fotogramma restano in coda. Tra quelli intermedi, si
            // privilegiano i cambiamenti locali e, a parità, le schermate durate più a lungo.
            let lasted = |i: usize| self.frames[i + 1].3.duration_since(self.frames[i].3);
            let drop = (1..self.frames.len() - 1)
                .min_by(|&a, &b| {
                    let d = |i: usize| difference(&self.frames[i - 1].1, &self.frames[i].1);
                    // Un fotogramma senza cambiamenti significativi rispetto a uno dei due
                    // vicini è il primo da scartare, anche se è rimasto visibile più a lungo.
                    let distinct = |i: usize| {
                        let current = &self.frames[i].1;
                        changed_cells(&self.frames[i - 1].1, current) >= 2
                            && changed_cells(current, &self.frames[i + 1].1) >= 2
                    };
                    distinct(a).cmp(&distinct(b))
                        .then(lasted(a).cmp(&lasted(b)))
                        .then(d(a).total_cmp(&d(b)))
                })
                .unwrap_or(0);
            self.frames.remove(drop);
        }
        self.last = Some(sig.clone());
        self.pushed = Some(now);
        self.frames.push_back((img, sig, tag, now, changed));
        true
    }

    /// Quando è stata catturata la più vecchia schermata cambiata ancora da analizzare.
    pub fn oldest_changed(&self) -> Option<std::time::Instant> {
        self.frames.iter().find(|f| f.4).map(|f| f.3)
    }

    /// La prossima schermata da analizzare, con il suo dato.
    pub fn next_with(&mut self) -> Option<(Img, T)> {
        self.next_timed().map(|(img, tag, _, _)| (img, tag))
    }

    fn next_timed(&mut self) -> Option<(Img, T, std::time::Instant, bool)> {
        self.frames.pop_front().map(|f| (f.0, f.2, f.3, f.4))
    }
}

/// L'attesa di un fotogramma deve terminare anche quando la cattura fallisce:
/// altrimenti gli errori restano nel canale e l'interfaccia aspetta in silenzio.
fn wait_frame<T>(
    queue: &(Mutex<Frames<T>>, Condvar), stop: &AtomicBool,
    errors: &Receiver<String>,
) -> Result<Option<(Img, T, std::time::Instant, bool)>, String> {
    let (lock, cv) = queue;
    let mut frames = lock.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if let Some(frame) = frames.next_timed() { return Ok(Some(frame)); }
        match errors.try_recv() {
            Ok(error) => return Err(error),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Ok(None),
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
        if stop.load(Ordering::Relaxed) { return Ok(None); }
        frames = cv.wait_timeout(frames, std::time::Duration::from_millis(500)).unwrap_or_else(|e| e.into_inner()).0;
    }
}

/// Quante celle dell'impronta sono cambiate molto (una carta che compare o sparisce).
fn changed_cells(a: &[u8], b: &[u8]) -> usize {
    if a.len() != b.len() {
        return a.len().max(b.len());
    }
    a.chunks_exact(3).zip(b.chunks_exact(3)).filter(|(x, y)|
        x.iter().zip(*y).any(|(&a, &b)| a.abs_diff(b) >= 24)
    ).count()
}

/// Impronta di una schermata: il colore medio su una griglia 48 x 32.
pub fn signature(img: &Img) -> Vec<u8> {
    let (gw, gh) = (48usize, 32usize);
    let mut out = Vec::with_capacity(gw * gh * 3);
    for j in 0..gh {
        for i in 0..gw {
            let (x0, x1) = (i * img.w / gw, ((i + 1) * img.w / gw).max(i * img.w / gw + 1).min(img.w));
            let (y0, y1) = (j * img.h / gh, ((j + 1) * img.h / gh).max(j * img.h / gh + 1).min(img.h));
            let (mut sum, mut n) = ([0u32; 3], 0u32);
            // Un punto ogni due per lato basta.
            for y in (y0..y1).step_by(2) {
                for x in (x0..x1).step_by(2) {
                    let p = img.at(x, y);
                    for (channel, value) in sum.iter_mut().zip(p) { *channel += u32::from(value); }
                    n += 1;
                }
            }
            out.extend(sum.map(|v| (v / n.max(1)) as u8));
        }
    }
    out
}

/// Differenza media fra due impronte (0-255).
pub fn difference(a: &[u8], b: &[u8]) -> f32 {
    if a.len() != b.len() {
        return 255.0;
    }
    a.chunks_exact(3).zip(b.chunks_exact(3)).map(|(x, y)| {
        x.iter().zip(y).map(|(&a, &b)| a.abs_diff(b)).max().unwrap_or(0) as u32
    }).sum::<u32>() as f32 / (a.len() / 3).max(1) as f32
}

/// Le schermate molto grandi (monitor 4K) si rimpiccioliscono, così l'analisi è molto più
/// veloce, ma solo se le carte restano grandi abbastanza da leggerle: con il gioco in una finestra
/// di uno schermo grande le carte sono piccole e la schermata si legge intera.
///
/// Di quanto rimpicciolire una schermata larga `w` con carte lunghe `card_px` pixel (0 se non si
/// sa ancora: allora si legge intera finché non si vedono le carte).
pub fn shrink_factor(w: usize, card_px: u32) -> usize {
    w.div_ceil(2000).min(card_px as usize / MIN_CARD_PX).max(1)
}

/// Le carte, nella schermata rimpicciolita, restano lunghe almeno tanto.
const MIN_CARD_PX: usize = 110;

/// Con le carte più corte di tanto (pixel dello schermo) la schermata si ingrandisce prima di
/// leggerla: il rilevamento è fatto per carte di un centinaio di pixel, e sulle carte piccole
/// perde bordi e angoli (docs/SPERIMENTAZIONE-RICONOSCIMENTO.md §3.8).
const GROW_BELOW: u32 = 75;

/// Le carte ingrandite diventano lunghe tanto (al più il doppio): misurato meglio di 110 (§3.8).
const GROW_TO: f32 = 150.0;

/// Al più tanti pixel nella schermata ingrandita (la lettura resta veloce).
const GROW_MAX_PX: f32 = 3_200_000.0;

/// Di quanto ingrandire una schermata `w` x `h` con carte lunghe `card_px` pixel (1 se non serve,
/// o se non si sa ancora quanto sono grandi le carte). `grown`: la schermata prima era ingrandita
/// (allora si smette solo con le carte un po' più lunghe, così non si cambia a ogni lettura).
pub fn grow_factor(w: usize, h: usize, card_px: u32, grown: bool) -> f32 {
    let below = if cfg!(test) {
        if std::env::var_os("SENZA_INGRANDIRE").is_some() {
            return 1.0;
        }
        std::env::var("INGRANDISCI_SOTTO").ok().and_then(|v| v.parse().ok()).unwrap_or(GROW_BELOW)
    } else {
        GROW_BELOW
    };
    if card_px == 0 || card_px >= below + if grown { 10 } else { 0 } {
        return 1.0;
    }
    let target = if cfg!(test) { std::env::var("INGRANDISCI_A").ok().and_then(|v| v.parse().ok()).unwrap_or(GROW_TO) } else { GROW_TO };
    let f = (target / card_px as f32).min(2.0).min((GROW_MAX_PX / (w * h).max(1) as f32).sqrt());
    if f < 1.15 { 1.0 } else { f }
}

/// La schermata alla grandezza per leggerla: rimpicciolita se è molto grande (`shrink_factor`),
/// ingrandita se le carte sono piccole (`grow_factor`). Con quanti pixel dello schermo fanno un
/// pixel della schermata da leggere (più di 1 se rimpicciolita, meno se ingrandita). `was`: lo
/// stesso per la schermata prima.
pub fn for_reading(img: Img, card_px: u32, was: f32) -> (Img, f32) {
    let k = shrink_factor(img.w, card_px);
    if k > 1 {
        return (shrink_by(img, k), k as f32);
    }
    let f = grow_factor(img.w, img.h, card_px, was < 1.0);
    if f > 1.0 {
        (grow_by(&img, f), 1.0 / f)
    } else {
        (img, 1.0)
    }
}

/// Ingrandisce di `f` volte per lato (interpolazione lineare): con le carte piccole il
/// rilevamento lavora su carte della grandezza per cui è fatto.
pub fn grow_by(img: &Img, f: f32) -> Img {
    let (w, h) = ((img.w as f32 * f) as usize, (img.h as f32 * f) as usize);
    let mut out = Img::new(w, h);
    for y in 0..h {
        for x in 0..w {
            out.px[y * w + x] = img.sample((x as f32 + 0.5) / f - 0.5, (y as f32 + 0.5) / f - 0.5);
        }
    }
    out
}

/// Rimpicciolisce di `k` volte per lato.
pub fn shrink_by(img: Img, k: usize) -> Img {
    if k <= 1 {
        return img;
    }
    let (w, h) = (img.w / k, img.h / k);
    let mut out = Img::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 3];
            for dy in 0..k {
                for dx in 0..k {
                    let p = img.at(x * k + dx, y * k + dy);
                    for c in 0..3 {
                        acc[c] += p[c] as u32;
                    }
                }
            }
            out.px[y * w + x] = acc.map(|a| (a / (k * k) as u32) as u8);
        }
    }
    out
}

#[cfg(test)]
impl<S> Reader<S> {
    /// Canale finito per riprodurre ritardi dell'interfaccia senza catturare il desktop.
    pub(crate) fn from_readings(readings: impl IntoIterator<Item = Reading<S>>) -> Self {
        let (tx, rx) = channel();
        for reading in readings { tx.send(reading).ok().unwrap(); }
        Reader {
            rx, stop: Arc::new(AtomicBool::new(false)), lag_ms: Arc::new(AtomicU64::new(0)), last_result: Arc::new(Mutex::new(std::time::Instant::now())),
            work: Arc::new(Mutex::new(None)), last: Arc::new(Mutex::new(None)), spot: Arc::new(Mutex::new(Spot::default())),
        }
    }
}

#[cfg(test)]
impl<S> Reader<S> {
    pub fn with_readings(readings: Vec<Reading<S>>, frame_id: u64) -> Self {
        let (tx, rx) = sync_channel(readings.len().max(1));
        for r in readings {
            tx.try_send(r).ok().unwrap();
        }
        let now = std::time::Instant::now();
        let spot = Spot {
            capture_ok: true,
            frame_id,
            captured: Some(now),
            tracked: Some(now),
            place: Some((
                capture::Geom { x: -1600.0, y: 0.0, w: 1600.0, h: 900.0, scale: 1.25 },
                screen::Area::default(),
            )),
            ..Default::default()
        };
        Self {
            rx,
            stop: Arc::new(AtomicBool::new(false)),
            lag_ms: Arc::new(AtomicU64::new(0)),
            last_result: Arc::new(Mutex::new(std::time::Instant::now())),
            work: Arc::new(Mutex::new(None)),
            last: Arc::new(Mutex::new(None)),
            spot: Arc::new(Mutex::new(spot)),
        }
    }
}

impl<S> Drop for Reader<S> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use briscola_core::cards;

    #[test]
    fn placement_requires_matching_fresh_frame_and_healthy_capture() {
        let now = std::time::Instant::now();
        let reader = Reader::<Scene>::with_readings(vec![], 7);
        let mut spot = reader.spot.lock().unwrap();
        assert!(!spot.placement(7, now).hidden);
        assert!(spot.placement(6, now).hidden);
        let placement = spot.placement(7, now);
        let card = placement.on_desktop([0.25, 0.25, 0.5, 0.5]).unwrap();
        assert_eq!(card, [-1200.0, 225.0, -800.0, 450.0]);
        spot.shift = [0.1, 0.0];
        // Previously acquired placement stays immutable even while the worker advances.
        assert_eq!(placement.on_desktop([0.25, 0.25, 0.5, 0.5]), Some(card));
        assert!(spot.placement(7, now + SIGHT_MAX_AGE + std::time::Duration::from_millis(1)).hidden);
        spot.capture_ok = false;
        assert!(spot.placement(7, now).on_desktop([0.0; 4]).is_none());
    }

    #[test]
    fn placement_maps_independent_cards_and_rejects_older_live_frames() {
        let image = site();
        let scene = read(&image);
        assert!(!scene.hand.is_empty());
        let rect = scene.hand[0].at;
        let mut moved = Img::new(image.w, image.h);
        for y in 0..image.h - 7 {
            let source = &image.px[y * image.w..(y + 1) * image.w - 13];
            let start = (y + 7) * image.w + 13;
            moved.px[start..start + source.len()].copy_from_slice(source);
        }
        let now = std::time::Instant::now();
        let geom = capture::Geom { x: -1600.0, y: 0.0, w: image.w as f32, h: image.h as f32, scale: 1.25 };
        let place = (geom, screen::Area::default());
        let spot = Mutex::new(Spot {
            frame_id: 42,
            captured: Some(now),
            tracked: Some(now),
            capture_ok: true,
            place: Some(place),
            cards: scene.hand.iter().filter_map(|v| live::CardTrack::new(&image, v.at)).collect(),
            ..Default::default()
        });
        let later = now + std::time::Duration::from_millis(16);
        follow(&spot, &moved, place, later);
        let measured = spot.lock().unwrap().placement(42, later).on_desktop(rect).unwrap();
        let expected = place_on_desktop(
            [
                rect[0] + 13.0 / image.w as f32,
                rect[1] + 7.0 / image.h as f32,
                rect[2] + 13.0 / image.w as f32,
                rect[3] + 7.0 / image.h as f32,
            ],
            geom,
            place.1,
        );
        assert!(measured.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1.1), "{measured:?} vs {expected:?}");
        // A delayed recognizer/capture worker cannot rewind a newer live position.
        follow(&spot, &image, place, now);
        assert_eq!(spot.lock().unwrap().placement(42, later).on_desktop(rect), Some(measured));
    }

    #[test]
    fn capture_errors_wake_the_reader_without_a_frame() {
        fn failed(_: usize) -> Result<(Img, usize, capture::Geom), String> {
            Err("capture disconnected".into())
        }
        let reader = Reader::start_with_capture(
            Arc::new(Mutex::new(Memory::in_memory())),
            eframe::egui::Context::default(),
            std::time::Duration::from_millis(100),
            read_briscola,
            failed,
        );
        assert!(
            matches!(reader.rx.recv_timeout(std::time::Duration::from_secs(3)), Ok(Reading::Error(e)) if e == "capture disconnected")
        );
        assert!(reader.placement(0).hidden);
    }

    fn site() -> Img {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/sito_briscola.png");
        capture::load_png(&path).unwrap()
    }

    fn read(img: &Img) -> Scene {
        read_briscola(img, &Mutex::new(Memory::in_memory())).expect("carte")
    }

    fn screenshot() -> Scene {
        read(&site())
    }

    fn best(v: &CardView) -> Card {
        v.best(CardSet::FULL).unwrap().0
    }

    #[test]
    fn learned_slots_do_not_move_a_card_from_a_distant_table() {
        forget_focus();
        let card = |x: f32, y: f32| {
            let mut view = CardView::new(Img::new(1, 1), [0.0, 1.0], [0.0; 40]);
            view.at = [x - 0.035, y - 0.07, x + 0.035, y + 0.07];
            view
        };

        // Impara il tavolo A e controlla che una carta vicina venga ancora riassegnata.
        let pair = Scene {
            opp_slot: Some(card(0.30, 0.35)),
            my_slot: Some(card(0.40, 0.55)),
            ..Scene::default()
        };
        for _ in 0..5 {
            place_table(&mut pair.clone());
        }
        let mut near = Scene {
            opp_slot: Some(card(0.40, 0.55)),
            ..Scene::default()
        };
        place_table(&mut near);
        assert!(near.opp_slot.is_none() && near.my_slot.is_some());

        // Tavolo B alla stessa risoluzione: la carta più a destra è dell'avversario.
        let mut distant = Scene {
            opp_slot: Some(card(0.70, 0.39)),
            ..Scene::default()
        };
        place_table(&mut distant);
        assert!(distant.opp_slot.is_some() && distant.my_slot.is_none());

        // Tornando al tavolo A, il campione lontano non ha modificato le posizioni.
        let mut back = Scene { opp_slot: Some(card(0.40, 0.55)), ..Scene::default() };
        place_table(&mut back);
        assert!(back.opp_slot.is_none() && back.my_slot.is_some());
    }

    #[test]
    fn capture_error_is_delivered_without_a_frame() {
        let queue = (Mutex::new(Frames::<()>::new(std::time::Duration::from_millis(350))), Condvar::new());
        let (tx, rx) = channel();
        tx.send("cattura non disponibile".to_owned()).unwrap();
        let result = wait_frame(&queue, &AtomicBool::new(false), &rx);
        assert!(matches!(result, Err(error) if error == "cattura non disponibile"));
        drop(tx);
        assert!(matches!(wait_frame(&queue, &AtomicBool::new(false), &rx), Ok(None)));
    }

    #[test]
    fn silent_reader_becomes_stale_even_without_new_messages() {
        let reader = Reader::<Scene>::from_readings(Vec::new());
        *reader.last_result.lock().unwrap() = std::time::Instant::now() - std::time::Duration::from_secs(7);
        assert!(reader.lag() >= std::time::Duration::from_secs(7));
        // Pochi secondi senza risultati con lo schermo fermo non sono un ritardo.
        *reader.last_result.lock().unwrap() = std::time::Instant::now() - std::time::Duration::from_secs(3);
        assert_eq!(reader.lag(), std::time::Duration::ZERO);
        // Una schermata cambiata in attesa da due secondi sì.
        *reader.work.lock().unwrap() = Some(std::time::Instant::now() - std::time::Duration::from_secs(2));
        assert!(reader.lag() >= std::time::Duration::from_secs(2));
    }

    #[test]
    fn queued_frames_keep_the_capture_time() {
        let mut queue = Frames::new(std::time::Duration::from_millis(350));
        let captured = std::time::Instant::now() - std::time::Duration::from_secs(3);
        queue.offer_with(Img::new(16, 16), (), captured);
        let (_, _, time, _) = queue.next_timed().unwrap();
        assert!(time.elapsed() >= std::time::Duration::from_secs(3));
    }

    #[test]
    fn dark_screen_fallback_never_panics() {
        // Include nero totale, la soglia 60 e i livelli in cui a > b nel vecchio codice.
        for value in 0..=120u8 {
            let mut img = Img::new(16, 16);
            img.px.fill([value; 3]);
            let _ = detect::Paper::darker(&img);
        }
    }

    #[test]
    fn small_cards_are_never_shrunk() {
        // Carte ancora sconosciute: si legge intera.
        assert_eq!(shrink_factor(3840, 0), 1);
        // Finestra a 100% su 2560 o 4K: carte di 110 pixel, niente rimpicciolimento.
        assert_eq!(shrink_factor(2560, 110), 1);
        assert_eq!(shrink_factor(3840, 110), 1);
        // 4K con il sito ingrandito: carte di 230 pixel, si dimezza (restano 115).
        assert_eq!(shrink_factor(3840, 230), 2);
        // Schermi piccoli: mai.
        assert_eq!(shrink_factor(1920, 400), 1);
    }

    /// Le schermate catturate spesso e analizzate lentamente: lo stato che dura una cattura sola
    /// (una presa chiusa subito) resta in coda, anche se l'analisi è indietro di molte catture.
    #[test]
    fn brief_states_are_never_dropped() {
        let t0 = std::time::Instant::now();
        let at = |ms: u64| t0 + std::time::Duration::from_millis(ms);
        let plain = |v: u8| {
            let mut i = Img::new(96, 64);
            i.px.iter_mut().for_each(|p| *p = [v, v, v]);
            i
        };
        let mut f = Frames::new(std::time::Duration::from_millis(600));
        // Schermo fermo: si tiene solo ogni tanto.
        let mut kept = 0;
        for k in 0..10 {
            kept += f.offer(plain(50), at(k * 100)) as usize;
        }
        assert!(kept <= 3, "{kept} schermate uguali tenute");
        // Uno stato breve fra molti cambiamenti (animazioni), con l'analisi ferma.
        assert!(f.offer(plain(200), at(1000)));
        for k in 0..20 {
            f.offer(plain(60 + (k % 2) as u8 * 2), at(1100 + k * 100));
        }
        let mut seen = Vec::new();
        while let Some(i) = f.next() {
            seen.push(i.px[0][0]);
        }
        assert!(seen.contains(&200), "stato breve perso: {seen:?}");
    }

    #[test]
    fn unique_card_frame_survives_a_full_queue() {
        use synth::{table, Deal, Look};

        let images: Vec<_> = [7, 14, 21, 26, 32]
            .map(|card| reference::card_image(0, card))
            .into_iter()
            .collect();
        let board = |opponent: bool| table(&Deal {
            hand: images[..3].iter().collect(),
            opp: opponent.then_some(&images[3]),
            mine: None,
            trump: Some(&images[4]),
            deck: true,
        }, &Look::default());
        let empty = board(false);
        let played = board(true);

        let start = std::time::Instant::now();
        let mut queue = Frames::new(std::time::Duration::from_millis(500));
        for (id, millis) in [0, 16, 32, 96, 160, 224, 288, 352, 416].into_iter().enumerate() {
            let mut frame = if id == 1 { played.clone() } else { empty.clone() };
            if id % 2 == 1 && id != 1 {
                for rgb in &mut frame.px {
                    *rgb = rgb.map(|channel| channel.saturating_add(4));
                }
            }
            let timestamp = start + std::time::Duration::from_millis(millis);
            assert!(queue.offer_with(frame, id, timestamp), "fotogramma {id} non ammesso");
        }

        let mut card_frame = None;
        while let Some((frame, id)) = queue.next_with() {
            if id == 1 {
                card_frame = Some(frame);
            }
        }
        let frame = card_frame.expect("la coda ha espulso l'unico fotogramma con la carta");
        forget_focus();
        let scene = read(&frame);
        assert!(scene.opp_slot.iter().chain(&scene.my_slot).any(|view| best(view) == 26));
    }

    #[test]
    fn brief_equal_luminance_suit_change_is_queued() {
        let mut frame = Img::new(640, 480);
        frame.px.fill([60, 80, 50]);
        let now = std::time::Instant::now();
        let mut queue = Frames::new(std::time::Duration::from_secs(1));
        assert!(queue.offer(frame.clone(), now));
        for y in 220..252 {
            for x in 310..334 {
                frame.px[y * frame.w + x] = [180, 20, 50];
            }
        }
        assert!(queue.offer(frame, now + std::time::Duration::from_millis(80)));
    }

    /// Su uno schermo grande una carta sola che compare cambia poco nel complesso, ma la schermata
    /// si tiene lo stesso; e se l'analisi resta indietro si scartano i momenti delle animazioni,
    /// non gli stati rimasti fermi.
    #[test]
    fn small_changes_are_kept() {
        let t0 = std::time::Instant::now();
        let at = |ms: u64| t0 + std::time::Duration::from_millis(ms);
        let screen = |card: Option<(usize, usize)>| {
            let mut i = Img::new(1920, 1080);
            i.px.iter_mut().for_each(|p| *p = [30, 90, 40]);
            if let Some((cx, cy)) = card {
                for y in cy..cy + 110 {
                    for x in cx..cx + 76 {
                        i.px[y * 1920 + x] = [245, 245, 240];
                    }
                }
            }
            i
        };
        let mut f = Frames::new(std::time::Duration::from_millis(600));
        assert!(f.offer(screen(None), at(0)));
        assert!(f.offer(screen(Some((900, 400))), at(100)), "carta comparsa non tenuta");
        // Analisi ferma: uno stato fermo (la carta in tavola per mezzo secondo) e poi
        // un'animazione lunga: lo stato fermo resta.
        let mut f = Frames::new(std::time::Duration::from_millis(600));
        f.offer(screen(None), at(0));
        f.offer(screen(Some((900, 400))), at(100));
        for k in 0..20u64 {
            f.offer(screen(Some((100 + k as usize * 60, 700))), at(600 + k * 100));
        }
        let mut seen = Vec::new();
        while let Some(i) = f.next() {
            seen.push(i.at(910, 410)[0]);
        }
        assert!(seen.contains(&245), "stato fermo perso");
    }

    /// Lo screenshot del sito: l'avversario ha giocato il 2 di bastoni, tu il 7 di spade, in mano
    /// hai fante di denari e asso di coppe, la briscola (sotto il mazzo) è il cavallo di denari.
    fn check(s: &Scene) {
        let name = |v: Option<&CardView>| v.map(|v| cards::name(best(v)));
        assert_eq!(name(s.opp_slot.as_ref()), Some(cards::name(cards::make(3, 1))), "avversario");
        assert_eq!(name(s.my_slot.as_ref()), Some(cards::name(cards::make(2, 6))), "tua giocata");
        let hand: Vec<String> = s.hand.iter().map(|v| cards::name(best(v))).collect();
        assert_eq!(hand, vec![cards::name(cards::make(0, 7)), cards::name(cards::make(1, 0))], "mano");
        assert_eq!(name(s.trump.as_ref()), Some(cards::name(cards::make(0, 8))), "briscola");
    }

    /// Riquadri delle carte scoperte nello screenshot: le due in tavola, la mano, la briscola.
    const FACES: [(usize, usize, usize, usize); 4] =
        [(520, 270, 650, 460), (520, 570, 650, 760), (470, 780, 715, 975), (315, 790, 435, 925)];

    /// Ricolora i pixel del panno (verdi e fuori dalle carte) con `f`.
    fn recolor(img: &mut Img, f: impl Fn([u8; 3]) -> [u8; 3]) {
        for y in 0..img.h {
            for x in 0..img.w {
                let inside = FACES.iter().any(|&(x0, y0, x1, y1)| (x0..x1).contains(&x) && (y0..y1).contains(&y));
                let p = img.px[y * img.w + x];
                if !inside && p[1] as i32 > p[0] as i32 + 30 && p[1] >= p[2] {
                    img.px[y * img.w + x] = f(p);
                }
            }
        }
    }

    #[test]
    fn reads_real_screenshot() {
        // (L'app legge lo schermo di continuo: dopo qualche lettura sa quale mazzo usa il sito.)
        crate::vision::reference::forget_style();
        for _ in 0..3 {
            screenshot();
        }
        let s = screenshot();
        check(&s);
        assert!(s.trump.as_ref().unwrap().partial);
        // Ogni carta intera stacca la seconda abbastanza da essere inserita senza conferma (la
        // briscola vista a metà, finché non è in memoria, si fa confermare).
        for v in s.opp_slot.iter().chain(&s.my_slot).chain(&s.hand) {
            let (c, margin) = v.best(CardSet::FULL).unwrap();
            let mut sc: Vec<(Card, f32)> = (0..40u8).map(|k| (k, v.scores[k as usize])).collect();
            sc.sort_by(|a, b| b.1.total_cmp(&a.1));
            let top: Vec<(String, f32)> = sc[..3].iter().map(|x| (cards::name(x.0), x.1)).collect();
            assert!(margin >= 1.4, "{} distacco {margin} {top:?}", cards::name(c));
        }
    }

    /// Lo stesso tavolo di legno marrone invece che di panno verde.
    #[test]
    fn reads_a_wooden_table() {
        let mut img = site();
        recolor(&mut img, |[r, g, _]| [(g as f32 * 1.1) as u8, (g as f32 * 0.7) as u8, r.saturating_add(10)]);
        check(&read(&img));
    }

    /// Schermo scurito e con la luce notturna (colori più caldi).
    #[test]
    fn reads_a_dim_warm_screen() {
        let mut img = site();
        for p in img.px.iter_mut() {
            *p = [(p[0] as f32 * 0.75) as u8, (p[1] as f32 * 0.65) as u8, (p[2] as f32 * 0.5) as u8];
        }
        check(&read(&img));
    }

    /// Una scritta del sito sopra la carta dell'avversario (una fascia scura che la taglia in due)
    /// e la tua carta girata di mezzo giro, come la vede chi sta di fronte.
    #[test]
    fn reads_cards_under_writing_and_upside_down() {
        let mut img = site();
        for y in 345..372 {
            for x in 480..690 {
                img.px[y * img.w + x] = if (x / 3 + y / 4) % 5 == 0 { [255, 255, 255] } else { [35, 35, 40] };
            }
        }
        let (x0, y0, x1, y1) = (522, 572, 647, 758);
        let keep = img.clone();
        for y in y0..y1 {
            for x in x0..x1 {
                img.px[y * img.w + x] = keep.at(x1 - 1 - (x - x0), y1 - 1 - (y - y0));
            }
        }
        check(&read(&img));
    }

    /// Screenshot vero di un altro sito (Solitalian), con il tavolo di legno chiaro e un altro disegno
    /// delle carte: in mano due di spade, due di bastoni e sette di denari; la briscola, più piccola
    /// e coperta per metà dal mazzo, è il cinque di coppe. Tutto si inserisce senza conferma.
    #[test]
    fn reads_solitalian() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/solitalian_briscola.png");
        let s = read(&capture::load_png(&path).unwrap());
        let name = |v: &CardView| {
            let (c, margin) = v.best(CardSet::FULL).unwrap();
            assert!(margin >= 1.4, "{} distacco {margin}", cards::name(c));
            cards::name(c)
        };
        let hand: Vec<String> = s.hand.iter().map(name).collect();
        let want = [cards::make(2, 1), cards::make(3, 1), cards::make(0, 6)].map(cards::name);
        assert_eq!(hand, want.to_vec());
        let t = s.trump.as_ref().expect("briscola");
        assert!(t.partial);
        assert_eq!(name(t), cards::name(cards::make(1, 4)));
        assert!(s.opp_slot.is_none() && s.my_slot.is_none(), "niente carte in tavola");
    }

    /// Una carta vista intera (in mano) si riconosce con sicurezza anche quando torna sotto il mazzo,
    /// vista solo in parte: la memoria confronta la fascia che si vede.
    #[test]
    fn memory_recognizes_a_covered_trump() {
        let mut img = site();
        // L'asso di coppe della mano, girato di un quarto (la testa a destra) come la briscola:
        // se ne vede solo la parte che esce dal mazzo.
        let (x0, y0, w, h) = (588usize, 796usize, 112usize, 168usize);
        let keep = img.clone();
        for y in 0..w.min(110) {
            for x in 0..110 {
                // Colonna x della briscola visibile = riga dell'asso, contata dalla testa.
                let (sx, sy) = (x0 + y, y0 + (h - 1) - (h - 110 + x));
                img.px[(805 + y) * img.w + 322 + x] = keep.at(sx, sy);
            }
        }
        let mem = Mutex::new(Memory::in_memory());
        if let Ok(d) = std::env::var("SALVA") {
            img.save_png(std::path::Path::new(&format!("{d}/asso_sotto.png"))).unwrap();
        }
        let first = read_briscola(&img, &mem).expect("carte");
        let asso = cards::make(1, 0);
        let t = first.trump.as_ref().expect("briscola");
        // La partita la inserisce dalla mano, e la ricorda.
        mem.lock().unwrap_or_else(|e| e.into_inner()).remember(&first.hand[1].img, first.hand[1].vis, asso);
        let again = read_briscola(&img, &mem).expect("carte");
        let t2 = again.trump.as_ref().expect("briscola");
        let (c, margin) = t2.best(CardSet::FULL).unwrap();
        let (_, before) = t.best(CardSet::FULL).unwrap();
        assert_eq!(cards::name(c), cards::name(asso));
        assert!(margin >= 1.4 && margin > before + 3.0, "distacco {before} -> {margin}");
    }

    /// Niente carte scambiate per carte: i riquadri del sito, i dorsi e il mazzo non sono carte.
    #[test]
    fn ignores_what_is_not_a_card() {
        let mut img = site();
        // Toglie le carte scoperte coprendole col panno.
        let felt = img.at(900, 500);
        for (x0, y0, x1, y1) in FACES {
            for y in y0..y1 {
                for x in x0..x1 {
                    img.px[y * img.w + x] = felt;
                }
            }
        }
        assert!(read_briscola(&img, &Mutex::new(Memory::in_memory())).is_none());
    }

    /// Una carta ricordata si riconosce dalla memoria anche quando le forme non bastano.
    #[test]
    fn memory_decides_figures() {
        let s = screenshot();
        let mut mem = Memory::in_memory();
        let fante = cards::make(0, 7);
        mem.remember(&s.hand[0].img, WHOLE, fante);
        let mut v = s.hand[0].clone();
        recognize(&mut v, &mem);
        let (c, margin) = v.best(CardSet::FULL).unwrap();
        assert_eq!(c, fante);
        assert!(margin > 3.0);
        // Le altre carte non vengono scambiate per il fante.
        let mut other = s.hand[1].clone();
        recognize(&mut other, &mem);
        assert_ne!(best(&other), fante);
    }

    /// Copia un rettangolo dell'immagine in un altro punto.
    fn paste(img: &mut Img, from: (usize, usize, usize, usize), to: (usize, usize)) {
        let (x0, y0, w, h) = from;
        let block: Vec<[u8; 3]> =
            (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| img.at(x0 + x, y0 + y)).collect();
        for y in 0..h {
            for x in 0..w {
                img.px[(to.1 + y) * img.w + to.0 + x] = block[y * w + x];
            }
        }
    }

    /// Un tavolo di scopa costruito dallo screenshot vero: le due carte giocate diventano quattro
    /// carte in tavola su due righe, la mano resta la stessa.
    fn scopa_screen() -> Img {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/sito_briscola.png");
        let mut img = capture::load_png(&path).unwrap();
        let (bastoni, spade) = ((522, 272, 125, 186), (522, 572, 125, 186));
        let keep = img.clone();
        // Toglie le due carte dal centro coprendole col panno lì accanto.
        paste(&mut img, (760, 272, 125, 186), (522, 272));
        paste(&mut img, (760, 572, 125, 186), (522, 572));
        let mut put = |from: (usize, usize, usize, usize), to: (usize, usize)| {
            for y in 0..from.3 {
                for x in 0..from.2 {
                    img.px[(to.1 + y) * img.w + to.0 + x] = keep.at(from.0 + x, from.1 + y);
                }
            }
        };
        put(bastoni, (420, 300));
        put(spade, (600, 300));
        put(spade, (330, 540));
        put(bastoni, (700, 540));
        img
    }

    #[test]
    fn reads_scopa_table() {
        let img = scopa_screen();
        let s = read_scopa(&img, &Mutex::new(Memory::in_memory())).expect("tavolo");
        assert_eq!(s.hand.len(), 2, "mano");
        let names: Vec<String> = s.table.iter().map(|v| cards::name(best(v))).collect();
        let (b2, s7) = (cards::name(cards::make(3, 1)), cards::name(cards::make(2, 6)));
        assert_eq!(names, vec![b2.clone(), s7.clone(), s7, b2]);
        assert_eq!(cards::name(best(&s.hand[1])), cards::name(cards::make(1, 0)));
    }

    /// Tavolo aperto ma senza carte scoperte: scena vuota, non "tavolo assente".
    #[test]
    fn empty_scopa_table() {
        let mut img = scopa_screen();
        let felt = img.at(900, 500);
        for p in img.px.iter_mut() {
            if (p[1] as i32 > p[0] as i32 + 30 && p[1] > p[2]) || p[0] > 150 {
                *p = felt;
            }
        }
        let s = read_scopa(&img, &Mutex::new(Memory::in_memory())).expect("tavolo");
        assert!(s.hand.is_empty() && s.table.is_empty());
    }
}

/// Banco di prova del riconoscimento: lo screenshot del sito a varie scale, con il mazzo che copre
/// di più la briscola e sfocato. Stampa per ogni lettura se la carta è giusta e il distacco (in
/// somiglianza: 0.175 basta per inserirla senza conferma), più il tempo di una lettura.
/// `cargo test --release -p brismaster banco -- --ignored --nocapture`
#[cfg(test)]
mod bench {
    use super::*;
    use briscola_core::cards;

    fn resize(img: &Img, k: f32) -> Img {
        let raw: Vec<u8> = img.px.iter().flatten().copied().collect();
        let im = image::RgbImage::from_raw(img.w as u32, img.h as u32, raw).unwrap();
        let (w, h) = ((img.w as f32 * k) as u32, (img.h as f32 * k) as u32);
        let r = image::imageops::resize(&im, w, h, image::imageops::FilterType::Triangle);
        Img { w: w as usize, h: h as usize, px: r.pixels().map(|p| p.0).collect() }
    }

    fn report(tag: &str, img: &Img) -> (usize, usize) {
        let want = [
            ("briscola", cards::make(0, 8)),
            ("avv", cards::make(3, 1)),
            ("tua", cards::make(2, 6)),
            ("mano0", cards::make(0, 7)),
            ("mano1", cards::make(1, 0)),
        ];
        let s = read_briscola(img, &Mutex::new(memory::Memory::in_memory()));
        let mut line = format!("{tag:<14}");
        let (mut ok, mut auto) = (0, 0);
        let views: Vec<Option<CardView>> = match &s {
            Some(s) => {
                let hand = |i: usize| s.hand.get(i).cloned();
                vec![s.trump.clone(), s.opp_slot.clone(), s.my_slot.clone(), hand(0), hand(1)]
            }
            None => vec![None; 5],
        };
        for ((name, c), v) in want.iter().zip(views) {
            match v.and_then(|v| v.best(cards::CardSet::FULL)) {
                Some((b, m)) => {
                    let good = b == *c;
                    ok += good as usize;
                    auto += (good && m >= 1.4) as usize;
                    line += &format!(" {name}:{}{:.2}", if good { "ok " } else { "NO " }, m / 8.0);
                }
                None => line += &format!(" {name}:--      "),
            }
        }
        println!("{line}");
        (ok, auto)
    }

    /// Cosa trova la lettura in una schermata qualsiasi:
    /// `IMG=schermata.png cargo test --release -p brismaster diagnosi_immagine -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn diagnosi_immagine() {
        let img = capture::load_png(std::path::Path::new(&std::env::var("IMG").unwrap())).unwrap();
        let t0 = std::time::Instant::now();
        let a = detect::analyze(&img);
        println!("analisi in {:?}", t0.elapsed());
        if let Ok(out) = std::env::var("DIAG") {
            screen::diagnose(&img).save_png(std::path::Path::new(&out)).unwrap();
        }
        println!("card_l {:.0}", a.card_l);
        if let Ok(prima) = std::env::var("SCENA") {
            for p in prima.split(',').filter(|p| !p.is_empty()) {
                let im = capture::load_png(std::path::Path::new(p)).unwrap();
                let _ = detect::scene(&im);
            }
            let t1 = std::time::Instant::now();
            let sc = detect::scene(&img);
            println!("scena in {:?}", t1.elapsed());
            let show = |v: &CardView| format!("{}({:.1})", cards::short(v.best(cards::CardSet::FULL).unwrap().0), v.best(cards::CardSet::FULL).unwrap().1);
            println!("scena: B {:?} A {:?} T {:?} M {:?}", sc.trump.as_ref().map(show), sc.opp_slot.as_ref().map(show), sc.my_slot.as_ref().map(show), sc.hand.iter().map(show).collect::<Vec<_>>());
        }
        for (i, f) in a.cards.iter().enumerate() {
            let (c, m) = f.view.best(cards::CardSet::FULL).unwrap();
            if let Ok(dir) = std::env::var("CARTE") {
                f.view.img.save_png(&std::path::Path::new(&dir).join(format!("carta{i}.png"))).unwrap();
                println!("  simboli carta{i}: {:?}", pips::count(&f.view.img));
            }
            println!(
                "card ({:.0},{:.0}) {:.0}x{:.0} land {} vis {:.2?} parz {} score {:.3} {} {:.2}",
                f.rect.cx,
                f.rect.cy,
                2.0 * f.rect.hw,
                2.0 * f.rect.hh,
                f.rect.landscape(),
                f.view.vis,
                f.view.partial,
                f.score,
                cards::name(c),
                m
            );
            let mut rec = f.view.clone();
            recognize(&mut rec, &Memory::in_memory());
            let lp = nn::log_probs(&f.view.img).unwrap_or([0.0; 40]);
            let nb = (0..40).max_by(|&x, &y| lp[x].total_cmp(&lp[y])).unwrap();
            println!("    riconosciuta: {:?}; rete {} {:.3}", rec.best(cards::CardSet::FULL).map(|(c, m)| (cards::short(c), m)), cards::short(nb as u8), lp[nb]);
            let mut order: Vec<usize> = (0..40).collect();
            order.sort_by(|&x, &y| f.view.scores[y].total_cmp(&f.view.scores[x]));
            println!(
                "    prime: {}",
                order[..4]
                    .iter()
                    .map(|&k| format!(
                        "{} {:.2} (sim {:.3}, stili {:?})",
                        cards::short(k as u8),
                        f.view.scores[k],
                        f.view.sim[k],
                        f.view.styles.iter().map(|s| (s[k] * 1000.0).round() / 1000.0).collect::<Vec<_>>()
                    ))
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
        }
        for b in a.blobs.iter() {
            println!(
                "   blob ({:.0},{:.0}) {:.0}x{:.0} white {:.2} bg {:.2}",
                b.rect.cx,
                b.rect.cy,
                2.0 * b.rect.hw,
                2.0 * b.rect.hh,
                b.white,
                b.bg
            );
        }
    }

    #[test]
    #[ignore]
    fn banco() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/sito_briscola.png");
        let site = capture::load_png(&path).unwrap();
        let (mut ok, mut auto, mut n) = (0, 0, 0);
        for k in [1.0f32, 0.85, 0.7, 0.6, 0.5, 0.42] {
            let (a, b) = report(&format!("scala {k}"), &resize(&site, k));
            (ok, auto, n) = (ok + a, auto + b, n + 5);
        }
        // Mazzo spostato sopra la briscola: se ne vede meno.
        for dx in [15usize, 30, 45] {
            let mut img = site.clone();
            let keep = site.clone();
            for y in 770..955 {
                for x in (210..330).rev() {
                    img.px[y * img.w + x + dx] = keep.at(x, y);
                }
            }
            if let Ok(d) = std::env::var("SALVA") {
                img.save_png(std::path::Path::new(&format!("{d}/mazzo{dx}.png"))).unwrap();
            }
            let (a, b) = report(&format!("mazzo +{dx}"), &img);
            (ok, auto, n) = (ok + a, auto + b, n + 5);
        }
        // Sfocato (schermo ridimensionato due volte).
        let sf = resize(&resize(&site, 0.55), 1.0 / 0.55);
        if let Ok(d) = std::env::var("SALVA") {
            sf.save_png(std::path::Path::new(&format!("{d}/sfocato.png"))).unwrap();
        }
        let (a, b) = report("sfocato", &sf);
        (ok, auto, n) = (ok + a, auto + b, n + 5);
        println!("giuste {ok}/{n}, automatiche {auto}/{n}");
        for _ in 0..3 {
            let t = std::time::Instant::now();
            let _ = read_briscola(&site, &Mutex::new(memory::Memory::in_memory()));
            println!("lettura 1524x1032 in {:?}", t.elapsed());
        }
    }
}

/// Tavoli finti con mazzi, tavoli e colori diversi: il riconoscimento deve reggere anche su
/// grafiche che non conosce (un terzo mazzo napoletano, mai visto) e su quelle che conosce ma
/// ridisegnate un po' diverse (proporzioni, colori, sfocatura, carte ruotate, scale diverse).
#[cfg(test)]
mod other_sites {
    use super::synth::{table, writing_only, Bg, Deal, Look};
    use super::*;
    use briscola_core::cards::{self, Card};

    fn load_jpg(path: &std::path::Path) -> Img {
        let im = image::open(path).unwrap().to_rgba8();
        Img::from_rgba(im.width() as usize, im.height() as usize, im.as_raw())
    }

    /// I bastoni del mazzo che l'app non conosce, dall'asso al re.
    fn unknown_deck() -> Vec<(Card, Img)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/mazzo_despues");
        (1..=10).map(|k| (cards::make(3, k - 1), load_jpg(&dir.join(format!("bastoni_{k}.jpg"))))).collect()
    }

    #[test]
    fn learned_unfamiliar_deck_rotated_and_dimmed() {
        let mut memory = Memory::in_memory();
        let mut deck = Vec::new();
        for card in 0..40u8 {
            let mut image = Img::new(CARD_W, CARD_H);
            let mut state = u32::from(card) + 101;
            for y in (12..CARD_H-12).step_by(12) {
                for x in (12..CARD_W-12).step_by(12) {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    let ink = if state >> 31 == 0 { [25, 15, 20] } else { [220, 170, 90] };
                    for row in y..y+8 { image.px[row*CARD_W+x..row*CARD_W+x+8].fill(ink); }
                }
            }
            memory.remember(&image, WHOLE, card);
            deck.push(image);
        }
        for (card, mut image) in deck.into_iter().enumerate() {
            image.px.reverse();
            for channel in image.px.iter_mut().flatten() {
                *channel = (*channel as u16 * 9 / 10) as u8;
            }
            let mut view = CardView::new(image, WHOLE, [0.3; 40]);
            recognize(&mut view, &memory);
            assert_eq!(view.best(CardSet::FULL).unwrap().0 as usize, card);
        }
    }

    #[test]
    fn outline_alone_never_gives_a_large_identity_margin() {
        let img = super::reference::card_image(1, 7);
        let mut sims = [0.0; 40];
        sims[7] = 0.99;
        let mut view = CardView::new(img, WHOLE, sims);
        view.recovered_outline = true;
        recognize(&mut view, &Memory::in_memory());
        let (chosen, margin) = view.best(CardSet::FULL).unwrap();
        assert_eq!(chosen, 7);
        assert!(margin <= 1.01);
    }

    #[test]
    fn duplicate_hand_preserves_confirmations() {
        let mut hand: Vec<_> = (0..3).map(|_| CardView::new(Img::new(CARD_W, CARD_H), WHOLE, [0.0; 40])).collect();
        hand[0].scores[2] = 6.0;
        hand[1].scores[3] = 6.0;
        hand[2].scores[2] = 3.0;
        hand[2].scores[3] = 2.9;
        hand[2].scores[4] = 2.8;
        resolve_hand_duplicates(&mut hand);
        let (chosen, margin) = hand[2].best(CardSet::FULL).unwrap();
        assert_eq!(chosen, 4);
        assert!(margin < 1.4);
        assert_eq!(hand[0].scores[2], 6.0);
        assert_eq!(hand[1].scores[3], 6.0);
    }

    fn looks() -> Vec<Look> {
        let d = Look::default();
        vec![
            d,
            Look { bg: Bg::Wood, card_h: 150.0, pile: 0.8, cover: 0.5, ..d },
            Look { bg: Bg::Felt([30, 60, 140]), card_h: 110.0, aspect: Some(0.68), fan: 6.0, ..d },
            Look { bg: Bg::Felt([120, 25, 30]), card_h: 190.0, hue: 8.0, sat: 1.3, gamma: 0.85, ..d },
            Look { bg: Bg::Wood, card_h: 130.0, sat: 0.75, gamma: 1.2, blur: true, cover: 0.4, ..d },
            Look {
                bg: Bg::Felt([20, 105, 70]),
                card_h: 140.0,
                aspect: Some(0.56),
                hue: -8.0,
                fan: -5.0,
                pile: 0.85,
                ..d
            },
            Look { bg: Bg::Felt([45, 45, 50]), card_h: 85.0, hue: 15.0, sat: 0.6, ..d },
            Look {
                bg: Bg::Felt([90, 60, 30]),
                card_h: 230.0,
                aspect: Some(0.66),
                hue: -15.0,
                gamma: 1.3,
                fan: 8.0,
                cover: 0.55,
                ..d
            },
            Look { bg: Bg::Sea, card_h: 160.0, pile: 0.8, ..d },
            Look { bg: Bg::Sky, card_h: 140.0, pile: 0.8, cover: 0.5, ..d },
            Look { bg: Bg::Sea, card_h: 120.0, aspect: Some(0.7), blur: true, fan: 5.0, ..d },
        ]
    }

    /// Per ogni ruolo: (giusta, abbastanza sicura da non chiedere conferma).
    /// La memoria condivisa fra i tavoli (con MEMORIA: come sullo stesso sito), e se le carte
    /// lette si insegnano (l'utente conferma o corregge ogni carta della prima partita).
    static SHARED: Mutex<Option<Memory>> = Mutex::new(None);
    static TEACH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    fn check(img: &Img, want: &[(&str, Card)]) -> Vec<(String, bool, bool)> {
        let shared = std::env::var_os("MEMORIA").is_some();
        let mem = if shared { SHARED.lock().unwrap().take().unwrap_or_else(Memory::in_memory) } else { Memory::in_memory() };
        let mem = Mutex::new(mem);
        let s = read_briscola(img, &mem);
        if shared {
            let mut m = mem.into_inner().unwrap();
            if std::env::var_os("MEMLOG").is_some() && !TEACH.load(std::sync::atomic::Ordering::Relaxed) {
                if let Some(sc) = &s {
                    for (role, c) in want {
                        let v = match *role {
                            "briscola" => sc.trump.as_ref(),
                            "avversario" => sc.opp_slot.as_ref(),
                            "tua" => sc.my_slot.as_ref(),
                            r => sc.hand.get(r[4..].parse::<usize>().unwrap_or(9)),
                        };
                        if let Some(v) = v {
                            let b = v.best(CardSet::FULL).unwrap().0;
                            if b != *c {
                                let sim = m.similarity(&v.img, v.vis);
                                if let Ok(d) = std::env::var("MEMDUMP") {
                                    let n = std::fs::read_dir(&d).map(|r| r.count()).unwrap_or(0);
                                    let _ = v.img.save_png(std::path::Path::new(&format!("{d}/v{c:02}_{n:03}_{:.2}.png", sim[*c as usize].unwrap_or(-1.0))));
                                }
                                eprintln!("MEM vera {c} sim {:?} punti {:.1}; letta {b} sim {:?} punti {:.1}; parziale {}", sim[*c as usize], v.scores[*c as usize], sim[b as usize], v.scores[b as usize], v.partial);
                            }
                        }
                    }
                }
            }
            if TEACH.load(std::sync::atomic::Ordering::Relaxed) {
                // (L'utente conferma la carta che vede: se manca una carta della mano, le altre
                // non si possono abbinare per posizione e non si insegnano.)
                let hand = want.iter().filter(|(r, _)| r.starts_with("mano")).count();
                if let Some(sc) = &s {
                    for (role, c) in want.iter().filter(|(r, _)| !r.starts_with("mano") || sc.hand.len() == hand) {
                        let v = match *role {
                            "briscola" => sc.trump.as_ref(),
                            "avversario" => sc.opp_slot.as_ref(),
                            "tua" => sc.my_slot.as_ref(),
                            r => sc.hand.get(r[4..].parse::<usize>().unwrap_or(9)),
                        };
                        if let Some(v) = v {
                            if let Ok(d) = std::env::var("MEMDUMP") {
                                let n = std::fs::read_dir(&d).map(|r| r.count()).unwrap_or(0);
                                let _ = v.img.save_png(std::path::Path::new(&format!("{d}/t{c:02}_{n:03}.png")));
                            }
                            m.remember(&v.img, v.vis, *c);
                        }
                    }
                }
            }
            *SHARED.lock().unwrap() = Some(m);
        }
        let get = |role: &str| -> Option<CardView> {
            let s = s.as_ref()?;
            match role {
                "briscola" => s.trump.clone(),
                "avversario" => s.opp_slot.clone(),
                "tua" => s.my_slot.clone(),
                r => s.hand.get(r[4..].parse::<usize>().ok()?).cloned(),
            }
        };
        let mut out: Vec<(String, bool, bool)> = want
            .iter()
            .map(|(role, c)| match get(role).and_then(|v| v.best(CardSet::FULL)) {
                Some((b, m)) => {
                    let note = if b != *c {
                        let v = get(role).unwrap();
                        if let Ok(d) = std::env::var("SBAGLIATE") {
                            let n = std::fs::read_dir(&d).map(|r| r.count()).unwrap_or(0);
                            let _ = v.img.save_png(std::path::Path::new(&format!("{d}/{n:03}_vera{c:02}_letta{b:02}.png")));
                        }
                        let nn = super::nn::log_probs(&v.img).map(|lp| (0..40).max_by(|&a, &b| lp[a].total_cmp(&lp[b])).unwrap() as Card);
                        format!(" (sbagliata{}; rete {} {}, parziale {})", if m >= 1.4 { " SICURA" } else { "" }, nn.map(cards::name).unwrap_or_default(), if nn == Some(*c) { "GIUSTA" } else { "no" }, v.partial)
                    } else if m < 1.4 {
                        format!(" (da confermare, {m:.2})")
                    } else {
                        String::new()
                    };
                    (format!("{role}: {}{note}", cards::name(b)), b == *c, b == *c && m >= 1.4)
                }
                None => (format!("{role}: non vista"), false, false),
            })
            .collect();
        // Nessuna carta in più (una scritta presa per una carta della mano).
        let hand = want.iter().filter(|(r, _)| r.starts_with("mano")).count();
        let seen = s.as_ref().map_or(0, |s| s.hand.len());
        if seen != hand {
            if std::env::var_os("SCARTI").is_some() {
                eprintln!("MANO {:?}", s.as_ref().map(|s| s.hand.iter().map(|v| v.at).collect::<Vec<_>>()));
            }
            out.push((format!("mano di {seen} carte invece di {hand}"), false, false));
        }
        out
    }

    /// Distribuisce le carte a turno: tre in mano, una per parte in tavola, la briscola.
    /// Con `writing` scritte attorno alle carte e loghi stampati sul tavolo (sotto le carte, più o
    /// meno marcati a seconda del tavolo).
    fn run(deck: &[(Card, Img)], label: &str, writing: bool, verbose: bool) -> (usize, usize, usize) {
        let (mut ok, mut auto, mut n) = (0, 0, 0);
        for (k, look) in looks().iter().enumerate() {
            let logo = if writing { [0.35, 0.6, 0.9][k % 3] } else { 0.0 };
            let look = &Look { writing, logo, ..*look };
            let pick = |i: usize| &deck[(k * 6 + i) % deck.len()];
            let (h0, h1, h2, o, m, t) = (pick(0), pick(1), pick(2), pick(3), pick(4), pick(5));
            let deal = Deal {
                hand: vec![&h0.1, &h1.1, &h2.1],
                opp: Some(&o.1),
                mine: Some(&m.1),
                trump: Some(&t.1),
                deck: true,
            };
            let mut img = table(&deal, look);
            if std::env::var_os("RICARICA").is_some() {
                let p = std::env::temp_dir().join("ricarica.png");
                img.save_png(&p).unwrap();
                img = capture::load_png(&p).unwrap();
            }
            // (Ogni tavolo è un sito diverso: niente di quello imparato sul tavolo prima.)
            detect::forget_reading_state();
            if verbose {
                let dir = std::env::var("SALVA").ok();
                if let Some(d) = dir {
                    img.save_png(std::path::Path::new(&format!("{d}/{label}_{k}.png"))).unwrap();
                    screen::diagnose(&img)
                        .save_png(std::path::Path::new(&format!("{d}/{label}_{k}_diag.png")))
                        .unwrap();
                }
            }
            let want = [
                ("mano0", h0.0),
                ("mano1", h1.0),
                ("mano2", h2.0),
                ("avversario", o.0),
                ("tua", m.0),
                ("briscola", t.0),
            ];
            let res = check(&img, &want);
            if verbose {
                let line: Vec<String> = res.iter().map(|r| r.0.clone()).collect();
                println!("{label} tavolo {k}: {}", line.join(" · "));
            }
            ok += res.iter().filter(|r| r.1).count();
            auto += res.iter().filter(|r| r.2).count();
            n += res.len();
        }
        (ok, auto, n)
    }

    /// Banco dei mazzi sconosciuti: per ogni cartella di MAZZI (40 immagini, "00.png".."39.png"
    /// col numero della carta) le carte su tutti i tavoli, in due ordini diversi.
    /// `MAZZI=cartella cargo test --release -p brismaster mazzi_sconosciuti -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mazzi_sconosciuti() {
        let root = std::path::PathBuf::from(std::env::var("MAZZI").expect("MAZZI=cartella"));
        let mut dirs: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        let verbose = std::env::var("VERBOSO").is_ok();
        let (mut tok, mut tauto, mut tn) = (0, 0, 0);
        for dir in dirs {
            let cards: Vec<(Card, Img)> = (0..40u8)
                .filter_map(|c| {
                    let p = dir.join(format!("{c:02}.png"));
                    p.exists().then(|| (c, load_jpg(&p)))
                })
                .collect();
            if cards.len() < 40 {
                continue;
            }
            let label = dir.file_name().unwrap().to_string_lossy().to_string();
            let (mut ok, mut auto, mut n) = (0, 0, 0);
            let learning = std::env::var_os("MEMORIA").is_some();
            *SHARED.lock().unwrap() = None;
            for (pass, (mul, add)) in [(7u8, 0u8), (13, 5)].into_iter().enumerate() {
                TEACH.store(learning && pass == 0, std::sync::atomic::Ordering::Relaxed);
                let deck: Vec<(Card, Img)> = (0..40u8).map(|i| cards[((i * mul + add) % 40) as usize].clone()).collect();
                let r = run(&deck, &format!("{label}{mul}"), false, verbose);
                if learning {
                    println!("   {label} giro {} ({}): giuste {}/{}, senza conferma {}/{}", pass + 1, if pass == 0 { "si impara" } else { "dopo aver imparato" }, r.0, r.2, r.1, r.2);
                    if pass == 0 {
                        continue;
                    }
                }
                ok += r.0;
                auto += r.1;
                n += r.2;
            }
            println!("== {label}: giuste {ok}/{n}, senza conferma {auto}/{n}");
            tok += ok;
            tauto += auto;
            tn += n;
        }
        println!("== TOTALE: giuste {tok}/{tn} ({:.1}%), senza conferma {tauto}/{tn} ({:.1}%)", 100.0 * tok as f32 / tn.max(1) as f32, 100.0 * tauto as f32 / tn.max(1) as f32);
    }

    /// Salva le carte dei mazzi di riferimento (ESPORTA=cartella/stile/NN.png).
    #[test]
    #[ignore]
    fn esporta_riferimenti() {
        let root = std::path::PathBuf::from(std::env::var("ESPORTA").expect("ESPORTA=cartella"));
        for style in 0..crate::vision::reference::STYLE_COUNT {
            let d = root.join(format!("rif{style}"));
            std::fs::create_dir_all(&d).unwrap();
            for c in 0..40u8 {
                reference::card_image(style, c).save_png(&d.join(format!("{c:02}.png"))).unwrap();
            }
        }
    }

    /// Le carte come le vede l'app sui tavoli (raddrizzate, 100x160), dai mazzi di riferimento su
    /// tavoli a caso: per allenare la rete. VISTE=cartella, VISTE_N=tavoli per mazzo.
    #[test]
    #[ignore]
    fn esporta_viste() {
        let root = std::path::PathBuf::from(std::env::var("VISTE").expect("VISTE=cartella"));
        let per: usize = std::env::var("VISTE_N").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
        std::fs::create_dir_all(&root).unwrap();
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f32 / (1u64 << 53) as f32
        };
        let mut saved = 0;
        for style in 0..crate::vision::reference::STYLE_COUNT {
            let deck: Vec<Img> = (0..40u8).map(|c| reference::card_image(style, c)).collect();
            for k in 0..per {
                let bg = match (rnd() * 6.0) as usize {
                    0 => Bg::Wood,
                    1 => Bg::Felt([20, 105, 70]),
                    2 => Bg::Felt([30, 60, 140]),
                    3 => Bg::Felt([120, 25, 30]),
                    4 => Bg::Felt([(rnd() * 200.0) as u8, (rnd() * 200.0) as u8, (rnd() * 200.0) as u8]),
                    _ => Bg::Felt([60, 60, 60]),
                };
                let look = Look {
                    bg,
                    card_h: 100.0 + rnd() * 110.0,
                    aspect: (rnd() < 0.4).then(|| 0.5 + rnd() * 0.22),
                    hue: (rnd() - 0.5) * 30.0,
                    sat: 0.6 + rnd() * 0.8,
                    gamma: 0.8 + rnd() * 0.45,
                    blur: rnd() < 0.25,
                    fan: if rnd() < 0.4 { rnd() * 8.0 } else { 0.0 },
                    pile: 0.7 + rnd() * 0.4,
                    cover: 0.3 + rnd() * 0.3,
                    ..Look::default()
                };
                let mut pick: Vec<u8> = Vec::new();
                while pick.len() < 6 {
                    let c = (rnd() * 40.0) as u8 % 40;
                    if !pick.contains(&c) {
                        pick.push(c);
                    }
                }
                let deal = Deal {
                    hand: vec![&deck[pick[0] as usize], &deck[pick[1] as usize], &deck[pick[2] as usize]],
                    opp: Some(&deck[pick[3] as usize]),
                    mine: Some(&deck[pick[4] as usize]),
                    trump: Some(&deck[pick[5] as usize]),
                    deck: true,
                };
                let img = table(&deal, &look);
                detect::forget_reading_state();
                let Some(s) = read_briscola(&img, &Mutex::new(Memory::in_memory())) else { continue };
                let mut views: Vec<(u8, &CardView)> = Vec::new();
                if s.hand.len() == 3 {
                    for i in 0..3 {
                        views.push((pick[i], &s.hand[i]));
                    }
                }
                if let Some(v) = &s.opp_slot {
                    views.push((pick[3], v));
                }
                if let Some(v) = &s.my_slot {
                    views.push((pick[4], v));
                }
                for (i, (c, v)) in views.into_iter().enumerate() {
                    // (Solo le carte lette giuste: un'etichetta sbagliata insegnerebbe male.)
                    if v.partial || v.best(CardSet::FULL).map(|b| b.0) != Some(c) {
                        continue;
                    }
                    v.img.save_png(&root.join(format!("{c:02}_{style}_{k}_{i}.png"))).unwrap();
                    saved += 1;
                }
            }
        }
        println!("viste salvate: {saved}");
    }

    /// La rete su una carta 100x160 (RETE=immagine): le tre carte più probabili.
    #[test]
    #[ignore]
    fn prova_rete() {
        let img = load_jpg(std::path::Path::new(&std::env::var("RETE").unwrap()));
        let p = super::nn::log_probs(&img).expect("rete");
        let mut o: Vec<usize> = (0..40).collect();
        o.sort_by(|&a, &b| p[b].total_cmp(&p[a]));
        println!("rete: {:?}", o[..3].iter().map(|&c| (c, (p[c] * 1000.0).round() / 1000.0)).collect::<Vec<_>>());
    }

    fn known_deck(style: usize) -> Vec<(Card, Img)> {
        // Un ordine che mescola semi e valori.
        (0..40u8).map(|i| i.wrapping_mul(7) % 40).map(|c| (c, reference::card_image(style, c))).collect()
    }

    /// Stampa i tavoli uno per uno (con SALVA=cartella salva anche le immagini e la diagnosi).
    /// `cargo test --release -p brismaster banco_siti -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn banco_siti() {
        let writing = std::env::var("SCRITTE").is_ok();
        for (label, deck) in
            [("sconosciuto", unknown_deck()), ("wikimedia", known_deck(0)), ("classico", known_deck(1))]
        {
            let (ok, auto, n) = run(&deck, label, writing, true);
            println!("== {label}: giuste {ok}/{n}, senza conferma {auto}/{n}");
        }
    }

    /// Stampe classiche con carta avorio, pigmenti meno contrastati e illustrazione
    /// spostata leggermente nel riquadro. Si può ripetere con BRIS_SENZA_CONCORDANZA=1.
    #[test]
    #[ignore]
    fn banco_stampe_classiche() {
        for (style, label) in ["wikimedia-avorio", "classico-avorio"].into_iter().enumerate() {
            let mut deck = known_deck(style);
            for (_, image) in &mut deck {
                let source = image.clone();
                for y in 0..image.h {
                    for x in 0..image.w {
                        let u = (x as f32 - image.w as f32 * 0.018).max(0.0);
                        let v = (y as f32 + image.h as f32 * 0.015).min((image.h - 1) as f32);
                        let pixel = source.sample(u, v);
                        image.px[y * image.w + x] = std::array::from_fn(|c| {
                            (20.0 + pixel[c] as f32 * [0.91, 0.86, 0.78][c]) as u8
                        });
                    }
                }
            }
            let (ok, auto, n) = run(&deck, label, false, true);
            println!("== {label}: giuste {ok}/{n}, senza conferma {auto}/{n}");
        }
    }

    /// Un mazzo napoletano che l'app non conosce: tutte le carte giuste e senza conferma, su tutti
    /// i tavoli (legno, panni di colori diversi, carte piccole e grandi, colori e proporzioni
    /// diversi, sfocato, mano a ventaglio, briscola coperta di più o di meno).
    #[test]
    fn unknown_deck_on_many_tables() {
        let (ok, auto, n) = run(&unknown_deck(), "sconosciuto", false, false);
        assert_eq!((ok, auto), (n, n), "giuste {ok}/{n}, senza conferma {auto}/{n}");
    }

    /// I mazzi che conosce, ridisegnati un po' diversi su tutti i tavoli.
    #[test]
    fn known_decks_redrawn() {
        for style in 0..2 {
            let (ok, auto, n) = run(&known_deck(style), "noto", false, false);
            assert_eq!((ok, auto), (n, n), "mazzo {style}: giuste {ok}/{n}, senza conferma {auto}/{n}");
        }
    }

    /// Scritte sullo schermo (finestrelle, chat, punteggi, nomi, scritte grandi, anche toccate dalle
    /// carte) e loghi stampati sul tavolo: nessuno è preso per una carta e le carte restano giuste.
    #[test]
    fn writing_is_not_a_card() {
        for look in looks() {
            let img = writing_only(&look);
            let s = read_briscola(&img, &Mutex::new(Memory::in_memory()));
            assert!(s.is_none(), "{look:?}: carte viste fra le scritte");
        }
        // Tutte giuste. Le carte del mazzo sconosciuto più piccole (85 pixel) sono al limite della
        // sicurezza: le finestrelle bianche spostano di poco il bianco dello schermo, e una può
        // chiedere conferma.
        for (label, deck, confirm) in [("sconosciuto", unknown_deck(), 1), ("noto", known_deck(1), 0)] {
            let (ok, auto, n) = run(&deck, label, true, std::env::var_os("VERBOSO").is_some());
            assert!(ok == n && auto + confirm >= n, "{label} con scritte: giuste {ok}/{n}, senza conferma {auto}/{n}");
        }
    }

    /// Bordo e inchiostro delle carte vere e di quello che si prende per carta fra scritte e loghi.
    /// `cargo test --release -p brismaster misure_scritte -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn misure_scritte() {
        let show = |tag: &str, img: &Img| {
            for f in detect::analyze(img).cards {
                let (b, i) = detect::traits(&f.view.img, f.view.vis);
                println!(
                    "{tag} ({:.0},{:.0}) parz {} vis {:.2}-{:.2} sim {:.2} bordo {b:.2} inchiostro {i:.3} tavolo {:.3}",
                    f.rect.cx,
                    f.rect.cy,
                    f.partial(),
                    f.view.vis[0],
                    f.view.vis[1],
                    f.score,
                    f.inside
                );
            }
        };
        for (k, look) in looks().iter().enumerate() {
            show(&format!("FALSA t{k}"), &writing_only(look));
        }
        for (label, deck) in [("sconosciuto", unknown_deck()), ("wiki", known_deck(0)), ("classico", known_deck(1))] {
            for (k, look) in looks().iter().enumerate() {
                let pick = |i: usize| &deck[(k * 6 + i) % deck.len()];
                let deal = Deal {
                    hand: vec![&pick(0).1, &pick(1).1, &pick(2).1],
                    opp: Some(&pick(3).1),
                    mine: Some(&pick(4).1),
                    trump: Some(&pick(5).1),
                    deck: true,
                };
                show(&format!("vera {label} t{k}"), &table(&deal, look));
                let logo = [0.35, 0.6, 0.9][k % 3];
                show(&format!("scritte {label} t{k}"), &table(&deal, &Look { writing: true, logo, ..*look }));
            }
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        for name in ["sito_briscola.png", "solitalian_briscola.png", "solitalian_mare.jpg"] {
            show(&format!("vera {name}"), &load_jpg(&dir.join(name)));
        }
    }

    /// Per ogni carta disegnata piccola (110 pixel, come sul tavolo di prova) quale carta si legge
    /// e con che distacco dalla seconda. `cargo test --release -p brismaster distacco_carte -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn distacco_carte() {
        let style: usize = std::env::var("STILE").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
        let deck = known_deck(style);
        let look = Look { card_h: 110.0, aspect: Some(76.0 / 110.0), ..Look::default() };
        let mut rows = Vec::new();
        for chunk in deck.chunks(3) {
            let hand: Vec<&Img> = chunk.iter().map(|c| &c.1).collect();
            let img = table(&Deal { hand, opp: None, mine: None, trump: None, deck: true }, &look);
            let s = read_briscola(&img, &Mutex::new(Memory::in_memory())).expect("carte");
            for (k, v) in s.hand.iter().enumerate() {
                let Some(&(want, _)) = chunk.get(k) else { continue };
                let (got, m) = v.best(CardSet::FULL).unwrap();
                // La seconda carta più probabile.
                let mut sc: Vec<(Card, f32)> = (0..40u8).map(|c| (c, v.scores[c as usize])).collect();
                sc.sort_by(|a, b| b.1.total_cmp(&a.1));
                rows.push((
                    m,
                    format!(
                        "{:<20} -> {:<20} distacco {m:5.2}  seconda {}",
                        cards::name(want),
                        cards::name(got),
                        cards::name(sc[1].0)
                    ),
                ));
            }
        }
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, r) in rows.iter().take(15) {
            println!("{r}");
        }
    }
}

#[cfg(test)]
mod replay;
