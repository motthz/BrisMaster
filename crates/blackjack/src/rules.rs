//! Regole del tavolo. Cambiano la strategia giusta e il vantaggio del banco.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DoubleRule {
    /// Raddoppio su qualsiasi prima coppia di carte.
    Any,
    /// Solo con 9, 10 o 11 (tipico del blackjack europeo).
    NineToEleven,
    /// Solo con 10 o 11.
    TenEleven,
}

impl DoubleRule {
    pub fn allows(self, total: u8, soft: bool) -> bool {
        match self {
            DoubleRule::Any => true,
            DoubleRule::NineToEleven => !soft && (9..=11).contains(&total),
            DoubleRule::TenEleven => !soft && (10..=11).contains(&total),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            DoubleRule::Any => "su qualsiasi coppia",
            DoubleRule::NineToEleven => "solo 9, 10, 11",
            DoubleRule::TenEleven => "solo 10, 11",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rules {
    /// Mazzi nel sabot (1..=8).
    pub decks: u8,
    /// Il banco chiede carta sul 17 morbido (H17). Falso = sta su tutti i 17 (S17).
    pub hit_soft17: bool,
    /// Il banco controlla subito se ha blackjack con A o 10 scoperto (regola americana).
    /// Falso = niente carta coperta (regola europea): col blackjack del banco si perdono
    /// anche le puntate aggiunte con raddoppi e divisioni.
    pub peek: bool,
    pub double: DoubleRule,
    /// Raddoppio permesso dopo una divisione.
    pub double_after_split: bool,
    /// Numero massimo di mani dopo le divisioni (2 = una sola divisione).
    pub max_hands: u8,
    /// Gli assi divisi si possono ridividere.
    pub resplit_aces: bool,
    /// Gli assi divisi ricevono una sola carta (regola quasi universale).
    pub one_card_split_aces: bool,
    /// Resa tardiva: si abbandona la mano e si perde mezza puntata.
    pub surrender: bool,
    /// Pagamento del blackjack: 1.5 (3:2) o 1.2 (6:5).
    pub blackjack_pays: f64,
    /// Frazione del sabot giocata prima di rimescolare. 0 = rimescolamento a ogni mano,
    /// come nei giochi online a generatore casuale e nelle macchine a rimescolamento continuo.
    pub penetration: f64,
}

impl Default for Rules {
    fn default() -> Self {
        Rules::preset(0)
    }
}

impl Rules {
    pub const PRESETS: [&'static str; 4] =
        ["Europeo (online tipico)", "Atlantic City", "Las Vegas Strip", "Las Vegas 6:5 (sconsigliato)"];

    pub fn preset(i: usize) -> Rules {
        let base = Rules {
            decks: 6,
            hit_soft17: false,
            peek: false,
            double: DoubleRule::NineToEleven,
            double_after_split: true,
            max_hands: 2,
            resplit_aces: false,
            one_card_split_aces: true,
            surrender: false,
            blackjack_pays: 1.5,
            penetration: 0.0,
        };
        match i {
            1 => Rules {
                decks: 8,
                peek: true,
                double: DoubleRule::Any,
                max_hands: 4,
                surrender: true,
                penetration: 0.75,
                ..base
            },
            2 => Rules {
                decks: 6,
                hit_soft17: true,
                peek: true,
                double: DoubleRule::Any,
                max_hands: 4,
                surrender: true,
                penetration: 0.75,
                ..base
            },
            3 => Rules { blackjack_pays: 1.2, ..Rules::preset(2) },
            _ => base,
        }
    }

    /// Descrizione compatta, per tabelle e rapporti.
    pub fn short(&self) -> String {
        let mut s = format!(
            "{} mazzi, {}, {}, raddoppio {}{}, fino a {} mani",
            self.decks,
            if self.hit_soft17 { "H17" } else { "S17" },
            if self.peek { "carta coperta (peek)" } else { "senza carta coperta" },
            self.double.label(),
            if self.double_after_split { " anche dopo divisione" } else { "" },
            self.max_hands,
        );
        if self.resplit_aces {
            s += ", assi ridivisibili";
        }
        if self.surrender {
            s += ", resa";
        }
        s += if self.blackjack_pays < 1.4 { ", BJ paga 6:5" } else { ", BJ paga 3:2" };
        if self.penetration > 0.0 {
            s += &format!(", rimescola al {:.0}%", 100.0 * self.penetration);
        } else {
            s += ", rimescola a ogni mano";
        }
        s
    }
}
