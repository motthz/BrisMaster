//! Assistente: inserisci a mano briscola, carte in mano, pescate e giocate; il motore consiglia la mossa.

use crate::cards_ui::{self, BEST, CARD_SIZE, GOLD};
use crate::psyfly;
use crate::record::{self, Advice, CardRec, Event, Recorder};
use crate::vision::memory::{similarity, Memory};
use crate::vision::{read_briscola, CardView, Img, Reader, Reading, Scene};
use briscola_core::ai::{evaluate_with_log, MoveEval};
use briscola_core::cards::{self, Card, CardSet};
use briscola_core::infer;
use briscola_core::{Observation, Trick};
use eframe::egui::{self, vec2, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

pub const ME: u8 = 0;
pub const OPP: u8 = 1;
const SMALL_CARD: Vec2 = Vec2::new(60.0, 92.0);

/// Stato della partita come lo conosce il giocatore.
#[derive(Clone, Copy, Debug)]
pub struct Position {
    pub trump_card: Option<Card>,
    /// Chi apre la prima presa (scelto dall'utente).
    pub first: Option<u8>,
    pub hand: CardSet,
    pub played: CardSet,
    /// Carte nel mazzo, briscola scoperta compresa.
    pub deck_len: u8,
    pub opp_hand_len: u8,
    pub leader: u8,
    pub table: Option<Card>,
    pub points: [u8; 2],
    pub trump_card_owner: Option<u8>,
    /// Carte che l'utente deve ancora indicare come pescate.
    pub to_draw: u8,
    pub tricks: u8,
}

/// Cosa serve sapere dall'utente in questo momento.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Trump,
    First,
    Draw(u8),
    MyMove,
    OppMove,
    Over,
}

impl Default for Position {
    fn default() -> Self {
        Position {
            trump_card: None,
            first: None,
            hand: CardSet::EMPTY,
            played: CardSet::EMPTY,
            deck_len: 34,
            opp_hand_len: 3,
            leader: ME,
            table: None,
            points: [0, 0],
            trump_card_owner: None,
            to_draw: 3,
            tricks: 0,
        }
    }
}

impl Position {
    pub fn step(&self) -> Step {
        if self.trump_card.is_none() {
            Step::Trump
        } else if self.first.is_none() {
            Step::First
        } else if self.to_draw > 0 {
            Step::Draw(self.to_draw)
        } else if self.tricks >= 20 {
            Step::Over
        } else if self.to_move() == ME {
            Step::MyMove
        } else {
            Step::OppMove
        }
    }

    pub fn to_move(self) -> u8 {
        if self.table.is_some() {
            1 - self.leader
        } else {
            self.leader
        }
    }

    /// Carte di cui non so dove siano (mano avversaria o mazzo coperto).
    fn unseen(&self) -> CardSet {
        let mut u = CardSet::FULL.minus(self.hand).minus(self.played);
        if let Some(t) = self.table {
            u.remove(t);
        }
        if let Some(t) = self.trump_card {
            u.remove(t);
        }
        u
    }

    /// Carte che l'utente può indicare nel passo corrente.
    pub fn allowed(&self) -> CardSet {
        match self.step() {
            Step::Trump => CardSet::FULL,
            Step::Draw(_) => self.unseen(),
            Step::OppMove => {
                let mut u = self.unseen();
                // La briscola scoperta, se l'ha pescata lui e non l'ha ancora giocata.
                let t = self.trump_card.unwrap();
                if self.trump_card_owner == Some(OPP) && !self.played.contains(t) && self.table != Some(t) {
                    u.insert(t);
                }
                u
            }
            Step::MyMove => self.hand,
            Step::First | Step::Over => CardSet::EMPTY,
        }
    }

    /// Applica una carta indicata dall'utente nel passo corrente. Falso se non è ammessa.
    pub fn enter(&mut self, c: Card) -> Option<Option<Trick>> {
        if !self.allowed().contains(c) {
            return None;
        }
        match self.step() {
            Step::Trump => {
                self.trump_card = Some(c);
                Some(None)
            }
            Step::Draw(_) => {
                self.hand.insert(c);
                self.to_draw -= 1;
                Some(None)
            }
            Step::MyMove | Step::OppMove => Some(self.play(c)),
            _ => None,
        }
    }

    pub fn set_first(&mut self, p: u8) {
        self.first = Some(p);
        self.leader = p;
    }

    fn play(&mut self, c: Card) -> Option<Trick> {
        let p = self.to_move();
        if p == ME {
            self.hand.remove(c);
        } else {
            self.opp_hand_len -= 1;
        }
        let Some(lead) = self.table else {
            self.table = Some(c);
            return None;
        };
        let trump_card = self.trump_card.unwrap();
        let winner = if cards::beats(c, lead, cards::suit(trump_card)) { p } else { self.leader };
        let pts = cards::points(lead) + cards::points(c);
        self.points[winner as usize] += pts;
        self.played.insert(lead);
        self.played.insert(c);
        self.tricks += 1;
        let trick = Trick { leader: self.leader, lead, follow: c, winner, points: pts };
        self.table = None;
        self.leader = winner;
        if self.deck_len > 0 {
            // Pesca prima chi ha preso; l'ultima carta del mazzo è la briscola scoperta.
            self.opp_hand_len += 1;
            if self.deck_len == 2 && winner == OPP {
                self.hand.insert(trump_card);
                self.trump_card_owner = Some(ME);
            } else {
                self.to_draw = 1;
                if self.deck_len == 2 {
                    self.trump_card_owner = Some(OPP);
                }
            }
            self.deck_len -= 2;
        }
        Some(trick)
    }

    pub fn observation(&self) -> Observation {
        let trump_card = self.trump_card.unwrap_or(0);
        Observation {
            me: ME,
            hand: self.hand,
            opp_hand_len: self.opp_hand_len,
            deck_len: self.deck_len,
            trump_card,
            trump: cards::suit(trump_card),
            leader: self.leader,
            table: self.table,
            points: self.points,
            played: self.played,
            trump_card_owner: self.trump_card_owner,
        }
    }
}

/// Spiegazione a parole della mossa consigliata.
pub fn explain(obs: &Observation, evals: &[MoveEval]) -> Vec<String> {
    let best = evals[0];
    let c = best.card;
    let trump = obs.trump;
    let mut out = Vec::new();
    match obs.table {
        Some(t) => {
            let pts = cards::points(t) + cards::points(c);
            if cards::beats(c, t, trump) {
                if cards::suit(c) == trump && cards::suit(t) != trump {
                    out.push(format!("Tagli con la briscola: prendi {} e porti a casa {pts} punti.", cards::name(t)));
                } else {
                    out.push(format!("Prendi la presa con {}: incassi {pts} punti.", cards::name(c)));
                }
            } else if pts == 0 {
                out.push("Lasci la presa: sul tavolo non ci sono punti, scarti senza perdere nulla.".into());
            } else {
                let could = obs.hand.iter().find(|&h| cards::beats(h, t, trump));
                match could {
                    Some(h) => out.push(format!(
                        "Lasci {pts} punti all'avversario: prendere con {} ti costerebbe di più nel resto della partita.",
                        cards::name(h)
                    )),
                    None => out.push(format!(
                        "Non puoi prendere: dai all'avversario il meno possibile ({pts} punti in tutto)."
                    )),
                }
            }
        }
        None => {
            let p = cards::points(c);
            out.push(if cards::suit(c) == trump {
                if p >= 10 {
                    "Apri con un carico di briscola: le simulazioni dicono che conviene incassarlo ora.".to_string()
                } else {
                    "Apri con una briscola: togli briscole all'avversario e tieni la mano.".to_string()
                }
            } else if p == 0 {
                "Apri con un liscio: se l'avversario prende, guadagna poco o niente.".to_string()
            } else if p >= 10 {
                "Apri con un carico: secondo le simulazioni l'avversario difficilmente lo prende o lo taglia."
                    .to_string()
            } else {
                format!("Apri con una figura ({p} punti): rischio contenuto, tieni i lisci per dopo.")
            });
        }
    }
    if let Some(second) = evals.get(1) {
        let d = best.win_prob - second.win_prob;
        if d < 0.015 {
            out.push(format!("Scelta quasi indifferente: {} vale praticamente lo stesso.", cards::name(second.card)));
        } else if d >= 0.08 {
            out.push(format!(
                "Scelta netta: la seconda opzione ({}) ha {:.0} punti percentuali in meno di vittoria.",
                cards::name(second.card),
                d * 100.0
            ));
        }
    }
    if obs.deck_len == 0 {
        out.push("Mazzo finito: le carte dell'avversario sono note e il calcolo è esatto.".into());
    } else if obs.deck_len <= 4 {
        out.push("Ultime pescate: il motore risolve ogni finale possibile in modo esatto.".into());
    }
    let mut out_trumps = obs.unknown();
    if obs.trump_card_owner == Some(1 - obs.me) {
        out_trumps.insert(obs.trump_card);
    }
    let trumps: Vec<Card> = out_trumps.iter().filter(|&x| cards::suit(x) == trump).collect();
    let big: Vec<&str> = trumps
        .iter()
        .filter(|&&x| cards::points(x) >= 10)
        .map(|&x| cards::RANK_NAMES[cards::rank(x) as usize])
        .collect();
    let mut line = format!("Briscole che non hai ancora visto: {}", trumps.len());
    if !big.is_empty() {
        line += &format!(" (tra cui {})", big.join(" e "));
    }
    out.push(line + ".");
    let mine = obs.points[obs.me as usize];
    if mine <= 60 {
        out.push(format!("Hai {mine} punti: te ne mancano {} per arrivare a 61.", 61 - mine));
    } else {
        out.push(format!("Hai già {mine} punti: la partita è vinta."));
    }
    out
}

/// Risultato dell'analisi di una posizione.
struct Analysis {
    evals: Vec<MoveEval>,
    reasons: Vec<String>,
    samples: u32,
}

pub struct Assistant {
    pos: Position,
    /// Posizione, prese e eventi registrati prima di ogni azione (per annullare), e se l'azione
    /// l'ha fatta l'app da sola perché obbligata.
    history: Vec<(Position, usize, usize, bool)>,
    tricks: Vec<Trick>,
    rec: Recorder,
    generation: u64,
    pending_generation: u64,
    analysis: Option<Analysis>,
    pending: Option<Receiver<Analysis>>,
    analysis_started: Option<std::time::Instant>,
    analysis_retry_at: Option<std::time::Instant>,
    analysis_failures: u8,
    analysis_error: Option<String>,
    /// Tasti digitati finora per la carta in corso (es. "3" in attesa del seme).
    keys: String,
    entry_error: Option<String>,
    /// Spazio premuto mentre il motore calcolava: gioca il consiglio appena arriva.
    play_best_queued: bool,
    vision: Vision,
}

/// Distacco minimo fra la carta più probabile e la seconda per inserirla senza chiedere.
const AUTO_MARGIN: f32 = 1.4;

/// Letture con una carta in attesa di conferma prima di accettarla da soli.
const AUTO_ACCEPT: u32 = 6;

/// Letture con lo schermo cambiato e la partita ferma prima di ricostruirla dallo storico.
const STALL: u32 = 8;
/// Letture con la partita ferma e lo schermo più avanti dopo le quali si guarda indietro.
const BEHIND: u32 = 4;
/// Letture con la partita ferma dopo le quali si riparte da soli da quello che si vede.
const RESTART: u32 = 12;

/// Lettura della partita dallo schermo.
#[derive(Default)]
struct Vision {
    reader: Option<Reader<Scene>>,
    memory: Arc<Mutex<Memory>>,
    status: String,
    /// Carte riconosciute all'ultima lettura: si agisce solo se la lettura si ripete uguale
    /// (niente carte a metà animazione).
    seen: Option<Vec<Option<Card>>>,
    /// Carta riconosciuta con poca sicurezza, in attesa che l'utente confermi o la corregga.
    confirm: Option<Confirm>,
    /// Ultima carta inserita dalla lettura (con la lunghezza della cronologia prima di inserirla),
    /// per dimenticarla se l'utente la annulla.
    last: Option<(Card, Img, usize)>,
    /// Ultima scena letta: serve a ricordare la carta pescata quando l'utente la scrive a mano.
    scene: Option<Scene>,
    /// Immagine della briscola di questa partita: se sullo schermo ne compare un'altra, il sito ha
    /// distribuito una nuova partita.
    trump_img: Option<Img>,
    /// Candidati annullati dall'utente: se ricompaiono si chiede conferma invece di inserirli.
    doubts: Vec<(Step, Card)>,
    /// Letture stabili di fila in cui lo schermo contraddice la partita inserita.
    mismatch: u32,
    /// Letture stabili di fila che mostrano una partita nuova.
    new_deal: u32,
    /// Letture di fila con le carte in vista ma senza la briscola.
    no_trump: u32,
    /// Le carte viste finora in questa partita, in mano e in tavola: la partita si ricostruisce da
    /// qui, qualunque sia la velocità del sito e il posto in cui disegna le carte.
    sightings: Sightings,
    /// Scelta del monitor e dell'area del tavolo, e diagnosi.
    picker: crate::vision::screen::AreaPicker,
    /// L'ultima lettura non ha trovato il tavolo.
    lost: bool,
    /// Le carte decise dall'utente (scritte o confermate), con le prese fatte fino a quel momento
    /// e il passo: servono a ricostruire la partita dallo storico.
    user_moves: Vec<(u8, Step, Card)>,
    /// Chi apre la partita, se l'ha detto l'utente.
    user_first: Option<u8>,
    /// Vero mentre è la lettura dello schermo a inserire carte (non l'utente).
    auto: bool,
    /// Letture di fila in cui lo schermo è cambiato ma la partita no.
    stall: u32,
    /// Letture di fila senza tavolo, e se da quando il tavolo è tornato non ci si è ancora
    /// riallineati: solo allora la partita può essere andata avanti senza che si vedesse.
    away: u32,
    gap: bool,
    /// Indici dei riallineamenti senza eventi completi: non sono giocate osservate.
    gaps: Vec<usize>,
    /// La coda è elaborata a piccoli lotti per lasciare sempre rispondere l'interfaccia.
    catching_up: bool,
    inconsistent: bool,
    /// La carta in attesa di conferma e da quante letture aspetta.
    confirm_age: (Option<(Step, Card)>, u32),
    /// Letture passate ad aspettare una carta dell'avversario che non si è vista.
    unseen_opp: u32,
    /// Letture di fila, a mazzo finito, senza più carte in gioco sullo schermo.
    ended: u32,
    /// Letture di fila con la partita ferma e lo schermo diverso da quello che la partita dice.
    stuck: u32,
    /// L'ultima lettura (mano e carta in tavola) da cui ripartire, e da quante letture è uguale.
    anchor: (Vec<Card>, Option<Card>, u32),
    /// L'ora della lettura in corso (ms) e dell'ultimo scatto dei contatori d'attesa: le attese si
    /// misurano in tempo, non in letture (durante un'animazione ne arrivano molte in un attimo).
    now_ms: Option<u64>,
    last_tick: Option<u64>,
    tick: bool,
    t0: Option<std::time::Instant>,
    /// Dove sta la briscola scoperta sullo schermo.
    trump_at: Option<[f32; 4]>,
    /// Le carte della lettura prima (quale e dove) e la misura delle carte ferme.
    prev_views: Vec<(Card, [f32; 4])>,
    card_size: (f32, f32),
    /// La lettura (mano e carta in tavola) a cui rimettersi in pari, e da quante letture è uguale.
    behind: (Vec<Card>, Option<Card>, u32),
    /// Dopo una ripartenza a tavolo vuoto chi apre è una supposizione (io), e da quale lettura.
    guessed_leader: Option<u64>,
    restart_after: Option<std::time::Instant>,
    frame_id: u64,
    enabled: bool,
}

struct Confirm {
    step: Step,
    card: Card,
    img: Img,
    /// Fascia della carta che si vedeva sullo schermo.
    vis: [f32; 2],
    texture: Option<egui::TextureHandle>,
}

impl Default for Assistant {
    fn default() -> Self {
        Assistant {
            pos: Position::default(),
            history: Vec::new(),
            tricks: Vec::new(),
            rec: Recorder::default(),
            generation: 0,
            pending_generation: 0,
            analysis: None,
            pending: None,
            analysis_started: None,
            analysis_retry_at: None,
            analysis_failures: 0,
            analysis_error: None,
            keys: String::new(),
            entry_error: None,
            play_best_queued: false,
            vision: Vision::default(),
        }
    }
}

impl Assistant {
    fn changed(&mut self) {
        self.generation += 1;
        self.analysis = None;
        // Keep one in-flight calculation; its epoch will reject an obsolete result.
        if self.pending.is_none() { self.analysis_started = None; }
        self.analysis_retry_at = None;
        self.analysis_failures = 0;
        self.analysis_error = None;
        self.keys.clear();
        self.entry_error = None;
        self.play_best_queued = false;
        self.vision.seen = None;
        self.vision.confirm = None;
    }

    fn set_first(&mut self, p: u8) {
        if !self.vision.auto {
            self.vision.user_first = Some(p);
        }
        self.first_entry(p);
        self.auto_fill();
    }

    fn first_entry(&mut self, p: u8) {
        self.history.push((self.pos, self.tricks.len(), self.rec.events.len(), false));
        self.pos.set_first(p);
        self.rec.events.push(Event::PrimoDiMano { chi: record::who(p) });
        self.changed();
    }

    /// Sostituisce la carta pescata `wrong` con `right` e riscrive la partita da quella pescata,
    /// rifacendo tutto quello che è venuto dopo. Falso (e niente cambia) se la partita non torna più,
    /// per esempio perché la carta sbagliata è già stata giocata.
    fn correct_draw(&mut self, wrong: Card, right: Card) -> bool {
        self.correct_event(|e| matches!(e, Event::Pescata { carta } if carta.id == wrong), right)
    }

    /// Come `correct_draw`, per il primo evento della cronologia che soddisfa `is`.
    fn correct_event(&mut self, is: impl Fn(&Event) -> bool, right: Card) -> bool {
        // Una correzione non può rigiocare un tratto di cui mancano le carte.
        if !self.vision.gaps.is_empty() {
            return false;
        }
        let Some(i) = self
            .history
            .iter()
            .position(|h| self.rec.events.get(h.2).is_some_and(&is))
        else {
            return false;
        };
        // Le azioni da rifare: una per voce della cronologia (ognuna ha il suo evento).
        enum Act {
            Card(Card, bool),
            First(u8),
        }
        let acts: Vec<Act> = self.history[i..]
            .iter()
            .map(|h| match &self.rec.events[h.2] {
                Event::PrimoDiMano { chi } => Act::First(if *chi == record::who(ME) { ME } else { OPP }),
                Event::Briscola { carta } | Event::Pescata { carta } | Event::Giocata { carta, .. } => {
                    Act::Card(carta.id, h.3)
                }
            })
            .collect();
        let hint_step = match self.history[i].0.step() { Step::Draw(_) => Step::Draw(0), step => step };
        let hint_trick = self.history[i].0.tricks;
        let hint_card = match self.rec.events.get(self.history[i].2) {
            Some(Event::Briscola { carta } | Event::Pescata { carta } | Event::Giocata { carta, .. }) => Some(carta.id),
            _ => None,
        };
        let saved = (self.pos, self.history.clone(), self.tricks.clone(), self.rec.events.clone());
        let (p, n, e, _) = self.history[i];
        self.pos = p;
        self.tricks.truncate(n);
        self.rec.events.truncate(e);
        self.history.truncate(i);
        let mut ok = true;
        for (k, a) in acts.into_iter().enumerate() {
            match a {
                Act::Card(c, auto) => ok = self.record_entry(if k == 0 { right } else { c }, auto),
                Act::First(p) => self.first_entry(p),
            }
            if !ok {
                break;
            }
        }
        if !ok {
            (self.pos, self.history, self.tricks, self.rec.events) = saved;
            self.rec.write(&self.pos);
            self.changed();
        } else if let Some(wrong) = hint_card {
            for (trick, step, card) in &mut self.vision.user_moves {
                if *trick == hint_trick && *step == hint_step && *card == wrong {
                    *card = right;
                }
            }
        }
        self.vision.last = None;
        ok
    }

