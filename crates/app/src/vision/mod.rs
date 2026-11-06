//! Lettura della partita dallo schermo: trova le carte scoperte su qualunque tavolo, le riconosce
//! confrontandole con le 40 carte napoletane di riferimento (anche quelle coperte in parte) e le
//! passa all'assistente. Legge solo i pixel dello schermo: non tocca il sito e non gioca al posto
//! dell'utente.

pub mod capture;
pub mod detect;
pub mod memory;
pub mod reference;
pub mod screen;
pub mod track;
#[cfg(test)]
pub(crate) mod synth;

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
    /// Carta raddrizzata, CARD_W x CARD_H, con la testa in alto.
    pub img: Img,
    /// Vista solo in parte (la briscola sotto il mazzo, una carta tagliata o coperta).
    pub partial: bool,
    /// Fascia visibile della carta: frazioni dell'altezza, dalla testa.
    pub vis: [f32; 2],
    /// Somiglianza (-1..1) con ciascuna delle 40 carte di riferimento.
    pub sim: [f32; 40],
    /// Punteggio di ciascuna delle 40 carte (più alto = più probabile).
    pub scores: [f32; 40],
    /// Dove sta la carta intera nella schermata letta: sinistra, alto, destra, basso, in frazioni
    /// della schermata (serve alla Psy Fly per volarci sopra).
    pub at: [f32; 4],
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

/// Punteggi delle 40 carte dalla somiglianza con le carte di riferimento.
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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Condvar, Mutex};

/// Punteggi della carta: somiglianza con le carte di riferimento, più quella con le immagini delle
/// carte già viste su questo sito (che quando c'è decide).
pub fn recognize(view: &mut CardView, mem: &Memory) {
    let mut s = reference_scores(&view.sim);
    for (c, sim) in mem.similarity(&view.img, view.vis).iter().enumerate() {
        if let Some(v) = *sim {
            if v >= memory::SAME {
                s[c] += 6.0 + (v - memory::SAME) * 20.0;
            }
        }
    }
    view.scores = s;
}

