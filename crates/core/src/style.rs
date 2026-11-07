//! Giocatori "umani" costruiti da manopole: memoria, aggressività, gusto per i carichi, errori...
//!
//! Ogni personaggio sceglie la carta con il punteggio più alto secondo le sue manopole, sbaglia
//! con una probabilità che può crescere quando è sotto (tilt) o verso la fine (fretta) e, se è
//! bravo nel finale, a mazzo vuoto gioca in modo perfetto. Servono a mettere il motore contro
//! combinazioni di difetti che nessuno stile scritto a mano prevede.
//!
//! Servono anche a riconoscere lo stile di un avversario vero (vedi [`Adapt`]).

use crate::ai::{Player, Solver};
use crate::game::Game;
use crate::net::Net;
use rand::seq::SliceRandom;
use crate::cards::*;
use crate::Observation;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// Le 12 manopole, tutte tra 0 e 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knobs {
    /// Quante carte uscite ricorda (ognuna con questa probabilità, sempre le stesse).
    pub memoria: f64,
    /// Voglia di prendere la presa anche quando vale poco.
    pub aggressivita: f64,
    /// Quanto gli costa usare una briscola.
    pub risparmio: f64,
    /// Quanto valgono per lui i punti presi (prendere i carichi con la briscola).
    pub caccia: f64,
    /// Voglia di aprire con Assi e Tre per fare punti subito.
    pub apre_carichi: f64,
    /// Quanto evita di regalare punti.
    pub protegge: f64,
    /// Voglia di aprire di briscola.
    pub apre_briscola: f64,
    /// Tiene l'Asso e il Tre di briscola fino alla fine.
    pub tiene_assi: f64,
    /// Mosse a caso.
    pub errori: f64,
    /// Probabilità di giocare il finale (mazzo vuoto) in modo perfetto.
    pub finale: f64,
    /// Errori in più quando è sotto di 10 punti o più.
    pub tilt: f64,
    /// Errori in più man mano che la partita va avanti (il tempo stringe).
    pub fretta: f64,
}

impl Knobs {
    /// Giocatore medio: niente di estremo, pochi errori.
    pub const MEDIO: Knobs = Knobs {
        memoria: 0.5,
        aggressivita: 0.5,
        risparmio: 0.5,
        caccia: 0.5,
        apre_carichi: 0.2,
        protegge: 0.6,
        apre_briscola: 0.2,
        tiene_assi: 0.4,
        errori: 0.03,
        finale: 0.5,
        tilt: 0.0,
        fretta: 0.0,
    };

    /// Manopole estratte dal seme: le stesse ogni volta per lo stesso numero.
    pub fn random(seed: u64) -> Knobs {
        let mut r = SmallRng::seed_from_u64(seed ^ 0x5045_5253_4F4E_4121);
        let mut u = || r.gen::<f64>();
        Knobs {
            memoria: u(),
            aggressivita: u(),
            risparmio: u(),
            caccia: u(),
            apre_carichi: u() * 0.8,
            protegge: u(),
            apre_briscola: u() * 0.7,
            tiene_assi: u(),
            errori: u() * u() * 0.3,
            finale: u(),
            tilt: u() * u(),
            fretta: u() * u(),
        }
    }
}

