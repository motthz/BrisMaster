//! La Psy Fly: una mosca psichica in pixel art che vola sopra il tavolo del sito, in una finestra
//! trasparente sempre in primo piano che lascia passare i clic, e va a posarsi sulla carta che il
//! motore consiglia. Non tocca il sito: indica soltanto.

use crate::vision::capture::Geom;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// Cosa deve fare la mosca, dedotto dalla partita.
#[derive(Clone, Debug, PartialEq)]
pub enum Guide {
    /// Il tavolo non si vede.
    NoTable,
    /// Tocca all'avversario (o bisogna inserire qualcosa): gironzola.
    Waiting,
    /// Action advice for games where a decision is not a card play (hit, stand, bet...).
    Decision(String),
    /// Tocca a te e il motore sta pensando.
    Thinking,
    /// The engine has a move, but its complete visual location is unavailable.
    Seeking,
    /// La carta da giocare e, nella scopa, le carte da prendere (pixel del desktop).
    Play {
        id: AdviceId,
        card: [f32; 4],
        take: Vec<[f32; 4]>,
    },
    /// One or more cards to select/discard, with game-specific action wording.
    Select { id: AdviceId, card: [f32; 4], take: Vec<[f32; 4]>, text: String },
    Won,
    Lost,
    Tie,
    /// Le carte lette non si vedono più per un attimo (il sito le sta animando, lo schermo è
    /// cambiato): niente da segnare, la mosca resta dov'è senza cambiare umore.
    Hidden,
}

/// Semantic identity: changing monitor position must not change the advised move.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AdviceId {
    pub card: u8,
    pub take: u64,
}

/// Quello che la mosca sa del tavolo.
pub struct Sight {
    /// Il monitor dove si gioca (None finché la lettura non lo trova).
    pub monitor: Option<Geom>,
    /// Le tue carte sullo schermo (pixel del desktop): ci gironzola sopra.
    pub hand: Vec<[f32; 4]>,
    pub guide: Guide,
    pub context: (u8, u64),
    pub played_card: Option<u8>,
}

impl Sight {
    /// Reject invalid capture/tracking geometry before constructing a native viewport.
    fn validated(mut self) -> Self {
        let Some(g) = self.monitor.filter(|g| {
            [g.x, g.y, g.w, g.h, g.scale].iter().all(|n| n.is_finite())
                && g.w > 1.0 && g.h > 1.0 && g.scale > 0.0
        }) else { return Self::off(); };
        let valid = |r: &[f32; 4]| {
            r.iter().all(|v| v.is_finite()) && r[2] > r[0] && r[3] > r[1]
                && r[0] < g.x + g.w && r[2] > g.x
                && r[1] < g.y + g.h && r[3] > g.y
        };
        self.hand.retain(valid);
        if let Guide::Play { card, take, .. } | Guide::Select { card, take, .. } = &self.guide {
            if !valid(card) || !take.iter().all(valid) { self.guide = Guide::Seeking; }
        }
        self
    }

    /// Niente da indicare: la lettura dello schermo è spenta.
    pub fn off() -> Sight {
        Sight { monitor: None, hand: Vec::new(), guide: Guide::NoTable, context: (0, 0), played_card: None }
    }
}

/// Per quanti secondi la carta da giocare resta segnata se la lettura la perde.
const STICKY: f32 = 2.0;

/// Un pixel della mosca, in punti dell'interfaccia.
const PX: f32 = 4.0;
const TITLE: &str = "BrisMaster - Psy Fly";

// Colori della mosca.
const OUTLINE: Color32 = Color32::from_rgb(34, 14, 48);
const BODY: Color32 = Color32::from_rgb(150, 96, 222);
const BODY_LIGHT: Color32 = Color32::from_rgb(206, 170, 255);
const BODY_DARK: Color32 = Color32::from_rgb(98, 52, 168);
const STRIPE: Color32 = Color32::from_rgb(70, 30, 120);
const EYE: Color32 = Color32::from_rgb(255, 250, 255);
const PUPIL: Color32 = Color32::from_rgb(20, 8, 30);
const BLUSH: Color32 = Color32::from_rgb(255, 128, 190);
const PSY: Color32 = Color32::from_rgb(200, 120, 255);
const PSY_PINK: Color32 = Color32::from_rgb(255, 110, 210);
const TAKE: Color32 = Color32::from_rgb(255, 205, 70);

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Spark,
    Heart,
    Star,
    Zed,
    Tear,
    Ring,
    Confetti,
    /// Un colpo nemico che punta la mosca: lei lo devia e diventa un colpo psichico amico.
    Orb,
    Deflected,
}

struct Part {
    pos: Pos2,
    vel: Vec2,
    age: f32,
    life: f32,
    kind: Kind,
    color: Color32,
    size: f32,
}

struct Bubble {
    text: String,
    age: f32,
    life: f32,
    /// Fumetto dei pensieri (a pallini) invece che parlato.
    thought: bool,
}

/// Stato d'animo, per accorgersi dei cambi.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mood {
    Lost,
    Bored,
    Sleepy,
    Thinking,
    Pointing,
    Party,
    Sad,
}

/// Small performances have their own clocks, separate from the game state.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Gesture {
    Wave,
    Stretch,
    Groom,
    Scan,
    Bow,
    Dance,
}

impl Gesture {
    fn duration(self) -> f32 {
        match self {
            Self::Wave => 1.8,
            Self::Stretch => 2.4,
            Self::Groom => 2.6,
            Self::Scan => 2.8,
            Self::Bow => 2.0,
            Self::Dance => 3.4,
        }
    }
}

pub struct Fly {
    pub on: bool,
    pos: Pos2,
    vel: Vec2,
    t: f32,
    /// Verso in cui guarda (1 a destra, -1 a sinistra) e verso disegnato, che lo segue girandosi.
    facing: f32,
    face: f32,
    /// Dove guardano le pupille.
    look: Vec2,
    next_blink: f32,
    blink: f32,
    mood: Mood,
    mood_t: f32,
    seeking: bool,
    /// La carta indicata l'ultima volta (per accorgersi che è cambiata o è stata giocata).
    target: Option<[f32; 4]>,
    /// Quale carta del percorso (carta da giocare, poi quelle da prendere) sta indicando.
    hop: usize,
    hop_t: f32,
    /// Picchiata sulla carta in corso: da 0 a 1.
    dive: Option<f32>,
    next_dive: f32,
    /// Giravolta in corso (da 0 a 1).
    spin: Option<f32>,
    next_spin: f32,
    /// "!" sopra la testa.
    alert: f32,
    /// Rimbalzo elastico del corpo (da 1 a 0): risveglio, tocchi, festa.
    boing: f32,
    /// Prossimo colpo nemico da deviare, quando si annoia.
    next_orb: f32,
    /// Il mouse dall'ultimo fotogramma (punti della finestra) e se il tasto sinistro era premuto.
    cursor: Option<Pos2>,
    was_down: bool,
    /// Da quanto il mouse le sta fermo vicino (grattini).
    pet: f32,
    /// Tempo prima della prossima reazione al mouse (per non reagire a ogni movimento).
    fuss: f32,
    /// Le gira la testa (da quando le hai dato un colpetto), in secondi.
    dizzy: f32,
    /// Scuote la testa ("no, quella!") e annuisce ("sì, quella!"), in secondi.
    shake: f32,
    nod: f32,
    /// La carta della mano su cui sta il mouse (per reagire una volta sola).
    hover_card: Option<usize>,
    /// Danza a otto in corso (da 0 a 1).
    eight: Option<f32>,
    /// Ultime posizioni, per la scia di immagini fantasma.
    trail: std::collections::VecDeque<Pos2>,
    trail_t: f32,
    /// Il mouse è passato sull'interruttore: saluta.
    greet: bool,
    hovered: bool,
    bubble: Option<Bubble>,
    next_chatter: f32,
    parts: Vec<Part>,
    rng: SmallRng,
    started: bool,
    /// Monitor su cui è stata sistemata la finestra (su Windows, a pixel veri).
    placed: Option<Geom>,
    /// L'ultima carta da giocare indicata e quando: se la lettura la perde per un attimo (la carta
    /// coperta dal mouse o da un'animazione del sito) resta segnata.
    last_play: Option<(Guide, std::time::Instant)>,
    /// Il monitor delle carte, per accorgersi che sono passate su un altro.
    monitor: Option<Geom>,
    /// Le carte segnate come sono disegnate adesso (scivolano verso la posizione letta).
    shown: Vec<Rect>,
    retarget: bool,
    played_confirmed: bool,
    advice: Option<AdviceId>,
    context: Option<(u8, u64)>,
    gesture: Option<(Gesture, f32)>,
    next_gesture: f32,
    bank: f32,
    wing_phase: f32,
    native_retry: f64,
    native_error: Option<String>,
}

impl Default for Fly {
    fn default() -> Self {
        Fly {
            on: false,
            pos: pos2(400.0, 300.0),
            vel: Vec2::ZERO,
            t: 0.0,
            facing: 1.0,
            face: 1.0,
            look: Vec2::ZERO,
            next_blink: 2.0,
            blink: 0.0,
            mood: Mood::Lost,
            mood_t: 0.0,
            seeking: false,
            target: None,
            hop: 0,
            hop_t: 0.0,
            dive: None,
            next_dive: 1.2,
            spin: None,
            next_spin: 6.0,
            alert: 0.0,
            boing: 0.0,
            next_orb: 7.0,
            cursor: None,
            was_down: false,
            pet: 0.0,
            fuss: 0.0,
            dizzy: 0.0,
            shake: 0.0,
            nod: 0.0,
            hover_card: None,
            eight: None,
            trail: Default::default(),
            trail_t: 0.0,
            greet: false,
            hovered: false,
            bubble: None,
            next_chatter: 3.0,
            parts: Vec::new(),
            rng: SmallRng::seed_from_u64(0x5917_f1e5),
            started: false,
            placed: None,
            last_play: None,
            monitor: None,
            shown: Vec::new(),
            retarget: false,
            played_confirmed: false,
            advice: None,
            context: None,
            gesture: Some((Gesture::Wave, 0.0)),
            next_gesture: 4.0,
            bank: 0.0,
            wing_phase: 0.0,
            native_retry: 0.0,
            native_error: None,
        }
    }
}

const PLAY: [&str; 7] = [
    "Questa!",
    "Gioca questa!",
    "Psss... questa!",
    "Fidati di me!",
    "Bzz! Qui!",
    "Questa è quella buona!",
    "Vai con questa!",
];
const PLAY_TAKE: [&str; 3] = ["Questa... e prendi queste!", "Bzz! Presa!", "Questa, e via le carte gialle!"];
const PLAYED: [&str; 4] = ["Ottimo!", "Così si fa!", "Bzzz-perfetto!", "Yeee!"];
const WAITING: [&str; 4] = ["Tocca all'avversario...", "Bzz bzz...", "Che giocherà?", "Ti tengo d'occhio le carte!"];
const SWAT: [&str; 4] = ["Deviato!", "Bzz-pam!", "Non mi prendi!", "Respinto!"];
const HELLO: [&str; 3] = ["Ciao!", "Eccomi, bzz!", "Mi hai chiamato?"];
const POKED: [&str; 4] = ["Ahia!", "Ehi! Mi gira la testa...", "Bzz... quante stelline", "Non si toccano le mosche!"];
const STARTLED: [&str; 3] = ["Ehi!", "Piano col mouse!", "Bzz! Che spavento!"];
const PETTED: [&str; 4] = ["Mmm, grattini!", "Bzzzz... che bello", "Ancora, ancora!", "Fusa da mosca: brrr"];
const NOT_THAT: [&str; 3] = ["No no, quella!", "Non quella!", "Bzz! L'altra!"];
const LOST: [&str; 3] = ["Dove sono le carte?", "Non vedo il tavolo!", "Bzz? Tutto buio..."];

impl Fly {
    /// L'interruttore della mosca: un pulsante a levetta con la mosca sopra. Vero se è cambiato.
    pub fn switch(&mut self, ui: &mut egui::Ui) -> bool {
        let size = vec2(46.0, 24.0);
        let (rect, mut resp) = ui.allocate_exact_size(size, egui::Sense::click());
        if resp.clicked() {
            self.on = !self.on;
            resp.mark_changed();
        }
        let k = ui.ctx().animate_bool_responsive(resp.id, self.on);
        let p = ui.painter();
        let track = lerp_color(Color32::from_gray(70), Color32::from_rgb(120, 60, 190), k);
        p.rect_filled(rect, 12.0, track);
        p.rect_stroke(rect, 12.0, Stroke::new(2.0, OUTLINE), StrokeKind::Inside);
        // La mosca fa da pomello: a riposo a sinistra, accesa a destra con l'aura.
        let x = egui::lerp(rect.left() + 12.0..=rect.right() - 12.0, k);
        let c = pos2(x, rect.center().y);
        if k > 0.0 {
            let t = ui.input(|i| i.time) as f32;
            let r = 10.0 + 1.5 * (t * 4.0).sin();
            p.circle_stroke(c, r, Stroke::new(1.5, PSY.gamma_multiply(0.7 * k)));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        }
        let flap = if self.on { ((ui.input(|i| i.time) * 20.0) as i32) % 3 } else { 1 };
        let mut pose = Pose::new(1.6);
        pose.flap = flap;
        // La mosca dell'interruttore guarda il mouse e sorride quando ci passa sopra.
        if let Some(m) = ui.input(|i| i.pointer.hover_pos()) {
            pose.eyes = Eyes::Open((m - c).normalized());
        }
        if resp.hovered() {
            pose.mouth = Mouth::Grin;
            if self.on && !self.hovered {
                self.greet = true;
            }
        }
        self.hovered = resp.hovered();
        draw_fly(p, c, pose);
        let resp = resp.on_hover_text(
            "Psy Fly: una mosca psichica vola sul tavolo del sito e si posa sulla carta che ti consiglio. \
             Lascia passare i clic: le carte le giochi sempre tu.",
        );
        let _ =
            ui.label(egui::RichText::new("Psy Fly").strong().color(if self.on { BODY_LIGHT } else { Color32::GRAY }));
        if let Some(error) = &self.native_error {
            ui.colored_label(Color32::LIGHT_RED, "Psy Fly: overlay non pronto").on_hover_text(error);
        }
        resp.changed()
    }

