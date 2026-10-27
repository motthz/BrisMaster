//! Lettura della partita dallo schermo: trova le carte sul tavolo verde del sito, le riconosce
//! (seme dal colore, valore dal numero di simboli, figure dai disegni) e le passa all'assistente.
//! Legge solo i pixel dello schermo: non tocca il sito e non gioca al posto dell'utente.

pub mod capture;
pub mod classify;
pub mod detect;
pub mod french;
pub mod memory;

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
    /// Carta raddrizzata, CARD_W x CARD_H.
    pub img: Img,
    /// Vista solo in parte (la briscola sotto il mazzo).
    pub partial: bool,
    /// Punteggio di somiglianza per ciascuna delle 40 carte (più alto = più probabile).
    pub scores: [f32; 40],
}

pub const CARD_W: usize = 100;
pub const CARD_H: usize = 160;

use briscola_core::cards::{Card, CardSet};
pub use detect::ScopaScene;
use memory::Memory;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

/// Somma ai punteggi di forma e colore la somiglianza con le carte già viste.
pub fn recognize(view: &mut CardView, mem: &Memory) {
    let mut s = classify::structural(&view.img, view.partial);
    for (c, sim) in mem.similarity(&view.img, view.partial).iter().enumerate() {
        if let Some(v) = *sim {
            if v >= memory::SAME {
                s[c] += 6.0 + (v - memory::SAME) * 20.0;
            }
        }
    }
    view.scores = s;
}

impl CardView {
    /// La carta più probabile fra `among`, e quanto stacca la seconda.
    pub fn best(&self, among: CardSet) -> Option<(Card, f32)> {
        let mut v: Vec<(Card, f32)> = among.iter().map(|c| (c, self.scores[c as usize])).collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let first = *v.first()?;
        let margin = v.get(1).map_or(f32::INFINITY, |s| first.1 - s.1);
        Some((first.0, margin))
    }
}

/// Esito di una lettura dello schermo.
pub enum Reading<S> {
    Scene(S),
    /// Nessun tavolo con carte sullo schermo.
    NoTable,
    Error(String),
}

/// Lettura per la briscola: trova le carte e le riconosce (None se non c'è nessuna carta scoperta).
pub fn read_briscola(img: &Img, mem: &Memory) -> Option<Scene> {
    let mut scene = detect::scene(img);
    let found = scene.trump.is_some() || !scene.hand.is_empty() || scene.opp_slot.is_some() || scene.my_slot.is_some();
    if !found {
        return None;
    }
    for v in scene.trump.iter_mut().chain(&mut scene.opp_slot).chain(&mut scene.my_slot).chain(&mut scene.hand) {
        recognize(v, mem);
    }
    Some(scene)
}

/// Lettura per la scopa: mano e carte in tavola, riconosciute (None se il tavolo non si vede).
pub fn read_scopa(img: &Img, mem: &Memory) -> Option<ScopaScene> {
    let mut scene = detect::scopa(img)?;
    for v in scene.hand.iter_mut().chain(&mut scene.table) {
        recognize(v, mem);
    }
    Some(scene)
}

/// Legge lo schermo in un thread a parte, qualche volta al secondo, finché non viene fermato.
pub struct Reader<S> {
    pub rx: Receiver<Reading<S>>,
    stop: Arc<AtomicBool>,
}

impl<S: Send + 'static> Reader<S> {
    /// `read` trova e riconosce le carte di un gioco; `every` è l'intervallo fra due letture.
    pub fn start(
        mem: Arc<Mutex<Memory>>,
        ctx: eframe::egui::Context,
        every: std::time::Duration,
        read: fn(&Img, &Memory) -> Option<S>,
    ) -> Reader<S> {
        let (tx, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::spawn(move || {
            let mut monitor = 0;
            while !flag.load(Ordering::Relaxed) {
                let t0 = std::time::Instant::now();
                let reading = match capture::grab(monitor) {
                    Ok((img, count)) => {
                        let scene = read(&img, &mem.lock().unwrap());
                        match scene {
                            Some(s) => Reading::Scene(s),
                            None => {
                                // Prova il monitor successivo.
                                monitor = (monitor + 1) % count.max(1);
                                Reading::NoTable
                            }
                        }
                    }
                    Err(e) => Reading::Error(e),
                };
                if tx.send(reading).is_err() {
                    break;
                }
                ctx.request_repaint();
                std::thread::sleep(every.saturating_sub(t0.elapsed()));
            }
        });
        Reader { rx, stop }
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

    fn screenshot() -> Scene {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/sito_briscola.png");
        let img = capture::load_png(&path).unwrap();
        let mut scene = detect::scene(&img);
        let mem = Memory::in_memory();
        for v in scene.trump.iter_mut().chain(&mut scene.opp_slot).chain(&mut scene.my_slot).chain(&mut scene.hand) {
            recognize(v, &mem);
        }
        scene
    }

    fn best(v: &CardView) -> Card {
        v.best(CardSet::FULL).unwrap().0
    }

    /// Screenshot vero del sito: l'avversario ha giocato il 2 di bastoni, tu il 7 di spade,
    /// in mano hai fante di denari e asso di coppe, la briscola è il cavallo di denari.
    #[test]
    fn reads_real_screenshot() {
        let s = screenshot();
        let opp = s.opp_slot.as_ref().expect("carta dell'avversario");
        let mine = s.my_slot.as_ref().expect("carta tua");
        assert_eq!(cards::name(best(opp)), cards::name(cards::make(3, 1)));
        assert_eq!(cards::name(best(mine)), cards::name(cards::make(2, 6)));
        assert_eq!(s.hand.len(), 2);
        assert_eq!(cards::name(best(&s.hand[1])), cards::name(cards::make(1, 0)));
        // Il fante: figura, fante o re.
        let f = best(&s.hand[0]);
        assert!(matches!(cards::rank(f), 7 | 9), "{}", cards::name(f));
        assert_eq!(cards::suit(f), 0, "tiene in mano una moneta: denari");
        let t = s.trump.as_ref().expect("briscola");
        assert!(t.partial);
        assert_eq!(cards::rank(best(t)), 8, "{}", cards::name(best(t)));
    }

    /// Una carta ricordata si riconosce dalla memoria anche quando le forme non bastano.
    #[test]
    fn memory_decides_figures() {
        let s = screenshot();
        let mut mem = Memory::in_memory();
        let fante = cards::make(0, 7);
        mem.remember(&s.hand[0].img, fante);
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
        let s = read_scopa(&img, &Memory::in_memory()).expect("tavolo");
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
            if detect::is_felt(*p) || p[0] > 150 {
                *p = felt;
            }
        }
        let s = read_scopa(&img, &Memory::in_memory()).expect("tavolo");
        assert!(s.hand.is_empty() && s.table.is_empty());
    }
}