/// I 20 stili umani con nome (in aggiunta ai 10 stili scritti a mano del banco di prova).
pub const STYLES: &[(&str, &str, Knobs)] = &[
    ("liscio", "Liscia sempre: non prende quasi mai, butta la carta più piccola", Knobs { aggressivita: 0.0, caccia: 0.15, protegge: 1.0, ..Knobs::MEDIO }),
    ("caricatore", "Carica sempre: apre con Assi e Tre e non protegge i punti", Knobs { apre_carichi: 1.0, protegge: 0.0, ..Knobs::MEDIO }),
    ("asso-di-briscola", "Tiene l'Asso e il Tre di briscola fino alla fine, a ogni costo", Knobs { tiene_assi: 1.0, risparmio: 0.8, ..Knobs::MEDIO }),
    ("conta-briscole", "Ricorda bene le carte uscite ma gioca senza fantasia", Knobs { memoria: 0.9, finale: 0.3, ..Knobs::MEDIO }),
    ("tilt", "Va in tilt: quando è sotto comincia a sbagliare", Knobs { tilt: 1.0, ..Knobs::MEDIO }),
    ("finale-alla-cieca", "Gioca il finale senza contare", Knobs { finale: 0.0, memoria: 0.3, ..Knobs::MEDIO }),
    ("memoria-corta", "Dimentica quasi tutte le carte uscite", Knobs { memoria: 0.1, finale: 0.2, ..Knobs::MEDIO }),
    ("memoria-perfetta", "Ricorda tutto e gioca il finale perfetto", Knobs { memoria: 1.0, finale: 1.0, errori: 0.0, ..Knobs::MEDIO }),
    ("prende-tutto", "Vuole prendere ogni presa, briscole comprese", Knobs { aggressivita: 1.0, caccia: 1.0, risparmio: 0.0, ..Knobs::MEDIO }),
    ("avaro", "Non molla una briscola nemmeno sotto tortura", Knobs { risparmio: 1.0, caccia: 0.3, ..Knobs::MEDIO }),
    ("apre-di-briscola", "Apre di briscola per stanare quelle avversarie", Knobs { apre_briscola: 1.0, risparmio: 0.2, ..Knobs::MEDIO }),
    ("domenica", "Giocatore della domenica: distratto, ricorda poco", Knobs { errori: 0.15, memoria: 0.3, finale: 0.3, ..Knobs::MEDIO }),
    ("circolo", "Esperto da circolo: ricorda quasi tutto, sbaglia di rado", Knobs { memoria: 0.9, finale: 0.9, errori: 0.02, protegge: 0.8, ..Knobs::MEDIO }),
    ("frettoloso", "Il tempo stringe: verso la fine sbaglia sempre di più", Knobs { fretta: 1.0, ..Knobs::MEDIO }),
    ("cacciatore", "Caccia i carichi: briscola appena vede punti in tavola", Knobs { caccia: 1.0, risparmio: 0.25, ..Knobs::MEDIO }),
    ("prudente", "Prudente: protegge i punti, prende solo quando costa poco", Knobs { protegge: 1.0, risparmio: 0.7, aggressivita: 0.2, ..Knobs::MEDIO }),
    ("spavaldo", "Spavaldo: apre forte e prende tutto quello che può", Knobs { apre_carichi: 0.8, aggressivita: 0.8, apre_briscola: 0.5, ..Knobs::MEDIO }),
    ("nonno", "Il nonno: lento e attento, tiene le briscole, sbaglia poco", Knobs { memoria: 0.5, finale: 0.6, protegge: 0.8, risparmio: 0.9, errori: 0.05, ..Knobs::MEDIO }),
    ("impulsivo", "Impulsivo: prende tutto, sbaglia, ha fretta", Knobs { errori: 0.1, aggressivita: 1.0, fretta: 0.5, ..Knobs::MEDIO }),
    ("calcolatore-distratto", "Calcolatore distratto: ricorda tutto ma una mossa su otto è a caso", Knobs { memoria: 1.0, finale: 1.0, errori: 0.12, ..Knobs::MEDIO }),
];

pub struct Persona {
    pub name: String,
    pub k: Knobs,
    /// Le carte che questo personaggio ricorda quando escono (sempre le stesse).
    remembers: CardSet,
}

impl Persona {
    pub fn new(name: String, k: Knobs, seed: u64) -> Persona {
        let mut r = SmallRng::seed_from_u64(seed ^ 0x4D45_4D4F_5249_4121);
        let remembers = (0..NUM_CARDS as u8).filter(|_| r.gen::<f64>() < k.memoria).collect();
        Persona { name, k, remembers }
    }

    pub fn style(name: &str) -> Option<Persona> {
        let (n, how, k) = STYLES.iter().find(|s| s.0 == name)?;
        let seed = n.bytes().fold(0u64, |h, b| h.wrapping_mul(131).wrapping_add(b as u64));
        Some(Persona::new(format!("Stile «{n}» ({})", how.to_lowercase()), *k, seed))
    }

    pub fn numbered(n: u64) -> Persona {
        Persona::new(format!("Personaggio n. {n}"), Knobs::random(n), n)
    }

    fn error_rate(&self, obs: &Observation) -> f64 {
        let k = &self.k;
        let me = obs.me as usize;
        let behind = obs.points[1 - me] as i32 - obs.points[me] as i32 >= 10;
        let progress = obs.played.len() as f64 / NUM_CARDS as f64;
        (k.errori + if behind { 0.3 * k.tilt } else { 0.0 } + 0.3 * k.fretta * progress).min(0.9)
    }