    /// Disegna la mosca nella sua finestra sopra il monitor del gioco (se è accesa e il monitor è noto).
    pub fn show(&mut self, ctx: &egui::Context, sight: Sight) {
        let sight = sight.validated();
        if !self.on {
            *self = Fly::default();
            return;
        }
        let Some(g) = sight.monitor else {
            self.last_play = None;
            self.shown.clear();
            self.advice = None;
            self.placed = None;
            self.target = None;
            self.monitor = None;
            self.context = None;
            self.played_confirmed = false;
            return;
        };
        self.played_confirmed = sight.played_card.is_some_and(|c| self.advice.is_some_and(|a| a.card == c));
        if self.context != Some(sight.context) {
            self.last_play = None;
            self.advice = None;
            if !self.played_confirmed {
                self.target = None;
            }
            self.shown.clear();
            self.hop = 0;
            self.hop_t = 0.0;
            self.context = Some(sight.context);
        }
        if self.monitor.is_some_and(|m| m != g) {
            // Le carte sono passate su un altro monitor: la mosca ci rientra volando dal bordo,
            // invece di comparire di colpo da un'altra parte.
            *self = Fly { on: true, context: Some(sight.context), ..Fly::default() };
        }
        self.monitor = Some(g);
        let guide = self.sticky(sight.guide.clone());
        let sight = Sight { guide, ..sight };
        // La finestra copre il monitor (un pixel in meno in basso: una finestra grande quanto lo
        // schermo Windows la tratterebbe da schermo intero e nasconderebbe la barra).
        let title = format!("{TITLE} ({})", std::process::id());
        let builder = egui::ViewportBuilder::default()
            .with_title(&title)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_active(false)
            .with_taskbar(false)
            .with_resizable(false)
            .with_position(pos2(g.x / g.scale, g.y / g.scale))
            .with_inner_size(vec2(g.w / g.scale, (g.h - 1.0) / g.scale));
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("psy_fly"), builder, |ui, _| {
            let now = ui.input(|i| i.time);
            if self.placed != Some(g) && now >= self.native_retry {
                match native::place(&title, g) {
                    Ok(()) => {
                        self.placed = Some(g);
                        self.native_error = None;
                    }
                    Err(e) => {
                        self.native_error = Some(e);
                        self.native_retry = now + 1.0;
                    }
                }
            }
            // Do not draw an overlay that could contaminate its own screen capture.
            if cfg!(windows) && self.placed != Some(g) {
                return;
            }
            let ppp = ui.ctx().pixels_per_point();
            let to_local = |r: [f32; 4]| {
                Rect::from_min_max(
                    pos2((r[0] - g.x) / ppp, (r[1] - g.y) / ppp),
                    pos2((r[2] - g.x) / ppp, (r[3] - g.y) / ppp),
                )
            };
            let screen = Rect::from_min_size(Pos2::ZERO, vec2(g.w / ppp, g.h / ppp));
            let hand: Vec<Rect> = sight.hand.iter().map(|&r| to_local(r)).collect();
            let dt = ui.input(|i| i.unstable_dt).clamp(0.001, 0.1);
            let guide = self.visual_play(&sight.guide, to_local, dt);
            let painter = ui.painter().clone();
            // Il mouse lo legge da Windows: la finestra della mosca lascia passare i clic.
            let mouse = native::cursor().map(|(x, y, down)| (pos2((x - g.x) / ppp, (y - g.y) / ppp), down));
            self.advance(dt, screen, &hand, &sight.guide, guide.as_ref());
            self.interact(dt, &hand, guide.as_ref(), mouse);
            if let Guide::Decision(text) | Guide::Select { text, .. } = &sight.guide {
                if self.bubble.as_ref().is_none_or(|b| b.text != *text) { self.say(text, 60.0); }
            }
            self.paint(&painter, guide.as_ref());
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    fn visual_play(&mut self, guide: &Guide, map: impl Fn([f32; 4]) -> Rect, dt: f32) -> Option<(Rect, Vec<Rect>)> {
        match guide {
            Guide::Play { id, card, take } | Guide::Select { id, card, take, .. } => {
                let all = std::iter::once(*card).chain(take.iter().copied()).map(map).collect();
                let all = self.glide(*id, all, dt);
                Some((all[0], all[1..].to_vec()))
            }
            _ => {
                if *guide != Guide::Hidden {
                    self.shown.clear();
                    self.advice = None;
                }
                None
            }
        }
    }

    /// Reazioni al mouse: lo guarda, si spaventa se arriva di corsa, fa le fusa se ci resta
    /// vicino, si stordisce se la clicchi, e mentre indica una carta annuisce sopra quella giusta
    /// e scuote la testa sopra le altre. `mouse` è in punti della finestra, col tasto sinistro.
    fn interact(&mut self, dt: f32, hand: &[Rect], play: Option<&(Rect, Vec<Rect>)>, mouse: Option<(Pos2, bool)>) {
        self.fuss = (self.fuss - dt).max(0.0);
        self.dizzy = (self.dizzy - dt).max(0.0);
        self.shake = (self.shake - dt).max(0.0);
        self.nod = (self.nod - dt).max(0.0);
        if std::mem::take(&mut self.greet) {
            self.gesture = Some((Gesture::Wave, 0.0));
            let s = self.pick(&HELLO);
            self.say(s, 1.6);
            self.boing = 0.8;
            self.spin = Some(0.0);
        }
        let Some((m, down)) = mouse else {
            self.cursor = None;
            return;
        };
        let speed = self.cursor.map_or(0.0, |c| (m - c).length() / dt.max(0.001));
        let click = down && !self.was_down;
        self.cursor = Some(m);
        self.was_down = down;
        let d = (m - self.pos).length();
        // Lo sguardo segue il mouse quando è vicino (se non sta indicando una carta).
        if d < 320.0 && self.mood != Mood::Pointing {
            let want = (m - self.pos) / 120.0;
            self.look += (want.clamp(vec2(-1.0, -1.0), vec2(1.0, 1.0)) - self.look) * (1.0 - (-10.0 * dt).exp());
        }
        // Un colpetto: le gira la testa.
        if click && d < 14.0 * PX {
            self.dizzy = 1.8;
            self.spin = Some(0.0);
            self.boing = 1.0;
            let at = self.pos;
            self.burst(at, 6, Kind::Star, 180.0);
            let s = self.pick(&POKED);
            self.say(s, 1.6);
            self.vel += vec2(self.rng.gen_range(-400.0..400.0), -500.0);
            self.fuss = 1.0;
            self.pet = 0.0;
            if self.mood == Mood::Sleepy {
                self.mood = Mood::Bored;
                self.mood_t = 0.0;
            }
            return;
        }
        // Il mouse che arriva di corsa la spaventa e scatta via (non mentre indica una carta: lì il
        // mouse corre fra le carte della mano).
        if d < 110.0 && speed > 1500.0 && self.fuss <= 0.0 && self.mood != Mood::Pointing {
            let away = (self.pos - m).normalized();
            self.vel += away * 1100.0;
            self.alert = 0.6;
            self.fuss = 2.5;
            let s = self.pick(&STARTLED);
            self.say(s, 1.2);
            if self.mood == Mood::Sleepy {
                self.mood = Mood::Bored;
                self.mood_t = 0.0;
            }
            return;
        }
        // Mouse fermo vicino: grattini.
        if d < 16.0 * PX && speed < 250.0 {
            self.pet += dt;
            if self.pet > 0.7 && self.fuss <= 0.0 {
                self.fuss = 0.35;
                let at = self.pos + vec2(self.rng.gen_range(-20.0..20.0), -30.0);
                let vx = self.rng.gen_range(-25.0..25.0);
                self.part(at, vec2(vx, -60.0), 1.3, Kind::Heart, PSY_PINK, 1.0);
                if self.bubble.is_none() {
                    let s = self.pick(&PETTED);
                    self.say(s, 1.6);
                }
            }
        } else {
            self.pet = (self.pet - dt * 2.0).max(0.0);
        }
        // Mentre indica: sopra la carta giusta annuisce, sopra un'altra della mano scuote la testa.
        if let (Mood::Pointing, Some((card, _))) = (self.mood, play) {
            if click && card.contains(m) {
                // La stai giocando!
                let at = self.pos;
                self.burst(at, 5, Kind::Spark, 120.0);
                self.say("Bzz! Aspetto la giocata…", 1.4);
                self.boing = 1.0;
                return;
            }
            let over = hand.iter().position(|r| r.contains(m));
            if over != self.hover_card {
                self.hover_card = over;
                match over {
                    Some(i) if hand[i].intersects(*card) && (hand[i].center() - card.center()).length() < 30.0 => {
                        self.nod = 0.8;
                        self.say("Sì, proprio quella!", 1.3);
                    }
                    Some(_) => {
                        self.shake = 0.8;
                        self.alert = 0.5;
                        let s = self.pick(&NOT_THAT);
                        self.say(s, 1.3);
                    }
                    None => {}
                }
            }
        } else {
            self.hover_card = None;
        }
    }

    /// La carta da giocare resta indicata per un po' anche se una lettura non la ritrova (tocca
    /// ancora a te ma la carta non si vede bene), finché la partita non va avanti.
    fn sticky(&mut self, guide: Guide) -> Guide {
        self.sticky_at(guide, std::time::Instant::now())
    }

    fn sticky_at(&mut self, guide: Guide, now: std::time::Instant) -> Guide {
        match guide {
            Guide::Play { .. } | Guide::Select { .. } => {
                self.last_play = Some((guide.clone(), now));
                guide
            }
            Guide::Thinking => match &self.last_play {
                Some((g, at)) if now.saturating_duration_since(*at).as_secs_f32() < STICKY => g.clone(),
                _ => guide,
            },
            _ => {
                self.last_play = None;
                guide
            }
        }
    }

    /// The outline uses the newest measured box directly. Only the fly has inertia.
    fn glide(&mut self, id: AdviceId, want: Vec<Rect>, _dt: f32) -> Vec<Rect> {
        self.retarget = self.advice != Some(id);
        self.advice = Some(id);
        self.shown = want;
        self.shown.clone()
    }

    fn say(&mut self, text: &str, life: f32) {
        self.bubble = Some(Bubble { text: text.to_string(), age: 0.0, life, thought: false });
    }

    fn pick<'a>(&mut self, list: &[&'a str]) -> &'a str {
        list[self.rng.gen_range(0..list.len())]
    }

    fn burst(&mut self, at: Pos2, n: usize, kind: Kind, speed: f32) {
        for _ in 0..n {
            let a = self.rng.gen_range(0.0..std::f32::consts::TAU);
            let s = self.rng.gen_range(0.4..1.0) * speed;
            let color = match kind {
                Kind::Heart => PSY_PINK,
                Kind::Confetti => {
                    [PSY, PSY_PINK, TAKE, Color32::from_rgb(110, 220, 255), Color32::from_rgb(140, 255, 160)]
                        [self.rng.gen_range(0..5)]
                }
                _ => {
                    if self.rng.gen_bool(0.5) {
                        PSY
                    } else {
                        PSY_PINK
                    }
                }
            };
            let life = self.rng.gen_range(0.6..1.3);
            self.parts.push(Part {
                pos: at,
                vel: vec2(a.cos(), a.sin()) * s,
                age: 0.0,
                life,
                kind,
                color,
                size: self.rng.gen_range(0.8..1.3),
            });
        }
    }

    fn part(&mut self, pos: Pos2, vel: Vec2, life: f32, kind: Kind, color: Color32, size: f32) {
        self.parts.push(Part { pos, vel, age: 0.0, life, kind, color, size });
    }

    /// Fa avanzare l'animazione di `dt` secondi.
    fn advance(&mut self, dt: f32, screen: Rect, hand: &[Rect], guide: &Guide, play: Option<&(Rect, Vec<Rect>)>) {
        // Fixed upper bound on simulation steps keeps motion consistent at 30/60/120 Hz.
        let steps = (dt / (1.0 / 120.0)).ceil().max(1.0) as u32;
        for _ in 0..steps {
            self.step(dt / steps as f32, screen, hand, guide, play);
        }
    }

    fn step(&mut self, dt: f32, screen: Rect, hand: &[Rect], guide: &Guide, play: Option<&(Rect, Vec<Rect>)>) {
        self.t += dt;
        let t = self.t;
        if !self.started {
            // Entra in scena dal bordo destro, a metà altezza.
            self.pos = pos2(screen.right() - 40.0, screen.center().y);
            self.vel = vec2(-600.0, -200.0);
            self.started = true;
            self.say("Bzz! Eccomi!", 1.8);
        }
        let hand_area = hand.iter().fold(None::<Rect>, |a, r| Some(a.map_or(*r, |a| a.union(*r))));
        let home =
            hand_area.map_or(pos2(screen.center().x, screen.bottom() - 240.0), |r| pos2(r.center().x, r.top() - 70.0));

        // Stato d'animo dalla partita.
        let mood = match guide {
            Guide::NoTable => Mood::Lost,
            Guide::Waiting => {
                if self.mood == Mood::Sleepy || (self.mood == Mood::Bored && self.mood_t > 16.0) {
                    Mood::Sleepy
                } else {
                    Mood::Bored
                }
            }
            Guide::Thinking | Guide::Seeking | Guide::Decision(_) => Mood::Thinking,
            Guide::Play { .. } | Guide::Select { .. } => Mood::Pointing,
            Guide::Won | Guide::Tie => Mood::Party,
            Guide::Lost => Mood::Sad,
            Guide::Hidden => self.mood,
        };
        let hidden = *guide == Guide::Hidden;
        let new_target =
            if hidden { self.target } else { play.map(|p| [p.0.left(), p.0.top(), p.0.right(), p.0.bottom()]) };
        // Una carta nuova da segnare, non la stessa carta che scivola con lo schermo.
        let moved = match (self.target, new_target) {
            _ if hidden => false,
            (Some(_), Some(_)) => std::mem::take(&mut self.retarget),
            (a, b) => a.is_some() != b.is_some(),
        };
        let source_changed = !hidden && self.seeking != (*guide == Guide::Seeking);
        if !hidden {
            self.seeking = *guide == Guide::Seeking;
        }
        if mood != self.mood || moved || source_changed {
            let was = self.mood;
            self.mood_t = 0.0;
            self.hop = 0;
            self.hop_t = 0.0;
            match mood {
                Mood::Pointing => {
                    self.alert = 0.7;
                    self.next_dive = 0.9;
                    self.gesture = Some((Gesture::Bow, 0.0));
                    let text =
                        if play.is_some_and(|p| !p.1.is_empty()) { self.pick(&PLAY_TAKE) } else { self.pick(&PLAY) };
                    self.say(text, 2.6);
                    let at = self.pos;
                    self.burst(at, 10, Kind::Spark, 220.0);
                }
                Mood::Party => {
                    let text = if *guide == Guide::Tie { "Pari e patta!" } else { "Evviva! Abbiamo vinto!" };
                    self.say(text, 4.0);
                    let at = self.pos;
                    self.burst(at, 40, Kind::Confetti, 420.0);
                    self.burst(at, 8, Kind::Heart, 160.0);
                    self.gesture = Some((Gesture::Dance, 0.0));
                }
                Mood::Sad => self.say("Uffa... la prossima la vinciamo!", 4.0),
                Mood::Thinking => {
                    if *guide == Guide::Seeking {
                        self.say("Bzz… cerco tutte le carte!", 3.0);
                    } else {
                        self.bubble = Some(Bubble { text: String::new(), age: 0.0, life: 60.0, thought: true });
                    }
                }
                Mood::Lost => self.say("Dove sono le carte?", 2.5),
                Mood::Sleepy => self.say("Zzz...", 2.0),
                Mood::Bored => {
                    if was == Mood::Pointing && self.target.is_some() && self.played_confirmed {
                        // La carta indicata è sparita dalla mano: giocata!
                        let text = self.pick(&PLAYED);
                        self.say(text, 2.0);
                        let at = self.pos;
                        self.burst(at, 6, Kind::Heart, 140.0);
                        self.spin = Some(0.0);
                    } else if was == Mood::Thinking {
                        self.bubble = None;
                    }
                }
            }
            if was == Mood::Thinking && mood != Mood::Thinking {
                if let Some(b) = &self.bubble {
                    if b.thought {
                        self.bubble = None;
                    }
                }
            }
            if was == Mood::Sleepy && mood != Mood::Sleepy {
                self.alert = 0.8;
                self.boing = 1.0;
            }
            self.mood = mood;
        }
        self.target = new_target;
        self.mood_t += dt;
        self.animate(dt, hidden);

        // Dove andare.
        let mt = self.mood_t;
        let mut goal = match self.mood {
            Mood::Lost => screen.center() + vec2((t * 0.5).sin() * screen.width() * 0.25, (t * 0.8).sin() * 120.0),
            Mood::Bored => home + vec2((t * 0.7).sin() * 160.0, (t * 1.3).sin() * 40.0 - 30.0),
            Mood::Sleepy => {
                // Si posa sopra l'ultima carta della mano, a destra.
                let r = hand.iter().max_by(|a, b| a.right().total_cmp(&b.right()));
                r.map_or(home, |r| pos2(r.right() - 14.0, r.top() - 6.0 * PX))
            }
            Mood::Thinking => home + vec2((t * 2.2).cos() * 60.0, (t * 2.2).sin() * 22.0),
            // Indicava una carta che per un attimo non si vede: resta lì sopra ad aspettare.
            Mood::Pointing if play.is_none() => self.pos,
            Mood::Pointing => {
                let (card, take) = play.expect("carta da indicare");
                let stops = 1 + take.len();
                self.hop_t += dt;
                let stay = if self.hop == 0 { 2.4 } else { 1.2 };
                if self.hop_t > stay && stops > 1 {
                    self.hop = (self.hop + 1) % stops;
                    self.hop_t = 0.0;
                    self.next_dive = 0.35;
                }
                let r = if self.hop == 0 { *card } else { take[self.hop - 1] };
                // Sopra la carta, un po' a zonzo.
                pos2(r.center().x, (r.top() - 9.0 * PX).max(screen.top() + 60.0))
                    + vec2((t * 2.0).cos() * 14.0, (t * 3.1).sin() * 6.0)
            }
            Mood::Party => home + vec2((t * 3.0).cos() * 130.0, (t * 3.0).sin() * 70.0 - 40.0),
            Mood::Sad => home + vec2((t * 0.5).sin() * 30.0, 20.0 + (t * 0.9).sin() * 6.0),
        };

        // Each performance eases in and out without moving the advice border.
        if let Some((gesture, age)) = self.gesture {
            let u = (age / gesture.duration()).clamp(0.0, 1.0);
            let envelope = (std::f32::consts::PI * u).sin().powi(2);
            match gesture {
                Gesture::Dance => {
                    goal += vec2(
                        (u * std::f32::consts::TAU * 2.0).sin() * 65.0,
                        -(u * std::f32::consts::TAU * 4.0).sin().abs() * 35.0,
                    ) * envelope
                }
                Gesture::Stretch => goal.y -= envelope * 12.0,
                Gesture::Scan => goal.x += (u * std::f32::consts::TAU).sin() * 25.0 * envelope,
                Gesture::Bow if self.mood == Mood::Pointing => goal.y += envelope * 12.0,
                _ => {}
            }
        }

        // Danza a otto, ogni tanto, quando si annoia.
        if let Some(e) = self.eight {
            let a = e * std::f32::consts::TAU * 2.0;
            goal += vec2(a.sin() * 90.0, (2.0 * a).sin() * 35.0);
            let ne = e + dt / 3.0;
            self.eight = (ne < 1.0 && matches!(self.mood, Mood::Bored | Mood::Party)).then_some(ne);
        }
        // Sbadiglio prima di addormentarsi.
        if self.mood == Mood::Bored && mt > 14.0 && mt - dt <= 14.0 {
            self.say("Aaaahm... che sonno", 1.8);
        }
        // Con la testa che gira barcolla.
        if self.dizzy > 0.0 {
            let wide = if self.mood == Mood::Pointing { 0.4 } else { 1.0 };
            goal += vec2((t * 7.0).sin() * 40.0, (t * 9.0).cos() * 18.0) * self.dizzy.min(1.0) * wide;
        }

        // Picchiata sulla carta: scende, la tocca (onda psichica) e risale.
        if self.mood == Mood::Pointing && play.is_some() {
            self.next_dive -= dt;
            if self.dive.is_none() && self.next_dive <= 0.0 && (self.pos - goal).length() < 50.0 {
                self.dive = Some(0.0);
            }
        } else {
            self.dive = None;
        }
        if let Some(d) = self.dive {
            let nd = d + dt / 0.5;
            let r = play.map_or(Rect::NOTHING, |(card, take)| {
                if self.hop == 0 {
                    *card
                } else {
                    take.get(self.hop - 1).copied().unwrap_or(*card)
                }
            });
            let depth = (r.top() + r.height() * 0.22) - goal.y;
            goal.y += depth * (nd.min(1.0) * std::f32::consts::PI).sin();
            if d < 0.5 && nd >= 0.5 && r.is_positive() {
                // Il tocco.
                let color = if self.hop == 0 { PSY } else { TAKE };
                self.part(r.center(), Vec2::ZERO, 0.7, Kind::Ring, color, 1.0);
                let tip = pos2(r.center().x, r.top() + r.height() * 0.22);
                self.burst(tip, 8, Kind::Spark, 260.0);
                self.boing = 0.6;
            }
            if nd >= 1.0 {
                self.dive = None;
                self.next_dive = self.rng.gen_range(1.4..2.4);
            } else {
                self.dive = Some(nd);
            }
        }

        // Volo: una molla un po' ballerina, più rapida quando indica una carta.
        let (k, damp) = match self.mood {
            Mood::Pointing => {
                (if self.dive.is_some() { 160.0 } else { 34.0 }, if self.dive.is_some() { 18.0 } else { 7.5 })
            }
            Mood::Sleepy => (10.0, 6.0),
            Mood::Party => (30.0, 6.0),
            _ => (8.0, 3.5),
        };
        let buzz = vec2((t * 9.0).sin() + (t * 23.0).cos() * 0.5, (t * 11.0).cos() + (t * 19.0).sin() * 0.5) * 60.0;
        let buzz = if self.mood == Mood::Sleepy && mt > 2.0 { Vec2::ZERO } else { buzz };
        (self.pos, self.vel) = spring(self.pos, self.vel, goal + buzz / k, k, damp, dt);
        let max = 1600.0;
        if self.vel.length() > max {
            self.vel = self.vel.normalized() * max;
        }
        self.pos = self.pos.clamp(screen.min + vec2(30.0, 30.0), screen.max - vec2(30.0, 30.0));

        // Bank into turns with damping; wings accelerate naturally with flight speed.
        let desired_bank = (self.vel.x / 2800.0).clamp(-0.28, 0.28);
        self.bank += (desired_bank - self.bank) * (1.0 - (-7.0 * dt).exp());
        let wing_hz = if self.mood == Mood::Sleepy { 1.0 } else { 8.0 + self.vel.length().min(1200.0) / 240.0 };
        self.wing_phase = (self.wing_phase + dt * wing_hz * std::f32::consts::TAU).rem_euclid(std::f32::consts::TAU);

        // Verso e sguardo.
        // Si gira solo quando vola davvero da quella parte, non per ogni ronzio sul posto.
        if self.vel.x.abs() > 260.0 {
            self.facing = self.vel.x.signum();
        }
        self.face += (self.facing - self.face) * (1.0 - (-14.0 * dt).exp());
        let look_at = match (self.mood, play) {
            (Mood::Pointing, Some((card, _))) => (card.center() - self.pos).normalized(),
            _ => (self.vel / 400.0).clamp(vec2(-1.0, -1.0), vec2(1.0, 1.0)),
        };
        self.look += (look_at - self.look) * (1.0 - (-8.0 * dt).exp());

        // Palpebre.
        self.next_blink -= dt;
        if self.next_blink <= 0.0 {
            self.blink = 0.14;
            self.next_blink = self.rng.gen_range(2.0..5.0);
        }
        self.blink = (self.blink - dt).max(0.0);
        self.alert = (self.alert - dt).max(0.0);

        // Giravolte ogni tanto.
        self.next_spin -= dt;
        if self.next_spin <= 0.0 && matches!(self.mood, Mood::Bored | Mood::Party | Mood::Lost) && self.spin.is_none() {
            if self.mood != Mood::Lost && self.eight.is_none() && self.rng.gen_bool(0.4) {
                self.eight = Some(0.0);
                self.say("Bzz-bzz, balletto!", 1.5);
            } else {
                self.spin = Some(0.0);
            }
            self.next_spin = if self.mood == Mood::Party { 1.5 } else { self.rng.gen_range(6.0..12.0) };
        }
        if let Some(s) = self.spin {
            let ns = s + dt / 0.7;
            self.spin = (ns < 1.0).then_some(ns);
            if ns >= 1.0 {
                let at = self.pos;
                self.burst(at, 5, Kind::Star, 120.0);
            }
        }

        // Colpi nemici da deviare, quando non ha niente da fare: arrivano da un lato e la mosca
        // li respinge con la sua forza psichica.
        self.next_orb -= dt;
        if self.next_orb <= 0.0 {
            self.next_orb = self.rng.gen_range(8.0..15.0);
            if matches!(self.mood, Mood::Bored | Mood::Lost | Mood::Thinking) {
                let side = if self.rng.gen_bool(0.5) { -1.0 } else { 1.0 };
                let from = pos2(self.pos.x + side * 420.0, self.pos.y + self.rng.gen_range(-160.0..60.0));
                let from = from.clamp(screen.min, screen.max);
                let vel = (self.pos - from).normalized() * 260.0;
                self.part(from, vel, 6.0, Kind::Orb, Color32::from_rgb(230, 50, 60), 1.0);
            }
        }
        let fly = self.pos;
        let mut swat = None;
        for p in self.parts.iter_mut().filter(|p| p.kind == Kind::Orb) {
            // Insegue la mosca, piano.
            let want = (fly - p.pos).normalized() * 280.0;
            p.vel += (want - p.vel) * (dt * 2.0).min(1.0);
            if (p.pos - fly).length() < 34.0 {
                p.kind = Kind::Deflected;
                p.color = PSY_PINK;
                p.age = 0.0;
                p.life = 1.6;
                p.vel = -p.vel.normalized() * 900.0 + vec2(0.0, -200.0);
                swat = Some(p.pos);
            }
        }
        if let Some(at) = swat {
            self.part(at, Vec2::ZERO, 0.45, Kind::Ring, PSY_PINK, 0.5);
            self.burst(at, 12, Kind::Spark, 320.0);
            self.boing = 0.8;
            self.alert = 0.5;
            if self.bubble.as_ref().is_none_or(|b| !b.thought) {
                let s = self.pick(&SWAT);
                self.say(s, 1.4);
            }
        }

        // Chiacchiere.
        self.next_chatter -= dt;
        if self.next_chatter <= 0.0 {
            self.next_chatter = self.rng.gen_range(9.0..16.0);
            if self.bubble.is_none() {
                match self.mood {
                    Mood::Bored => {
                        let s = self.pick(&WAITING);
                        self.say(s, 2.2);
                    }
                    Mood::Lost => {
                        let s = self.pick(&LOST);
                        self.say(s, 2.2);
                    }
                    _ => {}
                }
            }
        }

        // Scie, cuori, zeta, lacrime.
        let speed = self.vel.length();
        if speed > 500.0 && self.rng.gen_bool((1.0 - (1.0 - (speed / 2500.0).min(0.9)).powf(dt * 60.0)) as f64) {
            let p = self.pos - self.vel.normalized() * 22.0
                + vec2(self.rng.gen_range(-6.0..6.0), self.rng.gen_range(-6.0..6.0));
            let c = if self.rng.gen_bool(0.5) { PSY } else { PSY_PINK };
            self.part(p, -self.vel * 0.05, 0.45, Kind::Spark, c, 0.7);
        }
        let chance = |r: &mut SmallRng, per_s: f32| r.gen_bool((per_s * dt).min(1.0) as f64);
        match self.mood {
            Mood::Pointing if chance(&mut self.rng, 6.0) => {
                if let Some((card, _)) = play {
                    let x = self.rng.gen_range(card.left()..card.right());
                    let y = self.rng.gen_range(card.top()..card.bottom());
                    self.part(pos2(x, y), vec2(0.0, -30.0), 0.9, Kind::Star, Color32::WHITE, 0.8);
                }
            }
            Mood::Sleepy if mt > 1.5 && chance(&mut self.rng, 0.9) => {
                let p = self.pos + vec2(14.0 * self.face, -26.0);
                self.part(p, vec2(18.0, -38.0), 2.2, Kind::Zed, Color32::WHITE, 1.0);
            }
            Mood::Party if chance(&mut self.rng, 10.0) => {
                let x = self.rng.gen_range(screen.left()..screen.right());
                let c = [PSY, PSY_PINK, TAKE, Color32::from_rgb(110, 220, 255)][self.rng.gen_range(0..4)];
                let vx = self.rng.gen_range(-40.0..40.0);
                self.part(pos2(x, screen.top() - 10.0), vec2(vx, 240.0), 5.0, Kind::Confetti, c, 1.2);
            }
            Mood::Party if chance(&mut self.rng, 2.0) => {
                let p = self.pos + vec2(0.0, -20.0);
                let vx = self.rng.gen_range(-30.0..30.0);
                self.part(p, vec2(vx, -70.0), 1.4, Kind::Heart, PSY_PINK, 1.0);
            }
            Mood::Sad if chance(&mut self.rng, 1.5) => {
                let p = self.pos + vec2(6.0 * self.face, -2.0);
                self.part(p, vec2(self.face * 10.0, 20.0), 1.0, Kind::Tear, Color32::from_rgb(120, 200, 255), 1.0);
            }
            _ => {}
        }
        for p in &mut self.parts {
            p.age += dt;
            p.pos += p.vel * dt;
            match p.kind {
                Kind::Spark | Kind::Star => p.vel *= 1.0 - (dt * 3.0).min(1.0),
                Kind::Heart | Kind::Zed => p.vel.x += (p.age * 4.0).sin() * 30.0 * dt,
                Kind::Tear => p.vel.y += 500.0 * dt,
                Kind::Confetti => {
                    p.vel.y = (p.vel.y + 300.0 * dt).min(220.0);
                    p.vel.x += (p.age * 5.0 + p.size * 10.0).sin() * 120.0 * dt;
                }
                Kind::Ring | Kind::Orb => {}
                Kind::Deflected => p.vel.y += 200.0 * dt,
            }
        }
        // Scia dei colpi deviati.
        let trail: Vec<Pos2> =
            self.parts.iter().filter(|p| p.kind == Kind::Deflected && p.age < 1.0).map(|p| p.pos).collect();
        for at in trail {
            if self.rng.gen_bool(0.6) {
                self.part(at, Vec2::ZERO, 0.35, Kind::Spark, PSY, 0.6);
            }
        }
        self.boing = (self.boing - dt * 1.6).max(0.0);
        // Scia di immagini fantasma: le ultime posizioni, ogni pochi centesimi di secondo.
        self.trail_t += dt;
        if self.trail_t > 0.035 {
            self.trail_t = 0.0;
            self.trail.push_front(self.pos);
            self.trail.truncate(5);
        }
        self.parts.retain(|p| p.age < p.life && p.pos.y < screen.bottom() + 40.0);
        if self.parts.len() > 400 {
            let extra = self.parts.len() - 400;
            self.parts.drain(..extra);
        }
        if let Some(b) = &mut self.bubble {
            b.age += dt;
            if b.age > b.life {
                self.bubble = None;
            }
        }
    }

    fn animate(&mut self, dt: f32, hidden: bool) {
        if hidden {
            self.gesture = None;
            return;
        }
        if let Some((g, age)) = &mut self.gesture {
            *age += dt;
            if *age >= g.duration() {
                self.gesture = None;
            }
        }
        self.next_gesture -= dt;
        if self.gesture.is_none() && self.next_gesture <= 0.0 {
            self.next_gesture = self.rng.gen_range(5.0..9.0);
            let gesture = match self.mood {
                Mood::Bored => Some(if self.rng.gen_bool(0.5) { Gesture::Stretch } else { Gesture::Groom }),
                Mood::Thinking | Mood::Lost => Some(Gesture::Scan),
                Mood::Party => Some(Gesture::Dance),
                _ => None,
            };
            self.gesture = gesture.map(|g| (g, 0.0));
        }
    }

    fn paint(&self, p: &Painter, play: Option<&(Rect, Vec<Rect>)>) {
        let t = self.t;
        // Le carte indicate.
        if let Some((card, take)) = play {
            let pop = ease_out_back((self.mood_t / 0.35).min(1.0));
            for (i, r) in take.iter().enumerate() {
                let from = pos2(card.center().x, card.top());
                psychic_link(p, from, pos2(r.center().x, r.top()), t, i);
                highlight(p, *r, t + i as f32 * 0.4, TAKE, pop, self.hop == i + 1, false);
            }
            // La carta da giocare sempre a piena forza, anche mentre la mosca è sulle prese.
            highlight(p, *card, t, PSY, pop, true, false);
        }
        // Gli anelli dei tocchi.
        for part in self.parts.iter().filter(|q| q.kind == Kind::Ring) {
            let k = part.age / part.life;
            let a = 1.0 - k;
            let r = 20.0 + k * 140.0;
            pixel_ring(p, part.pos, r, part.color.gamma_multiply(a), PX);
            pixel_ring(p, part.pos, r * 0.6, Color32::WHITE.gamma_multiply(a * 0.6), PX * 0.75);
        }

        // La mosca.
        let mut at =
            self.pos + vec2((t * 13.0).sin(), (t * 17.0).cos()) * if self.mood == Mood::Sleepy { 0.0 } else { 0.65 };
        let mut sx = 1.0;
        if let Some(s) = self.spin {
            // Giravolta: un giro della morte piccolo, la mosca si schiaccia e si gira.
            let a = s * std::f32::consts::TAU;
            at += vec2(a.sin() * 26.0 * self.face, (1.0 - a.cos()) * -22.0);
            sx = (a * 0.5).cos();
        }
        let breathe = if self.mood == Mood::Sleepy { 1.0 + (t * 2.0).sin() * 0.05 } else { 1.0 };
        // Allungata nel verso del volo.
        let stretch = (self.vel.length() / 2200.0).min(0.25);
        // Rimbalzo elastico: si schiaccia e si allunga qualche volta, sempre meno.
        let wobble = (self.boing * 22.0).sin() * 0.28 * self.boing;
        let (sxx, syy) = ((1.0 + stretch) * breathe * (1.0 + wobble), (1.0 - stretch * 0.6) / breathe * (1.0 - wobble));
        // Annuisce (su e giù) o scuote la testa (si gira di qua e di là).
        if self.nod > 0.0 {
            at.y += (self.nod * 25.0).sin() * 5.0 * self.nod;
        }
        if self.shake > 0.0 {
            sx *= 1.0 - 0.18 * (self.shake * 20.0).sin();
        }
        // Ombra sul tavolo, più piccola e chiara quando vola alta.
        let ground = (at.y + 70.0).min(p.clip_rect().bottom() - 10.0);
        let k = 1.0 - ((ground - at.y - 70.0).abs() / 200.0).min(0.6);
        for (i, half) in [6.0f32, 8.0, 6.0].iter().enumerate() {
            let y = ground + (i as f32 - 1.0) * PX * 0.75;
            p.rect_filled(
                Rect::from_center_size(pos2(at.x, y), vec2(half * 2.0 * PX * 0.75 * k, PX * 0.75)),
                0.0,
                Color32::from_black_alpha((40.0 * k) as u8),
            );
        }
        // Scia di immagini fantasma quando sfreccia.
        let fast = (self.vel.length() - 450.0) / 700.0;
        if fast > 0.0 {
            for (i, q) in self.trail.iter().enumerate().skip(1) {
                let a = (0.45 - i as f32 * 0.09).max(0.0) * fast.min(1.0);
                let mut ghost = Pose::new(PX);
                ghost.face = self.face;
                ghost.ghost = Some(if i % 2 == 0 { PSY } else { PSY_PINK }.gamma_multiply(a));
                draw_fly(p, *q, ghost);
            }
        }
        // Aura psichica.
        let pulse = 0.5 + 0.5 * (t * 3.0).sin();
        for (i, a) in [(3.0, 0.06), (2.2, 0.08), (1.6, 0.10)].iter().enumerate() {
            let r = 13.0 * PX * a.0 / 3.0 + pulse * 3.0 + i as f32;
            p.circle_filled(at, r, PSY.gamma_multiply(a.1 + 0.03 * pulse));
        }
        if self.mood != Mood::Sleepy {
            pixel_ring(p, at, 12.0 * PX + pulse * 4.0, PSY.gamma_multiply(0.35 + 0.25 * pulse), PX * 0.75);
            // Puntini che girano attorno.
            for i in 0..3 {
                let a = t * 2.5 + i as f32 * std::f32::consts::TAU / 3.0;
                let q = at + vec2(a.cos() * 15.0 * PX * 0.9, a.sin() * 9.0 * PX * 0.9);
                px_rect(p, q, PX, PSY_PINK.gamma_multiply(0.8));
            }
        }
        let asleep = self.mood == Mood::Sleepy && self.mood_t > 2.0;
        let mut pose = Pose::new(PX * sxx);
        pose.face = self.face * sx;
        pose.sy = syy;
        pose.tilt = self.bank;
        pose.wing = if self.mood == Mood::Sleepy && self.mood_t > 2.0 { None } else { Some(self.wing_phase.sin()) };
        pose.antenna = (t * 5.0).sin() * 0.45;
        if let Some((gesture, age)) = self.gesture {
            let u = (age / gesture.duration()).clamp(0.0, 1.0);
            let e = (std::f32::consts::PI * u).sin().powi(2);
            match gesture {
                Gesture::Wave => {
                    pose.wave = Some(age * 13.0);
                    pose.tilt += (age * 7.0).sin() * 0.12 * e;
                }
                Gesture::Stretch => {
                    pose.sy *= 1.0 + 0.22 * e;
                    pose.s *= 1.0 - 0.08 * e;
                    pose.wave = Some(0.0);
                }
                Gesture::Groom => {
                    pose.rub = Some(age * 7.0);
                    pose.tilt -= 0.18 * e;
                }
                Gesture::Scan => {
                    pose.eyes = Eyes::Open(vec2((age * 3.0).sin(), -0.2));
                    pose.antenna *= 3.0;
                }
                Gesture::Bow => {
                    pose.tilt += 0.3 * e * self.face;
                    pose.sy *= 1.0 - 0.12 * e;
                }
                Gesture::Dance => {
                    pose.wave = Some(age * 12.0);
                    pose.tilt += (age * 8.0).sin() * 0.25 * e;
                }
            }
        }
        pose.flap = if asleep { 1 } else { ((t * 28.0) as i32).rem_euclid(3) };
        pose.eyes = if self.dizzy > 0.0 {
            Eyes::Dizzy
        } else if self.pet > 0.7 {
            Eyes::Love
        } else if (self.mood == Mood::Sleepy && self.mood_t > 1.0) || self.blink > 0.0 {
            Eyes::Closed
        } else if matches!(self.mood, Mood::Party) || self.spin.is_some() || self.nod > 0.0 {
            Eyes::Happy
        } else if self.mood == Mood::Sad {
            Eyes::Sad
        } else {
            Eyes::Open(self.look)
        };
        let yawning = self.mood == Mood::Bored && (14.0..15.6).contains(&self.mood_t);
        pose.mouth = if yawning || self.alert > 0.3 {
            Mouth::Open
        } else if self.dizzy > 0.0 || self.mood == Mood::Sad || self.shake > 0.0 {
            Mouth::Wavy
        } else if matches!(self.mood, Mood::Party | Mood::Pointing) || self.spin.is_some() || self.pet > 0.7 {
            Mouth::Grin
        } else {
            Mouth::Smile
        };
        pose.hat = match self.mood {
            Mood::Party => Hat::Party,
            Mood::Sleepy if self.mood_t > 1.0 => Hat::Night,
            _ => Hat::None,
        };
        if self.mood == Mood::Thinking {
            pose.rub = Some(t * 5.0);
        }
        if let Some((Gesture::Scan, age)) = self.gesture {
            pose.eyes = Eyes::Open(vec2((age * 3.0).sin(), -0.2));
        }
        draw_fly(p, at, pose);
        if self.mood == Mood::Sad {
            // Nuvoletta che le piove in testa.
            let cloud = at + vec2(0.0, -15.0 * PX + (t * 2.0).sin() * 2.0);
            sprite(p, cloud, PX * 0.9, &CLOUD, Color32::from_rgb(120, 130, 150));
            sprite(p, cloud + vec2(0.0, -PX * 0.4), PX * 0.75, &CLOUD, Color32::from_rgb(190, 200, 215));
            for i in 0..3 {
                let x = cloud.x + (i as f32 - 1.0) * 3.0 * PX;
                let y = cloud.y + 3.0 * PX + ((t * 2.5 + i as f32 * 0.37).fract()) * 7.0 * PX;
                px_rect(p, pos2(x, y), PX * 0.6, Color32::from_rgb(120, 200, 255));
            }
        }
        if self.dizzy > 0.0 {
            for i in 0..3 {
                let a = t * 6.0 + i as f32 * std::f32::consts::TAU / 3.0;
                let q = at + vec2(a.cos() * 7.0 * PX, -9.0 * PX + a.sin() * 2.0 * PX);
                sprite(p, q, PX * 0.6, &STAR, Color32::from_rgb(255, 230, 90));
            }
        }
        if self.mood == Mood::Lost && self.mood_t > 4.0 {
            // Confusa: stelline che le girano attorno alla testa.
            for i in 0..3 {
                let a = t * 4.0 + i as f32 * std::f32::consts::TAU / 3.0;
                let q = at + vec2(a.cos() * 7.0 * PX, -9.0 * PX + a.sin() * 2.0 * PX);
                sprite(p, q, PX * 0.6, &STAR, Color32::from_rgb(255, 230, 90));
            }
        }
        if self.alert > 0.0 {
            let k = ease_out_back(((0.7 - self.alert) / 0.2).clamp(0.0, 1.0));
            let top = at + vec2(0.0, -11.0 * PX - 8.0 * k);
            bang(p, top, PX * k.max(0.01));
        }

        // Le altre particelle.
        for part in self.parts.iter().filter(|q| q.kind != Kind::Ring) {
            let a = (1.0 - part.age / part.life).clamp(0.0, 1.0);
            let s = PX * 0.75 * part.size;
            match part.kind {
                Kind::Spark => sprite(p, part.pos, s, &SPARK, part.color.gamma_multiply(a)),
                Kind::Star => sprite(p, part.pos, s * 0.8, &STAR, part.color.gamma_multiply(a)),
                Kind::Heart => sprite(p, part.pos, s, &HEART, part.color.gamma_multiply(a)),
                Kind::Tear => sprite(p, part.pos, s * 0.8, &TEAR, part.color.gamma_multiply(a)),
                Kind::Confetti => {
                    let w = (part.age * 9.0 + part.size * 7.0).sin().abs().max(0.2);
                    p.rect_filled(
                        Rect::from_center_size(part.pos, vec2(s * 1.6 * w, s * 1.6)),
                        0.0,
                        part.color.gamma_multiply(a),
                    );
                }
                Kind::Zed => {
                    let size = 14.0 + part.age * 6.0;
                    outlined_text(p, part.pos, "z", size, Color32::WHITE.gamma_multiply(a), OUTLINE.gamma_multiply(a));
                }
                Kind::Orb => {
                    // Colpo nemico: palla rossa a pixel che pulsa.
                    let k = 1.0 + 0.15 * (part.age * 12.0).sin();
                    sprite(p, part.pos, s * 1.3 * k, &ORB, OUTLINE);
                    sprite(p, part.pos, s * 1.0 * k, &ORB, part.color);
                }
                Kind::Deflected => {
                    sprite(p, part.pos, s * 1.3, &ORB, OUTLINE.gamma_multiply(a));
                    sprite(p, part.pos, s, &ORB, part.color.gamma_multiply(a));
                }
                Kind::Ring => {}
            }
        }

        // Il segno della carta da giocare sopra tutto (mosca, scie, coriandoli), e una freccia che
        // la indica quando la mosca è lontana o distratta: la carta resta sempre segnata.
        if let Some((card, take)) = play {
            let pop = ease_out_back((self.mood_t / 0.35).min(1.0));
            for (i, r) in take.iter().enumerate() {
                highlight(p, *r, t + i as f32 * 0.4, TAKE, pop, self.hop == i + 1, true);
            }
            highlight(p, *card, t, PSY, pop, true, true);
            let perch = pos2(card.center().x, card.top() - 9.0 * PX);
            if self.hop != 0 || self.dizzy > 0.0 || (self.pos - perch).length() > 90.0 {
                let bob = (t * 6.0).sin().abs() * 10.0;
                let tip = pos2(card.center().x, card.top() - 14.0 - bob);
                sprite(p, tip + vec2(0.0, -3.0 * PX), PX * 1.25, &ARROW, OUTLINE);
                sprite(p, tip + vec2(0.0, -3.0 * PX), PX, &ARROW, PSY_PINK);
            }
        }

        // Il fumetto.
        if let Some(b) = &self.bubble {
            let k = ease_out_back((b.age / 0.25).min(1.0));
            let fade = ((b.life - b.age) / 0.3).clamp(0.0, 1.0);
            let text = if b.thought {
                let n = ((t * 3.0) as usize) % 4;
                ".".repeat(n.max(1))
            } else {
                b.text.clone()
            };
            bubble(p, at + vec2(0.0, -12.0 * PX), &text, k, fade, b.thought, t);
        }
    }
}

