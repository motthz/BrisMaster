//! Azioni, tabella della strategia di base generata dalle regole e strategie da simulare.

use crate::cards::{idx_label, Counts, Hand};
use crate::ev::{Analyzer, Situation};
use crate::rules::Rules;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Action {
    Stand,
    Hit,
    Double,
    Split,
    Surrender,
}

impl Action {
    pub const ALL: [Action; 5] = [Action::Stand, Action::Hit, Action::Double, Action::Split, Action::Surrender];

    pub fn label(self) -> &'static str {
        match self {
            Action::Stand => "Stai",
            Action::Hit => "Carta",
            Action::Double => "Raddoppia",
            Action::Split => "Dividi",
            Action::Surrender => "Arrenditi",
        }
    }
    /// Lettera per la tabella: S stai, H carta, D raddoppia, P dividi, R resa.
    pub fn letter(self) -> &'static str {
        match self {
            Action::Stand => "S",
            Action::Hit => "H",
            Action::Double => "D",
            Action::Split => "P",
            Action::Surrender => "R",
        }
    }
}

/// Azioni permesse dalle regole in una situazione.
#[derive(Clone, Copy, Debug)]
pub struct Legal {
    pub hit: bool,
    pub double: bool,
    pub split: bool,
    pub surrender: bool,
}

impl Legal {
    pub fn of(rules: &Rules, cards: &[usize], sit: Situation) -> Legal {
        let h = Hand::of(cards);
        let first = cards.len() == 2;
        let split_aces = sit.from_split && cards[0] == 0 && rules.one_card_split_aces;
        Legal {
            hit: !split_aces && h.total() < 21,
            double: first
                && !split_aces
                && rules.double.allows(h.total(), h.is_soft())
                && (!sit.from_split || rules.double_after_split),
            split: first
                && cards[0] == cards[1]
                && sit.hands.max(1) < rules.max_hands
                && (!sit.from_split || cards[0] != 0 || rules.resplit_aces),
            surrender: first && !sit.from_split && rules.surrender,
        }
    }
    pub fn allows(&self, a: Action) -> bool {
        match a {
            Action::Stand => true,
            Action::Hit => self.hit,
            Action::Double => self.double,
            Action::Split => self.split,
            Action::Surrender => self.surrender,
        }
    }
}

/// Una casella della tabella: EV medio di ogni azione sulle mani di due carte di quella
/// riga (None = non permessa dalle regole), dalla migliore alla peggiore.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartCell {
    pub ranked: Vec<(Action, f64)>,
}

impl ChartCell {
    pub fn best(&self) -> Action {
        self.ranked[0].0
    }
    /// Prima azione della classifica tra quelle permesse.
    pub fn choose(&self, legal: &Legal) -> Action {
        self.ranked.iter().map(|&(a, _)| a).find(|&a| legal.allows(a)).unwrap_or(Action::Stand)
    }
    pub fn ev(&self, a: Action) -> Option<f64> {
        self.ranked.iter().find(|r| r.0 == a).map(|r| r.1)
    }
    /// Codice da tabella: "Dh" = raddoppia, se non puoi carta; "Rs" = arrenditi, se non puoi stai.
    pub fn code(&self) -> String {
        let best = self.best();
        match best {
            Action::Double | Action::Surrender | Action::Split => {
                let next = self
                    .ranked
                    .iter()
                    .map(|r| r.0)
                    .find(|&a| a == Action::Hit || a == Action::Stand)
                    .unwrap_or(Action::Stand);
                if best == Action::Split {
                    "P".into()
                } else {
                    format!("{}{}", best.letter(), next.letter().to_lowercase())
                }
            }
            _ => best.letter().into(),
        }
    }
}

/// Strategia di base per un insieme di regole: righe dure 4..=21, morbide 12..=21, coppie A..10;
/// colonne = carta scoperta del banco (indice 0 = asso, 9 = dieci).
#[derive(Clone, Debug, PartialEq)]
pub struct Chart {
    pub rules: Rules,
    pub hard: Vec<[ChartCell; 10]>,
    pub soft: Vec<[ChartCell; 10]>,
    pub pairs: Vec<[ChartCell; 10]>,
    /// Valore atteso di un giro con la migliore scelta per ogni prima mano (sabot pieno).
    pub round_ev: f64,
}

