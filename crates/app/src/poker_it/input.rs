//! Inserimento veloce per il poker all'italiana: tastiera senza Invio e lettura delle tue carte
//! dallo schermo.
//!
//! Tastiera: le carte si scrivono valore e seme ("10c", "Aq", "Jf", "7p": cuori, quadri, fiori,
//! picche); Spazio fa quello che consiglia il motore; L lascia, V vede o parola, R punta o rilancia
//! (per te o per l'avversario, a seconda di chi tocca); al cambio 1-5 scelgono le tue carte e Invio
//! conferma, per l'avversario 0-5 dicono quante ne ha cambiate; Tab cambia chi parla per primo.
//!
//! Schermo: si leggono solo le tue carte (le 5 distribuite e quelle pescate al cambio); quando dalla
//! tua mano spariscono delle carte al tuo cambio, il cambio si inserisce da solo. Non si tocca il sito.

use super::{PokerIt, Stage};
use crate::cards_ui::GOLD;
use crate::vision::french::{self, PokerScene};
use crate::vision::memory::Memory;
use crate::vision::{CardView, Img, Reader, Reading};
use eframe::egui::{self, vec2, Color32, RichText, Stroke};
use poker_it::cards::{name, parse, Card, NUM_CARDS};
use poker_it::{Action, Phase};
use std::sync::{Arc, Mutex};

/// Distacco minimo fra la carta più probabile e la seconda per inserirla senza chiedere.
const AUTO_MARGIN: f32 = 1.4;

/// Lettura delle carte dallo schermo.
#[derive(Default)]
pub(super) struct Vision {
    reader: Option<Reader<PokerScene>>,
    memory: Arc<Mutex<Memory>>,
    status: String,
    /// Carte riconosciute all'ultima lettura: si agisce solo se la lettura si ripete uguale
    /// (niente carte a metà animazione).
    seen: Option<Vec<Option<Card>>>,
    /// Carta riconosciuta con poca sicurezza, in attesa che l'utente confermi o la corregga.
    confirm: Option<Confirm>,
    /// Ultima carta inserita dalla lettura: fase, immagine dell'angolo e quante azioni da annullare
    /// c'erano prima, per dimenticarla se l'utente la annulla.
    last: Option<(Stage, Card, Img, usize)>,
    scene: Option<PokerScene>,
    /// Carte annullate dall'utente: se ricompaiono si chiede conferma invece di inserirle.
    doubts: Vec<(Stage, Card)>,
}

struct Confirm {
    stage: Stage,
    card: Card,
    img: Img,
    texture: Option<egui::TextureHandle>,
}

impl Vision {
    /// Mano nuova: si tiene la lettura accesa e la memoria, il resto si azzera.
    pub(super) fn new_hand(&mut self) {
        self.seen = None;
        self.confirm = None;
        self.last = None;
        self.doubts.clear();
    }
}

fn top(v: &CardView) -> f32 {
    v.scores[..NUM_CARDS].iter().copied().fold(f32::MIN, f32::max)
}

/// Abbina le carte viste a quelle già note: una vista corrisponde a una carta nota se questa è fra
/// le più probabili per quella vista. Restituisce le viste nuove e le carte note che non si vedono.
fn match_views(views: &[CardView], known: &[Card]) -> (Vec<usize>, Vec<Card>) {
    let mut used = vec![false; views.len()];
    let mut missing = Vec::new();
    for &k in known {
        let cand = views
            .iter()
            .enumerate()
            .filter(|(i, _)| !used[*i])
            .map(|(i, v)| (i, v.scores[k as usize] - top(v)))
            .filter(|x| x.1 >= -1.0)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match cand {
            Some((i, _)) => used[i] = true,
            None => missing.push(k),
        }
    }
    ((0..views.len()).filter(|&i| !used[i]).collect(), missing)
}

/// Vero se `k` può essere l'inizio di una carta scritta (il valore senza il seme).
fn is_rank_prefix(k: &str) -> bool {
    matches!(k, "7" | "8" | "9" | "1" | "10" | "t" | "j" | "f" | "q" | "d" | "k" | "r" | "a")
}

impl PokerIt {
    /// Tastiera e lettura dello schermo: da chiamare a ogni fotogramma.
    pub(super) fn frame_input(&mut self, ctx: &egui::Context) {
        self.keyboard(ctx);
        self.poll_vision();
        if self.vision.reader.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
    }

    /// Le tue carte già note nella fase in corso.
    fn known_mine(&self) -> Vec<Card> {
        match self.stage {
            Stage::Setup => self.cards.clone(),
            Stage::NewCards => self.kept.iter().chain(&self.drawn).copied().collect(),
            Stage::Playing => self.hand.as_ref().map_or(Vec::new(), |h| h.hands[self.me as usize].to_vec()),
        }
    }