impl CardView {
    pub fn new(img: Img, vis: [f32; 2], sim: [f32; 40]) -> CardView {
        CardView { img, partial: vis[1] - vis[0] < 0.9, vis, sim, scores: reference_scores(&sim), at: [0.0; 4] }
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
    Scene(S),
    /// Nessun tavolo con carte sullo schermo.
    NoTable,
    Error(String),
}

/// Lettura per la briscola: trova le carte e le riconosce (None se non c'è nessuna carta scoperta).
/// La memoria resta bloccata solo mentre si confrontano le carte trovate, non durante la ricerca.
pub fn read_briscola(img: &Img, mem: &Mutex<Memory>) -> Option<Scene> {
    let mut scene = detect::scene(img);
    let found = scene.trump.is_some() || !scene.hand.is_empty() || scene.opp_slot.is_some() || scene.my_slot.is_some();
    if !found {
        return None;
    }
    let mem = mem.lock().unwrap_or_else(|e| e.into_inner());
    for v in scene.trump.iter_mut().chain(&mut scene.opp_slot).chain(&mut scene.my_slot).chain(&mut scene.hand) {
        recognize(v, &mem);
    }
    Some(scene)
}

/// Lettura per la scopa: mano e carte in tavola, riconosciute (None se il tavolo non si vede).
pub fn read_scopa(img: &Img, mem: &Mutex<Memory>) -> Option<ScopaScene> {
    let mut scene = detect::scopa(img)?;
    let mem = mem.lock().unwrap_or_else(|e| e.into_inner());
    for v in scene.hand.iter_mut().chain(&mut scene.table) {
        recognize(v, &mem);
    }
    Some(scene)
}

/// Le carte di una scena: dove stanno nella schermata (frazioni), le tue per prime.
pub trait Located {
    /// Le zone da inseguire quando lo schermo si sposta: la tua mano, o tutte le carte se non ce n'è.
    fn spots(&self) -> Vec<[f32; 4]>;
}

impl Located for Scene {
    fn spots(&self) -> Vec<[f32; 4]> {
        let hand: Vec<[f32; 4]> = self.hand.iter().map(|v| v.at).collect();
        if !hand.is_empty() {
            return hand;
        }
        self.trump.iter().chain(&self.opp_slot).chain(&self.my_slot).map(|v| v.at).collect()
    }
}

impl Located for ScopaScene {
    fn spots(&self) -> Vec<[f32; 4]> {
        let hand: Vec<[f32; 4]> = self.hand.iter().map(|v| v.at).collect();
        if !hand.is_empty() {
            return hand;
        }
        self.table.iter().map(|v| v.at).collect()
    }
}

/// Dove è stata presa una schermata: il monitor e l'area letta.
type Place = (capture::Geom, screen::Area);

/// Dove stanno le carte lette: il posto della schermata analizzata (il monitor dove sono state
/// viste davvero, non quello che si sta provando) e di quanto si sono spostate da allora.
#[derive(Default)]
struct Spot {
    place: Option<Place>,
    template: Option<track::Template>,
    shift: [f32; 2],
    /// Catture di fila in cui la zona delle carte non si ritrova: le carte non ci sono più (o lo
    /// schermo è cambiato) e non vanno più segnate finché un'analisi non le ritrova.
    misses: u32,
}

/// Dopo quante catture senza ritrovare le carte (un decimo di secondo l'una) non si segnano più.
const MISSES: u32 = 2;

/// Legge lo schermo in un thread a parte, qualche volta al secondo, finché non viene fermato.
pub struct Reader<S> {
    pub rx: Receiver<Reading<S>>,
    stop: Arc<AtomicBool>,
    last: Arc<Mutex<Option<Img>>>,
    /// Dove stanno le carte dell'ultima scena sul desktop.
    spot: Arc<Mutex<Spot>>,
}

/// Ogni quanto si cattura lo schermo: spesso, perché costa poco. Le schermate si analizzano poi
/// tutte, in ordine, anche se l'analisi è più lenta: un sito che chiude una presa in mezzo secondo
/// non deve far perdere nessuna carta.
const CAPTURE_EVERY: std::time::Duration = std::time::Duration::from_millis(100);

impl<S: Located + Send + 'static> Reader<S> {
    /// `read` trova e riconosce le carte di un gioco; `every` è l'intervallo massimo fra due letture
    /// quando lo schermo non cambia.
    pub fn start(
        mem: Arc<Mutex<Memory>>,
        ctx: eframe::egui::Context,
        every: std::time::Duration,
        read: fn(&Img, &Mutex<Memory>) -> Option<S>,
    ) -> Reader<S> {
        let (tx, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let last = Arc::new(Mutex::new(None));
        let queue = Arc::new((Mutex::new(Frames::new(every.max(std::time::Duration::from_millis(600)))), Condvar::new()));
        // Monitor da catturare (l'analisi passa al successivo se non trova il tavolo) e errori.
        let monitor = Arc::new(AtomicUsize::new(screen::area().lock().unwrap().monitor));
        let monitors = Arc::new(AtomicUsize::new(1));
        let (err_tx, err_rx) = channel::<String>();
        let spot: Arc<Mutex<Spot>> = Arc::new(Mutex::new(Spot::default()));

        // Cattura: ogni CAPTURE_EVERY, e tiene le schermate in cui qualcosa è cambiato.
        {
            let (flag, queue, monitor, monitors) = (stop.clone(), queue.clone(), monitor.clone(), monitors.clone());
            let spot = spot.clone();
            std::thread::spawn(move || {
                let mut chosen = None;
                while !flag.load(Ordering::Relaxed) {
                    let t0 = std::time::Instant::now();
                    let area = *screen::area().lock().unwrap_or_else(|e| e.into_inner());
                    if chosen != Some(area) {
                        // Area appena scelta: si legge il suo monitor.
                        monitor.store(area.monitor, Ordering::Relaxed);
                        chosen = Some(area);
                    }
                    match capture::grab_geom(monitor.load(Ordering::Relaxed)) {
                        Ok((img, count, geom)) => {
                            monitors.store(count.max(1), Ordering::Relaxed);
                            let table = shrink(screen::table(&img, &area));
                            follow(&spot, &table, (geom, area));
                            let (lock, cv) = &*queue;
                            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
                            if q.offer_at(table, Some((geom, area)), std::time::Instant::now()) {
                                cv.notify_one();
                            }
                        }
                        Err(e) => {
                            let _ = err_tx.send(e);
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
        std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                let table = {
                    let (lock, cv) = &*queue;
                    let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
                    loop {
                        if let Some(t) = q.next_at() {
                            break Some(t);
                        }
                        if flag.load(Ordering::Relaxed) {
                            break None;
                        }
                        q = cv.wait_timeout(q, std::time::Duration::from_millis(500)).unwrap_or_else(|e| e.into_inner()).0;
                    }
                };
                let reading = match table {
                    None => match err_rx.try_recv() {
                        Ok(e) => Reading::Error(e),
                        Err(_) => continue,
                    },
                    Some((table, at)) => {
                        // Un errore nella lettura di una schermata non deve fermare le letture
                        // dopo: si mostra e si continua.
                        let scene = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read(&table, &mem)));
                        *frame.lock().unwrap_or_else(|e| e.into_inner()) = Some(table.clone());
                        match scene {
                            Err(e) => {
                                let why = e
                                    .downcast_ref::<&str>()
                                    .map(|s| s.to_string())
                                    .or_else(|| e.downcast_ref::<String>().cloned())
                                    .unwrap_or_default();
                                Reading::Error(format!("errore interno ({why})"))
                            }
                            Ok(Some(s)) => {
                                // Le carte sono su questo monitor: la mosca va qui, e da qui si
                                // insegue la zona delle carte nelle catture che seguono.
                                let mut sp = found.lock().unwrap_or_else(|e| e.into_inner());
                                if at.is_some() {
                                    sp.place = at;
                                }
                                sp.template = track::union(&s.spots()).and_then(|u| track::Template::new(&table, u));
                                sp.shift = [0.0, 0.0];
                                sp.misses = 0;
                                Reading::Scene(s)
                            }
                            Ok(None) => {
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
                if tx.send(reading).is_err() {
                    flag.store(true, Ordering::Relaxed);
                    break;
                }
                ctx.request_repaint();
            }
        });
        Reader { rx, stop, last, spot }
    }
}

impl<S> Reader<S> {
    /// Il monitor dove sono state viste le carte (None finché non se ne vede nessuna).
    pub fn monitor(&self) -> Option<capture::Geom> {
        self.spot.lock().unwrap_or_else(|e| e.into_inner()).place.map(|p| p.0)
    }

    /// Vero se le carte dell'ultima scena non si vedono più sullo schermo (sparite, coperte, o lo
    /// schermo è cambiato del tutto): finché un'analisi non le ritrova non vanno segnate.
    pub fn lost(&self) -> bool {
        self.spot.lock().unwrap_or_else(|e| e.into_inner()).misses >= MISSES
    }

    /// Dove sta adesso sul desktop, in pixel veri (sinistra, alto, destra, basso), una carta
    /// dell'ultima scena letta (`CardView::at`), contando di quanto si è spostato lo schermo.
    pub fn on_desktop(&self, at: [f32; 4]) -> Option<[f32; 4]> {
        let sp = self.spot.lock().unwrap_or_else(|e| e.into_inner());
        let (g, area) = sp.place?;
        let [dx, dy] = sp.shift;
        Some(place_on_desktop([at[0] + dx, at[1] + dy, at[2] + dx, at[3] + dy], g, area))
    }

    /// L'ultima schermata letta (già ritagliata sul tavolo), per la diagnosi.
    pub fn last_frame(&self) -> Option<Img> {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Nella cattura appena fatta ritrova la zona delle carte dell'ultima scena, se è dello stesso
/// monitor e della stessa area: così le carte segnate seguono lo schermo che si sposta.
fn follow(spot: &Mutex<Spot>, table: &Img, at: Place) {
    let (template, hint) = {
        let sp = spot.lock().unwrap_or_else(|e| e.into_inner());
        if sp.place != Some(at) {
            return;
        }
        match &sp.template {
            Some(t) => (t.clone(), sp.shift),
            None => return,
        }
    };
    let found = template.find(table, hint);
    let mut sp = spot.lock().unwrap_or_else(|e| e.into_inner());
    // Solo se nel frattempo non è arrivata una scena nuova.
    if sp.place == Some(at) && sp.template.as_ref().is_some_and(|t| t.same(&template)) {
        match found {
            Some(shift) => {
                sp.shift = shift;
                sp.misses = 0;
            }
            None => sp.misses += 1,
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
/// tenuta (o ogni tanto anche se non cambia). Se l'analisi resta indietro, si scartano le schermate
/// più simili a quella prima, mai l'ultima: uno stato breve (una presa che resta in tavola un
/// attimo) non si perde.
pub struct Frames {
    frames: std::collections::VecDeque<(Img, Vec<u8>, Option<Place>)>,
    last: Option<Vec<u8>>,
    pushed: Option<std::time::Instant>,
    every: std::time::Duration,
}

/// Quante schermate al più restano in attesa.
const FRAMES: usize = 8;

impl Frames {
    pub fn new(every: std::time::Duration) -> Frames {
        Frames { frames: Default::default(), last: None, pushed: None, every }
    }

    /// Offre una schermata appena catturata; vero se è stata tenuta.
    #[cfg(test)]
    pub fn offer(&mut self, img: Img, now: std::time::Instant) -> bool {
        self.offer_at(img, None, now)
    }

    /// Come `offer`, ricordando dove è stata presa la schermata.
    pub fn offer_at(&mut self, img: Img, at: Option<Place>, now: std::time::Instant) -> bool {
        let sig = signature(&img);
        let changed = self.last.as_ref().is_none_or(|l| difference(l, &sig) >= 1.5);
        let stale = self.pushed.is_none_or(|t| now.duration_since(t) >= self.every);
        if !changed && !stale {
            return false;
        }
        if self.frames.len() >= FRAMES {
            // Si scarta la schermata (non l'ultima) più simile a quella prima.
            let drop = (1..self.frames.len() - 1)
                .min_by(|&a, &b| {
                    let d = |i: usize| difference(&self.frames[i - 1].1, &self.frames[i].1);
                    d(a).total_cmp(&d(b))
                })
                .unwrap_or(0);
            self.frames.remove(drop);
        }
        self.last = Some(sig.clone());
        self.pushed = Some(now);
        self.frames.push_back((img, sig, at));
        true
    }

    /// La prossima schermata da analizzare.
    #[cfg(test)]
    pub fn next(&mut self) -> Option<Img> {
        self.next_at().map(|f| f.0)
    }

    /// La prossima schermata da analizzare, con dove è stata presa.
    pub fn next_at(&mut self) -> Option<(Img, Option<Place>)> {
        self.frames.pop_front().map(|f| (f.0, f.2))
    }
}

/// Impronta di una schermata: la luminosità media su una griglia 48 x 32.
fn signature(img: &Img) -> Vec<u8> {
    let (gw, gh) = (48usize, 32usize);
    let mut out = Vec::with_capacity(gw * gh);
    for j in 0..gh {
        for i in 0..gw {
            let (x0, x1) = (i * img.w / gw, ((i + 1) * img.w / gw).max(i * img.w / gw + 1).min(img.w));
            let (y0, y1) = (j * img.h / gh, ((j + 1) * img.h / gh).max(j * img.h / gh + 1).min(img.h));
            let (mut sum, mut n) = (0u32, 0u32);
            // Un punto ogni due per lato basta.
            for y in (y0..y1).step_by(2) {
                for x in (x0..x1).step_by(2) {
                    let p = img.at(x, y);
                    sum += (p[0] as u32 * 3 + p[1] as u32 * 6 + p[2] as u32) / 10;
                    n += 1;
                }
            }
            out.push((sum / n.max(1)) as u8);
        }
    }
    out
}

/// Differenza media fra due impronte (0-255).
fn difference(a: &[u8], b: &[u8]) -> f32 {
    if a.len() != b.len() {
        return 255.0;
    }
    a.iter().zip(b).map(|(&x, &y)| (x as i32 - y as i32).unsigned_abs()).sum::<u32>() as f32 / a.len().max(1) as f32
}

/// Le schermate molto grandi (monitor 4K) si rimpiccioliscono: le carte restano grandi abbastanza
/// e l'analisi è molto più veloce.
pub fn shrink(img: Img) -> Img {
    let k = img.w.div_ceil(2000);
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

impl<S> Drop for Reader<S> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use briscola_core::cards;

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
        let s = screenshot();
        check(&s);
        assert!(s.trump.as_ref().unwrap().partial);
        // Ogni carta intera stacca la seconda abbastanza da essere inserita senza conferma (la
        // briscola vista a metà, finché non è in memoria, si fa confermare).
        for v in s.opp_slot.iter().chain(&s.my_slot).chain(&s.hand) {
            let (c, margin) = v.best(CardSet::FULL).unwrap();
            assert!(margin >= 1.4, "{} distacco {margin}", cards::name(c));
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
        for f in &a.cards {
            let (c, m) = f.view.best(cards::CardSet::FULL).unwrap();
            println!(
                "card ({:.0},{:.0}) {:.0}x{:.0} land {} vis {:.2?} score {:.3} {} {:.2}",
                f.rect.cx,
                f.rect.cy,
                2.0 * f.rect.hw,
                2.0 * f.rect.hh,
                f.rect.landscape(),
                f.view.vis,
                f.score,
                cards::name(c),
                m
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
    fn check(img: &Img, want: &[(&str, Card)]) -> Vec<(String, bool, bool)> {
        let s = read_briscola(img, &Mutex::new(Memory::in_memory()));
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
                        " (sbagliata)".to_string()
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
            let deal =
                Deal { hand: vec![&h0.1, &h1.1, &h2.1], opp: Some(&o.1), mine: Some(&m.1), trump: Some(&t.1), deck: true };
            let img = table(&deal, look);
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

    fn known_deck(style: usize) -> Vec<(Card, Img)> {
        // Un ordine che mescola semi e valori.
        (0..40u8).map(|i| (i * 7) % 40).map(|c| (c, reference::card_image(style, c))).collect()
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
            let (ok, auto, n) = run(&deck, label, true, false);
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
                rows.push((m, format!("{:<20} -> {:<20} distacco {m:5.2}  seconda {}", cards::name(want), cards::name(got), cards::name(sc[1].0))));
            }
        }
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, r) in rows.iter().take(15) {
            println!("{r}");
        }
    }
}

