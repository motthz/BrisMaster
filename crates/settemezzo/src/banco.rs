//! Modelli di banco.
//!
//! - `Soglia(t)`: il banco chiede carta finché non ha almeno `t` (mezzi punti), senza guardare le carte
//!   del giocatore. È il comportamento "a regola fissa" di molte versioni da casinò e da app.
//! - `Esperto`: il banco vede le carte scoperte del giocatore e decide ogni volta se fermarsi o tirare
//!   massimizzando la propria vincita attesa. Della carta coperta del giocatore sa solo che è una delle
//!   carte che lui non vede e che non fa sballare il giocatore (che ha detto "sto").

use crate::{Deck, Hand, Memo, Rules, CLASSES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Banco {
    /// Si ferma appena arriva almeno a questo punteggio (mezzi punti).
    Soglia(u8),
    /// Decide vedendo le carte scoperte del giocatore.
    Esperto,
}

impl Banco {
    pub const ALL: [Banco; 4] = [Banco::Soglia(10), Banco::Soglia(11), Banco::Soglia(12), Banco::Esperto];

    pub fn label(self) -> String {
        match self {
            Banco::Soglia(t) => format!("Banco a soglia {}", crate::fmt_points(t)),
            Banco::Esperto => "Banco esperto".into(),
        }
    }

    pub fn description(self) -> String {
        match self {
            Banco::Soglia(t) => format!(
                "Chiede carta finché ha meno di {} e poi si ferma, senza guardare le tue carte",
                crate::fmt_points(t)
            ),
            Banco::Esperto => "Guarda le tue carte scoperte e tira o si ferma per massimizzare la sua vincita".into(),
        }
    }

    pub fn key(self) -> u8 {
        match self {
            Banco::Soglia(t) => t,
            Banco::Esperto => 0,
        }
    }
}

/// Quante puntate incassa il giocatore se vince con questa mano.
#[inline]
pub fn pay(rules: &Rules, p: Hand) -> f64 {
    if p.reale() {
        rules.reale_paga as f64
    } else {
        1.0
    }
}

/// Le decisioni del banco, con la memoria del banco esperto.
#[derive(Clone)]
pub struct BancoBrain {
    pub rules: Rules,
    pub banco: Banco,
    memo: Memo<(f64, bool)>,
}

impl BancoBrain {
    pub fn new(rules: Rules, banco: Banco) -> Self {
        BancoBrain { rules, banco, memo: Memo::default() }
    }

    /// Il banco, con la mano `b`, chiede un'altra carta?
    /// `up`: carte scoperte del giocatore; `unseen`: le carte che il banco non vede
    /// (mazzo più la carta coperta del giocatore).
    pub fn hits(&mut self, b: Hand, up: Hand, unseen: &Deck) -> bool {
        if b.cards == 0 {
            return true;
        }
        if unseen.total() < 2 {
            return false;
        }
        match self.banco {
            Banco::Soglia(t) => b.total().is_some_and(|x| x < t),
            Banco::Esperto => self.esperto(unseen, b, up).1,
        }
    }

    /// Valore atteso per il banco (in puntate) e decisione ottima: (valore, tira).
    fn esperto(&mut self, s: &Deck, b: Hand, up: Hand) -> (f64, bool) {
        let key = s.pack() | b.pack() << 36 | up.pack() << 47;
        if let Some(&v) = self.memo.get(&key) {
            return v;
        }
        let rules = self.rules;
        // Carte coperte possibili per il giocatore: quelle che non lo fanno sballare.
        let ok: [bool; CLASSES] = std::array::from_fn(|k| !up.add(k).bust());
        let w: f64 = (0..CLASSES).filter(|&k| ok[k]).map(|k| s.n[k] as f64).sum();
        if w == 0.0 {
            let v = (0.0, false);
            self.memo.insert(key, v);
            return v;
        }
        let stand = (0..CLASSES)
            .filter(|&k| ok[k] && s.n[k] > 0)
            .map(|k| {
                let p = up.add(k);
                let u = if b.rank() >= p.rank() { 1.0 } else { -pay(&rules, p) };
                s.n[k] as f64 * u
            })
            .sum::<f64>()
            / w;
        let t = s.total() as f64;
        let mut v = (stand, false);
        if t >= 2.0 && b.cards < 30 {
            let mut hit = 0.0;
            for c in 0..CLASSES {
                if s.n[c] == 0 {
                    continue;
                }
                // Coppie (carta coperta del giocatore, carta pescata) distinte.
                let weight = s.n[c] as f64 * (w - if ok[c] { 1.0 } else { 0.0 });
                if weight == 0.0 {
                    continue;
                }
                let s2 = s.without(c);
                let nb = b.add(c);
                let val = if nb.bust() {
                    let w2: f64 = (0..CLASSES).filter(|&k| ok[k]).map(|k| s2.n[k] as f64).sum();
                    -(0..CLASSES).filter(|&k| ok[k]).map(|k| s2.n[k] as f64 * pay(&rules, up.add(k))).sum::<f64>() / w2
                } else {
                    self.esperto(&s2, nb, up).0
                };
                hit += weight * val;
            }
            hit /= w * (t - 1.0);
            if hit > stand + 1e-12 {
                v = (hit, true);
            }
        }
        self.memo.insert(key, v);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FIGURA, MATTA};

    #[test]
    fn soglia() {
        let r = Rules::default();
        let mut b = BancoBrain::new(r, Banco::Soglia(11));
        let d = r.full_deck();
        assert!(b.hits(Hand::EMPTY.add(5), Hand::EMPTY, &d));
        assert!(!b.hits(Hand::EMPTY.add(5).add(FIGURA), Hand::EMPTY, &d));
        assert!(!b.hits(Hand::EMPTY.add(MATTA), Hand::EMPTY, &d));
    }

    #[test]
    fn esperto_insegue_e_si_ferma() {
        let r = Rules::default();
        let mut b = BancoBrain::new(r, Banco::Esperto);
        // Il giocatore mostra un 5 (quindi ha almeno 5½): il banco con 4 deve tirare.
        let up = Hand::EMPTY.add(5);
        let s = r.full_deck().without(5).without(4);
        assert!(b.hits(Hand::EMPTY.add(4), up, &s));
        // Con 7½ il banco si ferma sempre.
        let s = r.full_deck().without(7).without(7).without(FIGURA);
        assert!(!b.hits(Hand::EMPTY.add(7).add(FIGURA), up, &s));
    }
}