pub const HARD_MIN: u8 = 4;
pub const SOFT_MIN: u8 = 12;

impl Chart {
    /// Calcola la tabella dagli EV esatti sul sabot pieno (qualche secondo per 8 mazzi).
    pub fn compute(rules: &Rules) -> Chart {
        let full = Counts::full(rules.decks);
        let cols: Vec<_> = (0..10)
            .map(|up| {
                let rules = *rules;
                std::thread::spawn(move || column(&rules, &full, up))
            })
            .collect();
        let cols: Vec<_> = cols.into_iter().map(|t| t.join().unwrap()).collect();
        let pick = |f: &dyn Fn(&Column) -> &Vec<ChartCell>, n: usize| -> Vec<[ChartCell; 10]> {
            (0..n).map(|row| std::array::from_fn(|up| f(&cols[up])[row].clone())).collect()
        };
        let n0 = full.total() as f64;
        let round_ev = (0..10).map(|up| full.0[up] as f64 / n0 * cols[up].ev).sum();
        Chart {
            rules: *rules,
            hard: pick(&|c| &c.hard, (22 - HARD_MIN) as usize),
            soft: pick(&|c| &c.soft, (22 - SOFT_MIN) as usize),
            pairs: pick(&|c| &c.pairs, 10),
            round_ev,
        }
    }

    pub fn cell(&self, cards: &[usize], up: usize) -> &ChartCell {
        let h = Hand::of(cards);
        if cards.len() == 2 && cards[0] == cards[1] {
            return &self.pairs[cards[0]][up];
        }
        if h.is_soft() {
            &self.soft[(h.total().max(SOFT_MIN) - SOFT_MIN) as usize][up]
        } else {
            &self.hard[(h.total().clamp(HARD_MIN, 21) - HARD_MIN) as usize][up]
        }
    }

    /// Azione della strategia di base, tenendo conto di cosa permettono le regole.
    pub fn decide(&self, cards: &[usize], up: usize, sit: Situation) -> Action {
        let legal = Legal::of(&self.rules, cards, sit);
        if !legal.hit {
            return Action::Stand;
        }
        let mut cell = self.cell(cards, up);
        if cards.len() == 2 && cards[0] == cards[1] && !legal.split && cell.best() == Action::Split {
            // Coppia che non si può più dividere: si gioca come il totale.
            let h = Hand::of(cards);
            cell = if h.is_soft() { &self.soft[0][up] } else { &self.hard[(h.total() - HARD_MIN) as usize][up] };
        }
        cell.choose(&legal)
    }

    /// Righe da mostrare: (etichetta, caselle).
    pub fn display_rows(&self) -> Vec<(String, &[ChartCell; 10])> {
        let mut v = Vec::new();
        for t in 5..=18u8 {
            let label = if t == 18 { "18+".to_string() } else { t.to_string() };
            v.push((label, &self.hard[(t - HARD_MIN) as usize]));
        }
        for t in 13..=20u8 {
            v.push((format!("A,{}", t - 11), &self.soft[(t - SOFT_MIN) as usize]));
        }
        for p in [0usize, 9, 8, 7, 6, 5, 4, 3, 2, 1] {
            v.push((format!("{},{}", idx_label(p), idx_label(p)), &self.pairs[p]));
        }
        v
    }
}

struct Column {
    hard: Vec<ChartCell>,
    soft: Vec<ChartCell>,
    pairs: Vec<ChartCell>,
    /// EV del giro con questa carta scoperta (migliore scelta per ogni mano).
    ev: f64,
}