    /// Quanto vale tenere `c` invece di giocarla, in punti: le briscole valgono molto
    /// (di più per chi le risparmia), le altre carte poco.
    fn keep_value(&self, c: Card, obs: &Observation) -> f64 {
        let k = &self.k;
        let s = strength(c) as f64;
        if suit(c) != obs.trump {
            return 0.3 * s;
        }
        let mut v = (0.5 + k.risparmio) * (8.0 + s);
        if points(c) >= 10 {
            v *= 1.0 + 2.0 * k.tiene_assi;
        }
        if obs.deck_len == 0 { v * 0.3 } else { v }
    }

    /// Carico non di briscola che nessuna carta rimasta (secondo la sua memoria) può prendere.
    fn safe_load(&self, c: Card, obs: &Observation) -> bool {
        let mut gone = CardSet(obs.played.0 & self.remembers.0).union(obs.hand);
        if obs.deck_len > 0 {
            gone.insert(obs.trump_card);
        }
        (0..10u8).all(|r| {
            let hi = make(suit(c), r);
            gone.contains(make(obs.trump, r)) && (strength(hi) <= strength(c) || gone.contains(hi))
        })
    }

    /// Punteggio della carta, in punti: quelli presi o regalati (pesati dallo stile),
    /// meno il valore di tenerla.
    fn utility(&self, c: Card, obs: &Observation) -> f64 {
        let k = &self.k;
        let p = points(c) as f64;
        let keep = self.keep_value(c, obs);
        match obs.table {
            Some(lead) => {
                let pts = points(lead) as f64 + p;
                if beats(c, lead, obs.trump) {
                    (0.5 + k.caccia) * pts + 3.0 * k.aggressivita - keep
                } else {
                    -(0.5 + k.protegge) * pts - keep
                }
            }
            None => {
                if suit(c) == obs.trump {
                    4.0 * k.apre_briscola - 0.5 * p - keep
                } else if p >= 10.0 && self.safe_load(c, obs) {
                    -keep
                } else {
                    (1.2 * k.apre_carichi - 0.5 - k.protegge) * p - keep
                }
            }
        }
    }
}

impl Player for Persona {
    fn name(&self) -> String {
        self.name.clone()
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        let hand = obs.hand.to_vec();
        if hand.len() == 1 {
            return hand[0];
        }
        if rng.gen::<f64>() < self.error_rate(obs) {
            return hand[rng.gen_range(0..hand.len())];
        }
        if obs.deck_len == 0 && rng.gen::<f64>() < self.k.finale {
            // A mazzo vuoto le carte avversarie sono quelle non uscite: informazione completa.
            let world = obs.determinize(rng);
            let mut solver = Solver::default();
            return *hand
                .iter()
                .max_by_key(|&&c| {
                    let mut n = world;
                    n.play(c);
                    solver.solve(&n, obs.me)
                })
                .unwrap();
        }
        // Un pizzico di rumore rompe i pareggi senza cambiare lo stile.
        *hand
            .iter()
            .max_by(|&&a, &&b| {
                let ua = self.utility(a, obs) + rng.gen::<f64>() * 0.5;
                let ub = self.utility(b, obs) + rng.gen::<f64>() * 0.5;
                ua.partial_cmp(&ub).unwrap()
            })
            .unwrap()
    }
}

/// Riconoscimento dello stile durante la partita: un insieme di modelli umani (personaggi con
/// manopole a caso, semi diversi da quelli del banco di prova) più il modello «forte» (la rete
/// di politica), pesati in modo bayesiano da quanto spiegano le carte giocate dall'avversario.
/// Le carte avversarie che non conosco si estraggono a caso: niente di nascosto viene usato.
pub struct Adapt {
    /// I modelli umani (l'ultimo modello, quello forte, è la rete di politica).
    pub cands: Vec<Persona>,
    /// Pesi all'inizio di ogni partita: quanto imparato dallo storico (vuoto = niente storico).
    history: Vec<f64>,
    /// Quanto fidarsi del modello riconosciuto (1 = del tutto). Di base ½: con fiducia piena un
    /// modello solo simile all'avversario vero costa punti (−1,8 contro «caricatore»).
    pub scale: f64,
    /// Logaritmo della verosimiglianza di ogni modello; l'ultimo è quello forte.
    logw: Vec<f64>,
    rng: SmallRng,
}