    /// Annulla l'ultima azione dell'utente, insieme a quelle che l'app ha fatto da sola dopo.
    fn undo(&mut self) {
        self.undo_entries();
        // Se si è annullata una carta inserita dalla lettura dello schermo, era sbagliata.
        if let Some((c, img, len)) = self.vision.last.take() {
            if self.history.len() <= len {
                self.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).forget(&img);
                self.vision.doubts.push((self.pos.step(), c));
            }
        }
    }

    fn undo_entries(&mut self) {
        while let Some((p, n, e, auto)) = self.history.pop() {
            let was_gap = self.vision.gaps.last() == Some(&self.history.len());
            self.vision.gaps.retain(|&i| i < self.history.len());
            if !was_gap {
                match self.rec.events.get(e) {
                    Some(Event::PrimoDiMano { .. }) => self.vision.user_first = None,
                    Some(Event::Briscola { carta } | Event::Pescata { carta } | Event::Giocata { carta, .. }) => {
                        let step = match p.step() { Step::Draw(_) => Step::Draw(0), step => step };
                        if let Some(i) = self.vision.user_moves.iter().rposition(|&(t, s, c)| t == p.tricks && s == step && c == carta.id) {
                            self.vision.user_moves.remove(i);
                        }
                    }
                    None => {}
                }
            }
            self.pos = p;
            self.tricks.truncate(n);
            self.rec.events.truncate(e);
            self.rec.write(&self.pos);
            self.changed();
            if !auto {
                break;
            }
        }
    }

    /// Inserisce da sola le carte obbligate: la pescata quando resta una sola carta possibile,
    /// l'ultima carta dell'avversario a mazzo finito, la tua ultima carta.
    fn auto_fill(&mut self) {
        while matches!(self.pos.step(), Step::Draw(_) | Step::MyMove | Step::OppMove) {
            let allowed = self.pos.allowed();
            if allowed.len() != 1 {
                break;
            }
            let c = allowed.iter().next().unwrap();
            if !self.record_entry(c, true) {
                break;
            }
        }
    }

    fn reset(&mut self) {
        let vision = std::mem::take(&mut self.vision);
        let generation = self.generation.wrapping_add(1);
        let pending = self.pending.take();
        let pending_generation = self.pending_generation;
        *self = Assistant::default();
        self.generation = generation;
        self.pending = pending;
        self.pending_generation = pending_generation;
        self.vision = Vision {
            enabled: vision.enabled,
            reader: vision.reader,
            memory: vision.memory,
            status: vision.status,
            picker: vision.picker,
            ..Default::default()
        };
        self.vision.doubts.clear();
    }

    /// Inserisce una carta indicata dall'utente. Falso se non è possibile adesso.
    fn enter(&mut self, c: Card) -> bool {
        let (tricks, step) = (self.pos.tricks, self.pos.step());
        let ok = self.record_entry(c, false);
        if ok {
            if !self.vision.auto {
                let step = if matches!(step, Step::Draw(_)) { Step::Draw(0) } else { step };
                self.vision.user_moves.push((tricks, step, c));
            }
            self.auto_fill();
        }
        ok
    }

    fn record_entry(&mut self, c: Card, auto: bool) -> bool {
        let before = self.pos;
        let event = match before.step() {
            Step::Trump => Event::Briscola { carta: CardRec::of(c) },
            Step::Draw(_) => Event::Pescata { carta: CardRec::of(c) },
            Step::MyMove => {
                let advice = self.analysis.as_ref().map(|a| Advice::new(&a.evals, a.samples));
                Event::play(&before, c, advice)
            }
            _ => Event::play(&before, c, None),
        };
        match self.pos.enter(c) {
            Some(trick) => {
                self.history.push((before, self.tricks.len(), self.rec.events.len(), auto));
                self.tricks.extend(trick);
                self.rec.events.push(event);
                self.rec.write(&self.pos);
                self.changed();
                true
            }
            None => false,
        }
    }

    /// Come ha giocato l'avversario finora, per dedurne le carte: si ricava dalla cronologia,
    /// confrontando ogni posizione con quella dopo la mossa.
    fn opp_log(&self) -> Vec<infer::Event> {
        let mut log = Vec::new();
        let afters = self.history.iter().skip(1).map(|h| h.0).chain(std::iter::once(self.pos));
        for (i, (&(before, ..), after)) in self.history.iter().zip(afters).enumerate() {
            // Dopo un buco si usa soltanto la sequenza nuovamente osservata: il salto
            // non descrive né una carta avversaria né una pescata conosciuta.
            if self.vision.gaps.last().is_some_and(|&gap| i <= gap) {
                continue;
            }
            let step = before.step();
            if step == Step::OppMove {
                let card = match before.table {
                    None => match after.table { Some(card) => card, None => return Vec::new() },
                    Some(lead) => {
                        let mut gone = after.played.minus(before.played);
                        gone.remove(lead);
                        match gone.iter().next() { Some(card) => card, None => return Vec::new() }
                    }
                };
                log.push(infer::Event::OppMove { seen: before.observation(), card });
            }
            // A ogni presa chiusa col mazzo non finito pesca anche l'avversario; l'ultima carta
            // del mazzo è la briscola scoperta, e la prende lui se la presa l'ho fatta io.
            if matches!(step, Step::OppMove | Step::MyMove) && before.table.is_some() && before.deck_len > 0 {
                log.push(infer::Event::OppDraw { trump: before.deck_len == 2 && after.leader == ME });
            }
        }
        log
    }

    fn analysis_failed(&mut self, why: &str, ctx: &egui::Context) {
        self.pending = None;
        self.analysis_started = None;
        self.play_best_queued = false;
        self.analysis_failures = (self.analysis_failures + 1).min(2);
        self.analysis_retry_at = (self.analysis_failures < 2).then(|| std::time::Instant::now() + std::time::Duration::from_millis(750));
        self.analysis_error = Some(format!("{why} {}", if self.analysis_failures < 2 {
            "Riprovo il consiglio automaticamente. Puoi continuare a inserire le carte."
        } else {
            "Puoi continuare a inserire le carte o riprovare il consiglio."
        }));
        ctx.request_repaint_after(std::time::Duration::from_millis(750));
    }

    fn screen_is_behind(&self) -> bool {
        self.vision.catching_up || self.vision.restart_after.is_some()
            || self.vision.reader.as_ref().is_some_and(|r| self.vision.inconsistent || (self.vision.lost && self.vision.gap) || r.lag() > std::time::Duration::from_millis(1500))
    }

    /// Un canale chiuso o un calcolo troppo lungo non lasciano il consiglio in attesa per sempre.
    fn poll(&mut self, samples: u32, ctx: &egui::Context) {
        if self.screen_is_behind() {
            self.analysis = None;
            self.pending = None;
            self.analysis_started = None;
            self.play_best_queued = false;
            return;
        }
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(_) if self.pending_generation != self.generation => {
                    self.pending=None; self.analysis_started=None;
                }
                Ok(a) if !a.evals.is_empty() => {
                    let best = a.evals.first().map(|e| e.card);
                    self.analysis = Some(a);
                    self.pending = None;
                    self.analysis_started = None;
                    self.analysis_retry_at = None;
                    self.analysis_error = (self.analysis_failures > 0).then(|| "Consiglio recuperato dopo un errore di calcolo.".into());
                    self.analysis_failures = 0;
                    if std::mem::take(&mut self.play_best_queued) {
                        if let Some(c) = best { self.enter(c); return; }
                    }
                }
                Ok(_) => self.analysis_failed("Il calcolo non ha restituito un consiglio.", ctx),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.analysis_failed("Il calcolo del consiglio si è interrotto.", ctx),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    if self.analysis_started.is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(15)) {
                        self.analysis_failed("Il consiglio non è arrivato in tempo.", ctx);
                    } else {
                        ctx.request_repaint_after(std::time::Duration::from_millis(60));
                    }
                }
            }
        }
        let wants = self.pos.step() == Step::MyMove && self.pos.hand.len() > 1;
        let stale = self.analysis.as_ref().is_some_and(|a| a.samples != samples);
        if wants && self.pending.is_none() && self.analysis_failures < 2 && (self.analysis.is_none() || stale) {
            if let Some(wait) = self.analysis_retry_at.and_then(|t| t.checked_duration_since(std::time::Instant::now())) {
                ctx.request_repaint_after(wait);
                return;
            }
            let obs = self.pos.observation();
            // Un secondo tentativo non dipende dalla deduzione che potrebbe aver fatto fallire il primo.
            let log = if self.analysis_failures > 0 { Vec::new() } else { self.opp_log() };
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let mut rng = SmallRng::from_entropy();
                let evals = evaluate_with_log(&obs, &crate::app::engine_cfg(samples), &log, &mut rng);
                let reasons = explain(&obs, &evals);
                let _ = tx.send(Analysis { evals, reasons, samples });
            });
            self.pending_generation = self.generation;
            self.pending = Some(rx);
            self.analysis_started = Some(std::time::Instant::now());
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }

    /// Tastiera sempre attiva, senza campo di testo né Invio: "3c" inserisce il tre di coppe appena
    /// digitato il seme; al tuo turno Spazio (o Invio) gioca il consiglio e 1, 2, 3 la carta in mano
    /// in quella posizione; per chi comincia 1 = tu, 2 = l'avversario.
    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx) {
            return;
        }
        // Niente pulsanti col focus: Spazio e Invio li premerebbero insieme alla scorciatoia.
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            self.undo();
        }
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            let step = self.pos.step();
            match ev {
                egui::Event::Key { key, pressed: true, modifiers, .. } if modifiers.is_none() => match key {
                    egui::Key::Space | egui::Key::Enter if self.vision.confirm.is_some() => self.accept_vision(),
                    egui::Key::Space | egui::Key::Enter if step == Step::MyMove => self.play_best(),
                    egui::Key::Enter if step == Step::Over => self.reset(),
                    egui::Key::Backspace => {
                        self.keys.pop();
                    }
                    egui::Key::Escape => {
                        self.keys.clear();
                        self.entry_error = None;
                        if let Some(c) = self.vision.confirm.take() {
                            self.reject(c.step, c.card);
                        }
                    }
                    _ => {}
                },
                egui::Event::Text(t) => {
                    for ch in t.chars().filter(|c| !c.is_whitespace()) {
                        self.type_char(ch.to_ascii_lowercase());
                    }
                }
                _ => {}
            }
        }
    }

    fn type_char(&mut self, ch: char) {
        match self.pos.step() {
            Step::First => match ch {
                '1' => self.set_first(ME),
                '2' => self.set_first(OPP),
                _ => {}
            },
            Step::MyMove => {
                if let Some(i) = ch.to_digit(10).filter(|d| *d >= 1) {
                    if let Some(c) = self.pos.hand.to_vec().get(i as usize - 1) {
                        self.enter(*c);
                    }
                }
            }
            Step::Trump | Step::Draw(_) | Step::OppMove => {
                self.keys.push(ch);
                self.entry_error = None;
                if let Some(c) = cards_ui::parse_card(&self.keys) {
                    self.keys.clear();
                    // Durante un recupero si può inserire una carta del passato:
                    // l'immagine attuale non ne è necessariamente la fotografia.
                    let learn_image = self.vision.reader.is_some() && !self.vision.lost
                        && !self.vision.inconsistent && !self.screen_is_behind();
                    let seen = self.vision.confirm.take();
                    // La carta che l'utente scrive a mano si impara dall'immagine sullo schermo:
                    // la pescata (l'unica carta della mano non ancora nota) o quella dell'avversario.
                    let shown = match self.pos.step() {
                        Step::Trump => {
                            let trump = self.vision.scene.as_ref().and_then(|s| s.trump.as_ref());
                            trump.map(|v| (v.img.clone(), v.vis))
                        }
                        Step::Draw(_) => self.new_hand_view().map(|v| (v.img.clone(), v.vis)),
                        Step::OppMove => self.opp_view_of(c),
                        _ => None,
                    };
                    // La carta proposta e corretta a mano era sbagliata.
                    if let Some(p) = seen
                        .as_ref()
                        .filter(|p| p.card != c && p.step == self.pos.step())
                    {
                        let (step, card) = (p.step, p.card);
                        self.reject(step, card);
                    }
                    if self.enter(c) {
                        if learn_image {
                            if let Some((img, vis)) = seen.map(|v| (v.img, v.vis)).or(shown) {
                                self.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).remember(&img, vis, c);
                            }
                        }
                    } else {
                        self.entry_error = Some(format!("{} non è possibile adesso", cards::name(c)));
                    }
                } else if !is_rank_prefix(&self.keys) {
                    self.entry_error = Some(format!("\"{}\" non è una carta: scrivi valore e seme, es. 3c", self.keys));
                    self.keys.clear();
                }
            }
            Step::Over => {}
        }
    }

    /// Accende o spegne la lettura dello schermo.
    fn set_vision(&mut self, on: bool, ctx: &egui::Context) {
        self.vision.enabled = on;
        if on && self.vision.reader.is_none() {
            self.vision.memory = Memory::shared();
            let every = std::time::Duration::from_millis(350);
            self.vision.reader = Some(Reader::start(self.vision.memory.clone(), ctx.clone(), every, read_briscola));
            self.vision.status = "Cerco il tavolo sullo schermo...".into();
        } else if !on {
            self.vision.reader = None;
            self.vision.restart_after = None;
            self.vision.catching_up = false;
            self.vision.inconsistent = false;
            self.vision.confirm = None;
            self.vision.status.clear();
        }
    }

    fn restart_vision(&mut self, ctx: &egui::Context) {
        self.vision.reader = None;
        self.vision.catching_up = false;
        self.vision.restart_after = None;
        self.vision.gap = true;
        self.vision.lost = true;
        self.set_vision(true, ctx);
    }

    /// Vero mentre si legge lo schermo.
    pub fn reading(&self) -> bool {
        self.vision.enabled && self.vision.reader.is_some()
    }

    /// Accende la lettura dello schermo (la Psy Fly ne ha bisogno per vedere le carte).
    pub fn vision_on(&mut self, ctx: &egui::Context) {
        self.set_vision(true, ctx);
    }

    pub fn vision_active(&mut self, active: bool, ctx: &egui::Context) {
        if active {
            if self.vision.enabled {
                self.set_vision(true, ctx);
            }
        } else {
            self.vision.reader = None;
            self.vision.lost = true;
            self.vision.gap = true;
            self.vision.restart_after = None;
            self.vision.catching_up = false;
        }
    }

    pub fn vision_off(&mut self, ctx: &egui::Context) {
        self.set_vision(false, ctx);
        self.vision.lost = true;
    }

    /// Cosa deve fare la Psy Fly: dove sono le tue carte sullo schermo e quale giocare.
    pub fn fly_sight(&self) -> psyfly::Sight {
        let Some(reader) = &self.vision.reader else {
            return psyfly::Sight::off();
        };
        let placement = reader.placement(self.vision.frame_id);
        let monitor = placement.monitor;
        if self.screen_is_behind() {
            return psyfly::Sight { monitor, hand: Vec::new(), guide: psyfly::Guide::Hidden, context:(0,self.generation), played_card:None };
        }
        // Le carte lette ma non più sullo schermo (sparite, coperte, schermo cambiato) non si
        // segnano: la mosca aspetta la prossima lettura invece di indicare un posto vuoto.
        let scene = self.vision.scene.as_ref().filter(|_| !self.vision.lost);
        if self.pos.step() != Step::Over && scene.is_some() && placement.hidden {
            return psyfly::Sight {
                monitor,
                hand: Vec::new(),
                guide: psyfly::Guide::Hidden,
                context: (0, self.generation),
                played_card: self.history.last().and_then(|(before, _, _, auto)| {
                    if !*auto {
                        before.hand.minus(self.pos.hand).iter().next()
                    } else {
                        None
                    }
                }),
            };
        }
        let hand: Vec<[f32; 4]> =
            scene.map_or(Vec::new(), |s| s.hand.iter().filter_map(|v| placement.on_desktop(v.at)).collect());
        let guide = match self.pos.step() {
            Step::Over => match self.pos.points[ME as usize] {
                p if p > 60 => psyfly::Guide::Won,
                60 => psyfly::Guide::Tie,
                _ => psyfly::Guide::Lost,
            },
            _ if scene.is_none() => psyfly::Guide::NoTable,
            Step::MyMove => {
                let best = self.analysis.as_ref().and_then(|a| a.evals.first()).map(|e| e.card);
                // La carta consigliata fra quelle in mano sullo schermo.
                let view = best.and_then(|c| crate::vision::locate(&scene?.hand, self.pos.hand, c));
                match view.and_then(|v| placement.on_desktop(v.at)) {
                    Some(card) => psyfly::Guide::Play {
                        id: psyfly::AdviceId { card: best.unwrap(), take: 0 },
                        card,
                        take: Vec::new(),
                    },
                    None if self.pending.is_some() => psyfly::Guide::Thinking,
                    None => psyfly::Guide::Seeking,
                }
            }
            _ => psyfly::Guide::Waiting,
        };
        psyfly::Sight {
            monitor,
            hand,
            guide,
            context: (0, self.generation),
            played_card: self.history.last().and_then(|(before, _, _, auto)| {
                if !*auto {
                    before.hand.minus(self.pos.hand).iter().next()
                } else {
                    None
                }
            }),
        }
    }

    /// Recupera tutte le letture in ordine, senza occupare un intero fotogramma con la coda.
    fn poll_vision(&mut self, ctx: &egui::Context) -> usize {
        let started = std::time::Instant::now();
        let mut processed = 0;
        self.vision.catching_up = false;
        while processed < 8 {
            let next = match &self.vision.reader {
                Some(reader) => reader.rx.try_recv(),
                None => break,
            };
            match next {
                Ok(reading) => {
                    self.take_reading(reading);
                    processed += 1;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.vision.lost = true;
                    self.vision.gap = true;
                    self.vision.status = "Lettura dello schermo interrotta: la riavvio conservando la partita.".into();
                    self.vision.restart_after.get_or_insert_with(|| std::time::Instant::now() + std::time::Duration::from_secs(1));
                    ctx.request_repaint_after(std::time::Duration::from_secs(1));
                    break;
                }
            }
            if processed == 8 || started.elapsed() >= std::time::Duration::from_millis(8) {
                self.vision.catching_up = true;
                ctx.request_repaint();
                break;
            }
        }
        processed
    }

    /// Una lettura dello schermo.
    fn take_reading(&mut self, latest: Reading<Scene>) {
        match Some(latest) {
            Some(Reading::Scene(scene, frame_id)) => {
                self.vision.status = format!(
                    "Vedo: briscola {} · {} carte in mano · {} in tavola",
                    if scene.trump.is_some() { "sì" } else { "no" },
                    scene.hand.len(),
                    scene.opp_slot.is_some() as u8 + scene.my_slot.is_some() as u8
                );
                let t0 = *self.vision.t0.get_or_insert_with(std::time::Instant::now);
                self.vision.now_ms = Some(t0.elapsed().as_millis() as u64);
                self.sync(&scene);
                self.vision.scene = Some(scene);
                self.vision.frame_id = frame_id;
                self.vision.lost = false;
            }
            Some(Reading::NoTable) => {
                self.lost_table();
                self.vision.lost = true;
                self.vision.status = "Non vedo il tavolo: tieni la partita aperta e visibile sullo schermo".into()
            }
            Some(Reading::Error(e)) => {
                self.lost_table();
                self.vision.lost = true;
                self.vision.status = format!("Lettura dello schermo non riuscita: {e}. Continuo dalla prossima schermata.");
            }
            None => {}
        }
    }

    /// Una lettura senza tavolo (un'altra finestra davanti, un'altra scheda).
    fn lost_table(&mut self) {
        self.vision.away += 1;
        if self.vision.away >= 3 {
            self.vision.gap = true;
        }
    }

    /// Inserisce nella partita quello che si vede sullo schermo, se è uguale alla lettura precedente.
    fn sync(&mut self, scene: &Scene) {
        let settled = self.settle(scene);
        let scene = &settled;
        // I contatori d'attesa scattano al più ogni 350 ms (ogni lettura, se l'ora non si sa).
        let tick = match (self.vision.now_ms, self.vision.last_tick) {
            (Some(t), Some(l)) if t.saturating_sub(l) < 350 => false,
            (Some(t), _) => {
                self.vision.last_tick = Some(t);
                true
            }
            (None, _) => true,
        };
        self.vision.tick = tick;
        let tick = tick as u32;
        self.vision.away = 0;
        // Una carta in attesa di conferma non ferma la partita: se l'utente non risponde entro
        // qualche lettura si accetta da sola (se era sbagliata la correggono i controlli dopo).
        match self.vision.confirm.as_ref().map(|c| (c.step, c.card)) {
            Some(key) => {
                if self.vision.confirm_age.0 == Some(key) {
                    self.vision.confirm_age.1 += tick;
                } else {
                    self.vision.confirm_age = (Some(key), 1);
                }
                if self.vision.confirm_age.1 >= AUTO_ACCEPT {
                    self.vision.confirm_age = (None, 0);
                    self.auto_accept();
                }
            }
            None => self.vision.confirm_age = (None, 0),
        }
        // La carta dell'avversario non si è vista e nessuno la scrive: dopo un po' la partita
        // riparte da quello che si vede (la presa non vista resta segnata come tale).
        if self.pos.step() == Step::OppMove && self.vision.status.starts_with("Non ho visto la carta giocata") {
            self.vision.unseen_opp += tick;
            if self.vision.unseen_opp >= 6 {
                self.vision.gap = true;
            }
        } else {
            self.vision.unseen_opp = 0;
        }
        let fixed = self.fix_layout(scene);
        let scene = fixed.as_ref().unwrap_or(scene);
        let best = |v: &CardView| v.best(CardSet::FULL).map(|b| b.0);
        let mut sig: Vec<Option<Card>> =
            [&scene.trump, &scene.opp_slot, &scene.my_slot].iter().map(|v| v.as_ref().and_then(best)).collect();
        sig.extend(scene.hand.iter().map(best));
        let prev = self.vision.seen.replace(sig.clone());
        let Some(prev) = prev else { return };
        if prev == sig {
            // Una partita nuova si riconosce solo da letture uguali di tutto lo schermo: una
            // briscola letta male una volta non deve far ricominciare la partita.
            // E solo se lo dicono più letture di fila: un errore di lettura non deve ricominciare
            // una partita in corso.
            if self.new_deal(scene) {
                self.vision.new_deal += tick;
                if self.vision.new_deal >= 4 || self.pos.step() == Step::Over {
                    self.reset();
                }
            } else {
                self.vision.new_deal = 0;
            }
            self.check_mismatch(scene);
        }
        self.vision.sightings.update(scene);
        if prev == sig {
            self.check_hand(scene);
        }
        // La briscola, da due letture uguali.
        if self.pos.step() == Step::Trump && prev.first() == sig.first() {
            self.sync_trump(scene);
        }
        // Le carte si vedono ma la briscola no (coperta, attaccata a qualcos'altro): si chiede.
        if self.pos.step() == Step::Trump && scene.trump.is_none() && !scene.hand.is_empty() {
            self.vision.no_trump += tick;
            if self.vision.no_trump >= 8 {
                self.vision.status = "Non trovo la briscola sullo schermo: scrivila (per esempio 7d) e continuo da solo".into();
            }
        } else {
            self.vision.no_trump = 0;
        }
        // Poi gli eventi dedotti da quello che si è visto, nell'ordine in cui il gioco li vuole.
        let before = self.history.len();
        for _ in 0..12 {
            if self.vision.confirm.is_some() || !self.advance(scene) {
                break;
            }
        }
        // Fine partita non vista: a mazzo finito, se sullo schermo non resta nessuna carta ancora
        // in gioco (né in mano né in tavola) per qualche lettura, la partita è finita anche se le
        // ultime giocate non si sono viste. Si chiude da sola, segnando le prese come non viste.
        if self.pos.deck_len == 0 && self.pos.tricks >= 17 && matches!(self.pos.step(), Step::MyMove | Step::OppMove) {
            let trump = self.pos.trump_card;
            let live = scene.hand.iter().chain(&scene.opp_slot).chain(&scene.my_slot).any(|v| {
                v.best(CardSet::FULL).is_some_and(|(c, _)| {
                    !self.pos.played.contains(c) && (Some(c) != trump || self.pos.hand.contains(c))
                })
            });
            self.vision.ended = if live { 0 } else { self.vision.ended + tick };
            if self.vision.ended >= 5 {
                self.vision.ended = 0;
                self.finish_unseen();
            }
        } else {
            self.vision.ended = 0;
        }
        // Il controllore: se lo schermo continua a cambiare ma la partita resta ferma, la partita
        // si rigioca da capo con tutto quello che si è visto finora (e le carte decise
        // dall'utente). Se la ricostruzione va più avanti, si tiene quella.
        // Fermo vuol dire: la partita non avanza e quello che si vede (mano e tavolo) non è quello
        // che la partita dice, anche con lo schermo fermo.
        let sure = |v: &CardView| v.best(CardSet::FULL).filter(|b| b.1 >= AUTO_MARGIN).map(|b| b.0);
        let seen_hand: Vec<Card> = scene.hand.iter().filter_map(sure).collect();
        let seen_table: Vec<Card> = [&scene.opp_slot, &scene.my_slot].into_iter().flatten().filter_map(sure).collect();
        let differs = seen_hand.len() != self.pos.hand.len() as usize
            || seen_hand.iter().any(|&c| !self.pos.hand.contains(c))
            || self.pos.table.is_some_and(|t| !seen_table.contains(&t))
            || (self.pos.table.is_none() && !seen_table.is_empty());
        if !differs { self.vision.inconsistent = false; }
        if self.history.len() != before || !differs {
            self.vision.stuck = 0;
        } else if !matches!(self.pos.step(), Step::Trump | Step::First | Step::Over) {
            self.vision.stuck += tick;
        }
        if self.history.len() != before || !differs {
            self.vision.stall = 0;
        } else if self.vision.stuck >= BEHIND && self.catch_up(scene) {
            // Indietro rispetto al tavolo: le prese mancanti si sono rigiocate dalle letture passate.
            self.vision.stuck = 0;
            self.vision.stall = 0;
            self.vision.inconsistent = false;
        } else if self.vision.stuck >= RESTART && self.resync_from(scene, true) {
            // Ferma da troppo: nessuno ha scritto la carta che mancava, si riparte da quello che
            // si vede (le prese non viste restano segnate come tali).
            self.vision.stuck = 0;
            self.vision.stall = 0;
            self.vision.inconsistent = false;
        } else if !matches!(self.pos.step(), Step::Trump | Step::Over) {
            self.vision.stall += tick;
            if self.vision.stall >= STALL {
                self.vision.stall = 0;
                if self.rebuild(scene) || self.resync(scene) {
                    self.vision.inconsistent = false;
                } else {
                    self.vision.inconsistent = true;
                    if self.vision.mismatch < 8 {
                        let action = match self.pos.step() {
                            Step::Draw(_) if self.pos.tricks == 0 => "Inserisci la carta iniziale che manca, anche se è già stata giocata.",
                            Step::Draw(_) => "Inserisci la carta pescata che manca nella presa indicata.",
                            Step::OppMove => "Inserisci la carta avversaria mancante nella presa indicata.",
                            Step::MyMove => "Seleziona la carta che hai giocato nella presa indicata.",
                            _ => "Puoi annullare l'ultima lettura.",
                        };
                        self.vision.status = format!("La lettura dello schermo non corrisponde alla partita: controllo lo storico. {action}");
                    }
                }
            }
        }
        self.vision.seen = Some(sig);
    }

    /// Toglie dalla lettura le carte in movimento: una carta che sta entrando, girandosi o
    /// scivolando sul tavolo si legge spesso come un'altra. Vale solo una carta ferma (uguale e
    /// nello stesso punto della lettura prima) o con la misura delle carte ferme viste finora.
    fn settle(&mut self, scene: &Scene) -> Scene {
        let id = |v: &CardView| v.best(CardSet::FULL).map(|b| b.0);
        let size = |v: &CardView| (v.at[2] - v.at[0], v.at[3] - v.at[1]);
        let prev = std::mem::take(&mut self.vision.prev_views);
        let still = |v: &CardView| {
            let (w, h) = size(v);
            prev.iter().any(|(c, at)| {
                Some(*c) == id(v)
                    && (at[0] + at[2] - v.at[0] - v.at[2]).abs() < 0.08 * w
                    && (at[1] + at[3] - v.at[1] - v.at[3]).abs() < 0.08 * h
                    && ((at[3] - at[1]) - h).abs() < 0.04 * h
                    && ((at[2] - at[0]) - w).abs() < 0.04 * w
            })
        };
        let (rw, rh) = self.vision.card_size;
        let normal = |v: &CardView| {
            let (w, h) = size(v);
            rh > 0.0 && (h / rh - 1.0).abs() < 0.025 && (w / rw - 1.0).abs() < 0.025
        };
        // La briscola scoperta, vista al suo posto, è la briscola anche quando la lettura la mette
        // in mano o in tavola (le altre carte si stanno muovendo e non si vedono).
        let trump = self.pos.trump_card;
        if let Some(v) = scene.trump.as_ref().filter(|v| trump.is_none() || id(v) == trump) {
            self.vision.trump_at = Some(v.at);
        }
        let spot = self.vision.trump_at;
        let is_trump = |v: &CardView| {
            let (w, h) = size(v);
            trump.is_some() && id(v) == trump && spot.is_some_and(|at| {
                (at[0] + at[2] - v.at[0] - v.at[2]).abs() < w && (at[1] + at[3] - v.at[1] - v.at[3]).abs() < h
            })
        };
        let keep = |v: &CardView| !is_trump(v) && (still(v) || normal(v));
        let moved = scene.hand.iter().chain(&scene.opp_slot).chain(&scene.my_slot).find(|v| is_trump(v)).cloned();
        let out = Scene {
            trump: scene.trump.clone().or(moved),
            opp_slot: scene.opp_slot.clone().filter(|v| keep(v)),
            my_slot: scene.my_slot.clone().filter(|v| keep(v)),
            hand: scene.hand.iter().filter(|v| keep(v)).cloned().collect(),
        };
        // La misura delle carte ferme (cambia piano se cambia lo zoom).
        let mut sizes: Vec<(f32, f32)> = scene.hand.iter().chain(&scene.opp_slot).chain(&scene.my_slot).filter(|v| still(v)).map(size).collect();
        sizes.sort_by(|a, b| a.1.total_cmp(&b.1));
        if let Some(&(w, h)) = sizes.get(sizes.len() / 2) {
            self.vision.card_size = if rh > 0.0 && (h / rh - 1.0).abs() < 0.1 { (0.7 * rw + 0.3 * w, 0.7 * rh + 0.3 * h) } else { (w, h) };
        }
        self.vision.prev_views = scene
            .hand
            .iter()
            .chain(&scene.opp_slot)
            .chain(&scene.my_slot)
            .filter_map(|v| Some((id(v)?, v.at)))
            .collect();
        out
    }

    /// Rigioca la partita da capo dallo storico delle letture (gli avvistamenti di ogni carta, con
    /// quando e dove) e dalle carte decise dall'utente, e la adotta se arriva più avanti di quella
    /// in corso: un evento capito male o in ritardo non blocca più la partita.
    fn rebuild(&mut self, scene: &Scene) -> bool {
        if !self.vision.gaps.is_empty() {
            return false;
        }
        let Some(trump) = self.pos.trump_card else { return false };
        let mut b = Assistant::default();
        b.rec.off = true;
        b.vision.memory = Arc::new(Mutex::new(self.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).isolated()));
        b.vision.sightings = self.vision.sightings.clone();
        b.vision.doubts = self.vision.doubts.clone();
        if !b.enter(trump) {
            return false;
        }
        if let Some(p) = self.vision.user_first {
            b.set_first(p);
        }
        let moves = &self.vision.user_moves;
        let mut used = vec![false; moves.len()];
        for _ in 0..160 {
            if b.pos.step() == Step::Over {
                break;
            }
            b.vision.confirm = None;
            if b.advance(scene) {
                continue;
            }
            // Dove le letture non bastano, la carta che aveva deciso l'utente a quel punto.
            let step = match b.pos.step() {
                Step::Draw(_) => Step::Draw(0),
                s => s,
            };
            let allowed = b.pos.allowed();
            let next = moves
                .iter()
                .enumerate()
                .find(|(i, (t, s, c))| !used[*i] && *t == b.pos.tricks && *s == step && allowed.contains(*c));
            match next {
                Some((i, &(_, _, c))) => {
                    used[i] = true;
                    if !b.enter(c) {
                        break;
                    }
                }
                None => break,
            }
        }
        if b.history.len() <= self.history.len() {
            return false;
        }
        // La ricostruzione deve tornare con quello che si vede adesso: le carte lette con sicurezza
        // in mano devono essere in mano (al più manca la pescata da segnare) e non giocate.
        let sure = |v: &CardView| v.best(CardSet::FULL).filter(|b| b.1 >= AUTO_MARGIN).map(|b| b.0);
        let seen: Vec<Card> = scene.hand.iter().filter_map(sure).collect();
        let missing = seen.iter().filter(|&&c| !b.pos.hand.contains(c)).count();
        if seen.iter().any(|&c| b.pos.played.contains(c)) || missing > b.pos.to_draw as usize {
            return false;
        }
        let events = std::mem::take(&mut b.rec.events);
        self.pos = b.pos;
        self.history = b.history;
        self.tricks = b.tricks;
        self.rec.events = events;
        self.rec.write(&self.pos);
        self.changed();
        self.vision.last = None;
        self.vision.status = format!("Partita ricostruita dallo storico delle letture: {} prese", self.pos.tricks);
        true
    }

    /// Il tavolo non si è visto per un po' (un'altra finestra davanti, un'altra scheda) e intanto si
    /// è giocato: la partita riparte da quello che si vede adesso. Le mie carte sparite dalla mano
    /// sono giocate, ogni presa passata ha fatto pescare una carta a testa, e chi muove si capisce
    /// dalla carta in tavola. Le carte giocate dall'avversario in quel tempo non si possono sapere
    /// (e i punti possono non tornare): si dice all'utente.
    fn resync(&mut self, scene: &Scene) -> bool {
        self.resync_from(scene, false)
    }

    /// `free`: la partita è ferma da molte letture e nessuno la sblocca. Si riparte anche senza
    /// un tratto al buio, anche a tavolo vuoto (apro io, finché non si vede il contrario), e le mie
    /// carte lette male contano se sono quelle che la partita si aspetta.
    fn resync_from(&mut self, scene: &Scene, free: bool) -> bool {
        // Solo dopo un tratto in cui il tavolo non si vedeva davvero.
        if !self.vision.gap && !free {
            return false;
        }
        let pos = self.pos;
        if matches!(pos.step(), Step::Trump | Step::First | Step::Over) {
            return false;
        }
        let Some(trump) = pos.trump_card else { return false };
        // La mano letta con sicurezza, uguale nelle ultime letture.
        let sure = |v: &CardView| {
            v.best(CardSet::FULL)
                .filter(|b| {
                    // (Ripartendo da soli vale anche una carta letta senza sicurezza, se è una che
                    // può esserci: deve restare la stessa per più letture, vedi sotto.)
                    b.1 >= AUTO_MARGIN
                        || (free && !pos.played.contains(b.0) && !(Some(b.0) == pos.trump_card && pos.deck_len > 2))
                })
                .map(|b| b.0)
        };
        let weak = scene.hand.iter().chain(&scene.opp_slot).chain(&scene.my_slot).any(|v| {
            v.best(CardSet::FULL).is_some_and(|b| b.1 < AUTO_MARGIN && !pos.hand.contains(b.0))
        });
        let hand: Vec<Card> = scene.hand.iter().filter_map(sure).collect();
        let now = self.vision.sightings.frame;
        let table: Vec<Card> = [&scene.opp_slot, &scene.my_slot].into_iter().flatten().filter_map(sure).collect();
        let on_table = scene.opp_slot.is_some() as usize + scene.my_slot.is_some() as usize;
        let stable = if free {
            // La stessa mano e la stessa tavola per tre letture.
            let key = (hand.clone(), table.first().copied());
            let a = &mut self.vision.anchor;
            if (a.0.as_slice(), a.1) == (key.0.as_slice(), key.1) { a.2 += 1 } else { *a = (key.0, key.1, 1) }
            a.2 >= if weak { 6 } else { 3 } && table.len() == on_table
        } else {
            matches!(self.vision.sightings.full_hands.as_slice(),
                [.., (fa, a), (fb, b)] if a == b && *b == hand && *fb + 1 >= now && *fa + 4 >= now)
        };
        if !stable || hand.len() != scene.hand.len() || hand.is_empty() {
            return false;
        }
        // In tavola una carta sola, sicura, che non sia la briscola: dice chi muove.
        let t = match table.as_slice() {
            [t] => Some(*t),
            [] if free => None,
            _ => return false,
        };
        if t == Some(trump) && pos.deck_len > 0 {
            return false;
        }
        let Some(t) = t else { return self.restart_empty(&hand, scene) };
        let missing: Vec<Card> = pos.hand.iter().filter(|c| !hand.contains(c)).collect();
        let new: Vec<Card> = hand.iter().copied().filter(|&c| !pos.hand.contains(c)).collect();
        // La carta in tavola è mia se era in mano, o se sta dalla mia parte del tavolo (pescata e
        // giocata mentre il tavolo non si vedeva).
        let mine_on_table = missing.contains(&t) || scene.my_slot.as_ref().and_then(sure) == Some(t);
        let drawn_and_played = mine_on_table && !pos.hand.contains(t);
        // Carte a testa all'inizio della presa in tavola: meno di tre vuol dire mazzo finito, e allora
        // le prese fatte si sanno esattamente (20 meno le carte rimaste).
        let each = hand.len() as u8 + mine_on_table as u8;
        // (E la briscola non è più al suo posto: con una carta della mano non letta le carte
        // sembrano meno di tre anche col mazzo ancora pieno.)
        let trump_shown = scene.trump.as_ref().and_then(sure) == Some(trump);
        let exhausted = each < 3 && pos.deck_len > 0 && !trump_shown;
        if new.is_empty() && missing.len() <= 1 && !exhausted {
            // Niente di nuovo in mano: non è un buco, al più una giocata non vista.
            return false;
        }
        let gone: Vec<Card> = missing.iter().copied().filter(|&c| c != t).collect();
        // Nuove carte impossibili (già uscite o la briscola ancora sotto il mazzo): lettura sbagliata.
        if new.iter().any(|&c| pos.played.contains(c) || (c == trump && pos.deck_len > 2)) {
            return false;
        }
        // Avevo aperto io una presa rimasta a metà: quella mia carta è una delle giocate non viste.
        // Se le carte nuove bastano già a spiegarla, le altre che mancano dalla mano non sono
        // giocate: sono solo non lette adesso, e restano in mano.
        let led = pos.table.is_some() && pos.step() == Step::OppMove;
        let drawn = new.len() + drawn_and_played as usize;
        let unread = !(exhausted || pos.deck_len == 0) && led && drawn == 1 && !gone.is_empty();
        let done = if exhausted || pos.deck_len == 0 {
            (20 - each).saturating_sub(pos.tricks)
        } else if unread {
            1
        } else {
            gone.len().max(drawn) as u8
        };
        if done == 0 {
            return false;
        }
        let mut p = pos;
        p.hand = CardSet::EMPTY;
        for &c in &gone {
            if unread {
                p.hand.insert(c);
            } else {
                p.played.insert(c);
            }
        }
        if let Some(x) = pos.table {
            p.played.insert(x);
        }
        for &c in &hand {
            p.hand.insert(c);
        }
        p.tricks = (pos.tricks + done).min(19);
        p.deck_len = if exhausted { 0 } else { pos.deck_len.saturating_sub(2 * done) };
        if p.deck_len == 0 && pos.deck_len > 0 {
            p.trump_card_owner = Some(if hand.contains(&trump) { ME } else { OPP });
        }
        p.to_draw = 0;
        p.table = Some(t);
        p.leader = if mine_on_table { ME } else { OPP };
        if mine_on_table {
            p.hand.remove(t);
        }
        let mine = p.hand.len() as u8;
        p.opp_hand_len = if mine_on_table { mine + 1 } else { mine.saturating_sub(1) };
        if !mine_on_table && pos.unseen().contains(t) {
            // La carta dell'avversario in tavola è nota.
        } else if !mine_on_table {
            return false;
        }
        self.vision.gaps.push(self.history.len());
        self.history.push((self.pos, self.tricks.len(), self.rec.events.len(), false));
        self.pos = p;
        self.vision.gap = false;
        self.changed();
        self.vision.last = None;
        self.vision.status = format!(
            "Ripreso dopo un tratto in cui il tavolo non si vedeva: {done} prese non viste (le carte dell'avversario di quelle prese e i punti possono non essere giusti)"
        );
        true
    }

    /// La partita è rimasta indietro rispetto al tavolo: si riguardano le letture passate (le mie
    /// carte uscite dalla mano, le carte viste in tavola, le carte comparse in mano, ognuna con
    /// quando si è vista la prima volta) e si rigiocano le prese mancanti secondo le regole, finché
    /// la partita torna uguale a quello che si vede adesso. Vale solo se torna: mano e tavolo dello
    /// schermo sono la verifica, quindi qui bastano anche avvistamenti brevi o poco sicuri.
    fn catch_up(&mut self, scene: &Scene) -> bool {
        let pos = self.pos;
        if matches!(pos.step(), Step::Trump | Step::First | Step::Over) || self.vision.confirm.is_some() {
            return false;
        }
        let read = |v: &CardView| {
            v.best(CardSet::FULL).filter(|b| b.1 >= AUTO_MARGIN || (b.1 >= 0.0 && pos.hand.contains(b.0))).map(|b| b.0)
        };
        let hand: Vec<Card> = scene.hand.iter().filter_map(read).collect();
        let table: Vec<Card> = [&scene.opp_slot, &scene.my_slot].into_iter().flatten().filter_map(read).collect();
        let on_table = scene.opp_slot.is_some() as usize + scene.my_slot.is_some() as usize;
        if hand.len() != scene.hand.len() || hand.is_empty() || table.len() != on_table || table.len() > 1 {
            self.vision.behind.2 = 0;
            return false;
        }
        let t = table.first().copied();
        let b = &mut self.vision.behind;
        if (b.0.as_slice(), b.1) == (hand.as_slice(), t) { b.2 += 1 } else { *b = (hand.clone(), t, 1) }
        if b.2 < 2 {
            return false;
        }
        let mut target = CardSet::EMPTY;
        for &c in &hand {
            target.insert(c);
        }
        let s = &self.vision.sightings;
        let seen = |x: &Sighting| if x.count == 0 { u64::MAX } else { x.first };
        let mut p = pos;
        let mut moves: Vec<Card> = Vec::new();
        for _ in 0..14 {
            if !moves.is_empty() && p.hand == target && p.table == t && !matches!(p.step(), Step::Draw(_)) {
                break;
            }
            let allowed = p.allowed();
            let next = match p.step() {
                // Una carta che ho adesso (o che ho avuto in mano nel frattempo): la vista prima.
                Step::Draw(_) => (0..40u8)
                    .filter(|&c| allowed.contains(c) && !p.hand.contains(c) && Some(c) != t)
                    .filter(|&c| target.contains(c) || s.in_hand(c))
                    .min_by_key(|&c| (seen(&s.hand[c as usize]), !target.contains(c))),
                // Una mia carta che non ho più (o che adesso è in tavola): quella uscita prima.
                Step::MyMove => p
                    .hand
                    .iter()
                    .filter(|&c| allowed.contains(c) && !target.contains(c))
                    .min_by_key(|&c| (Some(c) == t && p.hand.len() > target.len() as u32 + 1, seen(&s.table[c as usize]))),
                // Una carta vista in tavola che non è mai stata mia: quella comparsa prima.
                Step::OppMove => (0..40u8)
                    .filter(|&c| allowed.contains(c) && !target.contains(c) && !p.hand.contains(c) && s.hand[c as usize].count == 0)
                    .filter(|&c| s.table[c as usize].count >= 1 && s.table[c as usize].margin >= SIGHT_MARGIN)
                    .min_by_key(|&c| (Some(c) == t && p.hand.len() > target.len() as u32, seen(&s.table[c as usize]))),
                _ => None,
            };
            let Some(c) = next else { return false };
            if p.enter(c).is_none() {
                return false;
            }
            moves.push(c);
        }
        if moves.is_empty() || p.hand != target || p.table != t || matches!(p.step(), Step::Draw(_)) {
            return false;
        }
        // Torna: le stesse giocate entrano nella partita vera, una per una.
        let n = moves.len();
        self.vision.auto = true;
        for &c in &moves {
            if !self.pos.allowed().contains(c) || !self.enter(c) {
                break;
            }
        }
        self.vision.auto = false;
        self.vision.behind.2 = 0;
        self.vision.last = None;
        self.vision.status = format!("Ero rimasto indietro: ho ripreso {n} giocate dalle letture precedenti");
        true
    }

    /// Ripartenza a tavolo vuoto: le mie carte sono quelle che si vedono, le prese passate si
    /// contano dalle mie carte uscite (o dalle carte rimaste, a mazzo finito), e apro io finché una
    /// carta dell'avversario in tavola non dice il contrario.
    fn restart_empty(&mut self, hand: &[Card], scene: &Scene) -> bool {
        let pos = self.pos;
        let Some(trump) = pos.trump_card else { return false };
        let sights = &self.vision.sightings;
        let missing: Vec<Card> = pos.hand.iter().filter(|c| !hand.contains(c)).collect();
        let new: Vec<Card> = hand.iter().copied().filter(|&c| !pos.hand.contains(c)).collect();
        if new.iter().any(|&c| pos.played.contains(c) || (c == trump && pos.deck_len > 2) || Some(c) == pos.table) {
            return false;
        }
        // Una mia carta è uscita solo se non si vede in mano da un po' (non per una lettura persa).
        if missing.iter().any(|&c| sights.frame.saturating_sub(sights.hand[c as usize].last) < 4) {
            return false;
        }
        let led = pos.table.is_some() && pos.step() == Step::OppMove;
        let each = hand.len() as u8;
        let trump_shown = scene.trump.as_ref().and_then(|v| v.best(CardSet::FULL)).is_some_and(|b| b.0 == trump && b.1 >= AUTO_MARGIN);
        let exhausted = each < 3 && pos.deck_len > 0 && !trump_shown;
        let done = if exhausted || pos.deck_len == 0 {
            (20 - each).saturating_sub(pos.tricks)
        } else {
            (missing.len() + led as usize).max(new.len()) as u8
        };
        if done == 0 || pos.tricks + done > 19 {
            return false;
        }
        let deck = if exhausted { 0 } else { pos.deck_len.saturating_sub(2 * done) };
        // Le carte in mano devono tornare con le prese fatte: tre finché si pesca.
        let want = if pos.tricks + done <= 17 { 3 } else { 20 - (pos.tricks + done) };
        if each != want {
            return false;
        }
        let mut p = pos;
        for &c in &missing {
            p.played.insert(c);
        }
        if let Some(x) = pos.table {
            p.played.insert(x);
        }
        p.hand = CardSet::EMPTY;
        for &c in hand {
            p.hand.insert(c);
        }
        p.tricks = pos.tricks + done;
        p.deck_len = deck;
        if deck == 0 && pos.deck_len > 0 {
            p.trump_card_owner = Some(if hand.contains(&trump) { ME } else { OPP });
        }
        p.to_draw = 0;
        p.table = None;
        p.leader = ME;
        p.opp_hand_len = each;
        self.vision.gaps.push(self.history.len());
        self.history.push((self.pos, self.tricks.len(), self.rec.events.len(), false));
        self.pos = p;
        self.vision.gap = false;
        // Quello che si era visto in tavola prima non vale più per le prese nuove.
        let s = &mut self.vision.sightings;
        for c in 0..40usize {
            s.table[c] = Sighting::default();
            s.opponents[c] = Sighting::default();
            s.lone_mine[c] = Sighting::default();
            s.my_side[c] = Sighting::default();
            s.their_side[c] = Sighting::default();
            if !p.hand.contains(c as Card) {
                s.hand[c] = Sighting::default();
            }
        }
        self.vision.guessed_leader = Some(s.frame);
        self.changed();
        self.vision.last = None;
        self.vision.status = format!(
            "Ripreso da quello che si vede: {done} prese non viste (le carte dell'avversario di quelle prese e i punti possono non essere giusti)"
        );
        true
    }

    /// La briscola scoperta (chiede conferma se non è sicura).
    fn sync_trump(&mut self, scene: &Scene) {
        self.vision.auto = true;
        self.sync_trump_inner(scene);
        self.vision.auto = false;
    }

    fn sync_trump_inner(&mut self, scene: &Scene) {
        let Some(v) = &scene.trump else { return };
        let Some((card, margin)) = v.best(CardSet::FULL) else { return };
        if margin >= AUTO_MARGIN && !self.vision.doubts.contains(&(Step::Trump, card)) {
            let len = self.history.len();
            if self.enter(card) {
                self.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).remember(&v.img, v.vis, card);
                self.vision.last = Some((card, v.img.clone(), len));
            }
        } else if self.vision.confirm.is_none() {
            self.vision.confirm =
                Some(Confirm { step: Step::Trump, card, img: v.img.clone(), vis: v.vis, texture: None });
        }
    }

    /// Una lettura con la briscola in tavola mentre è ancora sotto il mazzo ha sbagliato a capire
    /// dov'è cosa (succede con la mano a ventaglio coperta da una scritta e il tavolo vuoto): la
    /// briscola è quella, e le altre carte fuori dalla mano sono in mano.
    fn fix_layout(&self, scene: &Scene) -> Option<Scene> {
        // Con lo zoom alto la carta giocata può essere scambiata per una seconda
        // riga della mano. Conta il movimento rispetto alle letture precedenti:
        // una sola carta sale, le altre restano dove stavano.
        let opening = self.pos.step() == Step::First;
        let expected = if opening { 3 } else { self.pos.hand.len() as usize };
        if (opening || self.pos.step() == Step::MyMove) && scene.my_slot.is_none()
            && scene.hand.len() == expected && scene.hand.len() >= 2
        {
            let mut moved = Vec::new();
            let mut stationary = 0;
            for (i, view) in scene.hand.iter().enumerate() {
                let Some((card, margin)) = view.best(CardSet::FULL).filter(|b| b.1 >= SIGHT_MARGIN) else { continue };
                let _ = margin;
                if !opening && !self.pos.hand.contains(card) { continue; }
                let Some(old) = self.vision.sightings.hand_places.get(card as usize).copied().flatten() else { continue };
                let height = (view.at[3] - view.at[1]).abs().max((old[3] - old[1]).abs());
                if height <= 0.001 { continue; }
                let dy = (view.at[1] + view.at[3] - old[1] - old[3]) / 2.0;
                if dy < -0.45 * height { moved.push(i); }
                else if dy.abs() < 0.15 * height { stationary += 1; }
            }
            if let [i] = moved.as_slice() {
                if stationary + 1 == scene.hand.len() {
                    let mut corrected = scene.clone();
                    corrected.my_slot = Some(corrected.hand.remove(*i));
                    return Some(corrected);
                }
            }
        }
        let t = self.pos.trump_card?;
        if self.pos.deck_len < 2 {
            return None;
        }
        let is_trump = |v: &CardView| {
            v.best(CardSet::FULL)
                .is_some_and(|(c, m)| c == t && m >= AUTO_MARGIN)
        };
        // Nella riga presa per la mano c'è una carta che in mano non può essere (la briscola ancora
        // sotto il mazzo, quella in tavola; una carta già giocata sì: alcuni siti la ridisegnano in
        // mano per un attimo): la mano è coperta (una notifica,
        // una finestra) e la riga più bassa è fatta d'altro. Di questa lettura vale solo la briscola.
        let impossible = |v: &CardView| {
            v.best(CardSet::FULL).is_some_and(|(c, m)| {
                m >= AUTO_MARGIN && ((c == t && self.pos.deck_len > 2) || self.pos.table == Some(c))
            })
        };
        if scene.hand.iter().any(impossible) {
            let trump = [&scene.trump, &scene.opp_slot, &scene.my_slot]
                .into_iter()
                .flatten()
                .chain(scene.hand.iter())
                .find(|v| is_trump(v))
                .cloned();
            return Some(Scene { trump, opp_slot: None, my_slot: None, hand: Vec::new() });
        }
        if !scene.opp_slot.as_ref().is_some_and(is_trump)
            && !scene.my_slot.as_ref().is_some_and(is_trump)
        {
            return None;
        }
        // L'altra carta fuori dalla mano resta in tavola (con le carte grandi la briscola finisce
        // vicino alle carte giocate), a meno che non sia una carta che so di avere in mano: allora
        // non si sa se è in mano o giocata e non si conta.
        let trump = [&scene.trump, &scene.opp_slot, &scene.my_slot].into_iter().flatten().find(|v| is_trump(v)).cloned();
        let mine = |v: &CardView| v.best(CardSet::FULL).is_some_and(|(c, _)| self.pos.hand.contains(c));
        let table = [&scene.opp_slot, &scene.my_slot].into_iter().flatten().find(|v| !is_trump(v) && !mine(v)).cloned();
        let out = Scene { trump, opp_slot: table, my_slot: None, hand: scene.hand.clone() };
        Some(out)
    }

    /// Un passo della partita dedotto dagli avvistamenti. Falso se per ora non si può andare avanti.
    fn advance(&mut self, scene: &Scene) -> bool {
        self.vision.auto = true;
        let ok = self.advance_inner(scene);
        self.vision.auto = false;
        ok
    }

    fn advance_inner(&mut self, scene: &Scene) -> bool {
        let pos = self.pos;
        let sights = &self.vision.sightings;
        let event = match pos.step() {
            Step::Trump | Step::Over => None,
            Step::First => {
                let Some(who) = sights.first_player(&pos) else { return false };
                self.set_first(who);
                return true;
            }
            Step::Draw(_) => {
                // All'ultima pescata la briscola era andata all'avversario (avevo vinto la presa),
                // ma ce l'ho in mano: la presa l'aveva vinta lui. Si corregge e si va avanti.
                let trump = pos.trump_card.unwrap_or(0);
                let mine = sights.in_hand(trump) && sights.recent(&sights.hand[trump as usize]);
                if pos.deck_len == 0 && pos.trump_card_owner == Some(OPP) && mine && !pos.played.contains(trump) {
                    self.pos.hand.insert(trump);
                    self.pos.to_draw = 0;
                    self.pos.trump_card_owner = Some(ME);
                    if let Some(t) = self.tricks.last_mut() {
                        if t.winner == ME {
                            t.winner = OPP;
                            self.pos.points[ME as usize] = self.pos.points[ME as usize].saturating_sub(t.points);
                            self.pos.points[OPP as usize] += t.points;
                            self.pos.leader = OPP;
                        }
                    }
                    self.vision.status = "Ultima pescata corretta: la briscola l'ho presa io".into();
                    self.rec.write(&self.pos);
                    self.changed();
                    return true;
                }
                sights.next_draw(&pos)
            }
            Step::MyMove => {
                // Dopo una ripartenza a tavolo vuoto si era supposto che aprissi io: se in tavola
                // compare una carta che non è mia, apre l'avversario.
                if let Some(f) = self.vision.guessed_leader.filter(|_| pos.table.is_none()) {
                    let mut q = pos;
                    q.leader = OPP;
                    if sights.next_opp_play(&q, scene).is_some_and(|(c, _)| sights.opp_sight(c).first > f) {
                        self.pos.leader = OPP;
                        self.vision.guessed_leader = None;
                        return true;
                    }
                } else {
                    self.vision.guessed_leader = None;
                }
                sights
                    .next_my_play(&pos, scene)
                    .or_else(|| sights.missed_my_play(&pos))
            }
            Step::OppMove => sights.next_opp_play(&pos, scene),
        };
        let (card, sure) = if let Some(e) = event {
            e
        } else {
            // La carta dell'avversario non si è vista, ma la partita è andata avanti: in mano ho
            // una carta nuova (la presa è chiusa), o se apriva lui ho già risposto con una mia
            // carta. Si chiede all'utente di scriverla, invece di restare fermi.
            // Prima si propone la carta intravista meglio in tavola (basta confermarla).
            if pos.step() != Step::OppMove
                || !(sights.drew_after(&pos) || (pos.table.is_none() && sights.answered(&pos)))
            {
                return false;
            }
            match sights
                .glimpsed_opp(&pos)
                .filter(|&c| !self.vision.doubts.contains(&(Step::OppMove, c)))
            {
                Some(c) => (c, false),
                None => {
                    self.vision.status =
                        "Non ho visto la carta giocata dall'avversario: scrivila (per esempio 3c) e continuo da solo".into();
                    return false;
                }
            }
        };
        let step = pos.step();
        let view = self.vision.sightings.view(card);
        // Una carta che l'utente ha già detto sbagliata si ripropone solo se ora la si vede con
        // molta sicurezza: altrimenti non si insiste, si chiede di scrivere la carta giusta.
        if self.vision.doubts.contains(&(step, card)) && !self.vision.sightings.clearly_seen(step, card) {
            self.vision.status = match step {
                Step::Draw(_) => "Non riconosco la carta pescata: scrivila (per esempio 3c) e continuo da solo".into(),
                Step::OppMove => {
                    "Non ho visto la carta giocata dall'avversario: scrivila (per esempio 3c) e continuo da solo".into()
                }
                _ => "Non riconosco la carta giocata: scrivila (per esempio 3c) e continuo da solo".into(),
            };
            return false;
        }
        // Una carta letta senza sicurezza, o annullata dall'utente, non si mette da sola: si chiede.
        if !sure || self.vision.doubts.contains(&(step, card)) {
            if let Some((img, vis)) = view {
                self.vision.confirm = Some(Confirm { step, card, img, vis, texture: None });
            }
            return false;
        }
        let len = self.history.len();
        if !self.enter(card) {
            return false;
        }
        if let Some((img, vis)) = view {
            self.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).remember(&img, vis, card);
            self.vision.last = Some((card, img, len));
        }
        true
    }

    /// Confronta la mano sullo schermo con quella inserita. Se una carta riconosciuta con sicurezza
    /// non risulta in mano e al suo posto ne risulta una che sullo schermo non c'è, quella pescata
    /// era stata inserita male (a mano o dalla lettura): si corregge e la partita si riscrive.
    fn check_hand(&mut self, scene: &Scene) {
        let still = matches!(self.pos.step(), Step::MyMove | Step::OppMove);
        if !still || self.vision.confirm.is_some() || scene.hand.len() != self.pos.hand.len() as usize {
            return;
        }
        // Una presa in corso che sullo schermo non c'è più si è chiusa senza che si vedesse: la mano
        // è cambiata per una giocata e una pescata, non per una lettura sbagliata.
        if self.pos.table.is_some() && scene.opp_slot.is_none() && scene.my_slot.is_none() {
            return;
        }
        // Al mio turno una carta sparita dalla mano e una nuova al suo posto sono una giocata e la
        // pescata che la segue (il sito ha chiuso la presa in un attimo), non una pescata letta male.
        let pending = match self.pos.step() {
            Step::MyMove => self.vision.sightings.next_my_play(&self.pos, scene).is_some_and(|e| e.1),
            _ => self.vision.sightings.next_opp_play(&self.pos, scene).is_some_and(|e| e.1),
        };
        if pending {
            return;
        }
        let seen: Vec<(Card, f32, &CardView)> =
            scene.hand.iter().filter_map(|v| v.best(CardSet::FULL).map(|(c, m)| (c, m, v))).collect();
        let extra: Vec<&(Card, f32, &CardView)> =
            seen.iter().filter(|(c, m, _)| !self.pos.hand.contains(*c) && *m >= AUTO_MARGIN).collect();
        let missing: Vec<Card> = self.pos.hand.iter().filter(|h| !seen.iter().any(|(c, ..)| c == h)).collect();
        let ([(right, _, view)], [wrong]) = (extra.as_slice(), missing.as_slice()) else { return };
        // Una proposta rifiutata dall'utente non deve subito sovrascrivere la sua
        // correzione: durante il recupero non impariamo dall'immagine attuale.
        if self.vision.doubts.iter().any(|(step, card)| matches!(step, Step::Draw(_)) && *card == *right) {
            return;
        }
        // La carta che manca si è vista in tavola: l'ho giocata, non era una pescata letta male.
        if self.vision.sightings.glimpsed(*wrong) {
            return;
        }
        // Mentre aspetto l'avversario la mia mano non può cambiare: se cambia una carta pescata prima
        // dell'ultima giocata, sono mosse che non si sono viste (una presa chiusa in un attimo), non
        // una lettura sbagliata, e si corregge solo una pescata recente (o la briscola dell'ultima).
        let recent_draws: Vec<Card> = self
            .rec
            .events
            .iter()
            .rev()
            .take_while(|e| !matches!(e, Event::Giocata { .. }))
            .filter_map(|e| match e {
                Event::Pescata { carta } => Some(carta.id),
                _ => None,
            })
            .collect();
        let trump_draw = Some(*wrong) == self.pos.trump_card && self.pos.trump_card_owner == Some(ME);
        let waiting = self.pos.step() == Step::OppMove;
        if waiting && !recent_draws.contains(wrong) && !trump_draw {
            return;
        }
        // La carta vista dev'essere ancora in gioco (non già uscita, né la briscola).
        let out = self.pos.played.contains(*right) || self.pos.table == Some(*right) || self.pos.trump_card == Some(*right);
        if out {
            return;
        }
        let (wrong, right) = (*wrong, *right);
        // All'ultima pescata la briscola era andata a me (avevo perso la presa), ma in mano non c'è
        // e al suo posto c'è un'altra carta: la presa l'avevo vinta io, ho pescato la carta coperta
        // e la briscola l'ha presa l'avversario. Si corregge mano, presa, punti e chi apre.
        if Some(wrong) == self.pos.trump_card && self.pos.trump_card_owner == Some(ME) && !self.pos.played.contains(wrong) {
            self.pos.hand.remove(wrong);
            self.pos.hand.insert(right);
            self.pos.trump_card_owner = Some(OPP);
            if let Some(t) = self.tricks.last_mut() {
                if t.winner == OPP {
                    t.winner = ME;
                    self.pos.points[OPP as usize] = self.pos.points[OPP as usize].saturating_sub(t.points);
                    self.pos.points[ME as usize] += t.points;
                    if self.pos.table.is_none() {
                        self.pos.leader = ME;
                    }
                }
            }
            self.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).remember(&view.img, view.vis, right);
            self.vision.status = format!("Ultima pescata corretta: la briscola l'ha presa l'avversario, io {}", cards::name(right));
            self.rec.write(&self.pos);
            self.changed();
            return;
        }
        if self.correct_draw(wrong, right) {
            self.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).remember(&view.img, view.vis, right);
            self.vision.status = format!("Mano corretta: {} era {}", cards::name(wrong), cards::name(right));
        }
    }

    /// Vero se lo schermo mostra una partita nuova: briscola scoperta, tre carte in mano e tavolo
    /// vuoto, quando la partita in corso è finita, ha il mazzo esaurito (la briscola non si
    /// vedrebbe più) o aveva un'altra briscola. Così si riparte anche dopo una partita interrotta.
    fn new_deal(&mut self, scene: &Scene) -> bool {
        let Some(t) = &scene.trump else { return false };
        if self.pos.trump_card.is_none() {
            return false;
        }
        let sure = |v: &CardView| v.best(CardSet::FULL).filter(|b| b.1 >= AUTO_MARGIN).map(|b| b.0);
        // Tre carte diverse fra loro e dalla briscola (una lettura confusa della schermata finale
        // vede due volte la stessa carta, o la briscola fra le carte in mano).
        let best = |v: &CardView| v.best(CardSet::FULL).map(|b| b.0);
        let ids: Vec<Option<Card>> = scene.hand.iter().map(best).collect();
        let distinct = ids.iter().enumerate().all(|(i, a)| a.is_some() && !ids[..i].contains(a)) && !ids.contains(&best(t));
        let fresh = scene.hand.len() == 3 && distinct && scene.opp_slot.is_none() && scene.my_slot.is_none();
        // Una carta della mano nuova è già stata giocata in questa partita: è un'altra partita.
        let replayed = scene.hand.iter().filter_map(sure).any(|c| self.pos.played.contains(c));
        // Briscola diversa: dalla carta riconosciuta (regge anche se cambia lo sfondo), o, se non
        // è sicura, dall'immagine.
        let very_sure = |v: &CardView| v.best(CardSet::FULL).filter(|b| b.1 >= 2.0 * AUTO_MARGIN).map(|b| b.0);
        let other_trump = match very_sure(t) {
            Some(c) => Some(c) != self.pos.trump_card,
            None if sure(t).is_some() => false,
            None => match &self.vision.trump_img {
                Some(known) => similarity(known, &t.img, t.vis) < 0.75,
                None => false,
            },
        };
        if !other_trump {
            // Segue i piccoli cambiamenti (il mazzo che si assottiglia sopra la briscola).
            self.vision.trump_img = Some(t.img.clone());
        }
        // A mazzo finito la briscola di solito sparisce, e se la si rivede è una partita nuova; ma
        // alcuni siti la lasciano disegnata: lo è solo se in mano non c'è più nessuna mia carta.
        // In una distribuzione nuova in mano non c'è più nessuna delle mie carte: una carta letta
        // male (il sette per il sei) non basta a ricominciare.
        let new_hand = scene.hand.iter().filter_map(sure).all(|c| !self.pos.hand.contains(c));
        let deck_over = self.pos.deck_len == 0;
        fresh && (self.pos.step() == Step::Over || (new_hand && (deck_over || other_trump || replayed)))
    }

    /// Conta le letture in cui lo schermo contraddice la partita inserita (un'altra briscola, una
    /// carta già giocata di nuovo in mano): se dura qualche secondo lo dice, invece di continuare
    /// a seguire una partita che non è quella sullo schermo.
    fn check_mismatch(&mut self, scene: &Scene) {
        let sure = |v: &CardView| v.best(CardSet::FULL).filter(|b| b.1 >= AUTO_MARGIN).map(|b| b.0);
        let trump = scene.trump.as_ref().and_then(sure).filter(|&c| self.pos.trump_card.is_some_and(|t| t != c));
        // (La briscola già giocata può restare disegnata al suo posto: non conta. E a mazzo finito,
        // con la mano quasi vuota, le ultime carte giocate stanno nella riga più bassa e sembrano
        // in mano: non si controlla.)
        let late = self.pos.deck_len == 0 || self.pos.step() == Step::Over;
        let replayed = scene
            .hand
            .iter()
            .filter_map(sure)
            .find(|&c| !late && self.pos.played.contains(c) && Some(c) != self.pos.trump_card);
        self.vision.mismatch = match (trump, replayed) {
            (None, None) => 0,
            _ => self.vision.mismatch + 1,
        };
        // La briscola inserita era in realtà una carta della mia mano (la si è vista in mano a
        // partita in corso) e sullo schermo la briscola è stabilmente un'altra: era stata letta
        // male all'inizio. Si corregge e la partita si riscrive.
        if let (Some(c), Some(t)) = (trump, self.pos.trump_card) {
            let free = !self.pos.played.contains(c) && !self.pos.hand.contains(c) && self.pos.table != Some(c);
            if self.vision.mismatch >= 4 && self.pos.deck_len > 0 && free && self.vision.sightings.in_hand(t) {
                if self.correct_event(|e| matches!(e, Event::Briscola { carta } if carta.id == t), c) {
                    if let Some(v) = &scene.trump {
                        self.vision
                            .memory
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .remember(&v.img, v.vis, c);
                    }
                    self.vision.mismatch = 0;
                    self.vision.status = format!(
                        "Briscola corretta: {} era {}",
                        cards::name(t),
                        cards::name(c)
                    );
                    return;
                }
            }
        }
        // Circa tre secondi di letture stabili.
        if self.vision.mismatch >= 8 {
            self.vision.inconsistent = true;
            let why = match (trump, replayed) {
                (Some(c), _) => format!("sullo schermo la briscola è {}", cards::name(c)),
                (_, Some(c)) => format!("in mano vedo {}, già giocato", cards::name(c)),
                _ => String::new(),
            };
            self.vision.status = format!(
                "Lo schermo non corrisponde alla partita inserita ({why}): se è una partita nuova premi \"Nuova partita\""
            );
        }
    }

    /// La carta della mano sullo schermo che non è nessuna di quelle già in mano: la pescata.
    fn new_hand_view(&self) -> Option<&CardView> {
        let scene = self.vision.scene.as_ref()?;
        let rest = unmatched_hand(scene, &self.pos);
        if rest.len() == 1 {
            Some(rest[0])
        } else {
            None
        }
    }

    /// L'immagine della carta dell'avversario sullo schermo, se somiglia a `c` più che alle carte
    /// già giocate (altrimenti è ancora quella della presa precedente).
    fn opp_view_of(&self, c: Card) -> Option<(Img, [f32; 2])> {
        let v = self.vision.scene.as_ref()?.opp_slot.as_ref()?;
        let old = self.pos.played.iter().map(|p| v.scores[p as usize]).fold(f32::MIN, f32::max);
        (v.scores[c as usize] > old).then(|| (v.img.clone(), v.vis))
    }

    /// L'utente conferma la carta riconosciuta con poca sicurezza: si inserisce e si ricorda.
    /// L'utente dice che la carta proposta non è quella: non si propone più da sola, e quello
    /// che si era visto di lei per quel passo si dimentica (era una lettura sbagliata).
    fn reject(&mut self, step: Step, card: Card) {
        self.vision.doubts.push((step, card));
        self.vision.sightings.forget(step, card);
    }

    /// Chiude la partita quando le ultime prese non si sono viste: le carte rimaste risultano
    /// giocate, senza inventare quelle dell'avversario (i punti restano non verificati).
    fn finish_unseen(&mut self) {
        self.vision.gaps.push(self.history.len());
        self.history.push((self.pos, self.tricks.len(), self.rec.events.len(), false));
        let mut p = self.pos;
        for c in p.hand.iter() {
            p.played.insert(c);
        }
        if let Some(t) = p.table {
            p.played.insert(t);
        }
        p.hand = CardSet::EMPTY;
        p.table = None;
        p.to_draw = 0;
        p.opp_hand_len = 0;
        p.tricks = 20;
        self.pos = p;
        self.changed();
        self.vision.last = None;
        self.vision.status = "Partita finita: le ultime prese non si sono viste (punti non verificati)".into();
    }

    /// Accetta da sola la carta proposta (senza impararne l'immagine: non è confermata).
    fn auto_accept(&mut self) {
        let Some(c) = self.vision.confirm.take() else { return };
        self.vision.auto = true;
        if c.step == self.pos.step() {
            self.enter(c.card);
        }
        self.vision.auto = false;
    }

    fn accept_vision(&mut self) {
        let Some(c) = self.vision.confirm.take() else { return };
        if c.step == self.pos.step() && self.enter(c.card) {
            self.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).remember(&c.img, c.vis, c.card);
        }
    }

    /// Testo per la diagnosi: cosa si vede e cosa è stato riconosciuto.
    fn vision_summary(&self) -> String {
        let mut out = format!("{}\n", self.vision.status);
        let Some(s) = &self.vision.scene else { return out };
        let mut line = |what: &str, v: &CardView| {
            let mut sc: Vec<(Card, f32)> = (0..40u8).map(|c| (c, v.scores[c as usize])).collect();
            sc.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let top: Vec<String> = sc[..3].iter().map(|(c, x)| format!("{} {x:.1}", cards::name(*c))).collect();
            out += &format!("{what}: {}{}\n", top.join(" · "), if v.partial { " (vista a metà)" } else { "" });
        };
        for (what, v) in [("Briscola", &s.trump), ("Avversario", &s.opp_slot), ("Tua giocata", &s.my_slot)] {
            if let Some(v) = v {
                line(what, v);
            }
        }
        for v in &s.hand {
            line("Mano", v);
        }
        out
    }

    /// Interruttore, stato e richiesta di conferma della lettura dello schermo.
    fn vision_ui(&mut self, ui: &mut egui::Ui) {
        let save_error = self.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).last_error().map(str::to_owned);
        if let Some(error) = save_error {
            ui.colored_label(Color32::LIGHT_RED, format!("Memoria carte non salvata: {error}"));
        }
        let mut on = self.vision.reader.is_some();
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut on, RichText::new("Leggi la partita dallo schermo").strong())
                .on_hover_text("BrisMaster guarda il tavolo del sito e inserisce da solo le carte. Non tocca il sito: le carte le giochi sempre tu.")
                .changed()
            {
                self.set_vision(on, ui.ctx());
            }
            if on {
                let n = self.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).len();
                ui.label(RichText::new(format!("{n}/40 carte in memoria")).small().weak())
                    .on_hover_text("Le carte già riconosciute si ricordano da sole e la volta dopo si riconoscono al primo colpo");
            }
        });
        if !self.vision.gaps.is_empty() {
            ui.colored_label(GOLD, "Partita ripresa con prese non viste: i punti e le carte mancanti non sono verificati. Puoi annullare il recupero con Ctrl+Z.");
        }
        if !on { return; }
        if self.screen_is_behind() {
            let message = if self.vision.inconsistent {
                "La lettura non coincide con la partita. Il consiglio è sospeso mentre controllo lo storico; puoi correggere le carte o annullare una lettura."
            } else if self.vision.catching_up {
                "Sto recuperando le letture precedenti. Il consiglio riprende appena il tavolo è aggiornato."
            } else {
                "Non ho una lettura aggiornata del tavolo. Il consiglio è sospeso."
            };
            ui.colored_label(GOLD, message);
            if (self.vision.inconsistent || self.vision.reader.as_ref().is_some_and(|r| r.lag() >= std::time::Duration::from_secs(5)))
                && ui.button("Riavvia lettura mantenendo la partita").clicked()
            {
                self.restart_vision(ui.ctx());
            }
        }
        ui.label(RichText::new(&self.vision.status).small().weak());
        let last = self.vision.reader.as_ref().and_then(|r| r.last_frame());
        let summary = self.vision_summary();
        self.vision.picker.controls(ui, last, &summary);
        let ctx = ui.ctx().clone();
        if let Some(c) = &mut self.vision.confirm {
            let tex = c.texture.get_or_insert_with(|| {
                let rgba: Vec<u8> = c.img.px.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                let image = egui::ColorImage::from_rgba_unmultiplied([c.img.w, c.img.h], &rgba);
                ctx.load_texture("vision_confirm", image, Default::default())
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
                            ui.label(RichText::new(format!("Mi sembra {}", cards::name(c.card))).size(17.0).strong());
                            ui.label("Spazio = giusto · se sbaglio scrivi la carta (es. Rd) · Esc = ignora");
                        });
                    });
                });
        }
        ui.add_space(6.0);
    }

    /// Gioca la carta consigliata, o la prenota se il motore sta ancora calcolando.
    fn play_best(&mut self) {
        if self.screen_is_behind() {
            self.entry_error = Some("Sto recuperando le letture: il consiglio riprende quando il tavolo è aggiornato.".into());
            return;
        }
        match self.analysis.as_ref().and_then(|a| a.evals.first()) {
            Some(e) => {
                let c = e.card;
                self.enter(c);
            }
            None => self.play_best_queued = true,
        }
    }

    fn eval_of(&self, c: Card) -> Option<(usize, MoveEval)> {
        let a = self.analysis.as_ref()?;
        a.evals.iter().position(|e| e.card == c).map(|i| (i, a.evals[i]))
    }

    /// Area di sinistra: tavolo con briscola, mazzo, presa in corso e mano.
    pub fn table_ui(&mut self, ui: &mut egui::Ui) {
        let pos = self.pos;
        let step = pos.step();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            steps_bar(ui, step, pos.tricks);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                if ui.button("Nuova partita").clicked() {
                    self.reset();
                }
                let undo = ui
                    .add_enabled(!self.history.is_empty(), egui::Button::new("Annulla ultima azione"))
                    .on_hover_text("Ctrl+Z");
                if undo.clicked() {
                    self.undo();
                }
            });
        });
        ui.add_space(10.0);

        // Avversario.
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(format!("Avversario · {} carte in mano · {} punti", pos.opp_hand_len, pos.points[1]))
                    .size(15.0)
                    .color(Color32::from_white_alpha(200)),
            );
            ui.horizontal(|ui| {
                ui.set_min_height(SMALL_CARD.y);
                let n = pos.opp_hand_len as usize;
                center(ui, n as f32 * (SMALL_CARD.x + 6.0));
                for _ in 0..n {
                    cards_ui::card(ui, None, SMALL_CARD, None, false);
                }
            });
        });
        ui.add_space(12.0);

        // Briscola e mazzo, presa in corso, punteggio.
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Briscola").color(GOLD).strong());
                let (rect, _) = ui.allocate_exact_size(CARD_SIZE, Sense::hover());
                match pos.trump_card {
                    Some(t) => {
                        let look = cards_ui::Look { dim: pos.deck_len == 0, ..Default::default() };
                        cards_ui::paint_face(ui.painter(), rect, t, look);
                    }
                    None => cards_ui::paint_slot(ui.painter(), rect, "?"),
                }
                let deck = match (pos.deck_len, pos.trump_card_owner) {
                    (0, Some(ME)) => "Mazzo finito · briscola tua".to_string(),
                    (0, _) => "Mazzo finito · briscola all'avversario".to_string(),
                    (n, _) => format!("Mazzo: {n} carte"),
                };
                ui.label(RichText::new(deck).color(Color32::from_white_alpha(190)));
            });
            ui.add_space(30.0);
            let (opp_c, my_c) = if pos.leader == ME { (None, pos.table) } else { (pos.table, None) };
            for (label, c, waiting) in
                [("Avversario", opp_c, step == Step::OppMove), ("Tu", my_c, step == Step::MyMove)]
            {
                ui.vertical(|ui| {
                    ui.label(RichText::new(label).color(Color32::from_white_alpha(200)));
                    let (rect, _) = ui.allocate_exact_size(CARD_SIZE, Sense::hover());
                    match c {
                        Some(c) => cards_ui::paint_face(ui.painter(), rect, c, Default::default()),
                        None => {
                            cards_ui::paint_slot(ui.painter(), rect, if waiting { "tocca a te" } else { "" });
                            if waiting {
                                let t = ui.input(|i| i.time) as f32;
                                let a = 0.5 + 0.5 * (t * 3.0).sin();
                                ui.painter().rect_stroke(
                                    rect,
                                    10.0,
                                    Stroke::new(2.0, GOLD.gamma_multiply(0.4 + 0.6 * a)),
                                    StrokeKind::Inside,
                                );
                                ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
                            }
                        }
                    }
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.add_space(20.0);
                score_box(ui, pos.points, pos.tricks, "Avversario");
            });
        });
        ui.add_space(6.0);
        if let Some(t) = self.tricks.last() {
            let (mine, theirs) = if t.leader == ME { (t.lead, t.follow) } else { (t.follow, t.lead) };
            let who = if t.winner == ME { "hai preso tu" } else { "ha preso l'avversario" };
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(format!(
                        "Ultima presa: tu {} · avversario {}: {who} ({} punti)",
                        cards::name(mine),
                        cards::name(theirs),
                        t.points
                    ))
                    .color(Color32::from_white_alpha(200)),
                );
            });
        }
        ui.add_space(10.0);

        // Mano, con le valutazioni del motore sotto ogni carta.
        let mut clicked = None;
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("La tua mano").size(16.0).strong().color(Color32::WHITE));
            ui.horizontal(|ui| {
                ui.set_min_height(CARD_SIZE.y + 40.0);
                let hand = pos.hand.to_vec();
                center(ui, hand.len() as f32 * (CARD_SIZE.x + 14.0));
                for (n, c) in hand.into_iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(CARD_SIZE.x + 6.0);
                        let ev = self.eval_of(c);
                        let best = matches!(ev, Some((0, _)));
                        let my_turn = step == Step::MyMove;
                        let glow = (my_turn && best).then_some(BEST);
                        let resp = cards_ui::card(ui, Some(c), CARD_SIZE, glow, my_turn);
                        if resp.clicked() {
                            clicked = Some(c);
                        }
                        if let Some((i, e)) = ev {
                            badge(ui, i == 0, e);
                        }
                        if my_turn {
                            ui.label(RichText::new(format!("tasto {}", n + 1)).color(GOLD));
                        }
                    });
                    ui.add_space(8.0);
                }
            });
            if step == Step::MyMove {
                ui.label(
                    RichText::new("Clicca la carta che giochi, o premi il suo tasto (Spazio = quella consigliata)")
                        .color(Color32::from_white_alpha(170)),
                );
            }
        });
        if let Some(c) = clicked {
            self.enter(c);
        }
    }

    /// Pannello di destra: richiesta di input oppure consiglio del motore. Nella finestra compatta
    /// sostituisce il tavolo: in cima c'è un riepilogo con la tua mano, niente elenco delle uscite.
    pub fn side_ui(&mut self, ui: &mut egui::Ui, samples: &mut u32, compact: bool) {
        let ctx = ui.ctx().clone();
        self.poll_vision(&ctx);
        if self.vision.restart_after.is_some_and(|t| t <= std::time::Instant::now()) {
            self.restart_vision(&ctx);
        }
        self.keyboard(&ctx);
        self.poll(*samples, &ctx);
        let step = self.pos.step();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if compact {
                self.compact_header(ui);
                ui.separator();
            }
            self.vision_ui(ui);
            if let Some(message) = self.analysis_error.clone() {
                ui.colored_label(GOLD, message);
                if self.pending.is_none() && ui.button("Riprova il consiglio").clicked() {
                    self.analysis_error = None;
                    self.analysis_failures = 0;
                    self.analysis_retry_at = None;
                    ctx.request_repaint();
                }
            }
            match step {
                Step::Trump => {
                    title(ui, "Qual è la briscola?", "Indica la carta scoperta sotto il mazzo.");
                    self.picker_and_entry(ui);
                }
                Step::First => {
                    title(ui, "Chi gioca per primo?", "Chi apre la prima presa della partita (tasti 1 e 2).");
                    ui.horizontal(|ui| {
                        let w = if compact { 150.0 } else { 200.0 };
                        let big = |t: &str| egui::Button::new(RichText::new(t).size(17.0)).min_size(vec2(w, 44.0));
                        if ui.add(big("1 · Comincio io")).clicked() {
                            self.set_first(ME);
                        }
                        if ui.add(big("2 · L'avversario")).clicked() {
                            self.set_first(OPP);
                        }
                    });
                }
                Step::Draw(n) => {
                    if self.pos.tricks == 0 {
                        title(ui, "Le tue tre carte", &format!("Indica le carte che hai in mano: ne mancano {n}."));
                    } else {
                        title(ui, "Che carta hai pescato?", "Il mazzo l'ha data a te dopo l'ultima presa.");
                    }
                    self.picker_and_entry(ui);
                }
                Step::OppMove => {
                    let sub =
                        if self.pos.table.is_some() { "Ha risposto alla tua carta." } else { "Apre lui questa presa." };
                    title(ui, "Che carta ha giocato l'avversario?", sub);
                    self.picker_and_entry(ui);
                }
                Step::MyMove => {
                    self.advice(ui, samples);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(if self.play_best_queued {
                            "Spazio premuto: inserisco il consiglio appena il motore ha finito."
                        } else {
                            "Spazio (o Invio) = hai giocato il consiglio · 1 2 3 = la carta in quella posizione della mano · oppure clicca la carta"
                        })
                        .color(GOLD),
                    );
                }
                Step::Over => {
                    let mine = self.pos.points[ME as usize];
                    let (t, col) = if self.vision.inconsistent || !self.vision.gaps.is_empty() {
                        ("Partita terminata: punti da verificare".to_string(), GOLD)
                    } else {
                        match mine {
                            p if p > 60 => (format!("Hai vinto {mine} a {}!", 120 - mine), BEST),
                            60 => ("Pareggio 60 a 60".to_string(), Color32::YELLOW),
                            _ => (format!("Hai perso {mine} a {}", 120 - mine), Color32::LIGHT_RED),
                        }
                    };
                    ui.add_space(10.0);
                    ui.label(RichText::new(t).size(28.0).strong().color(col));
                    ui.add_space(8.0);
                    if ui.button(RichText::new("Nuova partita (Invio)").size(17.0)).clicked() {
                        self.reset();
                    }
                }
            }
            if compact {
                return;
            }
            ui.add_space(14.0);
            ui.separator();
            ui.heading("Carte uscite");
            let mut gone = self.pos.played;
            if let Some(t) = self.pos.table {
                gone.insert(t);
            }
            tracker(ui, gone, self.pos.hand);
            ui.add_space(14.0);
            ui.separator();
            self.recording_ui(ui);
        });
    }

    /// Riepilogo della finestra compatta: briscola, mazzo, punti, presa in corso e la tua mano
    /// (numerata per i tasti 1 2 3 e cliccabile quando tocca a te).
    fn compact_header(&mut self, ui: &mut egui::Ui) {
        let pos = self.pos;
        let step = pos.step();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Presa {}/20", (pos.tricks + 1).min(20))).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Nuova partita").clicked() {
                    self.reset();
                }
                if ui
                    .add_enabled(!self.history.is_empty(), egui::Button::new("Annulla").small())
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    self.undo();
                }
            });
        });
        let mini = vec2(44.0, 68.0);
        let mut clicked = None;
        ui.horizontal(|ui| {
            for (label, c) in [("Briscola", pos.trump_card), ("In tavola", pos.table)] {
                ui.vertical(|ui| {
                    ui.label(RichText::new(label).small().color(GOLD));
                    let (rect, _) = ui.allocate_exact_size(mini, Sense::hover());
                    match c {
                        Some(c) => cards_ui::paint_face(ui.painter(), rect, c, Default::default()),
                        None => cards_ui::paint_slot(ui.painter(), rect, ""),
                    }
                });
            }
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("La tua mano").small());
                ui.horizontal(|ui| {
                    let my_turn = step == Step::MyMove;
                    for (i, c) in pos.hand.to_vec().into_iter().enumerate() {
                        ui.vertical(|ui| {
                            let best = matches!(self.eval_of(c), Some((0, _)));
                            let glow = (my_turn && best).then_some(BEST);
                            if cards_ui::card(ui, Some(c), mini, glow, my_turn).clicked() {
                                clicked = Some(c);
                            }
                            if my_turn {
                                ui.label(RichText::new(format!("{}", i + 1)).strong().color(GOLD));
                            }
                        });
                    }
                });
            });
        });
        ui.label(
            RichText::new(format!(
                "Tu {} · avversario {} punti · mazzo {} carte",
                pos.points[ME as usize], pos.points[OPP as usize], pos.deck_len
            ))
            .weak(),
        );
        if let Some(c) = clicked {
            self.enter(c);
        }
    }

    /// Registrazione di questa partita (la accende l'utente) e file da inviare.
    fn recording_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut on = self.rec.enabled;
            let text = if on {
                RichText::new("● Registra questa partita").color(Color32::LIGHT_RED).strong()
            } else {
                RichText::new("○ Registra questa partita").weak()
            };
            if ui
                .checkbox(&mut on, text)
                .on_hover_text("Accesa salva questa partita, anche le carte già inserite; spenta non salva niente. Ogni nuova partita parte spenta.")
                .changed()
            {
                self.rec.set_enabled(on, &self.pos);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Apri cartella partite").clicked() {
                    record::open_dir();
                }
                if ui
                    .button("Prepara file da inviare")
                    .on_hover_text("Raccoglie tutte le partite registrate in un solo file, da allegare in un messaggio per allenare il motore sulle partite vere")
                    .clicked()
                {
                    self.rec.export_msg = Some(match record::export(&self.rec.dir) {
                        Ok((path, 0)) => format!("Nessuna partita registrata da inviare (creato comunque {})", path.display()),
                        Ok((path, n)) => {
                            record::show_file(&path);
                            format!("Pronto: {} con {n} partite. Allegalo in un messaggio.", path.display())
                        }
                        Err(e) => format!("Impossibile preparare il file: {e}"),
                    });
                }
            });
        });
        let note = if self.rec.enabled {
            format!("Questa partita viene salvata, dalla prima carta giocata, in {}", self.rec.path().display())
        } else {
            "Questa partita non viene salvata.".to_string()
        };
        ui.label(RichText::new(note).small().weak());
        if let Some(m) = &self.rec.export_msg {
            ui.label(RichText::new(m).small().color(GOLD));
        }
        if let Some(e) = &self.rec.error {
            ui.label(RichText::new(e).color(Color32::LIGHT_RED));
        }
    }

    fn picker_and_entry(&mut self, ui: &mut egui::Ui) {
        if let Some(c) = cards_ui::picker(ui, "assist_picker", self.pos.allowed()) {
            self.enter(c);
        }
        ui.add_space(8.0);
        self.keys_hint(ui);
    }

    /// Cosa hai digitato finora e promemoria dei tasti.
    fn keys_hint(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Da tastiera:").weak());
            let shown = if self.keys.is_empty() { "_".to_string() } else { format!("{}_", self.keys.to_uppercase()) };
            ui.label(RichText::new(shown).monospace().size(18.0).color(GOLD));
        });
        if let Some(e) = &self.entry_error {
            ui.label(RichText::new(e).color(Color32::LIGHT_RED));
        }
        ui.label(
            RichText::new("Scrivi valore e seme, senza Invio: A 2 3 4 5 6 7 F C R · d c s b (es. 3c). Ctrl+Z annulla")
                .small()
                .weak(),
        );
    }

    fn advice(&mut self, ui: &mut egui::Ui, samples: &mut u32) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Consiglio").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("precision")
                    .selected_text(precision_label(*samples))
                    .show_ui(ui, |ui| {
                        for s in [1000, 3000, crate::app::ISMCTS_FROM] {
                            ui.selectable_value(samples, s, precision_label(s));
                        }
                    })
                    .response
                    .on_hover_text("Quanto calcola il motore: Veloce e Normale simulano distribuzioni delle carte coperte, Massima usa la ricerca ad albero (la più forte, circa un decimo di secondo)");
            });
        });
        if self.screen_is_behind() {
            ui.label("Consiglio sospeso mentre recupero il tavolo attuale.");
            return;
        }
        if self.pos.hand.len() == 1 {
            let c = self.pos.hand.iter().next().unwrap();
            ui.label(RichText::new(format!("Hai solo {}: giocala.", cards::name(c))).size(18.0));
            return;
        }
        let Some(a) = &self.analysis else {
            if self.pending.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Il motore sta simulando le possibili mani dell'avversario...");
                });
            } else if self.analysis_retry_at.is_some() && self.analysis_failures < 2 {
                ui.label("Sto riavviando il calcolo del consiglio...");
            } else {
                ui.label("Consiglio non disponibile. Puoi inserire la carta giocata o riprovare il calcolo.");
            }
            return;
        };
        let best = a.evals[0];
        egui::Frame::new()
            .fill(Color32::from_rgb(24, 52, 36))
            .stroke(Stroke::new(1.5, BEST))
            .corner_radius(10.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    cards_ui::card(ui, Some(best.card), vec2(62.0, 96.0), None, false);
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Gioca").color(Color32::from_white_alpha(180)));
                        ui.label(RichText::new(cards::name(best.card)).size(24.0).strong().color(Color32::WHITE));
                        ui.label(
                            RichText::new(format!(
                                "Vittoria {:.0}% · punti attesi {:.0}",
                                best.win_prob * 100.0,
                                best.exp_points
                            ))
                            .size(16.0)
                            .color(BEST),
                        );
                    });
                });
            });
        ui.add_space(8.0);
        for r in &a.reasons {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("•").color(GOLD));
                ui.label(r);
            });
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Tutte le carte").strong());
        for (i, e) in a.evals.iter().enumerate() {
            ui.horizontal(|ui| {
                let name = RichText::new(format!("{:<20}", cards::name(e.card))).monospace();
                ui.label(if i == 0 { name.color(BEST) } else { name });
                let (rect, _) = ui.allocate_exact_size(vec2(130.0, 14.0), Sense::hover());
                ui.painter().rect_filled(rect, 4.0, Color32::from_gray(50));
                let mut fill = rect;
                fill.set_width(rect.width() * e.win_prob as f32);
                ui.painter().rect_filled(fill, 4.0, if i == 0 { BEST } else { Color32::from_gray(150) });
                ui.label(format!("{:>3.0}%", e.win_prob * 100.0));
                ui.label(RichText::new(format!("{:.0} pt", e.exp_points)).weak());
            });
        }
        ui.label(
            RichText::new(if a.samples >= crate::app::ISMCTS_FROM {
                format!(
                    "Stime della ricerca ad albero ({} partite simulate sulle carte che non vedi), usando solo ciò che sai.",
                    a.samples
                )
            } else {
                format!("Stime su {} distribuzioni delle carte che non vedi, usando solo ciò che sai.", a.samples)
            })
            .small()
            .weak(),
        );
    }
}

