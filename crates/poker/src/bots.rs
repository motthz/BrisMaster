//! Giocatori a regole per i test: gli stili che si incontrano ai tavoli.
//!
//! Preflop decidono con la classifica delle mani iniziali (percentile della classe),
//! dopo il flop con l'equità contro una mano a caso e il prezzo da pagare.

use crate::cards::preflop_class;
use crate::equity::{equity_vs_random, preflop_percentile};
use crate::game::{Action, BB};
use crate::{Agent, View};
use rand::rngs::SmallRng;
use rand::Rng;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Style {
    Normal,
    Station,
    Maniac,
    Shover,
    Random,
}

#[derive(Clone, Debug)]
pub struct RuleBot {
    pub label: String,
    pub style: Style,
    /// Preflop, sul percentile della mano: rilancia per primo, paga (o entra pagando), rilancia sopra un rilancio.
    pub open: f64,
    pub call: f64,
    pub reraise: f64,
    /// Misura dell'apertura e delle puntate dopo il flop, in frazioni di piatto.
    pub open_size: f64,
    pub bet_size: f64,
    /// Dopo il flop, sull'equità: punta, paga se supera il prezzo di questo margine, rilancia.
    pub bet: f64,
    pub margin: f64,
    pub raise: f64,
    pub bluff: f64,
}

impl RuleBot {
    fn base(label: &str) -> RuleBot {
        RuleBot {
            label: label.into(),
            style: Style::Normal,
            open: 0.5,
            call: 0.6,
            reraise: 0.88,
            open_size: 0.75,
            bet_size: 0.66,
            bet: 0.62,
            margin: 0.12,
            raise: 0.85,
            bluff: 0.1,
        }
    }

    pub fn by_name(name: &str) -> Option<RuleBot> {
        let mut b = RuleBot::base(name);
        match name {
            "random" => b.style = Style::Random,
            "station" => b.style = Style::Station,
            "maniac" => b.style = Style::Maniac,
            "shover" => {
                b.style = Style::Shover;
                b.open = 0.7;
            }
            "rock" => {
                (b.open, b.call, b.reraise) = (0.88, 0.8, 0.96);
                (b.bet, b.margin, b.raise, b.bluff, b.bet_size) = (0.8, 0.25, 0.92, 0.0, 0.75);
            }
            "tag" => {}
            "lag" => {
                (b.open, b.call, b.reraise) = (0.2, 0.4, 0.75);
                (b.bet, b.margin, b.raise, b.bluff, b.bet_size) = (0.5, 0.03, 0.78, 0.35, 0.75);
            }
            "passive" => {
                (b.open, b.call, b.reraise) = (0.99, 0.2, 0.995);
                (b.bet, b.margin, b.raise, b.bluff) = (0.9, 0.0, 0.97, 0.0);
            }
            "equity" => {
                (b.open, b.call, b.reraise, b.open_size) = (0.5, 0.5, 0.85, 1.0);
                (b.bet, b.margin, b.raise, b.bluff, b.bet_size) = (0.7, 0.0, 0.85, 0.0, 1.0);
            }
            "tiny" => {
                b.open_size = 0.0;
                b.bet_size = 0.2;
                b.bluff = 0.3;
            }
            "overbet" => {
                b.bet_size = 2.5;
                b.open_size = 2.0;
                b.bluff = 0.25;
            }
            _ => return None,
        }
        Some(b)
    }

    fn raise(v: &View, frac: f64) -> Action {
        v.pot_raise(frac).unwrap_or(Action::Call)
    }

    fn all_in(v: &View) -> Action {
        v.raise_range.map(|(_, hi)| Action::RaiseTo(hi)).unwrap_or(Action::Call)
    }