fn column(rules: &Rules, full: &Counts, up: usize) -> Column {
    let mut a = Analyzer::new(*rules, up);
    let s1 = full.removed(up);
    let n1 = s1.total() as f64;
    // Somme pesate per riga: (peso, EV per azione).
    let mut hard = vec![(0.0, [None; 5]); (22 - HARD_MIN) as usize];
    let mut soft = vec![(0.0, [None; 5]); (22 - SOFT_MIN) as usize];
    let mut pairs = vec![(0.0, [None; 5]); 10];
    let mut hard_pairs = vec![(0.0, [None; 5]); (22 - HARD_MIN) as usize];
    let mut ev = 0.0;
    let sit = Situation { hands: 1, from_split: false };
    for p1 in 0..10 {
        for p2 in p1..10 {
            let s2 = s1.removed(p1);
            let w = if p1 == p2 {
                s1.0[p1] as f64 / n1 * s2.0[p2] as f64 / (n1 - 1.0)
            } else {
                2.0 * s1.0[p1] as f64 / n1 * s2.0[p2] as f64 / (n1 - 1.0)
            };
            let c = s2.removed(p2);
            ev += w * crate::ev::deal_ev(&mut a, &c, [p1, p2]);
            let h = Hand::of(&[p1, p2]);
            if h.is_blackjack() {
                continue;
            }
            let e = a.analyze(&c, &[p1, p2], sit);
            let evs: [Option<f64>; 5] = std::array::from_fn(|i| e.get(Action::ALL[i]));
            // Nelle righe dure e morbide la divisione non c'è: la coppia vale come il totale.
            let mut no_split = evs;
            no_split[3] = None;
            let add = |acc: &mut (f64, [Option<f64>; 5]), evs: [Option<f64>; 5]| {
                acc.0 += w;
                for (s, e) in acc.1.iter_mut().zip(evs) {
                    *s = e.map(|e| s.unwrap_or(0.0) + w * e);
                }
            };
            if p1 == p2 {
                add(&mut pairs[p1], evs);
                if h.is_soft() {
                    add(&mut soft[0], no_split);
                } else {
                    add(&mut hard_pairs[(h.total() - HARD_MIN) as usize], no_split);
                }
            } else if h.is_soft() {
                add(&mut soft[(h.total() - SOFT_MIN) as usize], no_split);
            } else {
                add(&mut hard[(h.total() - HARD_MIN) as usize], no_split);
            }
        }
    }
    // Totali duri che si fanno solo con una coppia (4 e 20): si usa la coppia.
    for (h, hp) in hard.iter_mut().zip(hard_pairs) {
        if h.0 == 0.0 {
            *h = hp;
        }
    }
    let finish = |rows: Vec<(f64, [Option<f64>; 5])>, multi: &dyn Fn(usize) -> Option<Hand>, a: &mut Analyzer| {
        rows.into_iter()
            .enumerate()
            .map(|(i, (w, sums))| {
                let mut ranked: Vec<(Action, f64)> = if w > 0.0 {
                    Action::ALL.iter().zip(sums).filter_map(|(&act, s)| s.map(|s| (act, s / w))).collect()
                } else {
                    // Totali che si raggiungono solo con tre o più carte (21 duro, 21 morbido):
                    // si confrontano stare e carta su una mano tipica.
                    let h = multi(i).unwrap();
                    let c = s1.removed(9).removed(9);
                    let stand = a.stand(&c, h);
                    let hit = if h.total() >= 21 { -1.0 } else { a.hit(&c, h) };
                    vec![(Action::Stand, stand), (Action::Hit, hit)]
                };
                ranked.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
                ChartCell { ranked }
            })
            .collect::<Vec<_>>()
    };
    let hard_multi = |i: usize| Some(Hand { hard: HARD_MIN + i as u8, has_ace: false, cards: 3 });
    let soft_multi = |i: usize| Some(Hand { hard: SOFT_MIN + i as u8 - 10, has_ace: true, cards: 3 });
    Column {
        hard: finish(hard, &hard_multi, &mut a),
        soft: finish(soft, &soft_multi, &mut a),
        pairs: finish(pairs, &|_| None, &mut a),
        ev,
    }
}

/// Strategia di base per il simulatore e per l'allenatore.
pub struct BasicStrategy {
    pub chart: Chart,
}