/// Carte della mano sullo schermo tolte quelle che corrispondono alle carte già in mano
/// (a ciascuna carta nota la vista che le somiglia di più).
fn unmatched_hand<'a>(scene: &'a Scene, pos: &Position) -> Vec<&'a CardView> {
    unmatched(scene.hand.iter().collect(), pos)
}

/// Le carte viste (`views`) che restano togliendo, per ogni carta in mano, quella che le somiglia
/// di più.
fn unmatched<'a>(mut views: Vec<&'a CardView>, pos: &Position) -> Vec<&'a CardView> {
    for k in pos.hand.iter() {
        let best = (0..views.len())
            .max_by(|&a, &b| views[a].scores[k as usize].partial_cmp(&views[b].scores[k as usize]).unwrap());
        if let Some(i) = best {
            views.remove(i);
        }
    }
    views
}

/// Vero se i tasti digitati possono ancora diventare una carta (manca il seme).
pub fn is_rank_prefix(keys: &str) -> bool {
    matches!(keys, "a" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "f" | "j" | "c" | "q" | "r" | "k" | "10")
}

fn precision_label(s: u32) -> String {
    match s {
        0..=1000 => format!("Veloce ({s} mondi)"),
        s if s < crate::app::ISMCTS_FROM => format!("Normale ({s} mondi)"),
        _ => format!("Massima (ISMCTS {s})"),
    }
}