/// Primo seme dei modelli: lontano dai personaggi del banco (`persona:0..200`).
pub const ADAPT_SEED: u64 = 1_000_000;
/// Mani avversarie estratte e campioni per modello a ogni mossa avversaria.
const ADAPT_HANDS: usize = 6;
const ADAPT_SAMPLES: usize = 8;

impl Adapt {
    pub fn new(n: usize) -> Adapt {
        let cands: Vec<Persona> = (0..n as u64)
            .map(|i| Persona::new(format!("modello {i}"), Knobs::random(ADAPT_SEED + i), ADAPT_SEED + i))
            .collect();
        Adapt { logw: vec![0.0; n + 1], history: Vec::new(), scale: 0.5, cands, rng: SmallRng::seed_from_u64(ADAPT_SEED) }
    }

    /// Nuova partita: si riparte da quanto imparato nelle partite precedenti (lo storico).
    pub fn reset(&mut self) {
        self.logw.iter_mut().for_each(|w| *w = 0.0);
        if !self.history.is_empty() {
            self.logw.copy_from_slice(&self.history);
        }
    }

    /// Quanto imparato finora diventa lo storico delle partite successive.
    pub fn remember(&mut self) {
        self.history = self.logw.clone();
    }

    /// Lo storico, da salvare (es. uno per ogni amico) e rimettere con `set_history`.
    pub fn history(&self) -> &[f64] {
        &self.history
    }

    pub fn set_history(&mut self, h: Vec<f64>) {
        if h.len() == self.logw.len() {
            self.history = h;
            self.reset();
        }
    }

    /// Probabilità di ogni modello (l'ultima è quella del modello forte, a priori ½).
    pub fn posterior(&self) -> Vec<f64> {
        let n = self.cands.len() as f64;
        let prior = |k: usize| if k == self.cands.len() { 0.5f64.ln() } else { (0.5 / n).ln() };
        let lw: Vec<f64> = self.logw.iter().enumerate().map(|(k, w)| w + prior(k)).collect();
        let max = lw.iter().cloned().fold(f64::MIN, f64::max);
        let e: Vec<f64> = lw.iter().map(|w| (w - max).exp()).collect();
        let sum: f64 = e.iter().sum();
        e.into_iter().map(|x| x / sum).collect()
    }

    /// L'avversario (che deve muovere in `g`) gioca `c`; `me` sono io.
    pub fn update(&mut self, g: &Game, me: u8, c: Card, policy: &Net) {
        let opp = 1 - me;
        let n = g.hands[opp as usize].len() as usize;
        if n < 2 || g.deck_len == 0 {
            return;
        }
        let mine = Observation::of(g, me);
        // Carte certe nella mano avversaria: quella giocata e la briscola scoperta, se l'ha pescata lui.
        let mut known = CardSet::default();
        known.insert(c);
        if g.trump_card_owner == Some(opp) && !mine.played.contains(g.trump_card) && g.table != Some(g.trump_card) {
            known.insert(g.trump_card);
        }
        let pool: Vec<Card> = mine.unknown().minus(known).to_vec();
        let need = n.saturating_sub(known.len() as usize);
        let mut sums = vec![0.0f64; self.logw.len()];
        for _ in 0..ADAPT_HANDS {
            let mut hand = known;
            for &x in pool.choose_multiple(&mut self.rng, need) {
                hand.insert(x);
            }
            let mut w = *g;
            w.hands[opp as usize] = hand;
            let obs = Observation::of(&w, opp);
            let mut pr = [0f32; 3];
            let k = policy.policy(&obs, 1.0, &mut pr);
            let at = obs.hand.iter().position(|x| x == c).unwrap();
            sums[self.cands.len()] += pr[..k][at] as f64;
            for (i, m) in self.cands.iter_mut().enumerate() {
                let hits = (0..ADAPT_SAMPLES).filter(|_| m.choose(&obs, &mut self.rng) == c).count();
                sums[i] += hits as f64 / ADAPT_SAMPLES as f64;
            }
        }
        for (w, s) in self.logw.iter_mut().zip(sums) {
            *w += (0.9 * s / ADAPT_HANDS as f64 + 0.1 / n as f64).ln();
        }
    }
}