impl BasicStrategy {
    pub fn new(rules: &Rules) -> BasicStrategy {
        BasicStrategy { chart: Chart::compute(rules) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::DoubleRule;

    /// Dal valore della carta (1 o 11 = asso) all'indice.
    fn v(x: usize) -> usize {
        if x == 11 {
            0
        } else {
            x - 1
        }
    }

    fn vegas_s17() -> Rules {
        Rules {
            decks: 6,
            hit_soft17: false,
            peek: true,
            double: DoubleRule::Any,
            double_after_split: true,
            max_hands: 4,
            resplit_aces: false,
            one_card_split_aces: true,
            surrender: false,
            blackjack_pays: 1.5,
            penetration: 0.75,
        }
    }

    /// Confronto con la tabella pubblicata per 6 mazzi, S17, DAS, senza resa
    /// (per esempio quella di Wizard of Odds).
    #[test]
    fn matches_published_chart() {
        let c = Chart::compute(&vegas_s17());
        let code = |cards: &[usize], up: usize| c.cell(&[v(cards[0]), v(cards[1])], v(up)).code();
        // Duri
        assert_eq!(code(&[10, 6], 10), "H"); // 16 contro 10: carta (senza resa)
        assert_eq!(code(&[10, 6], 6), "S"); // 16 contro 6
        assert_eq!(code(&[10, 2], 4), "S"); // 12 contro 4
        assert_eq!(code(&[10, 2], 3), "H"); // 12 contro 3
        assert_eq!(code(&[6, 5], 11), "H"); // 11 contro A: carta con S17 e 6 mazzi
        assert_eq!(code(&[6, 5], 10), "Dh"); // 11 contro 10
        assert_eq!(code(&[6, 4], 10), "H"); // 10 contro 10
        assert_eq!(code(&[5, 4], 3), "Dh"); // 9 contro 3
        assert_eq!(code(&[5, 4], 11), "H"); // 9 contro A
                                            // Morbidi
        assert_eq!(code(&[11, 7], 3), "Ds"); // A,7 contro 3
        assert_eq!(code(&[11, 7], 9), "H"); // A,7 contro 9
        assert_eq!(code(&[11, 7], 7), "S"); // A,7 contro 7
        assert_eq!(code(&[11, 2], 5), "Dh"); // A,2 contro 5
        assert_eq!(code(&[11, 8], 6), "S"); // A,8 contro 6 (S17)
                                            // Coppie
        assert_eq!(code(&[8, 8], 10), "P"); // 8,8 contro 10
        assert_eq!(code(&[11, 11], 11), "P"); // A,A
        assert_eq!(code(&[9, 9], 7), "S"); // 9,9 contro 7
        assert_eq!(code(&[9, 9], 8), "P"); // 9,9 contro 8
        assert_eq!(code(&[5, 5], 9), "Dh"); // 5,5 contro 9
        assert_eq!(code(&[4, 4], 5), "P"); // 4,4 contro 5 (DAS)
        assert_eq!(code(&[10, 10], 6), "S"); // 10,10
        assert_eq!(code(&[7, 7], 7), "P"); // 7,7 contro 7
        assert_eq!(code(&[6, 6], 3), "P"); // 6,6 contro 3
    }

    #[test]
    fn h17_and_surrender_change_the_chart() {
        let h17 = Rules { hit_soft17: true, surrender: true, ..vegas_s17() };
        let c = Chart::compute(&h17);
        let code = |cards: &[usize], up: usize| c.cell(&[v(cards[0]), v(cards[1])], v(up)).code();
        assert_eq!(code(&[6, 5], 11), "Dh"); // 11 contro A: raddoppia con H17
        assert_eq!(code(&[10, 6], 10), "Rh"); // 16 contro 10: resa
        assert_eq!(code(&[10, 5], 11), "Rh"); // 15 contro A: resa con H17
        assert_eq!(code(&[11, 8], 6), "Ds"); // A,8 contro 6 con H17
    }

    #[test]
    fn house_edge_in_known_range() {
        // 6 mazzi S17 DAS senza resa: vantaggio del banco circa 0,40% con la strategia di base.
        let c = Chart::compute(&vegas_s17());
        assert!((-0.0050..-0.0025).contains(&c.round_ev), "{}", c.round_ev);
    }
}
