//! Calcolo esatto del valore atteso: per ogni situazione si enumerano tutte le carte che possono
//! uscire, per il giocatore e poi per il banco, con le probabilità date dalle carte non ancora viste.
//! Nessuna simulazione: i numeri sono esatti per il modello di banco scelto.

use crate::banco::{Banco, BancoBrain};
use crate::{Deck, Hand, Memo, Rules, CLASSES};

/// Probabilità dei possibili esiti di una mano, per il giocatore.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Dist {
    /// Vince una puntata.
    pub win: f64,
    /// Vince con sette e mezzo reale.
    pub win_reale: f64,
    /// Sballa (perde subito).
    pub bust: f64,
    /// Perde al confronto con il banco (pareggi compresi).
    pub lose: f64,
}

impl Dist {
    fn of_win(rules: &Rules, p: Hand) -> Dist {
        if p.reale() && rules.reale_paga != 1 {
            Dist { win_reale: 1.0, ..Default::default() }
        } else {
            Dist { win: 1.0, ..Default::default() }
        }
    }
    const LOSE: Dist = Dist { win: 0.0, win_reale: 0.0, bust: 0.0, lose: 1.0 };
    const BUST: Dist = Dist { win: 0.0, win_reale: 0.0, bust: 1.0, lose: 0.0 };

    pub fn scaled(self, k: f64) -> Dist {
        Dist { win: self.win * k, win_reale: self.win_reale * k, bust: self.bust * k, lose: self.lose * k }
    }
    pub fn plus(self, o: Dist) -> Dist {
        Dist {
            win: self.win + o.win,
            win_reale: self.win_reale + o.win_reale,
            bust: self.bust + o.bust,
            lose: self.lose + o.lose,
        }
    }
    /// Vincita attesa in puntate.
    pub fn ev(&self, rules: &Rules) -> f64 {
        self.win + rules.reale_paga as f64 * self.win_reale - self.bust - self.lose
    }
    /// Probabilità di vincere la mano.
    pub fn p_win(&self) -> f64 {
        self.win + self.win_reale
    }
}

/// Il consiglio per una situazione.
#[derive(Clone, Copy, Debug)]
pub struct Advice {
    /// Esiti se ti fermi.
    pub stand: Dist,
    /// Esiti se chiedi carta e poi giochi al meglio (`None` se il mazzo è finito).
    pub hit: Option<Dist>,
    /// Probabilità di sballare con la prossima carta.
    pub p_bust_next: f64,
}

impl Advice {
    pub fn should_hit(&self, rules: &Rules) -> bool {
        self.hit.is_some_and(|h| h.ev(rules) > self.stand.ev(rules) + 1e-12)
    }
    pub fn best(&self, rules: &Rules) -> Dist {
        if self.should_hit(rules) {
            self.hit.unwrap()
        } else {
            self.stand
        }
    }
}

/// Risolutore esatto per un insieme di regole e un modello di banco. Tiene in memoria
/// le situazioni già calcolate, quindi conviene riusarlo.
#[derive(Clone)]
pub struct Solver {
    pub rules: Rules,
    pub brain: BancoBrain,
    stand_memo: Memo<Dist>,
    opt_memo: Memo<(Dist, Option<Dist>)>,
}

#[inline]
fn node_key(d: &Deck, hc: usize, up: Hand) -> u64 {
    d.pack() | (hc as u64) << 36 | up.pack() << 40
}

impl Solver {
    pub fn new(rules: Rules, banco: Banco) -> Self {
        Solver { rules, brain: BancoBrain::new(rules, banco), stand_memo: Memo::default(), opt_memo: Memo::default() }
    }

    pub fn banco(&self) -> Banco {
        self.brain.banco
    }