    fn preflop(&self, v: &View, rng: &mut SmallRng) -> Action {
        let pct = preflop_percentile(preflop_class(v.hole[0], v.hole[1]));
        let facing_raise = v.current_bet() > BB;
        if !facing_raise {
            if pct >= self.open || (v.to_call == 0 && rng.gen::<f64>() < self.bluff / 3.0) {
                return Self::raise(v, self.open_size);
            }
            return if v.to_call == 0 || pct >= self.call { Action::Call } else { Action::Fold };
        }
        // Contro un rilancio: più è grande, più servono mani forti.
        let price = v.to_call as f64 / (v.pot() + v.to_call) as f64;
        if pct >= self.reraise && v.raises_this_street < 4 {
            return Self::raise(v, 1.0);
        }
        if pct >= self.call + price * 0.4 { Action::Call } else { Action::Fold }
    }

    fn postflop(&self, v: &View, rng: &mut SmallRng) -> Action {
        let eq = equity_vs_random(v.hole, v.board, 300, rng);
        if v.to_call == 0 {
            if eq >= self.bet || rng.gen::<f64>() < self.bluff {
                return Self::raise(v, self.bet_size);
            }
            return Action::Call;
        }
        let price = v.to_call as f64 / (v.pot() + v.to_call) as f64;
        if eq >= self.raise {
            Self::raise(v, self.bet_size.max(0.75))
        } else if eq >= price + self.margin {
            Action::Call
        } else {
            Action::Fold
        }
    }
}

impl Agent for RuleBot {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn act(&mut self, v: &View, rng: &mut SmallRng) -> Action {
        match self.style {
            Style::Station => Action::Call,
            Style::Maniac => Self::raise(v, 1.0),
            Style::Random => match rng.gen_range(0..10) {
                0..=2 => Action::Fold,
                3..=6 => Action::Call,
                7..=8 => Self::raise(v, rng.gen_range(0.3..2.0)),
                _ => Self::all_in(v),
            },
            Style::Shover => {
                if v.street > 0 {
                    return Action::Call;
                }
                let pct = preflop_percentile(preflop_class(v.hole[0], v.hole[1]));
                if pct >= self.open {
                    Self::all_in(v)
                } else if v.to_call == 0 {
                    Action::Call
                } else {
                    Action::Fold
                }
            }
            Style::Normal => {
                if v.street == 0 {
                    self.preflop(v, rng)
                } else {
                    self.postflop(v, rng)
                }
            }
        }
    }
}

/// Un giocatore distratto: con probabilità `p` fa una mossa a caso.
pub struct Noisy {
    pub inner: Box<dyn Agent>,
    pub p: f64,
}

impl Agent for Noisy {
    fn name(&self) -> String {
        format!("noisy:{}:{}", self.p, self.inner.name())
    }
    fn act(&mut self, v: &View, rng: &mut SmallRng) -> Action {
        // L'azione dell'altro va chiesta comunque, perché il motore tiene traccia della mano.
        let a = self.inner.act(v, rng);
        if rng.gen::<f64>() < self.p {
            let mut r = RuleBot::by_name("random").unwrap();
            r.act(v, rng)
        } else {
            a
        }
    }
}

pub const STYLES: &[(&str, &str)] = &[
    ("random", "Azioni a caso"),
    ("station", "Stazione di chiamata: paga sempre, non rilancia e non lascia mai"),
    ("maniac", "Maniaco: rilancia il piatto a ogni occasione"),
    ("rock", "Roccia: gioca solo il 12% delle mani migliori, punta solo con mani fortissime"),
    ("tag", "Stretto-aggressivo: apre metà delle mani, punte e rilanci con criterio, qualche bluff"),
    ("lag", "Largo-aggressivo: apre l'80% delle mani, bluffa spesso"),
    ("passive", "Largo-passivo: entra pagando con quasi tutto, punta solo con mani fortissime"),
    ("shover", "All-in o fold prima del flop con il 30% delle mani migliori"),
    ("equity", "Calcolatore: paga quando l'equità supera il prezzo, punta il piatto con le mani forti"),
    ("tiny", "Puntate piccolissime (20% del piatto) e rilanci minimi, per mettere alla prova le misure fuori dall'albero"),
    ("overbet", "Puntate enormi (2,5 volte il piatto) con mani forti e bluff"),
];