/// Occhi della mosca.
#[derive(Clone, Copy, PartialEq)]
enum Eyes {
    Open(Vec2),
    Closed,
    Happy,
    Sad,
    /// A cuore: le fanno i grattini.
    Love,
    /// A spirale: le gira la testa.
    Dizzy,
}

#[derive(Clone, Copy, PartialEq)]
enum Mouth {
    Smile,
    Grin,
    /// Aperta: sbadiglio o spavento.
    Open,
    Wavy,
}

#[derive(Clone, Copy, PartialEq)]
enum Hat {
    None,
    Party,
    Night,
}

/// Come disegnare la mosca in questo fotogramma.
#[derive(Clone, Copy)]
struct Pose {
    /// Lato di un pixel.
    s: f32,
    /// Verso (1 a destra, -1 a sinistra; in mezzo è schiacciata, mentre si gira).
    face: f32,
    /// Altezza rispetto alla larghezza (per gli allungamenti e i rimbalzi).
    sy: f32,
    /// Fotogramma delle ali (0, 1, 2).
    flap: i32,
    wing: Option<f32>,
    tilt: f32,
    antenna: f32,
    wave: Option<f32>,
    eyes: Eyes,
    mouth: Mouth,
    hat: Hat,
    /// Zampette davanti che si sfregano (pensa), con la fase.
    rub: Option<f32>,
    /// Sagoma tutta di un colore (le immagini fantasma della scia psichica).
    ghost: Option<Color32>,
}