    /// Il giocatore si ferma: il banco gioca pescando da `d`.
    /// `hc` è la classe della carta coperta del giocatore, `up` le sue carte scoperte.
    pub fn stand(&mut self, d: &Deck, hc: usize, up: Hand) -> Dist {
        self.banker(d, hc, up, Hand::EMPTY)
    }

    fn banker(&mut self, d: &Deck, hc: usize, up: Hand, b: Hand) -> Dist {
        let key = node_key(d, hc, up) | b.pack() << 51;
        if let Some(&v) = self.stand_memo.get(&key) {
            return v;
        }
        let rules = self.rules;
        let p = up.add(hc);
        let t = d.total();
        let hits = t > 0 && self.brain.hits(b, up, &d.with(hc));
        let v = if !hits {
            if b.cards > 0 && b.rank() >= p.rank() {
                Dist::LOSE
            } else {
                Dist::of_win(&rules, p)
            }
        } else {
            let mut acc = Dist::default();
            for c in 0..CLASSES {
                if d.n[c] == 0 {
                    continue;
                }
                let nb = b.add(c);
                let r = if nb.bust() { Dist::of_win(&rules, p) } else { self.banker(&d.without(c), hc, up, nb) };
                acc = acc.plus(r.scaled(d.n[c] as f64 / t as f64));
            }
            acc
        };
        self.stand_memo.insert(key, v);
        v
    }

    /// Esiti di stare e di chiedere carta (giocando poi al meglio).
    pub fn options(&mut self, d: &Deck, hc: usize, up: Hand) -> (Dist, Option<Dist>) {
        let key = node_key(d, hc, up);
        if let Some(&v) = self.opt_memo.get(&key) {
            return v;
        }
        let rules = self.rules;
        let stand = self.stand(d, hc, up);
        let t = d.total();
        let hit = (t > 0).then(|| {
            let mut acc = Dist::default();
            for c in 0..CLASSES {
                if d.n[c] == 0 {
                    continue;
                }
                let nu = up.add(c);
                let r = if nu.add(hc).bust() {
                    Dist::BUST
                } else {
                    let (s, h) = self.options(&d.without(c), hc, nu);
                    match h {
                        Some(h) if h.ev(&rules) > s.ev(&rules) + 1e-12 => h,
                        _ => s,
                    }
                };
                acc = acc.plus(r.scaled(d.n[c] as f64 / t as f64));
            }
            acc
        });
        let v = (stand, hit);
        self.opt_memo.insert(key, v);
        v
    }

    /// Consiglio per il giocatore: `d` sono le carte che non vede (mazzo e carte coperte altrui),
    /// `hc` la classe della sua carta coperta, `up` le sue carte scoperte.
    pub fn advise(&mut self, d: &Deck, hc: usize, up: Hand) -> Advice {
        let (stand, hit) = self.options(d, hc, up);
        let t = d.total() as f64;
        let p = up.add(hc);
        let p_bust_next = if t > 0.0 {
            (0..CLASSES).filter(|&c| p.add(c).bust()).map(|c| d.n[c] as f64).sum::<f64>() / t
        } else {
            0.0
        };
        Advice { stand, hit, p_bust_next }
    }

    /// Esiti di una mano intera giocata da una strategia, a partire dal mazzo `deck` (di solito pieno).
    /// `hit_prob(solver, d, hc, up)` dice con che probabilità la strategia chiede carta.
    pub fn evaluate(&mut self, deck: &Deck, hit_prob: &mut dyn FnMut(&Deck, usize, Hand) -> f64) -> Dist {
        let mut memo = Memo::default();
        let t = deck.total() as f64;
        let mut acc = Dist::default();
        for hc in 0..CLASSES {
            if deck.n[hc] > 0 {
                let r = self.eval_node(&deck.without(hc), hc, Hand::EMPTY, hit_prob, &mut memo);
                acc = acc.plus(r.scaled(deck.n[hc] as f64 / t));
            }
        }
        acc
    }