    /// Le carte che si possono inserire adesso.
    fn allowed(&self, c: Card) -> bool {
        match self.stage {
            Stage::Setup => !self.cards.contains(&c) && self.cards.len() < 5,
            Stage::NewCards => !self.known_mine().contains(&c) && !self.gone.contains(&c),
            Stage::Playing => false,
        }
    }

    /// Inserisce una tua carta (distribuita o pescata). Falso se non è possibile adesso.
    fn enter(&mut self, c: Card) -> bool {
        if !self.allowed(c) {
            return false;
        }
        match self.stage {
            Stage::Setup => {
                self.cards.push(c);
                if self.cards.len() == 5 {
                    self.start();
                }
            }
            Stage::NewCards => self.add_drawn(c),
            Stage::Playing => {}
        }
        true
    }

    /// Toglie l'ultima carta inserita a mano (Backspace) prima che la mano parta.
    fn remove_last_card(&mut self) {
        let removed = match self.stage {
            Stage::Setup => self.cards.pop(),
            Stage::NewCards => self.drawn.pop(),
            Stage::Playing => None,
        };
        if let (Some(c), Some((_, lc, img, _))) = (removed, &self.vision.last) {
            if *lc == c {
                self.vision.memory.lock().unwrap().forget(img);
                self.vision.doubts.push((self.stage, c));
                self.vision.last = None;
            }
        }
    }

    /// Dopo un "Annulla": se ha tolto la carta inserita dalla lettura, quella carta non si ricorda.
    pub(super) fn vision_undone(&mut self) {
        if let Some((stage, c, img, len)) = self.vision.last.take() {
            if self.undo.len() <= len {
                self.vision.memory.lock().unwrap().forget(&img);
                self.vision.doubts.push((stage, c));
            } else {
                self.vision.last = Some((stage, c, img, len));
            }
        }
        self.vision.confirm = None;
        self.vision.seen = None;
    }

    /// Fa quello che consiglia il motore (l'azione più frequente della sua strategia).
    fn play_best(&mut self) {
        let Some(adv) = self.current_advice() else {
            return;
        };
        if let Some(&(a, _)) = adv.iter().max_by(|a, b| a.1.total_cmp(&b.1)) {
            self.act(a);
        }
    }

    /// Inverte chi parla per primo, finché nessuno ha ancora agito.
    fn toggle_first(&mut self) {
        match self.stage {
            Stage::Setup => self.me ^= 1,
            Stage::Playing if self.hand.as_ref().is_some_and(|h| h.history.is_empty()) => {
                self.me ^= 1;
                self.deal();
                self.advice = None;
            }
            _ => {}
        }
    }