impl Pose {
    fn new(s: f32) -> Pose {
        Pose {
            s,
            face: 1.0,
            sy: 1.0,
            flap: 1,
            wing: None,
            tilt: 0.0,
            antenna: 0.0,
            wave: None,
            eyes: Eyes::Open(vec2(1.0, 0.0)),
            mouth: Mouth::Smile,
            hat: Hat::None,
            rub: None,
            ghost: None,
        }
    }
}

/// Un pixel della mosca: quadrato di lato `s` col centro in `c`.
fn px_rect(p: &Painter, c: Pos2, s: f32, color: Color32) {
    p.rect_filled(Rect::from_center_size(c, vec2(s, s)), 0.0, color);
}

/// La mosca in pixel art, col centro in `c`.
fn draw_fly(p: &Painter, c: Pos2, pose: Pose) {
    let (w, h) = (pose.s * pose.face.abs().max(0.15), pose.s * pose.sy);
    let dir = if pose.face < 0.0 { -1.0 } else { 1.0 };
    let at = |x: f32, y: f32| {
        let v = vec2((x - 0.5) * w * dir, y * h);
        let (sin, cos) = pose.tilt.sin_cos();
        c + vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
    };
    let dot = |x: f32, y: f32, color: Color32| {
        let color = match pose.ghost {
            Some(g) if color.a() > 0 => g,
            _ => color,
        };
        p.rect_filled(Rect::from_center_size(at(x, y), vec2(w + 0.3, h + 0.3)), 0.0, color);
    };
    let blob = |cx: f32, cy: f32, rx: f32, ry: f32, fill: Color32, edge: Color32| {
        let inside = |x: f32, y: f32| ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0;
        for j in (cy - ry - 1.0) as i32..=(cy + ry + 1.0) as i32 {
            for i in (cx - rx - 1.0) as i32..=(cx + rx + 1.0) as i32 {
                let (x, y) = (i as f32, j as f32);
                if !inside(x, y) {
                    continue;
                }
                let border = !inside(x + 1.0, y) || !inside(x - 1.0, y) || !inside(x, y + 1.0) || !inside(x, y - 1.0);
                dot(x, y, if border { edge } else { fill });
            }
        }
    };
    let wing = |cx: f32, cy: f32, rx: f32, ry: f32, fill: Color32, edge: Color32| {
        if pose.wing.is_none() {
            blob(cx, cy, rx, ry, fill, edge);
            return;
        }
        let points = (0..24)
            .map(|i| {
                let phase = i as f32 / 24.0 * std::f32::consts::TAU;
                at(cx + rx * phase.cos(), cy + ry * phase.sin())
            })
            .collect();
        p.add(egui::Shape::convex_polygon(
            points,
            pose.ghost.unwrap_or(fill),
            Stroke::new(1.0, pose.ghost.unwrap_or(edge)),
        ));
    };
    // Ali sul dorso, tre fotogrammi: alzate, a metà, abbassate all'indietro.
    let (wx, wy, wrx, wry) = if let Some(wing) = pose.wing {
        let u = (wing + 1.0) * 0.5;
        (-4.0 + 3.0 * u, -6.0 - 3.5 * u, 3.5 - 1.3 * u, 1.6 + 2.4 * u)
    } else {
        match pose.flap {
            0 => (-1.0, -9.0, 2.2, 3.6),
            1 => (-2.5, -8.0, 3.0, 2.6),
            _ => (-4.0, -6.5, 3.4, 1.8),
        }
    };
    // Flexible antennae communicate curiosity independently of the wing beat.
    for (x, sign) in [(2.0, -1.0), (6.0, 1.0)] {
        for j in 0..4 {
            dot(x + sign * j as f32 * 0.35 + pose.antenna * j as f32 * 0.25, -7.0 - j as f32, OUTLINE);
        }
        dot(x + sign + pose.antenna, -11.0, PSY_PINK);
    }
    let wing_back =
        (Color32::from_rgba_unmultiplied(160, 190, 255, 120), Color32::from_rgba_unmultiplied(70, 90, 190, 210));
    let wing_front =
        (Color32::from_rgba_unmultiplied(225, 240, 255, 165), Color32::from_rgba_unmultiplied(100, 130, 225, 235));
    wing(wx - 2.0, wy + 0.5, wrx, wry, wing_back.0, wing_back.1);
    // Zampette: dondolano, o quelle davanti si sfregano mentre pensa.
    for (i, x) in [-3.0f32, -0.5, 2.0].iter().enumerate() {
        let swing = if (pose.flap + i as i32) % 2 == 0 { 0.0 } else { 1.0 };
        match (pose.wave, pose.rub) {
            (Some(ph), _) if i == 2 => {
                for j in 0..4 {
                    dot(7.0 + j as f32 * 1.1, 5.0 - j as f32 * 2.0 + (ph.sin() * j as f32 * 0.55), OUTLINE);
                }
            }
            (_, Some(ph)) if i == 2 => {
                let up = if (ph * 2.0) as i32 % 2 == 0 { 0.0 } else { 1.0 };
                dot(3.0, 5.0 - up, OUTLINE);
                dot(4.0, 4.0 - up, OUTLINE);
                dot(4.0, 5.0 - (1.0 - up), OUTLINE);
                dot(5.0, 4.0 - (1.0 - up), OUTLINE);
            }
            _ => {
                dot(*x, 6.0, OUTLINE);
                dot(*x - swing, 7.0, OUTLINE);
            }
        }
    }
    // Corpo: un ovale con la luce in alto a sinistra, le righe sul dietro e il contorno scuro.
    let (rx, ry, cy) = (6.6, 5.3, 0.5);
    let inside = |x: f32, y: f32| (x / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0;
    for j in -6..=7 {
        for i in -7..=7 {
            let (x, y) = (i as f32, j as f32);
            if !inside(x, y) {
                continue;
            }
            let border = !inside(x + 1.0, y) || !inside(x - 1.0, y) || !inside(x, y + 1.0) || !inside(x, y - 1.0);
            let color = if border {
                OUTLINE
            } else if (x == -2.0 || x == -4.0) && y > -3.0 {
                STRIPE
            } else if (x + 3.0).powi(2) + (y + 2.0).powi(2) < 3.5 {
                BODY_LIGHT
            } else if y >= 3.0 || (x <= -5.0 && y >= 1.0) {
                BODY_DARK
            } else {
                BODY
            };
            dot(x, y, color);
        }
    }
    dot(-4.0, -2.0, Color32::WHITE);
    dot(-3.0, -3.0, Color32::from_rgb(235, 215, 255));
    // Occhioni sporgenti sul davanti: 3 x 5 di bianco col contorno.
    for (n, ex) in [2.0f32, 6.0].into_iter().enumerate() {
        let (top, bot) = (-6.0, -2.0);
        for y in (top as i32)..=(bot as i32) {
            dot(ex - 2.0, y as f32, OUTLINE);
            dot(ex + 2.0, y as f32, OUTLINE);
        }
        for x in -1..=1 {
            dot(ex + x as f32, top - 1.0, OUTLINE);
            dot(ex + x as f32, bot + 1.0, OUTLINE);
        }
        let fill = |color: Color32| {
            for y in (top as i32)..=(bot as i32) {
                for x in -1..=1 {
                    dot(ex + x as f32, y as f32, color);
                }
            }
        };
        match pose.eyes {
            Eyes::Open(look) => {
                fill(EYE);
                // Pupilla avanti o indietro, in alto, al centro o in basso, secondo dove guarda.
                let px0 = if look.x * dir > 0.3 { ex } else { ex - 1.0 };
                let oy: f32 = if look.y > 0.35 {
                    1.0
                } else if look.y < -0.35 {
                    -1.0
                } else {
                    0.0
                };
                let py0 = (-5.0 + oy).clamp(-6.0, -4.0);
                for dy in 0..3 {
                    for dx in 0..2 {
                        dot(px0 + dx as f32, py0 + dy as f32, PUPIL);
                    }
                }
                // Il riflesso.
                dot(px0 + 1.0, py0, EYE);
            }
            Eyes::Closed => {
                fill(BODY);
                for x in -1..=1 {
                    dot(ex + x as f32, -3.0, OUTLINE);
                }
            }
            Eyes::Happy => {
                fill(EYE);
                dot(ex - 1.0, -3.0, OUTLINE);
                dot(ex, -4.0, OUTLINE);
                dot(ex + 1.0, -3.0, OUTLINE);
            }
            Eyes::Sad => {
                fill(EYE);
                dot(ex - 1.0, -4.0, OUTLINE);
                dot(ex, -3.0, PUPIL);
                dot(ex + 1.0, -3.0, PUPIL);
                dot(ex, -2.0, Color32::from_rgb(140, 210, 255));
            }
            Eyes::Love => {
                fill(EYE);
                for (dx, dy) in [(-1.0, -5.0), (1.0, -5.0), (-1.0, -4.0), (0.0, -4.0), (1.0, -4.0), (0.0, -3.0)] {
                    dot(ex + dx, dy, PSY_PINK);
                }
            }
            Eyes::Dizzy => {
                fill(EYE);
                // Una X in un occhio e una spirale nell'altro: più buffo.
                if n == 0 {
                    for (dx, dy) in [(-1.0, -5.0), (1.0, -5.0), (0.0, -4.0), (-1.0, -3.0), (1.0, -3.0)] {
                        dot(ex + dx, dy, PUPIL);
                    }
                } else {
                    for (dx, dy) in
                        [(-1.0, -5.0), (0.0, -5.0), (1.0, -5.0), (1.0, -4.0), (1.0, -3.0), (0.0, -3.0), (0.0, -4.0)]
                    {
                        dot(ex + dx, dy, PUPIL);
                    }
                }
            }
        }
    }
    // Guance e bocca.
    let blush = if pose.eyes == Eyes::Love { PSY_PINK } else { BLUSH };
    dot(0.0, 0.0, blush);
    dot(7.0, 0.0, blush);
    if pose.eyes == Eyes::Love {
        dot(-1.0, 0.0, blush);
    }
    match pose.mouth {
        Mouth::Grin => {
            dot(3.0, 0.0, OUTLINE);
            dot(5.0, 0.0, OUTLINE);
            dot(3.0, 1.0, OUTLINE);
            dot(4.0, 1.0, BLUSH);
            dot(5.0, 1.0, OUTLINE);
            dot(4.0, 2.0, OUTLINE);
        }
        Mouth::Open => {
            for (x, y) in
                [(3.0, 0.0), (4.0, 0.0), (5.0, 0.0), (3.0, 1.0), (5.0, 1.0), (3.0, 2.0), (4.0, 2.0), (5.0, 2.0)]
            {
                dot(x, y, OUTLINE);
            }
            dot(4.0, 1.0, Color32::from_rgb(200, 60, 110));
        }
        Mouth::Wavy => {
            dot(2.0, 1.0, OUTLINE);
            dot(3.0, 0.0, OUTLINE);
            dot(4.0, 1.0, OUTLINE);
            dot(5.0, 0.0, OUTLINE);
        }
        Mouth::Smile => {
            dot(3.0, 0.0, OUTLINE);
            dot(4.0, 1.0, OUTLINE);
            dot(5.0, 0.0, OUTLINE);
        }
    }
    // Ala davanti, col riflesso che brilla.
    wing(wx, wy, wrx, wry, wing_front.0, wing_front.1);
    dot(wx - 1.0, wy - (wry - 1.5).max(0.0), Color32::from_rgba_unmultiplied(255, 255, 255, 220));
    // Cappellini.
    match pose.hat {
        Hat::None => {}
        Hat::Party => {
            // Cono a righe con il pompon, un po' storto sopra gli occhi.
            let rows: [(f32, f32, f32); 5] =
                [(4.0, -8.0, 2.0), (4.0, -9.0, 1.5), (4.5, -10.0, 1.0), (4.5, -11.0, 0.5), (5.0, -12.0, 0.0)];
            for (k, (cx, y, half)) in rows.into_iter().enumerate() {
                let mut x = cx - half;
                while x <= cx + half + 0.01 {
                    let col = if (k + (x as i32) as usize).is_multiple_of(2) { TAKE } else { PSY_PINK };
                    dot(x.round(), y, col);
                    x += 1.0;
                }
                dot((cx - half - 1.0).round(), y, OUTLINE);
                dot((cx + half + 1.0).round(), y, OUTLINE);
            }
            dot(5.0, -13.0, Color32::WHITE);
            dot(6.0, -13.0, Color32::WHITE);
            dot(5.0, -14.0, Color32::WHITE);
        }
        Hat::Night => {
            // Berretto da notte che ricade all'indietro, col pompon.
            let blue = Color32::from_rgb(90, 120, 230);
            for x in 0..=7 {
                dot(x as f32, -8.0, Color32::WHITE);
            }
            for x in 0..=6 {
                dot(x as f32, -9.0, blue);
            }
            for x in -1..=4 {
                dot(x as f32, -10.0, blue);
            }
            for x in -3..=1 {
                dot(x as f32, -11.0, blue);
            }
            dot(-4.0, -10.0, blue);
            dot(-5.0, -9.0, Color32::WHITE);
            dot(-6.0, -9.0, Color32::WHITE);
            dot(-5.0, -8.0, Color32::WHITE);
        }
    }
}

const SPARK: [&str; 3] = [".x.", "xxx", ".x."];
const STAR: [&str; 5] = ["..x..", "..x..", "xxoxx", "..x..", "..x.."];
const HEART: [&str; 5] = [".x.x.", "xxxxx", "xxxxx", ".xxx.", "..x.."];
const ARROW: [&str; 6] = ["..xxx..", "..xxx..", "..xxx..", "xxxxxxx", ".xxxxx.", "..xxx.."];
const CLOUD: [&str; 4] = ["...xxx....", ".xxxxxxx..", "xxxxxxxxxx", ".xxxxxxxx."];
const ORB: [&str; 5] = [".xxx.", "xooxx", "xoxxx", "xxxxx", ".xxx."];
const TEAR: [&str; 4] = [".x.", "xxx", "xxx", ".x."];
const BANG: [&str; 7] = ["xx", "xx", "xx", "xx", "..", "xx", "xx"];

/// Un disegnino a pixel col centro in `c` ('x' nel colore dato, 'o' bianco).
fn sprite(p: &Painter, c: Pos2, s: f32, rows: &[&str], color: Color32) {
    let (h, w) = (rows.len() as f32, rows[0].len() as f32);
    for (j, row) in rows.iter().enumerate() {
        for (i, ch) in row.chars().enumerate() {
            let col = match ch {
                'x' => color,
                'o' => Color32::WHITE.gamma_multiply(color.a() as f32 / 255.0),
                _ => continue,
            };
            let q = c + vec2((i as f32 - (w - 1.0) / 2.0) * s, (j as f32 - (h - 1.0) / 2.0) * s);
            p.rect_filled(Rect::from_center_size(q, vec2(s + 0.3, s + 0.3)), 0.0, col);
        }
    }
}

/// Il punto esclamativo sopra la testa, col contorno.
fn bang(p: &Painter, c: Pos2, s: f32) {
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        sprite(p, c + vec2(dx, dy) * s, s, &BANG, OUTLINE);
    }
    sprite(p, c, s, &BANG, Color32::from_rgb(255, 230, 90));
}

