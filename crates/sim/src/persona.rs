//! Giocatori "umani" costruiti da manopole: memoria, aggressività, gusto per i carichi, errori...
//!
//! Ogni personaggio sceglie la carta con il punteggio più alto secondo le sue manopole, sbaglia
//! con una probabilità che può crescere quando è sotto (tilt) o verso la fine (fretta) e, se è
//! bravo nel finale, a mazzo vuoto gioca in modo perfetto. Servono a mettere il motore contro
//! combinazioni di difetti che nessuno stile scritto a mano prevede.
//!
//! - `stile:NOME`: uno dei 20 stili con nome di [`STYLES`];
//! - `persona:N`: il personaggio numero N, con le manopole estratte a caso dal seme N.

use briscola_core::ai::{Player, Solver};
use briscola_core::cards::*;
use briscola_core::Observation;
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

/// Persone vere sotto pressione: il giocatore `base` sbaglia sempre di più verso la fine
/// della partita (fino a `error` all'ultima presa).
pub struct Clock {
    pub base: Box<dyn Player>,
    pub error: f64,
}

impl Player for Clock {
    fn name(&self) -> String {
        format!("{} · il tempo stringe (fino a {:.0}% di errori)", self.base.name(), self.error * 100.0)
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        let progress = obs.played.len() as f64 / NUM_CARDS as f64;
        if obs.hand.len() > 1 && rng.gen_bool((self.error * progress).min(1.0)) {
            let h = obs.hand.to_vec();
            return h[rng.gen_range(0..h.len())];
        }
        self.base.choose(obs, rng)
    }
}

/// Tilt: il giocatore `base` sbaglia con probabilità `error` quando è sotto di 10 punti o più.
pub struct Tilt {
    pub base: Box<dyn Player>,
    pub error: f64,
}

impl Player for Tilt {
    fn name(&self) -> String {
        format!("{} · va in tilt quando è sotto ({:.0}% di errori)", self.base.name(), self.error * 100.0)
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        let me = obs.me as usize;
        let behind = obs.points[1 - me] as i32 - obs.points[me] as i32 >= 10;
        if behind && obs.hand.len() > 1 && rng.gen_bool(self.error) {
            let h = obs.hand.to_vec();
            return h[rng.gen_range(0..h.len())];
        }
        self.base.choose(obs, rng)
    }
}

/// Camaleonte: cambia stile ogni 4 prese, scegliendo a caso tra i giocatori dati.
/// Mette alla prova la deduzione delle carte, che non può contare su uno stile fisso.
pub struct Chameleon {
    pub n: u64,
    pub styles: Vec<Box<dyn Player>>,
    current: usize,
    last_switch: u32,
}

impl Chameleon {
    pub fn new(n: u64, styles: Vec<Box<dyn Player>>) -> Chameleon {
        Chameleon { n, styles, current: 0, last_switch: u32::MAX }
    }
}

impl Player for Chameleon {
    fn name(&self) -> String {
        format!("Camaleonte n. {}: cambia stile ogni 4 prese tra {} stili", self.n, self.styles.len())
    }
    fn choose(&mut self, obs: &Observation, rng: &mut SmallRng) -> Card {
        let tricks = obs.played.len() / 2;
        let block = tricks / 4;
        if self.last_switch != block {
            self.last_switch = block;
            self.current = rng.gen_range(0..self.styles.len());
        }
        self.styles[self.current].choose(obs, rng)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use briscola_core::{play_game, Game, HeuristicPlayer};

    #[test]
    fn personas_play_legal_and_differ() {
        let mut rng = SmallRng::seed_from_u64(9);
        assert_ne!(Knobs::random(1), Knobs::random(2));
        assert_eq!(Knobs::random(7), Knobs::random(7));
        let mut players: Vec<Box<dyn Player>> = STYLES.iter().map(|s| Box::new(Persona::style(s.0).unwrap()) as Box<dyn Player>).collect();
        players.extend((0..10).map(|n| Box::new(Persona::numbered(n)) as Box<dyn Player>));
        players.push(Box::new(Clock { base: Box::new(HeuristicPlayer), error: 0.5 }));
        players.push(Box::new(Tilt { base: Box::new(HeuristicPlayer), error: 0.5 }));
        players.push(Box::new(Chameleon::new(1, vec![Box::new(HeuristicPlayer), Box::new(Persona::numbered(3))])));
        for p in players.iter_mut() {
            for first in 0..2 {
                let g = Game::new(&mut rng, first);
                let mut h = HeuristicPlayer;
                let pts = play_game(g, &mut [p.as_mut(), &mut h], &mut rng);
                assert_eq!(pts[0] as u32 + pts[1] as u32, 120);
            }
        }
    }

    #[test]
    fn perfect_memory_style_beats_short_memory() {
        // Una sanità di base delle manopole: chi ricorda tutto batte chi dimentica.
        let mut rng = SmallRng::seed_from_u64(3);
        let (mut a, mut b) = (Persona::style("memoria-perfetta").unwrap(), Persona::style("domenica").unwrap());
        let mut total = 0u32;
        for i in 0..200 {
            let g = Game::new(&mut rng, (i % 2) as u8);
            total += play_game(g, &mut [&mut a, &mut b], &mut rng)[0] as u32;
        }
        assert!(total > 200 * 60, "media {}", total as f64 / 200.0);
    }
}