    fn eval_node(
        &mut self,
        d: &Deck,
        hc: usize,
        up: Hand,
        hit_prob: &mut dyn FnMut(&Deck, usize, Hand) -> f64,
        memo: &mut Memo<Dist>,
    ) -> Dist {
        let key = node_key(d, hc, up);
        if let Some(&v) = memo.get(&key) {
            return v;
        }
        let t = d.total();
        let q = if t > 0 { hit_prob(d, hc, up) } else { 0.0 };
        let mut acc = Dist::default();
        if q < 1.0 {
            acc = self.stand(d, hc, up).scaled(1.0 - q);
        }
        if q > 0.0 {
            for c in 0..CLASSES {
                if d.n[c] == 0 {
                    continue;
                }
                let nu = up.add(c);
                let r =
                    if nu.add(hc).bust() { Dist::BUST } else { self.eval_node(&d.without(c), hc, nu, hit_prob, memo) };
                acc = acc.plus(r.scaled(q * d.n[c] as f64 / t as f64));
            }
        }
        memo.insert(key, acc);
        acc
    }

    /// Valore della strategia ottimale su una mano intera.
    pub fn optimal_value(&mut self, deck: &Deck) -> Dist {
        let t = deck.total() as f64;
        let rules = self.rules;
        let mut acc = Dist::default();
        for hc in 0..CLASSES {
            if deck.n[hc] > 0 {
                let a = self.advise(&deck.without(hc), hc, Hand::EMPTY);
                acc = acc.plus(a.best(&rules).scaled(deck.n[hc] as f64 / t));
            }
        }
        acc
    }

    pub fn cache_len(&self) -> usize {
        self.stand_memo.len() + self.opt_memo.len()
    }
}

/// Probabilità che la mano sia ancora sotto la soglia: strategia ingenua "chiedo finché ho meno di t".
pub fn soglia(t: u8) -> impl FnMut(&Deck, usize, Hand) -> f64 {
    move |_, hc, up| {
        if up.add(hc).total().is_some_and(|x| x < t) {
            1.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dist_sums_to_one_and_optimal_dominates() {
        for matta in [true, false] {
            let rules = Rules { matta, reale_paga: 2 };
            for banco in Banco::ALL {
                let mut s = Solver::new(rules, banco);
                let deck = rules.full_deck();
                let opt = s.optimal_value(&deck);
                let tot = opt.win + opt.win_reale + opt.bust + opt.lose;
                assert!((tot - 1.0).abs() < 1e-9, "{tot}");
                // La strategia ottimale valutata come politica dà lo stesso valore.
                let mut s2 = s.clone();
                let mut pol = |d: &Deck, hc: usize, up: Hand| {
                    if s2.advise(d, hc, up).should_hit(&rules) {
                        1.0
                    } else {
                        0.0
                    }
                };
                let again = s.evaluate(&deck, &mut pol);
                assert!((again.ev(&rules) - opt.ev(&rules)).abs() < 1e-9);
                for t in 2..=15 {
                    let v = s.evaluate(&deck, &mut soglia(t)).ev(&rules);
                    assert!(v <= opt.ev(&rules) + 1e-9, "{banco:?} soglia {t}: {v} > {}", opt.ev(&rules));
                }
            }
        }
    }

    #[test]
    fn non_si_chiede_con_sette_e_mezzo() {
        let rules = Rules::default();
        let mut s = Solver::new(rules, Banco::Esperto);
        let d = rules.full_deck().without(7).without(0);
        let a = s.advise(&d, 7, Hand::EMPTY.add(0));
        assert!(!a.should_hit(&rules));
        assert_eq!(a.p_bust_next, 1.0);
        // Con una figura si chiede sempre: non si può sballare.
        let d = rules.full_deck().without(0);
        let a = s.advise(&d, 0, Hand::EMPTY);
        assert!(a.should_hit(&rules));
        assert_eq!(a.p_bust_next, 0.0);
    }
}