/// Un anello fatto di pixel.
fn pixel_ring(p: &Painter, c: Pos2, r: f32, color: Color32, s: f32) {
    let n = ((r * std::f32::consts::TAU) / (s * 1.6)).max(8.0) as usize;
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let q = c + vec2(a.cos(), a.sin()) * r;
        let q = pos2((q.x / s).round() * s, (q.y / s).round() * s);
        p.rect_filled(Rect::from_center_size(q, vec2(s, s)), 0.0, color);
    }
}

/// La carta indicata: bagliore che pulsa, bordo a pixel che scorre e angoli a staffa che saltellano.
/// Si disegna in due strati: `over` falso il bagliore e la tinta (sotto la mosca), vero il bordo
/// e le staffe (sopra la mosca e le particelle: la carta resta segnata qualunque cosa le passi sopra).
#[allow(clippy::too_many_arguments)]
fn highlight(p: &Painter, card: Rect, t: f32, color: Color32, _pop: f32, active: bool, over: bool) {
    let pulse = 0.5 + 0.5 * (t * 5.0).sin();
    // Geometry follows the measured card; pulse affects brightness, never its bounds.
    let r = card;
    let strength = if active { 1.0 } else { 0.55 };
    if over {
        p.rect_stroke(r, 6.0, Stroke::new(1.5, color.gamma_multiply(strength)), StrokeKind::Inside);
        highlight_border(p, r, t, color, pulse, strength);
        return;
    }
    for i in 0..4 {
        let k = i as f32;
        p.rect_stroke(
            r.expand(k * 3.0),
            8.0,
            Stroke::new(3.0, color.gamma_multiply((0.22 - k * 0.05) * strength)),
            StrokeKind::Outside,
        );
    }
    p.rect_filled(card, 6.0, color.gamma_multiply(0.10 * strength + 0.06 * pulse * strength));
}