    // --- Tastiera ------------------------------------------------------------------------------

    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx) {
            return;
        }
        // Niente pulsanti col focus: Spazio e Invio li premerebbero insieme alla scorciatoia.
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            self.back();
        }
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Key { key, pressed: true, modifiers, .. } if modifiers.is_none() => self.key(key),
                egui::Event::Text(t) => {
                    for ch in t.chars().filter(|c| !c.is_whitespace()) {
                        self.type_char(ch.to_ascii_lowercase());
                    }
                }
                _ => {}
            }
        }
    }

    fn key(&mut self, key: egui::Key) {
        let h = self.hand.as_ref();
        let over = h.is_some_and(|h| h.is_over());
        let my_draw = h.is_some_and(|h| !h.is_over() && h.to_act == self.me && h.phase == Phase::Draw);
        match key {
            egui::Key::Space | egui::Key::Enter if self.vision.confirm.is_some() => self.accept_vision(),
            egui::Key::Space if self.stage == Stage::Playing => self.play_best(),
            egui::Key::Enter if self.stage == Stage::Playing && over => self.reset(),
            egui::Key::Enter if self.stage == Stage::Playing && my_draw => {
                let mask = self.discard.iter().enumerate().fold(0u8, |m, (i, &d)| m | ((d as u8) << i));
                self.act(Action::Draw(mask));
            }
            egui::Key::Tab => self.toggle_first(),
            egui::Key::Backspace => {
                if self.keys.pop().is_none() {
                    self.remove_last_card();
                }
            }
            egui::Key::Escape => {
                self.keys.clear();
                self.entry_error = None;
                if let Some(c) = self.vision.confirm.take() {
                    self.vision.doubts.push((c.stage, c.card));
                }
            }
            _ => {}
        }
    }

    fn type_char(&mut self, ch: char) {
        if self.stage == Stage::Playing {
            self.action_key(ch);
            return;
        }
        self.keys.push(ch);
        self.entry_error = None;
        let parsed = if self.keys.len() >= 2 { parse(&self.keys) } else { None };
        if let Some(c) = parsed {
            self.keys.clear();
            let seen = self.vision.confirm.take();
            // La carta scritta a mano si impara dall'immagine sullo schermo.
            let shown = self.unmatched_view().map(|v| v.img.clone());
            let stage = self.stage;
            if self.enter(c) {
                if let Some(img) = seen.filter(|s| s.stage == stage).map(|s| s.img).or(shown) {
                    self.vision.memory.lock().unwrap().remember(&french::corner(&img), c);
                }
            } else {
                self.entry_error = Some(format!("{} non è possibile adesso", name(c)));
            }
        } else if !is_rank_prefix(&self.keys) {
            self.entry_error =
                Some(format!("\"{}\" non è una carta: scrivi valore e seme, es. 10c, Aq, Jf, 7p", self.keys));
            self.keys.clear();
        }
    }

    /// Tasti durante la mano: azioni di puntata e cambio, tue o dell'avversario.
    fn action_key(&mut self, ch: char) {
        let Some(h) = self.hand.clone() else { return };
        if h.is_over() {
            return;
        }
        let mine = h.to_act == self.me;
        let digit = ch.to_digit(10).map(|d| d as usize);
        if h.phase == Phase::Draw {
            match (mine, digit) {
                (true, Some(d @ 1..=5)) => self.discard[d - 1] ^= true,
                (false, Some(k @ 0..=5)) => self.act(Action::Draw(((1u16 << k) - 1) as u8)),
                _ => {}
            }
            return;
        }
        let legal = h.legal();
        let a = match ch {
            'l' => Action::Fold,
            'v' => Action::Call,
            'r' => Action::Raise,
            _ => return,
        };
        if legal.contains(&a) && !(a == Action::Fold && h.to_call() == 0) {
            self.act(a);
        }
    }

    /// Riga sotto le carte: quello che si sta scrivendo, o l'errore.
    pub(super) fn entry_hint(&self, ui: &mut egui::Ui) {
        if let Some(e) = &self.entry_error {
            ui.label(RichText::new(e).color(Color32::from_rgb(255, 150, 130)));
        } else if !self.keys.is_empty() {
            ui.label(RichText::new(format!("Scrivi il seme: {}_ (c, q, f, p)", self.keys.to_uppercase())).color(GOLD));
        }
    }

    // --- Lettura dello schermo -----------------------------------------------------------------

    fn set_vision(&mut self, on: bool, ctx: &egui::Context) {
        if on && self.vision.reader.is_none() {
            self.vision.memory = Memory::shared_french();
            let every = std::time::Duration::from_millis(250);
            self.vision.reader = Some(Reader::start(self.vision.memory.clone(), ctx.clone(), every, french::read));
            self.vision.status = "Cerco il tavolo sullo schermo...".into();
        } else if !on {
            self.vision.reader = None;
            self.vision.confirm = None;
            self.vision.status.clear();
        }
    }

    fn poll_vision(&mut self) {
        let Some(reader) = &self.vision.reader else {
            return;
        };
        let mut latest = None;
        while let Ok(r) = reader.rx.try_recv() {
            latest = Some(r);
        }
        match latest {
            Some(Reading::Scene(scene)) => {
                self.vision.status = match scene.hand.len() {
                    0 => "Vedo il tavolo, nessuna tua carta scoperta".into(),
                    1 => "Vedo 1 tua carta".into(),
                    n => format!("Vedo {n} tue carte"),
                };
                self.sync(&scene);
                self.vision.scene = Some(scene);
            }
            Some(Reading::NoTable) => {
                self.vision.status = "Non vedo il tavolo: tieni la partita aperta e visibile sullo schermo".into()
            }
            Some(Reading::Error(e)) => self.vision.status = format!("Lettura dello schermo non riuscita: {e}"),
            None => {}
        }
    }

    /// Inserisce quello che si vede sullo schermo, se è uguale alla lettura precedente.
    fn sync(&mut self, scene: &PokerScene) {
        let sig: Vec<Option<Card>> = scene.hand.iter().map(|v| french::best(v, |_| true).map(|b| b.0)).collect();
        if self.vision.seen.as_ref() != Some(&sig) {
            self.vision.seen = Some(sig);
            return;
        }
        // Più passi in una volta: le cinque carte distribuite, poi il cambio...
        for _ in 0..12 {
            let before = (self.stage, self.undo.len(), self.cards.len(), self.drawn.len());
            self.sync_step(scene);
            let after = (self.stage, self.undo.len(), self.cards.len(), self.drawn.len());
            if after == before || self.vision.confirm.is_some() {
                break;
            }
        }
    }

    fn sync_step(&mut self, scene: &PokerScene) {
        // Si agisce solo con tutte e cinque le carte in vista.
        if self.vision.confirm.is_some() || scene.hand.len() != 5 {
            return;
        }
        let views = &scene.hand;
        let known = self.known_mine();
        let (new, missing) = match_views(views, &known);
        match self.stage {
            Stage::Setup | Stage::NewCards => {
                if let Some(&i) = new.first() {
                    let stage = self.stage;
                    self.offer(stage, &views[i]);
                }
            }
            Stage::Playing => {
                let Some(h) = &self.hand else { return };
                let me = self.me as usize;
                let my_draw = !h.is_over() && h.to_act == self.me && h.phase == Phase::Draw;
                if my_draw && !new.is_empty() && new.len() == missing.len() {
                    // Hai cambiato sul sito: le carte sparite sono quelle scartate.
                    let mask = (0..5).filter(|&i| missing.contains(&h.hands[me][i])).fold(0u8, |m, i| m | (1 << i));
                    self.act(Action::Draw(mask));
                } else if new.len() >= 3 && !my_draw {
                    // Cinque carte diverse dalla tua mano: è cominciata la mano successiva.
                    self.reset();
                }
            }
        }
    }

    /// Propone la carta riconosciuta: la inserisce se è sicura, altrimenti chiede.
    fn offer(&mut self, stage: Stage, view: &CardView) {
        let Some((card, margin)) = french::best(view, |c| self.allowed(c)) else {
            return;
        };
        let doubtful = self.vision.doubts.contains(&(stage, card));
        if margin >= AUTO_MARGIN && !doubtful {
            let img = french::corner(&view.img);
            let len = self.undo.len();
            if self.enter(card) {
                self.vision.memory.lock().unwrap().remember(&img, card);
                self.vision.last = Some((stage, card, img, len));
            }
        } else {
            self.vision.confirm = Some(Confirm { stage, card, img: view.img.clone(), texture: None });
        }
    }

    /// L'utente conferma la carta riconosciuta con poca sicurezza: si inserisce e si ricorda.
    fn accept_vision(&mut self) {
        let Some(c) = self.vision.confirm.take() else {
            return;
        };
        if c.stage == self.stage && self.enter(c.card) {
            self.vision.memory.lock().unwrap().remember(&french::corner(&c.img), c.card);
        }
    }

    /// La carta sullo schermo non ancora inserita, se ce n'è una sola.
    fn unmatched_view(&self) -> Option<&CardView> {
        let scene = self.vision.scene.as_ref()?;
        match match_views(&scene.hand, &self.known_mine()).0.as_slice() {
            [i] => Some(&scene.hand[*i]),
            _ => None,
        }
    }

    /// Interruttore, stato e richiesta di conferma della lettura dello schermo.
    pub(super) fn vision_ui(&mut self, ui: &mut egui::Ui) {
        let mut on = self.vision.reader.is_some();
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut on, RichText::new("Leggi le mie carte dallo schermo").strong())
                .on_hover_text(
                    "BrisMaster guarda il tavolo del sito e inserisce da solo le tue 5 carte e quelle che peschi \
                     al cambio. Le puntate le inserisci tu (tasti L, V, R). Non tocca il sito: giochi sempre tu.",
                )
                .changed()
            {
                self.set_vision(on, ui.ctx());
            }
            if on {
                let n = self.vision.memory.lock().unwrap().len();
                ui.label(RichText::new(format!("{n}/32 carte in memoria")).small().weak()).on_hover_text(
                    "Le carte già riconosciute si ricordano da sole e la volta dopo si riconoscono al primo colpo",
                );
            }
        });
        if !on {
            return;
        }
        ui.label(RichText::new(&self.vision.status).small().weak());
        let ctx = ui.ctx().clone();
        if let Some(c) = &mut self.vision.confirm {
            let tex = c.texture.get_or_insert_with(|| {
                let rgba: Vec<u8> = c.img.px.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                let image = egui::ColorImage::from_rgba_unmultiplied([c.img.w, c.img.h], &rgba);
                ctx.load_texture("poker_vision_confirm", image, Default::default())
            });
            egui::Frame::new()
                .fill(Color32::from_rgb(60, 48, 20))
                .stroke(Stroke::new(1.5, GOLD))
                .corner_radius(8.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Image::new(&*tex).fit_to_exact_size(vec2(50.0, 80.0)));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("Mi sembra {}", name(c.card))).size(17.0).strong());
                            ui.label("Spazio = giusto · se sbaglio scrivi la carta (es. Kc) · Esc = ignora");
                        });
                    });
                });
        }
        ui.add_space(6.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poker_it::cards::make;

    fn load(f: &str) -> Img {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata").join(f);
        crate::vision::capture::load_png(&path).unwrap()
    }

    fn app() -> PokerIt {
        let mut p = PokerIt::load(None);
        p.vision.memory = Arc::new(Mutex::new(Memory::default()));
        p
    }

    /// Fa leggere la stessa schermata due volte (la lettura agisce solo se si ripete uguale).
    fn show(p: &mut PokerIt, f: &str) {
        let scene = french::read(&load(f), &Memory::default()).unwrap();
        p.sync(&scene);
        p.sync(&scene);
        p.vision.scene = Some(scene);
    }

    fn cards(v: &[(u8, u8)]) -> Vec<Card> {
        let mut c: Vec<Card> = v.iter().map(|&(r, s)| make(r, s)).collect();
        c.sort_unstable();
        c
    }

    /// Dallo schermo: le 5 carte entrano da sole e la mano parte.
    #[test]
    fn deal_from_screen() {
        let mut p = app();
        show(&mut p, "poker_classico.png");
        assert!(p.vision.confirm.is_none());
        assert_eq!(p.stage, Stage::Playing);
        let mut mine = p.hand.as_ref().unwrap().hands[0].to_vec();
        mine.sort_unstable();
        assert_eq!(mine, cards(&[(7, 0), (3, 3), (4, 2), (0, 1), (2, 0)]));
    }

    /// Al tuo cambio le carte sparite dalla mano sono gli scarti, quelle nuove le pescate.
    #[test]
    fn draw_from_screen() {
        let mut p = app();
        // Mano 8♥ Q♣ 7♠ 7♦ 9♦; sullo schermo dopo il cambio ci sono 8♥ Q♣ K♦ A♥ 10♠.
        for c in cards(&[(1, 3), (5, 1), (0, 0), (0, 2), (2, 2)]) {
            assert!(p.enter(c));
        }
        // Tu parli per primo: parola, parola dell'avversario, poi il tuo cambio.
        p.act(Action::Call);
        p.act(Action::Call);
        let h = p.hand.as_ref().unwrap();
        assert_eq!((h.phase, h.to_act), (Phase::Draw, 0));
        show(&mut p, "poker_classico2.png");
        let h = p.hand.as_ref().unwrap();
        assert_eq!(p.stage, Stage::Playing, "cambio e pescate inseriti");
        assert_eq!(h.discards[0].map(|m| m.count_ones()), Some(3));
        let mut mine = h.hands[0].to_vec();
        mine.sort_unstable();
        assert_eq!(mine, cards(&[(1, 3), (5, 1), (6, 2), (7, 3), (3, 0)]));
    }

    /// Cinque carte nuove a mano finita: si riparte da solo con la mano successiva.
    #[test]
    fn next_hand_from_screen() {
        let mut p = app();
        show(&mut p, "poker_classico.png");
        p.act(Action::Raise);
        p.act(Action::Fold);
        assert!(p.hand.as_ref().unwrap().is_over());
        show(&mut p, "poker_jumbo.png");
        assert_eq!(p.stage, Stage::Playing);
        assert_eq!(p.me, 1, "chi parla per primo si alterna");
        let mut mine = p.hand.as_ref().unwrap().hands[1].to_vec();
        mine.sort_unstable();
        assert_eq!(mine, cards(&[(0, 2), (4, 1), (3, 0), (6, 3), (7, 1)]));
    }

    #[test]
    fn keyboard_entry() {
        let mut p = app();
        for ch in "10cjqaq7p".chars() {
            p.type_char(ch);
        }
        assert_eq!(p.stage, Stage::Setup);
        assert_eq!(p.cards, vec![make(3, 3), make(4, 2), make(7, 2), make(0, 0)]);
        assert!(p.entry_error.is_none());
        // Stessa carta due volte: errore.
        for ch in "aq".chars() {
            p.type_char(ch);
        }
        assert!(p.entry_error.is_some());
        p.type_char('9');
        p.type_char('f');
        assert_eq!(p.stage, Stage::Playing);
        // Parla per primo l'utente: Spazio fa il consiglio, poi l'avversario punta con R.
        p.key(egui::Key::Space);
        assert_eq!(p.hand.as_ref().unwrap().history.len(), 1);
        p.type_char('r');
        assert_eq!(p.hand.as_ref().unwrap().history.len(), 2);
    }
}