fn title(ui: &mut egui::Ui, t: &str, sub: &str) {
    ui.label(RichText::new(t).size(22.0).strong().color(Color32::WHITE));
    ui.label(RichText::new(sub).color(Color32::from_white_alpha(170)));
    ui.add_space(8.0);
}

fn center(ui: &mut egui::Ui, w: f32) {
    ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
}

/// Barra dei passi: briscola, chi inizia, mano iniziale, partita.
fn steps_bar(ui: &mut egui::Ui, step: Step, tricks: u8) {
    let current = match step {
        Step::Trump => 0,
        Step::First => 1,
        Step::Draw(_) if tricks == 0 => 2,
        _ => 3,
    };
    for (i, name) in ["Briscola", "Chi inizia", "Mano iniziale", "Partita"].iter().enumerate() {
        let done = i < current;
        let active = i == current;
        let (fill, text) = if active {
            (GOLD, Color32::from_rgb(40, 30, 10))
        } else if done {
            (Color32::from_rgb(40, 110, 70), Color32::WHITE)
        } else {
            (Color32::from_black_alpha(60), Color32::from_white_alpha(150))
        };
        let label = format!("{} {}", i + 1, name);
        egui::Frame::new().fill(fill).corner_radius(12.0).inner_margin(vec2(10.0, 4.0)).show(ui, |ui| {
            ui.label(RichText::new(label).color(text).strong());
        });
    }
}