/// Il bordo a pixel che scorre e le staffe agli angoli.
fn highlight_border(p: &Painter, r: Rect, t: f32, color: Color32, pulse: f32, strength: f32) {
    // Formiche a pixel che girano attorno alla carta.
    let s = PX;
    let per = 2.0 * (r.width() + r.height());
    let n = (per / (s * 2.5)) as usize;
    for i in 0..n {
        let d = (i as f32 * s * 2.5 + t * 60.0) % per;
        let q = along(r, d);
        let c = if i % 2 == 0 { color } else { Color32::WHITE };
        p.rect_filled(Rect::from_center_size(q, vec2(s, s)), 0.0, c.gamma_multiply(0.9 * strength));
    }
    // Staffe agli angoli.
    let b = 8.0 + pulse * 5.0;
    let corners = [
        (r.left_top(), vec2(-1.0, -1.0)),
        (r.right_top(), vec2(1.0, -1.0)),
        (r.left_bottom(), vec2(-1.0, 1.0)),
        (r.right_bottom(), vec2(1.0, 1.0)),
    ];
    for (c, d) in corners {
        let o = c + d * b;
        for k in 0..4 {
            let k = k as f32 * s;
            px_rect(p, o - vec2(d.x * k, 0.0), s * 1.25, OUTLINE.gamma_multiply(strength));
            px_rect(p, o - vec2(0.0, d.y * k), s * 1.25, OUTLINE.gamma_multiply(strength));
            px_rect(p, o - vec2(d.x * k, 0.0), s * 0.8, color.gamma_multiply(strength));
            px_rect(p, o - vec2(0.0, d.y * k), s * 0.8, color.gamma_multiply(strength));
        }
    }
}

/// Il punto a distanza `d` lungo il bordo del rettangolo, in senso orario dall'angolo in alto a sinistra.
fn along(r: Rect, d: f32) -> Pos2 {
    let (w, h) = (r.width(), r.height());
    if d < w {
        pos2(r.left() + d, r.top())
    } else if d < w + h {
        pos2(r.right(), r.top() + d - w)
    } else if d < 2.0 * w + h {
        pos2(r.right() - (d - w - h), r.bottom())
    } else {
        pos2(r.left(), r.bottom() - (d - 2.0 * w - h))
    }
}

/// La scia psichica fra la carta da giocare e una carta da prendere: un arco di pallini che scorrono.
fn psychic_link(p: &Painter, a: Pos2, b: Pos2, t: f32, i: usize) {
    let mid = pos2((a.x + b.x) / 2.0, a.y.min(b.y) - 60.0 - 20.0 * i as f32);
    let n = ((b - a).length() / 14.0).clamp(8.0, 80.0) as usize;
    for k in 0..n {
        let u = (k as f32 + (t * 4.0).fract()) / n as f32;
        let q = a.lerp(mid, u).lerp(mid.lerp(b, u), u);
        let a8 = 0.35 + 0.45 * ((u * 12.0 - t * 6.0).sin() * 0.5 + 0.5);
        px_rect(p, q, PX * 0.9, TAKE.gamma_multiply(a8));
    }
}

/// Il fumetto sopra la mosca: riquadro bianco con gli angoli a pixel, contorno scuro e la coda
/// (a pallini se è un pensiero).
fn bubble(p: &Painter, tip: Pos2, text: &str, k: f32, fade: f32, thought: bool, t: f32) {
    let size = 17.0 * k.max(0.05);
    let font = FontId::proportional(size);
    let galley = p.layout_no_wrap(text.to_string(), font, OUTLINE);
    let pad = vec2(12.0, 8.0) * k.max(0.05);
    let sz = galley.size() + pad * 2.0;
    let bob = (t * 3.0).sin() * 2.0;
    let center = tip + vec2(0.0, -sz.y / 2.0 - 18.0 + bob);
    let screen = p.clip_rect();
    let mut r = Rect::from_center_size(center, sz);
    // Dentro lo schermo.
    let dx = (screen.left() + 8.0 - r.left()).max(0.0) - (r.right() - screen.right() + 8.0).max(0.0);
    let dy = (screen.top() + 8.0 - r.top()).max(0.0);
    r = r.translate(vec2(dx, dy));
    let s = PX * 0.75 * k.max(0.05);
    let ink = OUTLINE.gamma_multiply(fade);
    let paper = Color32::WHITE.gamma_multiply(fade);
    // Riquadro a gradini: contorno, poi la carta.
    let notched = |r: Rect, color: Color32| {
        p.rect_filled(r.shrink2(vec2(s, 0.0)), 0.0, color);
        p.rect_filled(r.shrink2(vec2(0.0, s)), 0.0, color);
    };
    notched(r.expand(s), ink);
    notched(r, paper);
    // Ombra in basso dentro, per lo stile a pixel.
    p.rect_filled(
        Rect::from_min_max(pos2(r.left() + s, r.bottom() - s), pos2(r.right() - s, r.bottom())),
        0.0,
        Color32::from_rgb(220, 200, 240).gamma_multiply(fade),
    );
    let x = tip.x.clamp(r.left() + 4.0 * s, r.right() - 4.0 * s);
    if thought {
        // Due nuvolette sempre più piccole verso la mosca.
        for (dy, half) in [(3.0, 2.0), (7.0, 1.0)] {
            let q = pos2(x, r.bottom() + dy * s);
            p.rect_filled(Rect::from_center_size(q, vec2(2.0, 2.0) * (half + 1.0) * s), 0.0, ink);
            p.rect_filled(Rect::from_center_size(q, vec2(2.0, 2.0) * half * s), 0.0, paper);
        }
    } else {
        // Coda a scalini verso la mosca: ogni riga un pixel più stretta, contorno ai lati.
        for (k, ink_half, paper_half) in [(0.0, 3.0, 2.0), (1.0, 2.0, 1.0), (2.0, 1.0, 0.0), (3.0, 0.5, 0.0)] {
            let y = r.bottom() + k * s;
            let row = |half: f32, color: Color32| {
                p.rect_filled(Rect::from_min_max(pos2(x - half * s, y), pos2(x + half * s, y + s)), 0.0, color);
            };
            row(ink_half, ink);
            if paper_half > 0.0 {
                row(paper_half, paper);
            }
        }
    }
    p.galley(r.center() - galley.size() / 2.0, galley, ink);
}

/// Testo col contorno scuro.
fn outlined_text(p: &Painter, at: Pos2, text: &str, size: f32, color: Color32, outline: Color32) {
    let font = FontId::proportional(size);
    for d in [vec2(-1.5, 0.0), vec2(1.5, 0.0), vec2(0.0, -1.5), vec2(0.0, 1.5)] {
        p.text(at + d, Align2::CENTER_CENTER, text, font.clone(), outline);
    }
    p.text(at, Align2::CENTER_CENTER, text, font, color);
}

fn spring(pos: Pos2, vel: Vec2, goal: Pos2, stiffness: f32, damping: f32, dt: f32) -> (Pos2, Vec2) {
    let alpha = damping * 0.5;
    let omega = (stiffness - alpha * alpha).max(0.001).sqrt();
    let decay = (-alpha * dt).exp();
    let (sin, cos) = (omega * dt).sin_cos();
    let offset = pos - goal;
    let next = (offset * cos + (vel + offset * alpha) * (sin / omega)) * decay;
    let velocity = (vel * cos - (vel * alpha + offset * stiffness) * (sin / omega)) * decay;
    (goal + next, velocity)
}

fn ease_out_back(x: f32) -> f32 {
    let (c1, c3) = (1.70158, 2.70158);
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

fn lerp_color(a: Color32, b: Color32, k: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Su Windows la finestra della mosca va messa a pixel veri sul monitor del gioco, e non deve
/// finire nelle foto dello schermo (la lettura non deve vedere la mosca).
#[cfg(windows)]
mod native {
    use super::Geom;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetWindowDisplayAffinity, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, WDA_EXCLUDEFROMCAPTURE,
    };

    /// Vero quando la finestra è stata trovata e sistemata.
    pub fn place(title: &str, g: Geom) -> Result<(), String> {
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
            if hwnd.is_null() {
                return Err("Finestra overlay non ancora disponibile".into());
            }
            if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
                return Err(format!("Esclusione cattura: {}", std::io::Error::last_os_error()));
            }
            if SetWindowPos(hwnd, HWND_TOPMOST, g.x as i32, g.y as i32, g.w as i32, g.h as i32 - 1, SWP_NOACTIVATE) == 0
            {
                return Err(format!("Posizionamento overlay: {}", std::io::Error::last_os_error()));
            }
        }
        Ok(())
    }

    /// Dove sta il mouse sul desktop (pixel veri) e se il tasto sinistro è premuto.
    pub fn cursor() -> Option<(f32, f32, bool)> {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut pt = POINT { x: 0, y: 0 };
        unsafe {
            if GetCursorPos(&mut pt) == 0 {
                return None;
            }
            let down = (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0;
            Some((pt.x as f32, pt.y as f32, down))
        }
    }
}

#[cfg(not(windows))]
mod native {
    use super::Geom;
    pub fn place(_title: &str, _g: Geom) -> Result<(), String> {
        Ok(())
    }