pub fn score_box(ui: &mut egui::Ui, points: [u8; 2], tricks: u8, opp: &str) {
    egui::Frame::new().fill(Color32::from_black_alpha(70)).corner_radius(10.0).inner_margin(12.0).show(ui, |ui| {
        ui.set_width(160.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("Tu  {}", points[0])).size(24.0).strong().color(Color32::WHITE));
            ui.label(RichText::new(format!("{opp}  {}", points[1])).size(18.0).color(Color32::from_white_alpha(200)));
            ui.label(
                RichText::new(format!("Presa {} di 20", (tricks + 1).min(20))).color(Color32::from_white_alpha(150)),
            );
            let (rect, _) = ui.allocate_exact_size(vec2(150.0, 8.0), Sense::hover());
            ui.painter().rect_filled(rect, 4.0, Color32::from_gray(60));
            let mut mine = rect;
            mine.set_width(rect.width() * points[0] as f32 / 120.0);
            ui.painter().rect_filled(mine, 4.0, BEST);
            let x = rect.left() + rect.width() * 61.0 / 120.0;
            ui.painter().vline(x, rect.expand(3.0).y_range(), Stroke::new(1.5, GOLD));
        });
    });
}

/// Etichetta sotto una carta della mano con la probabilità di vittoria.
pub fn badge(ui: &mut egui::Ui, best: bool, e: MoveEval) {
    let text = format!("{:.0}%", e.win_prob * 100.0);
    let (fill, col) =
        if best { (BEST, Color32::from_rgb(10, 40, 20)) } else { (Color32::from_black_alpha(90), Color32::WHITE) };
    let w = CARD_SIZE.x;
    let (rect, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
    let pill = Rect::from_center_size(rect.center(), vec2(64.0, 20.0));
    ui.painter().rect_filled(pill, 10.0, fill);
    ui.painter().text(pill.center(), Align2::CENTER_CENTER, text, FontId::proportional(14.0), col);
    ui.label(RichText::new(format!("{:.0} punti attesi", e.exp_points)).small().color(Color32::from_white_alpha(170)));
}

/// Griglia delle carte uscite: barrate se uscite, evidenziate se in mano.
pub fn tracker(ui: &mut egui::Ui, gone: CardSet, mine: CardSet) {
    egui::Grid::new("tracker").min_col_width(20.0).spacing([6.0, 4.0]).show(ui, |ui| {
        for s in 0..4u8 {
            let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
            cards_ui::paint_suit(ui.painter(), r.center(), 18.0, s);
            for k in cards_ui::RANK_ORDER {
                let c = cards::make(s, k);
                let txt = RichText::new(cards::RANK_SHORT[k as usize]).monospace().size(15.0);
                let txt = if gone.contains(c) {
                    txt.strikethrough().color(Color32::from_gray(90))
                } else if mine.contains(c) {
                    txt.color(Color32::BLACK).background_color(Color32::LIGHT_BLUE)
                } else {
                    txt.color(cards_ui::suit_color_light(s)).strong()
                };
                ui.label(txt);
            }
            ui.end_row();
        }
    });
    let remaining: u32 = CardSet::FULL.minus(gone).iter().map(|c| cards::points(c) as u32).sum();
    ui.label(format!("Punti ancora in gioco: {remaining}"));
    ui.label(RichText::new("Barrate = uscite, evidenziate = in mano a te").weak().small());
}

/// Quante volte e quando una carta è stata vista in un posto (in mano o in tavola), e quanto
/// stacca la seconda carta più probabile nella lettura migliore.
#[derive(Clone, Copy, Default)]
struct Sighting {
    count: u32,
    first: u64,
    last: u64,
    margin: f32,
}

impl Sighting {
    fn bump(&mut self, frame: u64, margin: f32) {
        if self.count == 0 {
            self.first = frame;
        }
        self.count += 1;
        self.last = frame;
        self.margin = self.margin.max(margin);
    }

    /// Vista abbastanza da crederci: due volte con sicurezza, o molte volte sempre uguale.
    fn sure(&self) -> bool {
        (self.count >= 2 && self.margin >= AUTO_MARGIN) || (self.count >= 5 && self.margin >= 0.8)
    }
}

/// Le carte viste durante la partita. La partita si ricostruisce da qui con le regole del gioco:
/// una carta che era in mano mia e va in tavola (o sparisce dalla mano) l'ho giocata io; una carta
/// mai stata in mano mia che compare in tavola l'ha giocata l'avversario; una carta nuova in mano
/// è una pescata. Così non conta né quanto in fretta il sito chiude le prese, né dove disegna le
/// carte, né se due letture di fila sono uguali.
#[derive(Default, Clone)]
struct Sightings {
    frame: u64,
    hand: Vec<Sighting>,
    table: Vec<Sighting>,
    /// Solo carte dalla parte dell'avversario: una mia carta letta male non
    /// deve diventare una giocata avversaria nello storico.
    opponents: Vec<Sighting>,
    /// Carte viste da sole dalla mia parte del tavolo: su molti siti le carte giocate stanno
    /// affiancate e la parte non dice di chi sono; contano per l'avversario se restano visibili.
    lone_mine: Vec<Sighting>,
    /// Carte viste dalla mia parte del tavolo, e dalla parte dell'avversario, comunque sia.
    my_side: Vec<Sighting>,
    their_side: Vec<Sighting>,
    /// Posizioni precedenti della mano: una carta salita verso il tavolo non
    /// resta in mano soltanto perché lo zoom l'ha avvicinata alle altre.
    hand_places: Vec<Option<[f32; 4]>>,
    /// L'ultima immagine (e fascia visibile) di ogni carta, per ricordarla.
    views: Vec<Option<(Img, [f32; 2])>>,
    /// Le letture della mano tutte riconosciute con sicurezza: (lettura, carte).
    full_hands: Vec<(u64, Vec<Card>)>,
    /// Quante carte avevo in mano quando è comparsa la prima carta in tavola.
    hand_at_first_play: Option<usize>,
}

/// Distacco minimo perché una lettura conti come avvistamento.
const SIGHT_MARGIN: f32 = 0.8;

impl Sightings {
    fn update(&mut self, scene: &Scene) {
        if self.hand.is_empty() {
            self.hand = vec![Sighting::default(); 40];
            self.table = vec![Sighting::default(); 40];
            self.opponents = vec![Sighting::default(); 40];
            self.lone_mine = vec![Sighting::default(); 40];
            self.my_side = vec![Sighting::default(); 40];
            self.their_side = vec![Sighting::default(); 40];
            self.hand_places = vec![None; 40];
            self.views = vec![None; 40];
        }
        self.frame += 1;
        let f = self.frame;
        let mut full = Vec::new();
        let mut all_sure = !scene.hand.is_empty();
        for v in &scene.hand {
            match v.best(CardSet::FULL) {
                Some((c, m)) if m >= SIGHT_MARGIN => {
                    self.hand[c as usize].bump(f, m);
                    self.hand_places[c as usize] = Some(v.at);
                    self.views[c as usize] = Some((v.img.clone(), v.vis));
                    all_sure &= m >= AUTO_MARGIN;
                    full.push(c);
                }
                // Letta senza sicurezza (coperta proprio dove due carte differiscono): conta come
                // avvistamento incerto, e se si ripete si chiede conferma.
                Some((c, m)) => {
                    self.hand[c as usize].bump(f, m);
                    if self.views[c as usize].is_none() {
                        self.views[c as usize] = Some((v.img.clone(), v.vis));
                    }
                    all_sure = false;
                }
                None => all_sure = false,
            }
        }
        if all_sure {
            self.full_hands.push((f, full));
            if self.full_hands.len() > 4 {
                self.full_hands.remove(0);
            }
        }
        let table: Vec<&CardView> = [&scene.opp_slot, &scene.my_slot].into_iter().flatten().collect();
        if !table.is_empty() && self.hand_at_first_play.is_none() {
            self.hand_at_first_play = Some(scene.hand.len());
        }
        // (In tavola si tengono anche le letture incerte: se si ripetono, si propone la carta da
        // confermare invece di chiederla da scrivere.)
        for v in table {
            if let Some((c, m)) = v.best(CardSet::FULL).filter(|b| b.1 >= 0.3) {
                self.table[c as usize].bump(f, m);
                self.views[c as usize] = Some((v.img.clone(), v.vis));
            }
        }
        if let Some((c, m)) = scene.opp_slot.as_ref().and_then(|v| v.best(CardSet::FULL)).filter(|b| b.1 >= 0.3) {
            self.opponents[c as usize].bump(f, m);
            self.their_side[c as usize].bump(f, m);
        }
        if let Some((c, m)) = scene.my_slot.as_ref().and_then(|v| v.best(CardSet::FULL)).filter(|b| b.1 >= 0.3) {
            self.my_side[c as usize].bump(f, m);
        }
        // Con due carte in tavola una è dell'avversario, da qualunque parte stia (la mia si
        // riconosce perché era in mano). Una carta sola dalla mia parte si tiene a parte.
        if let Some((c, m)) = scene.my_slot.as_ref().and_then(|v| v.best(CardSet::FULL)).filter(|b| b.1 >= 0.3) {
            if scene.opp_slot.is_some() {
                self.opponents[c as usize].bump(f, m);
            } else {
                self.lone_mine[c as usize].bump(f, m);
            }
        }
    }

    /// Gli avvistamenti di `c` che valgono come carta giocata dall'avversario: dalla sua parte, o
    /// in tavola insieme a un'altra, o da sola dalla mia parte se ci è rimasta (tre letture).
    fn opp_sight(&self, c: Card) -> Sighting {
        let (o, l) = (self.opponents[c as usize], self.lone_mine[c as usize]);
        if l.count < 3 {
            return o;
        }
        if o.count == 0 {
            return l;
        }
        Sighting { count: o.count + l.count, first: o.first.min(l.first), last: o.last.max(l.last), margin: o.margin.max(l.margin) }
    }

    /// In mano c'è con sicurezza una carta che non ho ancora: ho già pescato dopo la presa in corso.
    fn drew_after(&self, pos: &Position) -> bool {
        let allowed = pos.allowed().union(CardSet::FULL.minus(pos.played).minus(pos.hand));
        (0..40u8).any(|c| {
            allowed.contains(c) && !pos.hand.contains(c) && Some(c) != pos.table && self.in_hand(c) && !self.on_table(c)
        })
    }

    /// Ho aperto io e in mano c'è già la carta pescata dopo questa presa: da quale lettura.
    fn draw_seen(&self, pos: &Position) -> Option<u64> {
        pos.table?;
        let allowed = pos.allowed().union(CardSet::FULL.minus(pos.played).minus(pos.hand));
        (0..40u8)
            .filter(|&c| {
                allowed.contains(c) && !pos.hand.contains(c) && Some(c) != pos.table && self.in_hand(c)
                    && !self.on_table(c) && self.recent(&self.hand[c as usize])
            })
            .map(|c| self.hand[c as usize].first)
            .min()
    }

    /// Una mia carta in mano è andata in tavola (senza tornare in mano) e la presa è già chiusa:
    /// in tavola non si vede più.
    fn answered(&self, pos: &Position) -> bool {
        pos.hand.iter().any(|c| {
            let (t, h) = (&self.table[c as usize], &self.hand[c as usize]);
            self.on_table(c) && t.last >= h.last && t.last + 2 <= self.frame
        })
    }

    /// Vista nelle ultime letture.
    fn recent(&self, s: &Sighting) -> bool {
        self.frame.saturating_sub(s.last) <= 6
    }

    /// Vista di nuovo con molta sicurezza per il passo `step` (dopo che l'utente l'aveva scartata).
    fn clearly_seen(&self, step: Step, c: Card) -> bool {
        let s = match step {
            Step::Draw(_) => self.hand.get(c as usize),
            Step::OppMove | Step::MyMove => self.table.get(c as usize),
            _ => None,
        };
        s.is_some_and(|s| s.count >= 2 && s.margin >= 2.0 * AUTO_MARGIN)
    }

    /// Dimentica gli avvistamenti di `c` che hanno portato a proporla per il passo `step`.
    fn forget(&mut self, step: Step, c: Card) {
        let list = match step {
            Step::Draw(_) => &mut self.hand,
            Step::OppMove | Step::MyMove => &mut self.table,
            _ => return,
        };
        if let Some(s) = list.get_mut(c as usize) {
            *s = Sighting::default();
        }
    }

    fn view(&self, c: Card) -> Option<(Img, [f32; 2])> {
        self.views.get(c as usize).cloned().flatten()
    }

    fn in_hand(&self, c: Card) -> bool {
        self.hand.get(c as usize).is_some_and(|s| s.sure())
    }

    /// Vista in tavola: due volte con sicurezza, o una volta sola se poi è sparita (il sito ha
    /// chiuso la presa prima della lettura dopo).
    fn on_table(&self, c: Card) -> bool {
        self.table.get(c as usize).is_some_and(|s| s.sure() || (s.count >= 1 && s.margin >= AUTO_MARGIN && s.last < self.frame))
    }

    /// Vista in tavola almeno una volta, anche senza sicurezza.
    /// La carta dell'avversario intravista meglio in tavola nelle ultime letture, anche letta
    /// senza sicurezza (fra quelle che può avere giocato).
    fn glimpsed_opp(&self, pos: &Position) -> Option<Card> {
        let allowed = pos.allowed();
        let cut = self.draw_seen(pos);
        (0..40u8)
            .filter(|&c| allowed.contains(c) && !self.in_hand(c) && !pos.hand.contains(c))
            .filter(|&c| cut.is_none_or(|f| self.opp_sight(c).first < f))
            .filter(|&c| {
                self.opp_sight(c).count >= 1
                    && self.frame.saturating_sub(self.opp_sight(c).last) <= 20
            })
            .max_by_key(|&c| {
                (
                    self.opp_sight(c).count.min(4),
                    (self.opp_sight(c).margin * 10.0) as i32,
                )
            })
    }

    fn glimpsed(&self, c: Card) -> bool {
        self.table.get(c as usize).is_some_and(|s| s.count >= 1)
    }

    /// Chi ha aperto la partita: la prima carta comparsa in tavola (anche letta male) era mia se
    /// l'avevo vista in mano, o se in quel momento in mano me ne restavano due.
    fn first_player(&self, pos: &Position) -> Option<u8> {
        if !(0..40u8).any(|c| Some(c) != pos.trump_card && self.on_table(c)) {
            return None;
        }
        let lead = self.first_table_card(pos)?;
        Some(if self.in_hand(lead) || self.hand_at_first_play == Some(2) { ME } else { OPP })
    }

    /// La prossima pescata: la carta vista in mano da più tempo fra quelle non ancora inserite.
    /// All'inizio, se ho aperto io senza che la mia carta si vedesse prima in mano, è quella.
    fn next_draw(&self, pos: &Position) -> Option<(Card, bool)> {
        let allowed = pos.allowed();
        let opening = pos.tricks == 0 && pos.first == Some(ME) && pos.table.is_none();
        let lead = if opening { self.first_table_card(pos) } else { None };
        let sure = (0..40u8)
            .filter(|&c| allowed.contains(c) && self.in_hand(c))
            .min_by_key(|&c| self.hand[c as usize].first);
        // Devo ancora pescare ma la presa dopo è già cominciata: per l'ordine di gioco la mia carta
        // è la prima comparsa in tavola se apro io, la seconda se apre l'avversario. Se non è fra
        // quelle che so di avere, è proprio la pescata (giocata prima che la vedessi in mano).
        if pos.tricks > 0 && pos.table.is_none() && sure.is_none() {
            let mut new: Vec<Card> = (0..40u8)
                .filter(|&c| !pos.played.contains(c) && Some(c) != pos.trump_card && self.on_table(c))
                .collect();
            new.sort_by_key(|&c| self.table[c as usize].first);
            // (Se in tavola c'è una carta che so di avere, è quella la mia di questa presa: ne gioco
            // una sola.)
            let known = new.iter().any(|&c| pos.hand.contains(c));
            // Fra due carte comparse insieme la mia è quella vista (anche una volta sola) in mano,
            // o quella dalla mia parte del tavolo; solo senza questi indizi decide l'ordine di gioco.
            let pick = |f: &dyn Fn(Card) -> bool| match new.as_slice() {
                [a, b] if f(*a) != f(*b) => Some(if f(*a) { *a } else { *b }),
                _ => None,
            };
            let held = |c: Card| self.hand[c as usize].count >= 1;
            let my_side = |c: Card| self.my_side[c as usize].count >= 1 && self.their_side[c as usize].count == 0;
            let mine = pick(&held)
                .or_else(|| {
                    let together = matches!(new.as_slice(), [a, b] if self.table[*a as usize].first == self.table[*b as usize].first);
                    pick(&my_side).filter(|_| together)
                })
                .or_else(|| new.get(if pos.leader == ME { 0 } else { 1 }).copied())
                .filter(|_| !known);
            if let Some(c) = mine.filter(|&c| allowed.contains(c)) {
                return Some((c, true));
            }
        }
        // (Vista molte volte ma mai con sicurezza: si chiede conferma, per non far entrare in mano
        // una carta letta male, per esempio un sette scambiato per un sei.)
        if let Some(c) = sure {
            return Some((c, self.hand[c as usize].margin >= AUTO_MARGIN));
        }
        if let Some(c) = lead.filter(|&c| allowed.contains(c)) {
            return Some((c, self.on_table(c)));
        }
        // Una carta nuova in mano vista molte volte, ma mai con sicurezza (coperta proprio dove
        // differisce da un'altra): si aspetta, perché quando la gioco si vede in tavola; se resta
        // incerta per qualche secondo si chiede.
        (0..40u8)
            .filter(|&c| {
                let h = &self.hand[c as usize];
                // (Se poi è comparsa in tavola, anche lì incerta, aspettare non serve più.)
                let played = self.glimpsed(c) && self.recent(&self.table[c as usize]);
                allowed.contains(c) && ((h.count >= 20 && self.recent(h)) || (h.count >= 3 && played))
            })
            .max_by_key(|&c| (self.hand[c as usize].count, (self.hand[c as usize].margin * 10.0) as i32))
            .map(|c| (c, false))
    }

    /// La prima carta comparsa in tavola fra quelle non ancora giocate (anche letta male).
    fn first_table_card(&self, pos: &Position) -> Option<Card> {
        (0..40u8).filter(|&c| Some(c) != pos.trump_card && self.glimpsed(c) && !pos.played.contains(c)).min_by_key(
            |&c| (self.table[c as usize].first, std::cmp::Reverse((self.table[c as usize].margin * 10.0) as i32)),
        )
    }

    /// La mia prossima giocata: una mia carta vista in tavola, o sparita dalla mano mentre al suo
    /// posto è arrivata la pescata (o, a mazzo finito, la mano si è accorciata).
    fn next_my_play(&self, pos: &Position, scene: &Scene) -> Option<(Card, bool)> {
        if let Some(c) = pos
            .hand
            .iter()
            // (una carta rivista in mano dopo non era stata giocata: le carte in mano non tornano)
            .filter(|&c| {
                let (t, h) = (&self.table[c as usize], &self.hand[c as usize]);
                self.on_table(c) && t.last >= h.first && t.last >= h.last
            })
            .min_by_key(|&c| self.table[c as usize].first)
        {
            return Some((c, true));
        }
        if let Some(c) = self.played_from_hand(pos, scene) {
            return Some((c, true));
        }
        // Una mia carta vista in tavola più volte ma senza sicurezza: si chiede.
        pos.hand
            .iter()
            .filter(|&c| self.table[c as usize].count >= 2 && self.recent(&self.table[c as usize]))
            .max_by_key(|&c| self.table[c as usize].count)
            .map(|c| (c, false))
    }

    /// La mia giocata non si è vista, ma la partita è andata avanti (in mano ho già una carta
    /// nuova): si propone, da confermare, la carta della mano che non si vede da più tempo.
    fn missed_my_play(&self, pos: &Position) -> Option<(Card, bool)> {
        if !self.drew_after(pos) {
            return None;
        }
        pos.hand
            .iter()
            .filter(|&c| self.frame.saturating_sub(self.hand[c as usize].last) >= 3)
            .min_by_key(|&c| self.hand[c as usize].last)
            .map(|c| (c, false))
    }

    /// La mia carta sparita dalla mano mentre al suo posto è arrivata la pescata (o, a mazzo
    /// finito, la mano si è accorciata).
    fn played_from_hand(&self, pos: &Position, scene: &Scene) -> Option<Card> {
        // Le ultime due letture sicure della mano, recenti (una lettura di tanto tempo fa non dice
        // com'è la mano adesso), dicono la stessa cosa.
        let [.., (fa, a), (fb, b)] = self.full_hands.as_slice() else { return None };
        if *fb + 1 < self.frame || *fa + 4 < self.frame {
            return None;
        }
        if a != b || b.len() != scene.hand.len() {
            return None;
        }
        let missing: Vec<Card> = pos.hand.iter().filter(|c| !b.contains(c)).collect();
        let new = b.iter().filter(|c| !pos.hand.contains(**c)).count();
        let n = pos.hand.len() as usize;
        let fits = (b.len() == n && new == 1) || (b.len() + 1 == n && new == 0 && pos.deck_len == 0);
        match missing.as_slice() {
            [c] if fits => Some(*c),
            _ => None,
        }
    }

    /// La prossima giocata dell'avversario: una carta mai stata in mano mia comparsa in tavola,
    /// la prima in ordine di tempo.
    fn next_opp_play(&self, pos: &Position, scene: &Scene) -> Option<(Card, bool)> {
        let allowed = pos.allowed();
        let _ = scene;
        // Una carta comparsa in tavola dopo che avevo già pescato apre la presa dopo: non è la
        // risposta (non vista) alla mia carta.
        let cut = self.draw_seen(pos);
        let candidates = (0..40u8)
            .filter(|&c| allowed.contains(c) && !self.in_hand(c))
            .filter(|&c| cut.is_none_or(|f| self.opp_sight(c).first < f));
        if let Some(c) = candidates.clone().filter(|&c| {
            let sight = self.opp_sight(c);
            sight.sure() || (sight.count >= 1 && sight.margin >= AUTO_MARGIN && sight.last < self.frame)
        }).min_by_key(|&c| self.opp_sight(c).first) {
            return Some((c, true));
        }
        // Vista più volte di recente ma senza sicurezza: si chiede (una lettura sbagliata di tanto
        // tempo fa non conta).
        candidates
            .filter(|&c| self.opp_sight(c).count >= 2 && self.recent(&self.opp_sight(c)))
            .max_by_key(|&c| (self.opp_sight(c).count, (self.opp_sight(c).margin * 10.0) as i32))
            .map(|c| (c, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use briscola_core::ai::{evaluate, EngineConfig};
    use briscola_core::Game;
    use rand::Rng;

    /// Riproduce partite casuali inserendo solo ciò che vede il giocatore 0,
    /// e controlla che la posizione dell'assistente coincida con quella reale.
    #[test]
    fn tracks_real_games() {
        let mut rng = SmallRng::seed_from_u64(11);
        for n in 0..300 {
            let first = (n % 2) as u8;
            let mut g = Game::new(&mut rng, first);
            let mut a = Position::default();
            assert!(a.enter(g.trump_card).is_some());
            a.set_first(first);
            for c in g.hands[0].iter() {
                assert!(a.enter(c).is_some());
            }
            while !g.is_over() {
                let p = g.to_move();
                let m = g.legal_moves().to_vec();
                let c = m[rng.gen_range(0..m.len())];
                assert_eq!(a.step(), if p == ME { Step::MyMove } else { Step::OppMove });
                let before = g.hands[0];
                g.play(c);
                assert!(a.enter(c).is_some(), "carta rifiutata");
                // Le carte pescate dal giocatore 0 (tranne la briscola, che è automatica).
                for d in g.hands[0].minus(before).iter() {
                    if !a.hand.contains(d) {
                        assert!(matches!(a.step(), Step::Draw(1)));
                        assert!(a.enter(d).is_some());
                    }
                }
                let o = a.observation();
                let real = Observation::of(&g, 0);
                assert_eq!(o.hand, real.hand);
                assert_eq!(o.opp_hand_len, real.opp_hand_len);
                assert_eq!(o.deck_len, real.deck_len);
                assert_eq!(o.leader, real.leader);
                assert_eq!(o.table, real.table);
                assert_eq!(o.points, real.points);
                assert_eq!(o.played, real.played);
                assert_eq!(o.trump_card_owner, real.trump_card_owner);
            }
            assert_eq!(a.step(), Step::Over);
        }
    }

    /// Una partita inserita nell'assistente finisce per intero nel file, con esito e consigli.
    /// Il registro delle mosse avversarie ricavato dalla cronologia è lo stesso che il banco
    /// di prova tiene seguendo la partita vera.
    #[test]
    fn opponent_log_matches_real_game() {
        let dir = std::env::temp_dir().join(format!("brismaster-test-log-{}", std::process::id()));
        let mut rng = SmallRng::seed_from_u64(8);
        for n in 0..60 {
            let first = (n % 2) as u8;
            let mut g = Game::new(&mut rng, first);
            let mut a = Assistant::default();
            a.rec.dir = dir.clone();
            let mut log = infer::Log::default();
            assert!(a.enter(g.trump_card));
            a.set_first(first);
            for c in g.hands[0].iter() {
                assert!(a.enter(c));
            }
            while !g.is_over() {
                let m = g.legal_moves().to_vec();
                let c = m[rng.gen_range(0..m.len())];
                let before = g.hands[0];
                log.before_play(&g, ME, c);
                g.play(c);
                if !a.pos.played.contains(c) && a.pos.table != Some(c) {
                    assert!(a.enter(c));
                }
                for d in g.hands[0].minus(before).iter() {
                    if !a.pos.hand.contains(d) {
                        assert!(a.enter(d));
                    }
                }
            }
            // A fine partita (prima l'app può aver già inserito da sola le carte obbligate).
            assert_eq!(format!("{:?}", a.opp_log()), format!("{:?}", log.events));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_games() {
        let root = std::env::temp_dir().join(format!("brismaster-test-rec-{}", std::process::id()));
        let dir = root.join("Partite");
        let mut rng = SmallRng::seed_from_u64(5);
        let mut g = Game::new(&mut rng, ME);
        let mut a = Assistant::default();
        a.rec.dir = dir.clone();
        assert!(!a.rec.enabled, "la registrazione parte spenta");
        assert!(a.enter(g.trump_card));
        a.set_first(ME);
        for c in g.hands[0].iter() {
            assert!(a.enter(c));
        }
        let mut advised = 0;
        while !g.is_over() {
            if a.pos.step() == Step::MyMove && a.pos.hand.len() > 1 && advised < 2 {
                // Consiglio finto, per non far girare il motore nel test.
                let evals: Vec<MoveEval> =
                    a.pos.hand.iter().map(|card| MoveEval { card, win_prob: 0.5, exp_points: 60.0 }).collect();
                a.analysis = Some(Analysis { evals, reasons: Vec::new(), samples: 7 });
                advised += 1;
            }
            let c = g.legal_moves().to_vec()[0];
            let before = g.hands[0];
            g.play(c);
            // Le carte obbligate le ha già inserite l'app.
            if !a.pos.played.contains(c) && a.pos.table != Some(c) {
                assert!(a.enter(c));
            }
            for d in g.hands[0].minus(before).iter() {
                if !a.pos.hand.contains(d) {
                    assert!(a.enter(d));
                }
            }
            // A metà partita accendo la registrazione: si salva tutta, anche l'inizio.
            if a.pos.tricks == 10 && !a.rec.enabled {
                assert!(!dir.exists() || std::fs::read_dir(&dir).unwrap().next().is_none(), "spenta non salva");
                a.rec.set_enabled(true, &a.pos);
            }
        }
        let path = a.rec.path().to_path_buf();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["finita"], true);
        assert_eq!(v["risultato"]["punti_miei"], g.points[0]);
        let ev = v["eventi"].as_array().unwrap();
        assert_eq!(ev[0]["tipo"], "briscola");
        assert_eq!(ev[1]["chi"], "io");
        assert_eq!(ev.iter().filter(|e| e["tipo"] == "giocata").count(), 40);
        assert_eq!(ev.iter().filter(|e| e["consiglio"]["mondi"] == 7).count(), 2);
        // L'ultima presa si inserisce da sola: la tua ultima carta e quella dell'avversario.
        let autos: Vec<_> = a.history.iter().filter(|h| h.3).map(|h| h.0.step()).collect();
        assert!(autos.len() >= 2, "{autos:?} {:?}", a.pos.step());
        // Il file da inviare raccoglie la partita, accanto alla cartella delle partite.
        let (bundle, n) = record::export(&dir).unwrap();
        assert_eq!((n, bundle.parent().unwrap()), (1, root.as_path()));
        let b: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&bundle).unwrap()).unwrap();
        assert_eq!(b["partite"][0]["eventi"], v["eventi"]);
        assert!(b["partite"][0]["file"].as_str().unwrap().starts_with("partita_"));
        // Annullare tutto cancella il file di una partita mai cominciata.
        while !a.history.is_empty() {
            a.undo();
        }
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    /// Spegnere la registrazione cancella il file della partita; le partite salvate in
    /// automatico dalle versioni precedenti si cancellano una volta sola.
    #[test]
    fn recording_is_chosen_by_the_user() {
        let root = std::env::temp_dir().join(format!("brismaster-test-scelta-{}", std::process::id()));
        let dir = root.join("Partite");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("partita_2026-11-01_10-00-00.json"), "{}").unwrap();
        std::fs::write(dir.join("appunti.txt"), "mio").unwrap();
        std::fs::write(root.join(record::EXPORT_NAME), "{}").unwrap();
        assert_eq!(record::remove_automatic_once(&dir), 1);
        assert!(dir.join("appunti.txt").exists() && !root.join(record::EXPORT_NAME).exists());
        let mut a = Assistant::default();
        a.rec.dir = dir.clone();
        let mut rng = SmallRng::seed_from_u64(9);
        let g = Game::new(&mut rng, ME);
        assert!(a.enter(g.trump_card));
        a.set_first(ME);
        for c in g.hands[0].iter() {
            assert!(a.enter(c));
        }
        assert!(a.enter(g.hands[0].iter().next().unwrap()));
        a.rec.set_enabled(true, &a.pos);
        let path = a.rec.path().to_path_buf();
        assert!(path.exists());
        a.rec.set_enabled(false, &a.pos);
        assert!(!path.exists());
        // La seconda volta non cancella più niente, nemmeno le partite registrate a scelta.
        a.rec.set_enabled(true, &a.pos);
        assert_eq!(record::remove_automatic_once(&dir), 0);
        assert!(a.rec.path().exists());
        let _ = std::fs::remove_dir_all(root);
    }

    /// Inserimento da tastiera: la carta entra appena si digita il seme, senza Invio.
    #[test]
    fn types_cards_without_enter() {
        let mut a = Assistant::default();
        a.type_char('3');
        assert_eq!(a.pos.step(), Step::Trump, "manca ancora il seme");
        a.type_char('c');
        assert_eq!(a.pos.trump_card, Some(cards::make(1, 2)));
        a.type_char('1');
        assert_eq!(a.pos.first, Some(ME), "1 = comincio io");
        a.type_char('x');
        assert!(a.entry_error.is_some() && a.keys.is_empty());
        for ch in "10bas3c".chars() {
            a.type_char(ch);
        }
        assert_eq!(a.pos.hand.len(), 2, "il tre di coppe è la briscola scoperta: rifiutato");
        assert!(a.pos.hand.contains(cards::make(3, 9)) && a.pos.hand.contains(cards::make(2, 0)));
        assert!(a.entry_error.as_deref().unwrap().contains("non è possibile"));
        a.type_char('r');
        a.type_char('d');
        assert_eq!(a.pos.step(), Step::MyMove);
        // 2 = seconda carta della mano, nell'ordine in cui è mostrata.
        let second = a.pos.hand.to_vec()[1];
        a.type_char('2');
        assert_eq!(a.pos.table, Some(second));
        assert_eq!(a.pos.step(), Step::OppMove);
    }

    /// Spazio gioca il consiglio; se il motore sta ancora calcolando lo gioca appena arriva.
    #[test]
    fn space_plays_advice() {
        let mut a = Assistant::default();
        a.enter(cards::make(0, 4));
        a.set_first(ME);
        for c in [cards::make(1, 0), cards::make(2, 1), cards::make(3, 5)] {
            a.enter(c);
        }
        a.play_best();
        assert!(a.play_best_queued && a.pos.table.is_none());
        let (tx, rx) = channel();
        let evals = [cards::make(2, 1), cards::make(1, 0), cards::make(3, 5)]
            .map(|card| MoveEval { card, win_prob: 0.5, exp_points: 60.0 })
            .to_vec();
        tx.send(Analysis { evals, reasons: Vec::new(), samples: 7 }).unwrap();
        a.pending_generation = a.generation;
        a.pending = Some(rx);
        a.poll(7, &egui::Context::default());
        assert_eq!(a.pos.table, Some(cards::make(2, 1)));
        assert!(!a.play_best_queued);
        // Annulla torna a prima della giocata.
        a.undo();
        assert_eq!(a.pos.step(), Step::MyMove);
        assert_eq!(a.pos.hand.len(), 3);
    }

    /// Una carta vista sullo schermo, riconosciuta come `c` con distacco `margin`.
    #[test]
    fn poll_vision_preserves_intermediate_sightings_and_errors() {
        let c = cards::make(3, 2);
        let transient = Scene { opp_slot: Some(seen(c, 4.0)), ..Default::default() };
        let mut a = Assistant::default();
        a.vision.seen = Some(vec![]);
        a.vision.reader = Some(Reader::with_readings(
            vec![
                Reading::Scene(transient, 1),
                Reading::Scene(Scene::default(), 2),
                Reading::Error("lost capture".into()),
            ],
            2,
        ));
        a.poll_vision(&egui::Context::default());
        assert_eq!(a.vision.sightings.frame, 2);
        assert_eq!(a.vision.sightings.table[c as usize].count, 1);
        assert!(a.vision.lost);
        // The finite test producer closes after the error: recovery supersedes
        // its status message, while all intermediate scenes remain applied.
        assert!(a.vision.restart_after.is_some());
        a.take_reading(Reading::Error("lost capture".into()));
        assert!(a.vision.status.contains("lost capture"));
    }

    #[test]
    fn stale_analysis_is_discarded_without_starting_another_worker() {
        let mut a = Assistant::default();
        let (tx, rx) = channel();
        a.pending = Some(rx);
        a.changed();
        assert!(a.pending.is_some());
        tx.send(Analysis { evals: vec![], reasons: vec![], samples: 7 }).unwrap();
        a.poll(7, &egui::Context::default());
        assert!(a.pending.is_none() && a.analysis.is_none());
    }

    fn seen(c: Card, margin: f32) -> CardView {
        let mut scores = [0.0; 40];
        scores[c as usize] = margin;
        // Un disegno diverso per ogni carta, per la memoria delle immagini.
        let mut img = Img::new(crate::vision::CARD_W, crate::vision::CARD_H);
        for (i, p) in img.px.iter_mut().enumerate() {
            let v = ((i / 7) as u32).wrapping_mul(c as u32 + 3).wrapping_mul(2654435761).wrapping_shr(24) as u8;
            *p = [v, v / 2, 255 - v];
        }
        CardView { img, partial: false, vis: crate::vision::WHOLE, sim: [0.0; 40], scores, at: [0.0; 4] }
    }

    /// Ogni lettura arriva due volte, come dallo schermo (la prima si scarta per prudenza).
    fn show(a: &mut Assistant, scene: &Scene) {
        a.sync(scene);
        a.sync(scene);
    }

    /// La partita si segue da sola dallo schermo; le carte fantasma (già giocate ma ancora
    /// disegnate in mano dal sito) si ignorano.
    #[test]
    fn follows_the_screen() {
        let mut a = Assistant::default();
        let c = |s, r| cards::make(s, r);
        let mut scene = Scene { trump: Some(seen(c(0, 8), 4.0)), ..Default::default() };
        show(&mut a, &scene);
        assert_eq!(a.pos.trump_card, Some(c(0, 8)));
        scene.hand = vec![seen(c(3, 1), 4.0), seen(c(2, 6), 4.0), seen(c(1, 0), 4.0)];
        show(&mut a, &scene);
        assert_eq!(a.pos.step(), Step::First, "chi comincia si capisce dalla prima carta giocata");
        scene.opp_slot = Some(seen(c(3, 2), 4.0));
        show(&mut a, &scene);
        assert_eq!(a.pos.first, Some(OPP));
        assert_eq!(a.pos.hand.len(), 3);
        assert_eq!(a.pos.table, Some(c(3, 2)));
        assert_eq!(a.pos.step(), Step::MyMove);
        // Rispondo col 2 di bastoni: sparisce dalla mano e compare in tavola.
        scene.hand.remove(0);
        scene.my_slot = Some(seen(c(3, 1), 4.0));
        show(&mut a, &scene);
        assert_eq!(a.pos.tricks, 1);
        assert!(matches!(a.pos.step(), Step::Draw(1)));
        // Pesco; il sito disegna ancora il 2 di bastoni in mano (carta fantasma).
        scene.hand = vec![seen(c(3, 1), 4.0), seen(c(2, 6), 4.0), seen(c(1, 0), 4.0), seen(c(0, 3), 4.0)];
        show(&mut a, &scene);
        assert!(a.pos.hand.contains(c(0, 3)));
        assert!(!a.pos.hand.contains(c(3, 1)), "la carta fantasma non torna in mano");
        // La presa vecchia ancora in tavola non viene contata due volte.
        assert_eq!(a.pos.step(), Step::OppMove);
        assert_eq!(a.pos.tricks, 1);
    }

    /// Una pescata inserita sbagliata (qui scritta a mano): quando lo schermo mostra la mano vera,
    /// la carta si corregge da sola e la partita prosegue come se fosse stata giusta dall'inizio.
    #[test]
    fn corrects_a_wrong_draw_from_the_screen() {
        let mut a = Assistant::default();
        let c = |s, r| cards::make(s, r);
        let mut scene = Scene { trump: Some(seen(c(0, 8), 4.0)), ..Default::default() };
        show(&mut a, &scene);
        scene.hand = vec![seen(c(3, 1), 4.0), seen(c(2, 6), 4.0), seen(c(1, 0), 4.0)];
        show(&mut a, &scene);
        scene.opp_slot = Some(seen(c(3, 2), 4.0));
        show(&mut a, &scene);
        scene.hand.remove(0);
        scene.my_slot = Some(seen(c(3, 1), 4.0));
        show(&mut a, &scene);
        assert!(matches!(a.pos.step(), Step::Draw(_)));
        // Pesco il 4 di denari ma scrivo il 5.
        assert!(a.enter(c(0, 4)));
        assert!(a.pos.hand.contains(c(0, 4)));
        let tricks = a.pos.tricks;
        // Il sito toglie le carte dal tavolo e mostra la mano vera.
        let scene = Scene {
            trump: Some(seen(c(0, 8), 4.0)),
            hand: vec![seen(c(2, 6), 4.0), seen(c(1, 0), 4.0), seen(c(0, 3), 4.0)],
            ..Default::default()
        };
        show(&mut a, &scene);
        assert!(a.pos.hand.contains(c(0, 3)), "corretta");
        assert!(!a.pos.hand.contains(c(0, 4)));
        assert!(!a.vision.user_moves.iter().any(|&(_, _, card)| card == c(0, 4)), "la ricostruzione non deve riproporre la pescata errata");
        assert!(a.vision.user_moves.iter().any(|&(_, _, card)| card == c(0, 3)));
        assert_eq!(a.pos.tricks, tricks, "la presa resta");
        assert!(a.vision.status.contains("Mano corretta"), "{}", a.vision.status);
        // La carta sbagliata torna fra quelle ancora in gioco.
        assert!(a.pos.allowed().contains(c(0, 4)) || !a.pos.played.contains(c(0, 4)));
    }

    /// Screenshot veri di Solitalian: una partita sul tavolo di legno, poi il sito cambia sfondo
    /// (il mare) e distribuisce una partita nuova. L'app riparte da sola con la briscola e la mano
    /// nuove, riconosciute dallo schermo.
    #[test]
    fn follows_a_new_game_after_a_background_change() {
        let shot = |name: &str| {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata").join(name);
            crate::vision::capture::load_png(&path).unwrap()
        };
        let mut a = Assistant::default();
        let mem = a.vision.memory.clone();
        let wood = read_briscola(&shot("solitalian_briscola.png"), &mem).expect("legno");
        show(&mut a, &wood);
        let c = |s, r| cards::make(s, r);
        assert_eq!(a.pos.trump_card, Some(c(1, 4)));
        a.set_first(ME);
        show(&mut a, &wood);
        assert_eq!(a.pos.hand.len(), 3);
        assert_eq!(a.pos.step(), Step::MyMove);
        let sea = read_briscola(&shot("solitalian_mare.jpg"), &mem).expect("mare");
        // La partita nuova si riconosce dopo qualche lettura uguale (circa un secondo e mezzo).
        for _ in 0..3 {
            show(&mut a, &sea);
        }
        assert_eq!(a.pos.trump_card, Some(c(3, 1)), "briscola della partita nuova");
        assert_eq!(a.pos.step(), Step::First, "si aspetta chi comincia");
        a.set_first(OPP);
        show(&mut a, &sea);
        let hand: Vec<Card> = a.pos.hand.iter().collect();
        let mut want = vec![c(0, 3), c(3, 8), c(2, 0)];
        want.sort();
        assert_eq!(hand, want, "mano della partita nuova");
    }

    /// A metà partita il sito cambia sfondo (stessa partita, stesse carte): l'app non deve
    /// scambiarlo per una partita nuova e ricominciare.
    #[test]
    fn keeps_the_game_when_the_background_changes() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/solitalian_briscola.png");
        let wood = crate::vision::capture::load_png(&path).unwrap();
        // Lo stesso tavolo con lo sfondo blu al posto del legno (le carte restano com'erano).
        let mut blue = wood.clone();
        let cards_at = [(140, 420, 320, 570), (505, 740, 885, 915)];
        for (i, p) in blue.px.iter_mut().enumerate() {
            let (x, y) = (i % wood.w, i / wood.w);
            let on_card = cards_at.iter().any(|&(x0, y0, x1, y1)| (x0..x1).contains(&x) && (y0..y1).contains(&y));
            let [r, g, b] = *p;
            if !on_card && r > g && g > b && r < 235 {
                *p = [b / 3, g / 2, r];
            }
        }
        let mut a = Assistant::default();
        let mem = a.vision.memory.clone();
        let scene = read_briscola(&wood, &mem).expect("legno");
        show(&mut a, &scene);
        a.set_first(ME);
        show(&mut a, &scene);
        assert_eq!(a.pos.step(), Step::MyMove);
        let before = a.history.len();
        let other = read_briscola(&blue, &mem).expect("sfondo blu");
        assert!(other.trump.is_some() && other.hand.len() == 3);
        show(&mut a, &other);
        show(&mut a, &other);
        assert_eq!(a.history.len(), before, "nessuna partita nuova");
        assert_eq!(a.pos.step(), Step::MyMove);
    }

    /// A metà partita lo schermo mostra un'altra briscola (un'altra partita, senza la
    /// distribuzione nuova): dopo qualche secondo l'app lo dice invece di continuare in silenzio.
    #[test]
    fn warns_when_the_screen_is_another_game() {
        let mut a = Assistant::default();
        let c = |s, r| cards::make(s, r);
        let hand = vec![seen(c(3, 1), 4.0), seen(c(2, 6), 4.0), seen(c(1, 0), 4.0)];
        let scene = Scene { trump: Some(seen(c(0, 8), 4.0)), hand: hand.clone(), ..Default::default() };
        show(&mut a, &scene);
        a.set_first(ME);
        let other = Scene {
            trump: Some(seen(c(2, 2), 4.0)),
            hand: hand.clone(),
            opp_slot: Some(seen(c(1, 5), 4.0)),
            ..Default::default()
        };
        for _ in 0..10 {
            a.sync(&other);
        }
        assert_eq!(a.pos.trump_card, Some(c(0, 8)), "la partita inserita resta");
        assert!(a.vision.status.contains("non corrisponde"), "{}", a.vision.status);
    }

    /// Una partita interrotta a metà: quando compare un'altra briscola con tre carte nuove in mano
    /// e il tavolo vuoto, si riparte da capo senza premere "Nuova partita".
    #[test]
    fn notices_new_deal_mid_game() {
        let mut a = Assistant::default();
        let c = |s, r| cards::make(s, r);
        let hand = vec![seen(c(3, 1), 4.0), seen(c(2, 6), 4.0), seen(c(1, 0), 4.0)];
        let scene = Scene { trump: Some(seen(c(0, 8), 4.0)), hand: hand.clone(), ..Default::default() };
        show(&mut a, &scene);
        a.set_first(ME);
        show(&mut a, &scene);
        assert_eq!(a.pos.step(), Step::MyMove);
        // Stessa briscola, stessa mano: niente da rifare.
        show(&mut a, &scene);
        assert_eq!(a.pos.trump_card, Some(c(0, 8)));
        assert_eq!(a.pos.first, Some(ME));
        // Il sito distribuisce di nuovo: briscola diversa.
        let new_hand = vec![seen(c(2, 0), 4.0), seen(c(3, 4), 4.0), seen(c(1, 5), 4.0)];
        let deal = Scene { trump: Some(seen(c(1, 2), 4.0)), hand: new_hand, ..Default::default() };
        for _ in 0..3 {
            show(&mut a, &deal);
        }
        assert_eq!(a.pos.trump_card, Some(c(1, 2)));
        assert_eq!(a.pos.step(), Step::First);
    }

    /// Se la carta è incerta si chiede conferma; se l'utente annulla una carta letta male,
    /// la memoria la dimentica e la volta dopo si chiede invece di inserirla.
    #[test]
    fn asks_when_unsure() {
        let mut a = Assistant::default();
        let scene = Scene { trump: Some(seen(cards::make(0, 7), 1.0)), ..Default::default() };
        show(&mut a, &scene);
        assert!(a.pos.trump_card.is_none());
        assert_eq!(a.vision.confirm.as_ref().map(|c| c.card), Some(cards::make(0, 7)));
        a.accept_vision();
        assert_eq!(a.pos.trump_card, Some(cards::make(0, 7)));
        assert_eq!(a.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).len(), 1, "confermata: ora la ricorda");

        let mut b = Assistant::default();
        let scene = Scene { trump: Some(seen(cards::make(2, 2), 4.0)), ..Default::default() };
        show(&mut b, &scene);
        assert_eq!(b.pos.trump_card, Some(cards::make(2, 2)));
        b.undo();
        assert!(b.pos.trump_card.is_none());
        assert_eq!(b.vision.memory.lock().unwrap_or_else(|e| e.into_inner()).len(), 0, "annullata: dimenticata");
        show(&mut b, &scene);
        assert!(b.pos.trump_card.is_none() && b.vision.confirm.is_some(), "ora chiede conferma");
    }

    #[test]
    fn rejects_impossible_cards() {
        let mut a = Position::default();
        a.enter(cards::make(0, 0));
        a.set_first(ME);
        assert!(a.enter(cards::make(0, 0)).is_none(), "la briscola scoperta non si pesca");
        assert!(a.enter(cards::make(1, 0)).is_some());
        assert!(a.enter(cards::make(1, 0)).is_none(), "carta già in mano");
    }

    #[test]
    fn explains_advice() {
        let mut a = Position::default();
        a.enter(cards::make(0, 4));
        a.set_first(OPP);
        for c in [cards::make(1, 0), cards::make(2, 1), cards::make(3, 5)] {
            a.enter(c);
        }
        a.enter(cards::make(1, 2)); // l'avversario apre col Tre di Coppe
        let obs = a.observation();
        let mut rng = SmallRng::seed_from_u64(1);
        let evals = evaluate(&obs, &EngineConfig::strength(300), &mut rng);
        assert_eq!(evals[0].card, cards::make(1, 0), "l'Asso di Coppe prende il Tre");
        let r = explain(&obs, &evals);
        assert!(r[0].starts_with("Prendi la presa"), "{r:?}");
    }

    /// Cento partite diverse: l'interfaccia raccoglie tutte le letture solo alla fine.
    /// Anche le assenze del tavolo restano in ordine, senza perdere le giocate intermedie.
    #[test]
    fn background_queue_preserves_complete_games() {
        for seed in 70000..70100 {
            let mut rng = SmallRng::seed_from_u64(seed);
            let mut g = Game::new(&mut rng, (seed % 2) as u8);
            let mut readings = Vec::new();
            let mut hand: Vec<Card> = g.hands[0].iter().collect();
            let mut frame_no = 0;
            let mut frame = |hand: &[Card], slots: [Option<Card>; 2], g: &Game| {
                frame_no += 1;
                if frame_no % 11 == 0 {
                    for _ in 0..3 { readings.push(Reading::NoTable); }
                }
                let scene = Scene {
                    trump: (g.deck_len > 0).then(|| seen(g.trump_card, 4.0)),
                    hand: hand.iter().map(|&c| seen(c, 4.0)).collect(),
                    my_slot: slots[0].map(|c| seen(c, 4.0)),
                    opp_slot: slots[1].map(|c| seen(c, 4.0)),
                };
                for _ in 0..3 { readings.push(Reading::Scene(scene.clone(), 0)); }
            };
            frame(&hand, [None; 2], &g);
            while !g.is_over() {
                let mut slots = [None; 2];
                let mut drawn = Vec::new();
                for _ in 0..2 {
                    let who = g.to_move();
                    let moves = g.legal_moves().to_vec();
                    let c = moves[rng.gen_range(0..moves.len())];
                    let before = g.hands[0];
                    g.play(c);
                    slots[who as usize] = Some(c);
                    if who == ME { hand.retain(|&h| h != c); }
                    drawn = g.hands[0].minus(before).iter().filter(|d| !hand.contains(d)).collect();
                    frame(&hand, slots, &g);
                }
                hand.extend(drawn);
                frame(&hand, slots, &g);
                frame(&hand, [None; 2], &g);
            }
            let mut a = Assistant::default();
            a.rec.off = true;
            a.vision.memory = Arc::new(Mutex::new(Memory::in_memory()));
            a.vision.reader = Some(Reader::from_readings(readings));
            let ctx = egui::Context::default();
            let mut batches = 0;
            loop {
                let n = a.poll_vision(&ctx);
                assert!(n <= 8, "la coda non deve monopolizzare l'interfaccia");
                if n == 0 { break; }
                batches += 1;
                assert!(batches < 1000);
            }
            assert!(batches > 1);
            assert_eq!(a.pos.tricks, 20, "seed {seed}, {:?}", a.pos.step());
            assert_eq!(a.pos.points, g.points, "seed {seed}");
            assert_eq!(a.pos.played, g.played, "seed {seed}");
            assert!(a.pos.hand.is_empty());
            assert_eq!(a.pos.deck_len, 0);
        }
    }

    /// L'ultima pescata può già mostrare la briscola in mano mentre la nostra
    /// risposta, coperta da altre carte, non è ancora stata inserita.
    #[test]
    fn last_trump_draw_recovers_an_occluded_response() {
        let mut a = Assistant::default();
        a.rec.off = true;
        a.enter(0);
        a.set_first(OPP);
        for c in [10, 20, 30] { a.enter(c); }
        a.enter(1);
        a.pos.deck_len = 2;
        a.pos.tricks = 16;
        a.pos.played = CardSet::FULL;
        for c in [0, 1, 10, 20, 30, 11, 12, 13] { a.pos.played.remove(c); }
        let scene = Scene {
            hand: [20, 30, 0].map(|c| seen(c, 4.0)).to_vec(),
            opp_slot: Some(seen(1, 4.0)),
            ..Default::default()
        };
        show(&mut a, &scene);
        show(&mut a, &scene);
        assert_eq!(a.pos.tricks, 17);
        assert_eq!(a.pos.deck_len, 0);
        assert_eq!(a.pos.trump_card_owner, Some(ME));
        assert!(a.pos.hand.contains(0));
        assert!(!a.pos.hand.contains(10));
    }

    #[test]
    fn rejected_rebuild_does_not_teach_card_memory() {
        let mut a = Assistant::default();
        a.rec.off = true;
        a.vision.memory = Arc::new(Mutex::new(Memory::in_memory()));
        a.enter(0);
        a.set_first(ME);
        for c in [10, 20, 30] { a.enter(c); }
        let scene = Scene {
            trump: Some(seen(0, 4.0)),
            hand: [10, 20, 30].map(|c| seen(c, 4.0)).to_vec(),
            ..Default::default()
        };
        a.vision.sightings.update(&scene);
        a.vision.sightings.update(&scene);
        assert_eq!(a.vision.memory.lock().unwrap().len(), 0);
        assert!(!a.rebuild(&scene));
        assert_eq!(a.vision.memory.lock().unwrap().len(), 0);
    }

    #[test]
    fn a_misread_own_card_is_not_an_opponent_play() {
        let mut a = Assistant::default();
        a.rec.off = true;
        a.enter(0);
        a.set_first(OPP);
        for c in [10, 20, 30] { a.enter(c); }
        let own = Scene { my_slot: Some(seen(12, 4.0)), ..Default::default() };
        a.vision.sightings.update(&own);
        a.vision.sightings.update(&own);
        assert!(a.vision.sightings.next_opp_play(&a.pos, &own).is_none());
        let opp = Scene { opp_slot: Some(seen(13, 4.0)), ..Default::default() };
        a.vision.sightings.update(&opp);
        a.vision.sightings.update(&opp);
        assert_eq!(a.vision.sightings.next_opp_play(&a.pos, &opp), Some((13, true)));
    }

    #[test]
    fn moving_a_card_up_is_a_play_not_a_second_hand_row() {
        let mut a = manual_turn();
        let mut initial = Scene { hand: [10, 20, 30].map(|c| seen(c, 4.0)).to_vec(), ..Default::default() };
        for (i, view) in initial.hand.iter_mut().enumerate() {
            view.at = [0.3 + i as f32 * 0.15, 0.7, 0.4 + i as f32 * 0.15, 0.9];
        }
        a.vision.sightings.update(&initial);
        let mut played = initial.clone();
        played.hand[0].at = [0.45, 0.4, 0.55, 0.6];
        let fixed = a.fix_layout(&played).unwrap();
        assert_eq!(fixed.hand.len(), 2);
        assert_eq!(fixed.my_slot.unwrap().best(CardSet::FULL).unwrap().0, 10);
        // Una finestra spostata non deve produrre una giocata.
        for view in &mut initial.hand { view.at[1] -= 0.3; view.at[3] -= 0.3; }
        assert!(a.fix_layout(&initial).is_none());
    }

    fn manual_turn() -> Assistant {
        let mut a = Assistant::default();
        a.rec.off = true;
        a.enter(0);
        a.set_first(ME);
        for c in [10, 20, 30] { a.enter(c); }
        a
    }

    #[test]
    fn typing_a_historical_card_does_not_relabel_the_current_image() {
        for recovering in [false, true] {
            let mut a = manual_turn();
            a.enter(10);
            a.vision.memory = Arc::new(Mutex::new(Memory::in_memory()));
            a.vision.reader = Some(Reader::from_readings(Vec::<Reading<Scene>>::new()));
            a.vision.inconsistent = recovering;
            let view = seen(14, 4.0);
            a.vision.confirm = Some(Confirm { step: Step::OppMove, card: 14, img: view.img, vis: view.vis, texture: None });
            a.type_char('4');
            a.type_char('c'); // Quattro di Coppe: carta 13, eventualmente di una presa precedente.
            assert_eq!(a.pos.tricks, 1);
            assert_eq!(a.vision.memory.lock().unwrap().len(), if recovering { 0 } else { 1 });
        }
    }

    #[test]
    fn a_rejected_draw_does_not_overwrite_the_manual_recovery() {
        let mut a = manual_turn();
        a.enter(10);
        a.enter(1);
        assert!(matches!(a.pos.step(), Step::Draw(_)));
        a.vision.memory = Arc::new(Mutex::new(Memory::in_memory()));
        a.vision.reader = Some(Reader::from_readings(Vec::<Reading<Scene>>::new()));
        a.vision.inconsistent = true;
        let view = seen(14, 4.0);
        a.vision.confirm = Some(Confirm { step: a.pos.step(), card: 14, img: view.img, vis: view.vis, texture: None });
        a.type_char('4');
        a.type_char('c');
        let stale = Scene { trump: Some(seen(0, 4.0)), hand: [14, 20, 30].map(|c| seen(c, 4.0)).to_vec(), ..Default::default() };
        show(&mut a, &stale);
        show(&mut a, &stale);
        assert!(a.pos.hand.contains(13));
        assert!(!a.pos.hand.contains(14));
        assert_eq!(a.vision.memory.lock().unwrap().len(), 0);
    }

    #[test]
    fn disconnected_calculation_does_not_wait_forever() {
        let mut a = manual_turn();
        let (tx, rx) = channel();
        drop(tx);
        a.pending = Some(rx);
        a.analysis_started = Some(std::time::Instant::now());
        let ctx = egui::Context::default();
        a.poll(7, &ctx);
        assert!(a.pending.is_none());
        assert_eq!(a.analysis_failures, 1);
        assert!(a.analysis_error.as_ref().unwrap().contains("interrotto"));
        a.analysis_retry_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        a.poll(7, &ctx);
        assert!(a.pending.is_some(), "il consiglio riparte senza riavviare l'app");
    }

    #[test]
    fn stalled_calculations_have_a_bounded_retry() {
        let mut a = manual_turn();
        let ctx = egui::Context::default();
        for expected in [1, 2] {
            let (_tx, rx) = channel();
            a.pending = Some(rx);
            a.analysis_started = Some(std::time::Instant::now() - std::time::Duration::from_secs(16));
            a.poll(7, &ctx);
            assert!(a.pending.is_none());
            assert_eq!(a.analysis_failures, expected);
        }
        a.poll(7, &ctx);
        assert!(a.pending.is_none(), "nessuna cascata infinita di nuovi calcoli");
        assert!(a.analysis_error.as_ref().unwrap().contains("in tempo"));
        assert!(a.enter(10), "l'inserimento delle carte resta utilizzabile");
        assert_eq!(a.analysis_failures, 0);
    }

    #[test]
    fn inconsistent_screen_suspends_advice_and_recovers_when_it_matches() {
        let mut a = manual_turn();
        a.vision.reader = Some(Reader::from_readings(Vec::<Reading<Scene>>::new()));
        let wrong = Scene { trump: Some(seen(0, 4.0)), ..Default::default() };
        for _ in 0..12 { a.sync(&wrong); }
        assert!(a.vision.inconsistent);
        a.poll(7, &egui::Context::default());
        assert!(a.pending.is_none() && a.analysis.is_none());
        let right = Scene { trump: Some(seen(0, 4.0)), hand: [10, 20, 30].map(|c| seen(c, 4.0)).to_vec(), ..Default::default() };
        show(&mut a, &right);
        assert!(!a.vision.inconsistent);
        assert_eq!(a.pos.hand.len(), 3);
    }

    #[test]
    fn stale_screen_never_plays_the_old_advice() {
        let mut a = manual_turn();
        a.analysis = Some(Analysis { evals: vec![MoveEval { card: 10, win_prob: 0.5, exp_points: 60.0 }], reasons: Vec::new(), samples: 7 });
        a.vision.catching_up = true;
        a.play_best();
        assert_eq!(a.pos.hand.len(), 3);
        assert!(!a.play_best_queued);
        a.poll(7, &egui::Context::default());
        assert!(a.analysis.is_none() && a.pending.is_none());
    }

    #[test]
    fn disconnected_reader_requests_recovery_without_erasing_the_game() {
        let mut a = manual_turn();
        a.vision.reader = Some(Reader::from_readings(Vec::<Reading<Scene>>::new()));
        assert_eq!(a.poll_vision(&egui::Context::default()), 0);
        assert!(a.vision.restart_after.is_some());
        assert!(a.screen_is_behind());
        assert_eq!(a.pos.hand.len(), 3);
        assert_eq!(a.pos.trump_card, Some(0));
    }

    /// Il recupero dopo una scheda nascosta non è una giocata osservata: il motore
    /// non deve inventare una carta avversaria o andare in panic leggendo lo storico.
    #[test]
    fn inference_survives_a_gap_in_the_game() {
        let mut a = Assistant::default();
        a.rec.off = true;
        a.enter(0);
        a.set_first(OPP);
        for c in [10, 20, 30] { assert!(a.enter(c)); }
        assert!(a.enter(11));
        a.vision.gap = true;
        let scene = Scene {
            trump: Some(seen(0, 4.0)),
            hand: [12, 20, 30].map(|c| seen(c, 4.0)).to_vec(),
            opp_slot: Some(seen(13, 4.0)),
            ..Default::default()
        };
        a.vision.sightings.full_hands = vec![(1, vec![12, 20, 30]), (2, vec![12, 20, 30])];
        a.vision.sightings.frame = 2;
        assert!(a.resync(&scene));
        assert_eq!(a.pos.tricks, 1);
        let _ = a.opp_log();
        // Correggere una vecchia pescata non deve leggere una voce priva di evento.
        assert!(!a.correct_draw(12, 14));
        a.undo();
        assert_eq!(a.pos.tricks, 0);
        assert_eq!(a.pos.table, Some(11));
    }

    /// Ctrl+Z deve cancellare anche l'indizio manuale usato dalla ricostruzione.
    #[test]
    fn undone_cards_are_not_replayed() {
        let mut a = Assistant::default();
        a.rec.off = true;
        a.enter(0);
        a.set_first(ME);
        a.enter(10);
        a.undo();
        assert!(!a.vision.user_moves.iter().any(|&(_, _, c)| c == 10));
        a.undo();
        assert_eq!(a.vision.user_first, None);
    }

    /// Una partita intera giocata su un tavolo finto e seguita solo dallo schermo, come la vedrebbe
    /// l'app: ogni fotogramma del sito (carta giocata, risposta, presa che resta in tavola, pescata,
    /// tavolo sgombro) passa dalla lettura vera dello schermo. `linger` tiene la presa in tavola
    /// anche dopo la pescata (molti siti la lasciano un momento). Restituisce le prese seguite e le
    /// conferme chieste, o dove si è fermata.
    fn play_on_screen(
        seed: u64,
        style: usize,
        look: crate::vision::synth::Look,
        linger: bool,
        flicker: bool,
    ) -> Result<(u8, u32), String> {
        play_on_screen_with_light(seed, style, look, linger, flicker, [1.0; 3], false, false)
    }

    fn play_on_screen_with_light(
        seed: u64, style: usize, look: crate::vision::synth::Look,
        linger: bool, flicker: bool, gain: [f32; 3], gray: bool, allow_recovery: bool,
    ) -> Result<(u8, u32), String> {
        use crate::vision::reference::card_image;
        use crate::vision::synth::{table, Deal};
        let mut rng = SmallRng::seed_from_u64(seed);
        let first = (seed % 2) as u8;
        let mut g = Game::new(&mut rng, first);
        let imgs: Vec<Img> = (0..40u8).map(|c| card_image(style, c)).collect();
        let mut a = Assistant::default();
        let mut confirms = 0;
        let mut delays = Vec::new();
        a.rec.off = true;
        a.vision.memory = Arc::new(Mutex::new(Memory::in_memory()));
        // La mano sullo schermo, nell'ordine in cui il sito la disegna.
        let mut hand: Vec<Card> = g.hands[0].iter().collect();
        let true_draws = std::cell::RefCell::new(hand.clone());
        let true_plays: std::cell::RefCell<[Vec<Card>; 2]> = std::cell::RefCell::new([Vec::new(), Vec::new()]);
        let mut frame = 0;
        let mut look_at = |a: &mut Assistant, hand: &[Card], opp: Option<Card>, mine: Option<Card>, g: &Game, what: &str| -> Result<(), String> {
            let trump = (g.deck_len > 0).then(|| &imgs[g.trump_card as usize]);
            let deal = Deal {
                hand: hand.iter().map(|&c| &imgs[c as usize]).collect(),
                opp: opp.map(|c| &imgs[c as usize]),
                mine: mine.map(|c| &imgs[c as usize]),
                trump,
                deck: g.deck_len > 1,
            };
            let mut img = table(&deal, &look);
            for p in &mut img.px {
                for c in 0..3 { p[c] = (p[c] as f32 * gain[c]).clamp(0.0, 255.0) as u8; }
                if gray {
                    let y = ((p[0] as u32 * 3 + p[1] as u32 * 6 + p[2] as u32) / 10) as u8;
                    *p = [y; 3];
                }
            }
            frame += 1;
            if let Ok(d) = std::env::var("SALVA") {
                img.save_png(std::path::Path::new(&format!("{d}/partita_{seed}_{frame:03}.png"))).unwrap();
            }
            let Some(scene) = read_briscola(&img, &a.vision.memory) else { return Ok(()) };
            show(a, &scene);
            if std::env::var_os("BRIS_STRESS_LOG").is_some() {
                let best = |v: &CardView| v.best(CardSet::FULL).map(|(c, m)| (cards::name(c), m));
                println!("frame={frame}; {what}; vero={:?}/{:?}; letto mano={:?}, tavolo={:?}/{:?}; app prese={}, punti={:?}, passo={:?}, mano={:?}, tavolo={:?}",
                    g.played.len()/2, g.points, scene.hand.iter().map(best).collect::<Vec<_>>(), scene.opp_slot.as_ref().and_then(best), scene.my_slot.as_ref().and_then(best),
                    a.pos.tricks, a.pos.points, a.pos.step(), a.pos.hand.iter().map(cards::name).collect::<Vec<_>>(), a.pos.table.map(cards::name));
            }
            if flicker {
                // Mentre lo schermo è fermo, una lettura vede la briscola come un'altra carta, con
                // sicurezza; poi torna giusta.
                let mut wrong = scene.clone();
                if let Some(t) = &mut wrong.trump {
                    let other = (g.trump_card as usize + 7) % 40;
                    t.scores.swap(g.trump_card as usize, other);
                    t.sim.swap(g.trump_card as usize, other);
                }
                a.sync(&wrong);
                show(a, &scene);
            }
            // Quando il programma chiede esplicitamente la briscola non visibile,
            // l'utente può inserirla: si misura anche questo intervento, senza
            // fornire carte vere di nascosto al riconoscitore.
            if allow_recovery && a.pos.step() == Step::Trump && a.vision.status.starts_with("Non trovo la briscola") {
                confirms += 1;
                println!("seed={seed}; intervento manuale: briscola richiesta dall'app");
                assert!(a.enter(g.trump_card));
                show(a, &scene);
            }
            // L'utente conferma le carte che l'app chiede (se sono giuste).
            for _ in 0..3 {
                let Some(c) = &a.vision.confirm else { break };
                let right = match c.step {
                    Step::Trump => Some(g.trump_card),
                    Step::MyMove | Step::OppMove => {
                        let who = if c.step == Step::MyMove { ME } else { OPP };
                        let n = a.rec.events.iter().filter(|e| matches!(e, Event::Giocata { chi, .. } if *chi == record::who(who))).count();
                        true_plays.borrow()[who as usize].get(n).copied()
                    }
                    Step::Draw(_) => {
                        let draws = true_draws.borrow();
                        if a.pos.tricks == 0 {
                            // Le tre carte iniziali possono essere inserite in qualunque ordine.
                            if draws[..3].contains(&c.card) && !a.pos.hand.contains(c.card) {
                                Some(c.card)
                            } else {
                                draws[..3].iter().copied().find(|&card| !a.pos.hand.contains(card))
                            }
                        } else {
                            let n = a.rec.events.iter().filter(|e| matches!(e, Event::Pescata { .. })).count();
                            draws.get(n).copied()
                        }
                    }
                    _ => None,
                };
                if right != Some(c.card) {
                    if !allow_recovery {
                        return Err(format!("{what}: conferma non verificata per {:?}: {} (attesa {:?})", c.step, cards::name(c.card), right.map(cards::name)));
                    }
                    let proposed = a.vision.confirm.take().unwrap();
                    a.reject(proposed.step, proposed.card);
                    confirms += 1;
                    println!("seed={seed}; intervento manuale: rifiutata {} per {:?}, corretta {:?}", cards::name(proposed.card), proposed.step, right.map(cards::name));
                    if let Some(correct) = right {
                        if !a.enter(correct) {
                            return Err(format!("{what}: la correzione richiesta {} non è inseribile", cards::name(correct)));
                        }
                    }
                    show(a, &scene);
                    continue;
                }
                confirms += 1;
                a.accept_vision();
                show(a, &scene);
            }
            if allow_recovery && a.vision.inconsistent && a.vision.confirm.is_none() {
                let step = a.pos.step();
                let requested = match step {
                    Step::Draw(_) => {
                        let draws = true_draws.borrow();
                        if a.pos.tricks == 0 {
                            draws[..3].iter().copied().find(|&card| !a.pos.hand.contains(card))
                        } else {
                            let n = a.rec.events.iter().filter(|e| matches!(e, Event::Pescata { .. })).count();
                            draws.get(n).copied()
                        }
                    }
                    Step::MyMove | Step::OppMove => {
                        let who = if step == Step::MyMove { ME } else { OPP };
                        let n = a.rec.events.iter().filter(|e| matches!(e, Event::Giocata { chi, .. } if *chi == record::who(who))).count();
                        true_plays.borrow()[who as usize].get(n).copied()
                    }
                    _ => None,
                };
                if let Some(card) = requested {
                    if !a.pos.allowed().contains(card) {
                        return Err(format!("{what}: la carta richiesta per il recupero {} non è inseribile", cards::name(card)));
                    }
                    confirms += 1;
                    println!("seed={seed}; intervento manuale: carta mancante richiesta per {:?}: {}", step, cards::name(card));
                    assert!(a.enter(card));
                    show(a, &scene);
                }
            }
            Ok(())
        };
        let check = |a: &Assistant, g: &Game, what: &str| -> Result<(), String> {
            let (o, real) = (a.pos.observation(), Observation::of(g, 0));
            if a.pos.step() == Step::Trump || a.pos.step() == Step::First || matches!(a.pos.step(), Step::Draw(_)) && a.pos.tricks == 0 {
                return Err(format!("{what}: ferma al passo {:?}", a.pos.step()));
            }
            // L'ultima carta la gioca l'app da sola: può essere avanti di una presa.
            if g.deck_len == 0 && g.hands[0].len() <= 1 {
                return Ok(());
            }
            if o.hand != real.hand || o.table != real.table || o.played != real.played || o.leader != real.leader {
                return Err(format!(
                    "{what}: passo {:?}, prese {} (vere {}), mano {:?} (vera {:?}), tavolo {:?} (vero {:?})",
                    a.pos.step(),
                    a.pos.tricks,
                    real.played.len() / 2,
                    o.hand.iter().map(cards::name).collect::<Vec<_>>(),
                    real.hand.iter().map(cards::name).collect::<Vec<_>>(),
                    o.table.map(cards::name),
                    real.table.map(cards::name)
                ));
            }
            Ok(())
        };
        // L'inizio: briscola e mano, tavolo vuoto.
        look_at(&mut a, &hand, None, None, &g, "inizio")?;
        while !g.is_over() {
            let n = g.played.len() / 2 + 1;
            let mut slots: [Option<Card>; 2] = [None, None];
            let mut drawn = Vec::new();
            for turn in 0..2 {
                let p = g.to_move();
                let m = g.legal_moves().to_vec();
                let c = m[rng.gen_range(0..m.len())];
                let before = g.hands[0];
                g.play(c);
                true_plays.borrow_mut()[p as usize].push(c);
                slots[p as usize] = Some(c);
                if p == ME {
                    hand.retain(|&h| h != c);
                }
                drawn = g.hands[0].minus(before).iter().filter(|d| !hand.contains(d)).collect();
                true_draws.borrow_mut().extend(drawn.iter().copied());
                let what = format!("presa {n}, {} gioca {}", if p == ME { "io" } else { "lui" }, cards::name(c));
                look_at(&mut a, &hand, slots[1], slots[0], &g, &what)?;
                if turn == 0 {
                    if let Err(error) = check(&a, &g, &what) {
                        if !allow_recovery { return Err(error); }
                        delays.push(error);
                    }
                }
            }
            // La presa resta in tavola un momento.
            look_at(&mut a, &hand, slots[1], slots[0], &g, &format!("presa {n} in tavola"))?;
            hand.extend(drawn);
            if linger {
                look_at(&mut a, &hand, slots[1], slots[0], &g, &format!("presa {n}, pescata con la presa in tavola"))?;
            }
            look_at(&mut a, &hand, None, None, &g, &format!("presa {n}, pescata"))?;
            if let Err(error) = check(&a, &g, &format!("dopo la presa {n}")) {
                if !allow_recovery { return Err(error); }
                delays.push(error);
            }
        }
        if !delays.is_empty() {
            println!("seed={seed}; checkpoint in ritardo={}; primo={}", delays.len(), delays[0]);
        }
        if a.pos.tricks != 20 || a.pos.points != g.points || a.pos.played != g.played || !a.pos.hand.is_empty() || a.pos.deck_len != 0 || a.pos.opp_hand_len != 0 {
            return Err(format!("fine partita: {} prese, punti {:?} (veri {:?}), mano {}, mazzo {}",
                a.pos.tricks, a.pos.points, g.points, a.pos.hand.len(), a.pos.deck_len)
                + &format!(", briscola {:?} (vera {})", a.pos.trump_card.map(cards::name), cards::name(g.trump_card)));
        }
        Ok((a.pos.tricks, confirms))
    }

    #[test]
    fn reads_a_large_play_behind_the_hand() {
        use crate::vision::synth::{table, Deal, Look};
        use crate::vision::reference::card_image;
        let images: Vec<_> = (0..40).map(|c| card_image(0, c)).collect();
        let trump = cards::make(3, 1);
        let mine = cards::make(3, 0);
        let hand = [cards::make(1, 2), cards::make(2, 7)];
        let img = table(&Deal {
            hand: hand.iter().map(|&c| &images[c as usize]).collect(),
            opp: None, mine: Some(&images[mine as usize]), trump: Some(&images[trump as usize]), deck: true,
        }, &Look { card_h: 280.0, ..Look::default() });
        let scene = read_briscola(&img, &Mutex::new(Memory::in_memory())).unwrap();
        assert_eq!(scene.my_slot.as_ref().and_then(|v| v.best(CardSet::FULL)).map(|b| b.0), Some(mine));
    }

    /// Matrice riproducibile di partite intere, non soli ritagli di carte.
    /// BRIS_STRESS_CASO=nome seleziona una condizione; BRIS_STRESS_VALIDAZIONE=1 usa
    /// altri semi e combinazioni, senza riutilizzare le partite di sviluppo.
    #[test]
    #[ignore]
    fn briscola_extreme_games() {
        use crate::vision::synth::{Bg, Look};
        let d = Look::default();
        let validation = std::env::var_os("BRIS_STRESS_VALIDAZIONE").is_some();
        let cases = if validation {
            vec![
                ("riserva-legno-caldo", Look { bg: Bg::Wood, card_h: 125.0, cover: 0.5, writing: true, ..d }, [1.0, 0.85, 0.65], false),
                ("riserva-mare-piccole", Look { bg: Bg::Sea, card_h: 105.0, fan: -5.0, pile: 0.8, ..d }, [1.0; 3], false),
                ("riserva-grigio", Look { card_h: 180.0, logo: 0.5, ..d }, [1.0; 3], true),
                ("riserva-freddo", Look { card_h: 205.0, aspect: Some(0.7), ..d }, [0.72, 0.85, 1.0], false),
            ]
        } else {
            vec![
                ("normale", d, [1.0; 3], false),
                ("piccole", Look { card_h: 70.0, ..d }, [1.0; 3], false),
                ("grandi", Look { card_h: 280.0, ..d }, [1.0; 3], false),
                ("strette", Look { aspect: Some(0.44), ..d }, [1.0; 3], false),
                ("larghe", Look { aspect: Some(0.90), ..d }, [1.0; 3], false),
                ("ventaglio-negativo", Look { fan: -12.0, ..d }, [1.0; 3], false),
                ("ventaglio-positivo", Look { fan: 12.0, ..d }, [1.0; 3], false),
                ("tavolo-chiaro", Look { bg: Bg::Felt([220, 215, 200]), ..d }, [1.0; 3], false),
                ("tavolo-scuro", Look { bg: Bg::Felt([2, 3, 4]), ..d }, [1.0; 3], false),
                ("schermo-scuro", d, [0.45; 3], false),
                ("luce-calda", d, [1.0, 0.75, 0.50], false),
                ("luce-fredda", d, [0.55, 0.75, 1.0], false),
                ("grigio", d, [1.0; 3], true),
                ("poco-sature", Look { sat: 0.2, ..d }, [1.0; 3], false),
                ("molto-sature", Look { sat: 1.8, ..d }, [1.0; 3], false),
                ("briscola-coperta", Look { cover: 0.65, pile: 0.75, ..d }, [1.0; 3], false),
                ("scritte-sfocatura", Look { writing: true, logo: 0.9, blur: true, ..d }, [1.0; 3], false),
            ]
        };
        let selected = std::env::var("BRIS_STRESS_CASO").ok();
        let round = std::env::var("BRIS_STRESS_ROUND").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let validation_seed = std::env::var("BRIS_STRESS_SEED").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(99000);
        let mut failed = Vec::new();
        let mut ran = 0;
        for (i, (name, look, gain, gray)) in cases.into_iter().enumerate() {
            if selected.as_ref().is_some_and(|s| s != name) { continue; }
            if validation && round == 7 && i >= 2 { continue; }
            ran += 1;
            let seed = if validation { validation_seed + round * 101 + i as u64 } else { 1200 + i as u64 };
            let started = std::time::Instant::now();
            let result = play_on_screen_with_light(seed, (i + round as usize) % 2, look, (i + round as usize) % 2 == 0, false, gain, gray, true);
            println!("{name}; seed={seed}; {result:?}; {:.1}s", started.elapsed().as_secs_f32());
            if let Err(error) = result { failed.push(format!("{name}: {error}")); }
        }
        assert!(ran > 0, "nessun caso selezionato");
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    /// Partite intere seguite dallo schermo, su tavoli diversi: non si fermano mai e finiscono
    /// con la partita giusta. Sull'ultimo tavolo una lettura su tre vede la briscola sbagliata.
    #[test]
    fn follows_whole_games_on_screen() {
        use crate::vision::synth::{Bg, Look};
        let d = Look::default();
        let looks = [d, Look { bg: Bg::Wood, card_h: 150.0, pile: 0.8, ..d }, Look { writing: true, logo: 0.5, ..d }];
        let mut failed = Vec::new();
        for (k, look) in looks.iter().enumerate() {
            for (seed, linger) in [(k as u64 * 2 + 1, false), (k as u64 * 2 + 2, true)] {
                match play_on_screen(seed, k % 2, *look, linger, k == 2) {
                    Ok((tricks, confirms)) => {
                        println!("tavolo {k} partita {seed}: {tricks} prese, {confirms} conferme");
                        if tricks != 20 {
                            failed.push(format!("tavolo {k} partita {seed}: {tricks} prese"));
                        }
                    }
                    Err(e) => failed.push(format!("tavolo {k} partita {seed}: {e}")),
                }
            }
        }
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    /// Più partite su più tavoli, stampate una per una.
    /// `cargo test --release -p brismaster banco_partite -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn banco_partite() {
        use crate::vision::synth::{Bg, Look};
        let d = Look::default();
        let looks = [
            Look { writing: true, logo: 0.8, ..d },
            Look { bg: Bg::Sea, card_h: 130.0, ..d },
            Look { bg: Bg::Sky, card_h: 140.0, pile: 0.8, cover: 0.5, writing: true, ..d },
            Look { bg: Bg::Felt([30, 60, 140]), card_h: 110.0, aspect: Some(0.68), fan: 6.0, ..d },
            Look { bg: Bg::Wood, card_h: 130.0, blur: true, logo: 0.4, ..d },
        ];
        let (mut ok, mut n) = (0, 0);
        for (k, look) in looks.iter().enumerate() {
            for seed in [100 + k as u64 * 2, 101 + k as u64 * 2] {
                n += 1;
                match play_on_screen(seed, k % 2, *look, seed % 2 == 0, k % 2 == 0) {
                    Ok((t, c)) => {
                        ok += (t == 20) as usize;
                        println!("tavolo {k} partita {seed}: {t} prese, {c} conferme");
                    }
                    Err(e) => println!("tavolo {k} partita {seed}: {e}"),
                }
            }
        }
        println!("== partite finite: {ok}/{n}");
    }

    /// La briscola letta in modo diverso una lettura sì e una no (sicura ma sbagliata) non fa
    /// ricominciare la partita, e la partita prosegue.
    #[test]
    fn a_flickering_trump_does_not_restart_the_game() {
        let mut a = Assistant::default();
        let c = |s, r| cards::make(s, r);
        let hand = vec![seen(c(3, 1), 4.0), seen(c(2, 6), 4.0), seen(c(1, 0), 4.0)];
        let mut scene = Scene { trump: Some(seen(c(0, 8), 4.0)), hand: hand.clone(), ..Default::default() };
        show(&mut a, &scene);
        a.set_first(OPP);
        show(&mut a, &scene);
        assert_eq!(a.pos.step(), Step::OppMove);
        let wrong = Scene { trump: Some(seen(c(2, 3), 4.0)), ..scene.clone() };
        for _ in 0..6 {
            a.sync(&wrong);
            a.sync(&scene);
        }
        assert_eq!(a.pos.trump_card, Some(c(0, 8)), "la partita è ricominciata");
        assert_eq!(a.pos.first, Some(OPP));
        // L'avversario gioca mentre la briscola continua a cambiare: la carta entra lo stesso.
        scene.opp_slot = Some(seen(c(3, 2), 4.0));
        let wrong = Scene { trump: Some(seen(c(2, 3), 4.0)), ..scene.clone() };
        for _ in 0..3 {
            a.sync(&scene);
            a.sync(&wrong);
        }
        assert_eq!(a.pos.table, Some(c(3, 2)));
        assert_eq!(a.pos.trump_card, Some(c(0, 8)));
    }

    /// Partite registrate dal tavolo di prova (una schermata ogni 350 ms, con la situazione vera):
    /// le schermate passano all'app una alla volta, come dal vivo. Stampa dove l'app si ferma o
    /// sbaglia. `PARTITE=cartella cargo test --release -p brismaster partite_registrate -- --ignored --nocapture`
    /// (VERBOSO=1 stampa ogni lettura).
    #[test]
    #[ignore]
    fn partite_registrate() {
        let root = std::path::PathBuf::from(std::env::var("PARTITE").expect("PARTITE=cartella"));
        let verbose = std::env::var("VERBOSO").is_ok();
        // NESSUN_UTENTE=1: l'utente non fa niente (non conferma, non scrive): la partita deve
        // seguirsi da sola.
        let user = std::env::var("NESSUN_UTENTE").is_err();
        // CATTURA=ms: simula la cattura dell'app (ogni 100 ms, analisi di ms per schermata);
        // SCHERMO=larghezza: il tavolo è una finestra di uno schermo così largo.
        let sim = std::env::var("CATTURA")
            .ok()
            .and_then(|v| v.parse::<u64>().ok());
        // ZOOM=fattore: il sito ingrandito o rimpicciolito.
        let zoom = std::env::var("ZOOM").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
        // AGGIORNA=ms: l'interfaccia raccoglie le letture a questi intervalli (app non in primo
        // piano); VECCHIA_UI=1: come la 0.5.10, tiene solo l'ultima lettura di ogni raccolta.
        let ui = std::env::var("AGGIORNA").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        let old_ui = std::env::var("VECCHIA_UI").is_ok();
        let desk = std::env::var("SCHERMO")
            .ok()
            .and_then(|v| v.parse::<usize>().ok());
        let by_name = |n: &str| (0..40u8).find(|&c| cards::name(c) == n);
        let mut dirs: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).filter(|p| p.join("verita.json").exists()).collect();
        dirs.sort();
        let (mut ok, mut n) = (0, 0);
        let mut perfect = 0usize;
        for dir in dirs {
            let log: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(dir.join("verita.json")).unwrap()).unwrap();
            // Ogni partita come letta da sola: niente memoria dell'analisi dalla partita prima.
            crate::vision::detect::forget_reading_state();
            let mut a = Assistant::default();
            let mut confirms = 0;
            let mut wrong = Vec::new();
            let mut before: Vec<Card> = Vec::new();
            // La mano vera della lettura dopo (l'app può essere avanti di un attimo: la briscola
            // dell'ultima pescata le arriva da sola).
            let hands: Vec<Vec<Card>> = log
                .iter()
                .map(|e| e["hand"].as_array().unwrap().iter().filter_map(|v| by_name(v.as_str()?)).collect())
                .collect();
            let mut opp_cards: Vec<Card> = Vec::new();
            let mut recent_cards: Vec<Vec<Card>> = Vec::new();
            let mut typed = 0;
            // Primo punto in cui l'app resta indietro (o avanti) sul mazzo vero per più di 6 letture.
            let (mut off, mut first_off): (u32, Option<String>) = (0, None);
            let mut recent: Vec<String> = Vec::new();
            let load = |idx: usize| -> Img {
                let f = log[idx]["frame"].as_str().unwrap();
                let file = [format!("{f}.png"), format!("{f}.jpg")]
                    .into_iter()
                    .map(|n| dir.join(n))
                    .find(|p| p.exists())
                    .unwrap();
                let mut img = crate::vision::capture::load_png(&file).unwrap();
                // Il sito ingrandito o rimpicciolito (zoom del browser o di Windows).
                if (zoom - 1.0).abs() > 0.01 {
                    let (w, h) = ((img.w as f32 * zoom) as usize, (img.h as f32 * zoom) as usize);
                    let mut z = Img::new(w, h);
                    for y in 0..h {
                        for x in 0..w {
                            let mut acc = [0u32; 3];
                            for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                                let p = img.sample((x as f32 + dx) / zoom - 0.5, (y as f32 + dy) / zoom - 0.5);
                                (0..3).for_each(|c| acc[c] += p[c] as u32);
                            }
                            z.px[y * w + x] = acc.map(|a| (a / 4) as u8);
                        }
                    }
                    img = z;
                }
                let Some(w) = desk.filter(|&w| w > img.w) else {
                    return img;
                };
                // Il tavolo in una finestra di uno schermo più grande, catturato tutto: sopra la
                // barra del browser, sotto la barra delle applicazioni.
                let h = (w * 9 / 16).max(img.h + 100);
                let mut out = Img::new(w, h);
                out.px.iter_mut().for_each(|p| *p = [38, 42, 50]);
                for y in 0..h {
                    for x in 0..w {
                        if (20..60).contains(&y) && (120..120 + img.w).contains(&x) {
                            out.px[y * w + x] = [222, 225, 230];
                        } else if y + 40 >= h {
                            out.px[y * w + x] = [20, 20, 24];
                        }
                    }
                }
                for y in 0..img.h.min(h - 100) {
                    let row = (y + 60) * w + 120;
                    out.px[row..row + img.w.min(w - 120)]
                        .copy_from_slice(&img.px[y * img.w..y * img.w + img.w.min(w - 120)]);
                }
                out
            };
            // Le schermate da analizzare, in ordine, con quante letture uguali in più.
            let seq: Vec<(usize, u64, Option<u64>)> = match sim {
                None => (0..log.len())
                    .map(|idx| {
                        // Registrazioni a ogni cambiamento: se lo schermo resta fermo l'app lo
                        // rilegge ogni 600 ms (una lettura uguale in più ogni 600 ms, al più 3).
                        let extra = match (log[idx]["t"].as_u64(), log.get(idx + 1).and_then(|n| n["t"].as_u64())) {
                            (Some(t), Some(next)) => (next.saturating_sub(t) / 600).min(3),
                            _ => 0,
                        };
                        (idx, extra, None)
                    })
                    .collect(),
                Some(cost) => {
                    // Come nell'app: si cattura ogni 100 ms, si tengono le schermate cambiate e
                    // l'analisi ne prende una alla volta, `cost` ms ciascuna.
                    let ts: Vec<u64> = log
                        .iter()
                        .enumerate()
                        .map(|(i, e)| e["t"].as_u64().unwrap_or(i as u64 * 350))
                        .collect();
                    let base = std::time::Instant::now();
                    let mut q: crate::vision::Frames<usize> =
                        crate::vision::Frames::new(std::time::Duration::from_millis(600));
                    let (mut out, mut busy, mut cur, mut tick) =
                        (Vec::new(), 0u64, (usize::MAX, None::<Img>), 0u64);
                    while tick <= ts.last().copied().unwrap_or(0) + 1500 {
                        let i = ts.partition_point(|&t| t <= tick).saturating_sub(1);
                        if cur.0 != i {
                            cur = (i, Some(load(i)));
                        }
                        q.offer_with(cur.1.clone().unwrap(), i, base + std::time::Duration::from_millis(tick));
                        if busy <= tick {
                            if let Some((_, i)) = q.next_with() {
                                out.push((i, 0, tick + cost));
                                busy = tick + cost;
                            }
                        }
                        tick += 100;
                    }
                    // (Le schermate ancora in coda alla fine si leggono comunque, come nell'app.)
                    while let Some((_, i)) = q.next_with() {
                        out.push((i, 0, busy + cost));
                        busy += cost;
                    }
                    // L'interfaccia raccoglie le letture ogni `ui` ms; la 0.5.10 teneva solo
                    // l'ultima di ogni raccolta.
                    if old_ui {
                        let ui = ui.max(1);
                        let mut kept: Vec<(usize, u64, u64)> = Vec::new();
                        for r in out {
                            if kept.last().is_some_and(|k| k.2 / ui == r.2 / ui) {
                                kept.pop();
                            }
                            kept.push(r);
                        }
                        out = kept;
                    }
                    out.into_iter().map(|(i, e, t)| (i, e, Some(t))).collect()
                }
            };
            let mut card_px = 0u32;
            for &(idx, extra, at) in &seq {
                let e = &log[idx];
                let f = e["frame"].as_str().unwrap();
                // Come nell'app: le schermate grandi si rimpiccioliscono se le carte restano grandi.
                let img = load(idx);
                let k = crate::vision::shrink_factor(img.w, card_px);
                let img = crate::vision::shrink_by(img, k);
                let truth_hand: Vec<Card> = e["hand"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|v| by_name(v.as_str()?))
                    .collect();
                let truth_trick: Vec<(Card, bool)> = e["trick"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|t| Some((by_name(t["card"].as_str()?)?, t["owner"].as_str()? == "Tu")))
                    .collect();
                let trump = by_name(e["trump"].as_str().unwrap()).unwrap();
                let scene = read_briscola(&img, &a.vision.memory);
                card_px = (crate::vision::detect::last_card_l() * k as f32).round() as u32;
                if scene.is_none() {
                    a.lost_table();
                }
                if let Some(scene) = &scene {
                    let t = at.unwrap_or(e["t"].as_u64().unwrap_or(idx as u64 * 350));
                    a.vision.now_ms = Some(t);
                    a.sync(scene);
                    for k in 0..extra {
                        a.vision.now_ms = Some(t + 600 * (k + 1));
                        a.sync(scene);
                    }
                }
                // Le carte dell'avversario viste davvero finora (per scriverle quando l'app le chiede).
                for t in &truth_trick {
                    if !t.1 && !opp_cards.contains(&t.0) {
                        opp_cards.push(t.0);
                    }
                }
                if user && a.vision.status.starts_with("Non trovo la briscola") && a.pos.step() == Step::Trump && a.enter(trump) {
                    typed += 1;
                    a.vision.status.clear();
                }
                if user && a.vision.status.starts_with("Non ho visto la carta giocata") && a.pos.step() == Step::OppMove {
                    let gone = |c: Card| a.pos.played.contains(c) || a.pos.table == Some(c);
                    if let Some(&c) = opp_cards.iter().find(|&&c| !gone(c)) {
                        if a.enter(c) {
                            typed += 1;
                            if verbose {
                                println!(
                                    "{f} SCRITTA a mano la carta dell'avversario {}",
                                    cards::short(c)
                                );
                            }
                            a.vision.status.clear();
                        }
                    }
                }
                // L'app chiede di scrivere la pescata o la mia giocata: l'utente la scrive.
                if user && a.vision.status.starts_with("Non riconosco la carta") {
                    let allowed = a.pos.allowed();
                    let right = match a.pos.step() {
                        Step::Draw(_) => hands
                            .get(idx + 1)
                            .into_iter()
                            .chain([&truth_hand])
                            .flatten()
                            .copied()
                            .find(|&h| allowed.contains(h) && !a.pos.hand.contains(h)),
                        Step::MyMove => truth_trick.iter().find(|t| t.1 && allowed.contains(t.0)).map(|t| t.0),
                        _ => None,
                    };
                    if let Some(c) = right {
                        if a.enter(c) {
                            typed += 1;
                            if verbose {
                                println!("{f} SCRITTA a mano su richiesta: {}", cards::short(c));
                            }
                            a.vision.status.clear();
                        }
                    }
                }
                // L'utente conferma le carte che l'app chiede, se sono una di quelle sullo schermo.
                // Le carte viste sullo schermo nelle ultime letture (l'utente se le ricorda).
                recent_cards.push(truth_hand.iter().copied().chain(truth_trick.iter().map(|t| t.0)).collect::<Vec<Card>>());
                if recent_cards.len() > 6 {
                    recent_cards.remove(0);
                }
                if let Some(c) = a.vision.confirm.as_ref().filter(|_| user) {
                    let visible = c.card == trump || recent_cards.iter().any(|r| r.contains(&c.card));
                    if visible {
                        confirms += 1;
                        a.accept_vision();
                    } else {
                        // Proposta sbagliata: l'utente scrive la carta giusta.
                        wrong.push(format!("{f}: chiede {}", cards::name(c.card)));
                        let step = c.step;
                        let (img, vis) = (c.img.clone(), c.vis);
                        let card = c.card;
                        a.vision.confirm = None;
                        a.reject(step, card);
                        let allowed = a.pos.allowed();
                        let right = match step {
                            Step::Trump => Some(trump),
                            Step::Draw(_) => truth_hand.iter().copied().find(|&h| allowed.contains(h)),
                            Step::OppMove => truth_trick.iter().find(|t| !t.1 && allowed.contains(t.0)).map(|t| t.0),
                            Step::MyMove => truth_trick.iter().find(|t| t.1 && allowed.contains(t.0)).map(|t| t.0),
                            _ => None,
                        };
                        if let Some(r) = right {
                            // Come nell'app: la carta scritta a mano si impara dall'immagine proposta.
                            if step == a.pos.step() && a.enter(r) {
                                a.vision.memory.lock().unwrap().remember(&img, vis, r);
                                typed += 1;
                                if verbose {
                                    println!(
                                        "{f} SCRITTA a mano al posto della proposta: {}",
                                        cards::short(r)
                                    );
                                }
                            }
                        }
                    }
                }
                let truth_deck: u8 = e["deck"].as_str().unwrap_or("").trim_start_matches("Mazzo: ").parse().unwrap_or(0);
                {
                    let show = |v: &Option<CardView>| {
                        v.as_ref()
                            .and_then(|v| v.best(CardSet::FULL))
                            .map(|(c, m)| format!("{}({m:.1})", cards::short(c)))
                            .unwrap_or("-".into())
                    };
                    let line = scene
                        .as_ref()
                        .map(|s| {
                            format!(
                                "A {} T {} M [{}]",
                                show(&s.opp_slot),
                                show(&s.my_slot),
                                s.hand.iter().map(|v| show(&Some(v.clone()))).collect::<Vec<_>>().join(" ")
                            )
                        })
                        .unwrap_or_default();
                    recent.push(format!("{f} vero mazzo {truth_deck} M [{}] T [{}] | letto {line} | app {:?} mazzo {} mano [{}] tavolo {:?}",
                        truth_hand.iter().map(|&c| cards::short(c)).collect::<Vec<_>>().join(" "),
                        truth_trick.iter().map(|&(c, me)| format!("{}{}", cards::short(c), if me { "(tu)" } else { "" })).collect::<Vec<_>>().join(" "),
                        a.pos.step(), a.pos.deck_len, a.pos.hand.iter().map(cards::short).collect::<Vec<_>>().join(" "), a.pos.table.map(cards::short)));
                    if recent.len() > 10 { recent.remove(0); }
                }
                off = if a.pos.deck_len != truth_deck && a.pos.step() != Step::Trump { off + 1 } else { 0 };
                if off == 7 && first_off.is_none() {
                    first_off = Some(recent.join("\n    "));
                }
                if verbose {
                    let show = |v: &Option<CardView>| {
                        v.as_ref()
                            .and_then(|v| v.best(CardSet::FULL))
                            .map(|(c, m)| format!("{}({m:.1})", cards::short(c)))
                            .unwrap_or("-".into())
                    };
                    let line = scene.as_ref().map(|s| {
                        format!(
                            "B {} A {} T {} M [{}]",
                            show(&s.trump),
                            show(&s.opp_slot),
                            show(&s.my_slot),
                            s.hand.iter().map(|v| show(&Some(v.clone()))).collect::<Vec<_>>().join(" ")
                        )
                    });
                    println!(
                        "{f} vero: {} M [{}] T [{}] | letto: {} | app: {:?} mazzo {} prese {} mano [{}] tavolo {:?}",
                        e["deck"].as_str().unwrap_or(""),
                        truth_hand.iter().map(|&c| cards::short(c)).collect::<Vec<_>>().join(" "),
                        truth_trick.iter().map(|&(c, me)| format!("{}{}", cards::short(c), if me { "(tu)" } else { "" })).collect::<Vec<_>>().join(" "),
                        line.unwrap_or("niente".into()),
                        a.pos.step(),
                        a.pos.deck_len,
                        a.pos.tricks,
                        a.pos.hand.iter().map(cards::short).collect::<Vec<_>>().join(" "),
                        a.pos.table.map(cards::short)
                    );
                }
                let prev_hand = std::mem::replace(&mut before, truth_hand.clone());
                before.extend(prev_hand);
                // La mano dell'app deve essere (un sottoinsieme di) quella vera.
                for c in a.pos.hand.iter() {
                    // (la situazione vera si legge un attimo dopo la schermata: vale anche quella prima)
                    // (La briscola dell'ultima pescata l'app la sa dalle regole prima che il sito la
                    // faccia arrivare in mano: vale se poi in mano ci arriva davvero.)
                    let next = hands.get(idx + 1).is_some_and(|h| h.contains(&c))
                        || (Some(c) == a.pos.trump_card && hands[idx..].iter().any(|h| h.contains(&c)));
                    let held = truth_hand.contains(&c) || before.contains(&c) || next || truth_trick.iter().any(|t| t.0 == c && t.1);
                    if !held && a.pos.step() != Step::Over {
                        let msg = format!("{f}: l'app crede di avere {}", cards::name(c));
                        if !wrong.contains(&msg) && wrong.len() < 5 {
                            wrong.push(msg);
                        }
                    }
                }
            }
            n += 1;
            let _ = &mut before;
            // Controllo severo: tutta la partita giusta, carta per carta, senza aiuti.
            let mut real: Vec<(Card, bool, Card)> = Vec::new();
            for e in &log {
                let t: Vec<(Card, bool)> = e["trick"].as_array().unwrap().iter()
                    .filter_map(|t| Some((by_name(t["card"].as_str()?)?, t["owner"].as_str()? == "Tu"))).collect();
                if let [a, b] = t.as_slice() {
                    if real.last() != Some(&(a.0, a.1, b.0)) { real.push((a.0, a.1, b.0)); }
                }
            }
            let real_trump = by_name(log[0]["trump"].as_str().unwrap());
            let mut why: Vec<String> = Vec::new();
            if a.pos.step() != Step::Over { why.push(format!("ferma a {} prese", a.pos.tricks)); }
            if a.pos.trump_card != real_trump { why.push("briscola sbagliata".into()); }
            if !a.vision.gaps.is_empty() { why.push(format!("{} ripartenze con prese non viste", a.vision.gaps.len())); }
            if confirms + typed > 0 { why.push(format!("{} interventi dell'utente", confirms + typed)); }
            if !wrong.is_empty() { why.push("carte in mano sbagliate".into()); }
            if real.len() != 20 { why.push(format!("registrazione con {} prese", real.len())); }
            for (i, t) in a.tricks.iter().enumerate() {
                let got = (t.lead, t.leader == ME, t.follow);
                if real.get(i) != Some(&got) {
                    let show = |x: &(Card, bool, Card)| format!("{}{} {}", cards::short(x.0), if x.1 { "(tu)" } else { "" }, cards::short(x.2));
                    why.push(format!("presa {}: app {} vera {}", i + 1, show(&got), real.get(i).map(show).unwrap_or("-".into())));
                    break;
                }
            }
            if a.tricks.len() != 20 && a.pos.step() == Step::Over { why.push(format!("{} prese registrate", a.tricks.len())); }
            perfect += why.is_empty() as usize;
            println!("  severo: {}", if why.is_empty() { "PERFETTA".into() } else { why.join("; ") });
            // (Le proposte sbagliate corrette dall'utente non contano come errori della partita.)
            let done = a.pos.tricks == 20 && wrong.iter().all(|w| w.contains("chiede"));
            ok += done as usize;
            println!(
                "{}: {} prese su 20, passo {:?}, {} conferme, {} scritte a mano{}{}",
                dir.file_name().unwrap().to_string_lossy(),
                a.pos.tricks,
                a.pos.step(),
                confirms,
                typed,
                if wrong.is_empty() { String::new() } else { format!(", errori: {}", wrong.join("; ")) },
                if a.vision.status.is_empty() { String::new() } else { format!(" [{}]", a.vision.status) }
            );
            if let Some(t) = first_off {
                println!("  perde il passo qui:\n    {t}");
            }
        }
        println!("== partite seguite fino in fondo: {ok}/{n}");
        println!("== partite perfette (tutte le carte giuste, nessun intervento): {perfect}/{n}");
    }
}