    /// Fuori da Windows la mosca non vede il mouse.
    pub fn cursor() -> Option<(f32, f32, bool)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sight(guide: Guide) -> (Rect, Vec<Rect>, Guide) {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0));
        let hand = vec![
            Rect::from_min_size(pos2(600.0, 650.0), vec2(100.0, 160.0)),
            Rect::from_min_size(pos2(720.0, 650.0), vec2(100.0, 160.0)),
            Rect::from_min_size(pos2(840.0, 650.0), vec2(100.0, 160.0)),
        ];
        (screen, hand, guide)
    }

    /// La mosca arriva sopra la carta consigliata e ci resta.
    #[test]
    fn invalid_geometry_never_reaches_the_overlay() {
        let mut sight = Sight { monitor: Some(Geom { x: -1920.0, y: 0.0, w: 1920.0, h: 1080.0, scale: 1.5 }),
            hand: vec![[-100.0, 10.0, -50.0, 100.0], [f32::NAN, 0.0, 10.0, 20.0]],
            guide: Guide::Play { id: AdviceId::default(), card: [-100.0, 10.0, -50.0, 100.0],
                take: vec![[0.0, 0.0, 0.0, 0.0]] }, context: (0, 1), played_card: None };
        sight = sight.validated();
        assert_eq!(sight.hand.len(), 1);
        assert_eq!(sight.guide, Guide::Seeking);
        sight.monitor.as_mut().unwrap().scale = 0.0;
        assert!(sight.validated().monitor.is_none());
    }

    #[test]
    fn flies_to_the_card() {
        let (screen, hand, _) = sight(Guide::Thinking);
        let mut fly = Fly::default();
        let card = hand[2];
        let play = (card, Vec::new());
        let guide = Guide::Play { id: AdviceId::default(), card: [0.0; 4], take: vec![] };
        let mut near = 0;
        for i in 0..600 {
            fly.step(1.0 / 60.0, screen, &hand, &guide, Some(&play));
            if i > 120 && (fly.pos.x - card.center().x).abs() < 60.0 && fly.pos.y < card.top() + 40.0 {
                near += 1;
            }
        }
        assert!(near > 400, "vicino alla carta in {near} fotogrammi su 480");
        assert_eq!(fly.mood, Mood::Pointing);
    }

    /// Ogni stato d'animo si anima senza uscire dallo schermo e senza particelle infinite.
    #[test]
    fn every_mood_stays_on_screen() {
        for guide in [Guide::NoTable, Guide::Waiting, Guide::Thinking, Guide::Won, Guide::Lost, Guide::Tie] {
            let (screen, hand, guide) = sight(guide);
            let mut fly = Fly::default();
            for _ in 0..60 * 30 {
                fly.step(1.0 / 60.0, screen, &hand, &guide, None);
                assert!(screen.contains(fly.pos), "{guide:?}: {:?}", fly.pos);
            }
            assert!(fly.parts.len() <= 400);
        }
    }

    /// Quando si annoia arriva un colpo nemico: la mosca lo devia e lo rimanda indietro.
    #[test]
    fn swats_enemy_shots() {
        let (screen, hand, guide) = sight(Guide::Waiting);
        let mut fly = Fly::default();
        let mut deflected = false;
        for _ in 0..60 * 40 {
            fly.step(1.0 / 60.0, screen, &hand, &guide, None);
            deflected |= fly.parts.iter().any(|p| p.kind == Kind::Deflected);
        }
        assert!(deflected);
    }

    /// Il mouse: un clic sulla mosca la stordisce, il mouse fermo vicino le fa le fusa, sopra
    /// un'altra carta della mano scuote la testa, sopra quella consigliata annuisce.
    #[test]
    fn reacts_to_the_mouse() {
        let (screen, hand, _) = sight(Guide::Thinking);
        let mut fly = Fly::default();
        let dt = 1.0 / 60.0;
        for _ in 0..60 {
            fly.step(dt, screen, &hand, &Guide::Waiting, None);
        }
        let at = fly.pos;
        fly.interact(dt, &hand, None, Some((at, false)));
        fly.interact(dt, &hand, None, Some((at, true)));
        assert!(fly.dizzy > 0.0);
        let mut fly = Fly::default();
        for _ in 0..60 {
            fly.step(dt, screen, &hand, &Guide::Waiting, None);
        }
        for _ in 0..90 {
            let at = fly.pos;
            fly.interact(dt, &hand, None, Some((at + vec2(10.0, 0.0), false)));
        }
        assert!(fly.parts.iter().any(|p| p.kind == Kind::Heart));
        let mut fly = Fly::default();
        let play = (hand[1], Vec::new());
        let guide = Guide::Play { id: AdviceId::default(), card: [0.0; 4], take: vec![] };
        for _ in 0..60 {
            fly.step(dt, screen, &hand, &guide, Some(&play));
        }
        fly.interact(dt, &hand, Some(&play), Some((hand[0].center(), false)));
        assert!(fly.shake > 0.0);
        fly.interact(dt, &hand, Some(&play), Some((hand[1].center(), false)));
        assert!(fly.nod > 0.0);
    }

    /// La carta da giocare resta segnata se una lettura la perde per un attimo, e si toglie
    /// appena la partita va avanti.
    #[test]
    fn keeps_the_card_marked() {
        let mut fly = Fly::default();
        let play = Guide::Play { id: AdviceId::default(), card: [1.0, 2.0, 3.0, 4.0], take: vec![] };
        assert_eq!(fly.sticky(play.clone()), play);
        fly.t += 1.0;
        assert_eq!(fly.sticky(Guide::Thinking), play);
        let expired = fly.last_play.as_ref().unwrap().1 + std::time::Duration::from_secs_f32(STICKY);
        assert_eq!(fly.sticky_at(Guide::Thinking, expired), Guide::Thinking);
        // Carte sparite dallo schermo: niente segno appiccicato.
        assert_eq!(fly.sticky(play.clone()), play);
        assert_eq!(fly.sticky(Guide::Hidden), Guide::Hidden);
        assert_eq!(fly.sticky(Guide::Thinking), Guide::Thinking);
        assert_eq!(fly.sticky(play.clone()), play);
        assert_eq!(fly.sticky(Guide::NoTable), Guide::NoTable);
        assert_eq!(fly.sticky(Guide::Thinking), Guide::Thinking);
        assert_eq!(fly.sticky(play.clone()), play);
        assert_eq!(fly.sticky(Guide::Waiting), Guide::Waiting);
        assert_eq!(fly.sticky(Guide::Thinking), Guide::Thinking);
    }

    /// Se le carte spariscono per un attimo la mosca smette di segnarle ma non cambia umore e non
    /// festeggia; quando tornano non le riannuncia.
    #[test]
    fn hidden_cards_are_not_marked() {
        let (screen, hand, _) = sight(Guide::Thinking);
        let mut fly = Fly::default();
        let play = (hand[0], Vec::<Rect>::new());
        let guide = Guide::Play { id: AdviceId::default(), card: [0.0; 4], take: vec![] };
        for _ in 0..120 {
            let local = |_: [f32; 4]| play.0;
            let visual = fly.visual_play(&guide, local, 1.0 / 60.0);
            fly.step(1.0 / 60.0, screen, &hand, &guide, visual.as_ref());
        }
        fly.bubble = None;
        for _ in 0..30 {
            assert!(fly.visual_play(&Guide::Hidden, |_| play.0, 1.0 / 60.0).is_none());
            fly.step(1.0 / 60.0, screen, &hand, &Guide::Hidden, None);
        }
        assert_eq!(fly.mood, Mood::Pointing);
        assert!(fly.spin.is_none() && fly.bubble.is_none());
        let visual = fly.visual_play(&guide, |_| play.0, 1.0 / 60.0);
        fly.step(1.0 / 60.0, screen, &hand, &guide, visual.as_ref());
        assert!(fly.bubble.is_none(), "la stessa carta non si riannuncia");
    }

    /// Quando la carta indicata sparisce (giocata), la mosca festeggia.
    #[test]
    fn cheers_when_played() {
        let (screen, hand, _) = sight(Guide::Thinking);
        let mut fly = Fly::default();
        let play = (hand[0], Vec::<Rect>::new());
        let guide = Guide::Play { id: AdviceId::default(), card: [0.0; 4], take: vec![] };
        for _ in 0..120 {
            fly.step(1.0 / 60.0, screen, &hand, &guide, Some(&play));
        }
        fly.played_confirmed = true;
        fly.step(1.0 / 60.0, screen, &hand, &Guide::Waiting, None);
        assert!(fly.spin.is_some());
        assert!(fly.bubble.as_ref().is_some_and(|b| PLAYED.contains(&b.text.as_str())));
    }
    #[test]
    fn outlines_always_use_latest_geometry_without_lag() {
        let mut fly = Fly::default();
        let a = Rect::from_min_size(pos2(100.0, 100.0), vec2(80.0, 120.0));
        let b = a.translate(vec2(110.0, 0.0));
        let first = AdviceId { card: 1, take: 0 };
        fly.glide(first, vec![a], 0.016);
        fly.retarget = false;
        let moving = fly.glide(first, vec![b], 0.016)[0];
        assert_eq!(moving, b);
        assert!(!fly.retarget);
        assert_eq!(fly.glide(AdviceId { card: 2, take: 0 }, vec![b], 0.016), vec![b]);
        assert!(fly.retarget);
    }

    #[test]
    fn waiting_without_a_confirmed_play_does_not_celebrate() {
        let (screen, hand, _) = sight(Guide::Thinking);
        let mut fly = Fly::default();
        let play = (hand[0], vec![]);
        let guide = Guide::Play { id: AdviceId::default(), card: [0.0; 4], take: vec![] };
        fly.step(0.016, screen, &hand, &guide, Some(&play));
        fly.step(0.016, screen, &hand, &Guide::Waiting, None);
        assert!(fly.spin.is_none());
        assert!(!fly.bubble.as_ref().is_some_and(|b| PLAYED.contains(&b.text.as_str())));
    }

    #[test]
    fn exact_spring_is_independent_of_render_frame_rate() {
        let run = |hz: u32| {
            let mut position = pos2(10.0, 20.0);
            let mut velocity = vec2(150.0, -20.0);
            for _ in 0..hz {
                (position, velocity) = spring(position, velocity, pos2(400.0, 250.0), 34.0, 7.5, 1.0 / hz as f32);
            }
            (position, velocity)
        };
        let reference = run(120);
        for hz in [30, 60, 144] {
            let value = run(hz);
            assert!((value.0 - reference.0).length() < 0.02, "{hz} Hz: {value:?}");
            assert!((value.1 - reference.1).length() < 0.02);
        }
    }

    #[test]
    fn complete_motion_stays_consistent_at_30_60_and_120_hz() {
        let (screen, hand, guide) = sight(Guide::Thinking);
        let run = |hz: u32| {
            let mut fly = Fly::default();
            for _ in 0..hz * 2 {
                fly.advance(1.0 / hz as f32, screen, &hand, &guide, None);
            }
            fly
        };
        let reference = run(120);
        for hz in [30, 60] {
            let fly = run(hz);
            assert!((fly.pos - reference.pos).length() < 0.1, "{hz}: {:?} vs {:?}", fly.pos, reference.pos);
            assert!((fly.vel - reference.vel).length() < 0.1);
        }
    }

    #[test]
    fn every_new_gesture_finishes_and_paints_finite_geometry() {
        let (screen, hand, _) = sight(Guide::Waiting);
        for gesture in [Gesture::Wave, Gesture::Stretch, Gesture::Groom, Gesture::Scan, Gesture::Bow, Gesture::Dance] {
            let mut fly = Fly::default();
            fly.gesture = Some((gesture, 0.0));
            fly.next_gesture = 100.0;
            let ctx = egui::Context::default();
            for frame in 0..240 {
                fly.step(1.0 / 60.0, screen, &hand, &Guide::Waiting, None);
                if frame % 30 == 0 {
                    let mut output =
                        ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
                            fly.paint(ui.painter(), None);
                        });
                    output.textures_delta.clear();
                    for mesh in ctx.tessellate(output.shapes, output.pixels_per_point) {
                        if let egui::epaint::Primitive::Mesh(mesh) = mesh.primitive {
                            assert!(mesh.vertices.iter().all(|v| v.pos.x.is_finite() && v.pos.y.is_finite()));
                        }
                    }
                }
            }
            assert!(fly.gesture.is_none(), "{gesture:?}");
        }
    }

    /// Export the actual egui meshes and texture updates for a software-rendered preview.
    /// Run with BRISMASTER_PSY_PREVIEW=/tmp/psy-preview cargo test -p brismaster export_animation_preview -- --ignored
    #[test]
    #[ignore = "manual animation preview export"]
    fn export_animation_preview() {
        let dir = std::path::PathBuf::from(std::env::var_os("BRISMASTER_PSY_PREVIEW").expect("preview directory"));
        std::fs::create_dir_all(&dir).unwrap();
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(640.0, 400.0));
        let hand = vec![Rect::from_min_size(pos2(260.0, 250.0), vec2(80.0, 120.0))];
        let states = [
            ("Ciao!", Guide::Waiting, Gesture::Wave),
            ("Mi stiracchio", Guide::Waiting, Gesture::Stretch),
            ("Pulizia delle zampette", Guide::Waiting, Gesture::Groom),
            ("Cerco le carte", Guide::Seeking, Gesture::Scan),
            (
                "Questa!",
                Guide::Play { id: AdviceId::default(), card: [260.0, 250.0, 340.0, 370.0], take: vec![] },
                Gesture::Bow,
            ),
            ("Vittoria!", Guide::Won, Gesture::Dance),
        ];
        let mut frames = Vec::new();
        let mut serial = 0;
        for (label, guide, gesture) in states {
            let mut fly = Fly::default();
            let play = matches!(guide, Guide::Play { .. }).then(|| (hand[0], vec![]));
            for _ in 0..120 {
                fly.advance(1.0 / 60.0, screen, &hand, &guide, play.as_ref());
            }
            fly.pos = pos2(320.0, 190.0);
            fly.vel = Vec2::ZERO;
            fly.bubble = None;
            fly.parts.clear();
            fly.gesture = Some((gesture, 0.0));
            fly.next_gesture = 100.0;
            for frame in 0..36 {
                let motion = if play.is_some() {
                    vec2((frame as f32 * 0.12).sin() * 110.0, (frame as f32 * 0.17).cos() * 20.0)
                } else { Vec2::ZERO };
                let hand = vec![hand[0].translate(motion)];
                let current = if play.is_some() {
                    let r = hand[0];
                    Guide::Play { id: AdviceId::default(), card: [r.min.x,r.min.y,r.max.x,r.max.y], take: vec![] }
                } else { guide.clone() };
                let play = fly.visual_play(&current, |r| Rect::from_min_max(pos2(r[0],r[1]),pos2(r[2],r[3])),0.05);
                for _ in 0..3 {
                    fly.advance(1.0 / 60.0, screen, &hand, &current, play.as_ref());
                }
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        time: Some(serial as f64 / 20.0),
                        ..Default::default()
                    },
                    |ui| {
                        let p = ui.painter();
                        p.rect_filled(screen, 0.0, Color32::from_rgb(19, 34, 32));
                        p.text(
                            pos2(320.0, 36.0),
                            Align2::CENTER_CENTER,
                            label,
                            FontId::proportional(24.0),
                            Color32::WHITE,
                        );
                        if play.is_some() {
                            p.rect_filled(hand[0], 5.0, Color32::from_rgb(240, 232, 210));
                            p.text(hand[0].center(), Align2::CENTER_CENTER, "A", FontId::proportional(36.0), OUTLINE);
                        }
                        fly.paint(p, play.as_ref());
                    },
                );
                let mut textures = Vec::new();
                for (id, deltas) in &output.textures_delta.set {
                    for delta in deltas {
                        let egui::ImageData::Color(img) = &delta.image;
                        let pixels: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
                        let name = format!("atlas-{serial}-{}.png", textures.len());
                        image::save_buffer(
                            dir.join(&name),
                            &pixels,
                            img.size[0] as u32,
                            img.size[1] as u32,
                            image::ColorType::Rgba8,
                        )
                        .unwrap();
                        textures.push(serde_json::json!({"id": format!("{id:?}"), "path": name, "pos": delta.pos}));
                    }
                }
                output.textures_delta.clear();
                let meshes: Vec<_> = ctx.tessellate(output.shapes, output.pixels_per_point).into_iter().filter_map(|p| {
                    if let egui::epaint::Primitive::Mesh(mesh) = p.primitive {
                        let vertices: Vec<_> = mesh.vertices.iter().map(|v| {
                            let [r,g,b,a] = v.color.to_array();
                            [v.pos.x, v.pos.y, v.uv.x, v.uv.y, r as f32, g as f32, b as f32, a as f32]
                        }).collect();
                        Some(serde_json::json!({"vertices": vertices, "indices": mesh.indices, "texture": format!("{:?}", mesh.texture_id), "clip": [p.clip_rect.min.x,p.clip_rect.min.y,p.clip_rect.max.x,p.clip_rect.max.y]}))
                    } else { None }
                }).collect();
                frames.push(serde_json::json!({"label": label, "textures": textures, "meshes": meshes}));
                serial += 1;
            }
        }
        std::fs::write(dir.join("frames.json"), serde_json::to_vec(&frames).unwrap()).unwrap();
    }
}
