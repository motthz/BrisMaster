//! Regole della briscola a due giocatori.

use crate::cards::*;
use rand::seq::SliceRandom;
use rand::Rng;

/// Stato completo (informazione perfetta) di una partita.
///
/// Il mazzo si pesca dalla fine: `deck[0]` è la briscola scoperta,
/// che viene pescata per ultima.
#[derive(Clone, Copy, Debug)]
pub struct Game {
    pub hands: [CardSet; 2],
    pub deck: [Card; NUM_CARDS],
    pub deck_len: u8,
    pub trump_card: Card,
    pub trump: u8,
    /// Giocatore che apre la presa in corso.
    pub leader: u8,
    /// Carta giocata da chi ha aperto, se la presa è a metà.
    pub table: Option<Card>,
    pub points: [u8; 2],
    /// Carte uscite nelle prese già chiuse.
    pub played: CardSet,
    /// Chi ha pescato la briscola scoperta (se è già stata pescata).
    pub trump_card_owner: Option<u8>,
}

/// Una presa conclusa.
#[derive(Clone, Copy, Debug)]
pub struct Trick {
    pub leader: u8,
    pub lead: Card,
    pub follow: Card,
    pub winner: u8,
    pub points: u8,
}

impl Game {
    /// Nuova partita con mazzo mescolato; `first` apre la prima presa.
    pub fn new<R: Rng>(rng: &mut R, first: u8) -> Game {
        let mut order: Vec<Card> = (0..NUM_CARDS as u8).collect();
        order.shuffle(rng);
        Game::from_order(&order, first)
    }

    /// Nuova partita da un ordine di mazzo dato: si pesca da `order[39]` in giù,
    /// `order[0]` è la briscola scoperta.
    pub fn from_order(order: &[Card], first: u8) -> Game {
        assert_eq!(order.len(), NUM_CARDS);
        let mut deck = [0u8; NUM_CARDS];
        deck.copy_from_slice(order);
        let mut g = Game {
            hands: [CardSet::EMPTY; 2],
            deck,
            deck_len: NUM_CARDS as u8,
            trump_card: order[0],
            trump: suit(order[0]),
            leader: first,
            table: None,
            points: [0, 0],
            played: CardSet::EMPTY,
            trump_card_owner: None,
        };
        // Si danno tre carte a testa, una alla volta, cominciando da chi apre.
        for _ in 0..3 {
            g.draw(first);
            g.draw(1 - first);
        }
        g
    }

    #[inline]
    fn draw(&mut self, p: u8) {
        if self.deck_len == 0 {
            return;
        }
        self.deck_len -= 1;
        let c = self.deck[self.deck_len as usize];
        if self.deck_len == 0 {
            self.trump_card_owner = Some(p);
        }
        self.hands[p as usize].insert(c);
    }

    /// Giocatore che deve muovere.
    #[inline]
    pub fn to_move(&self) -> u8 {
        if self.table.is_some() {
            1 - self.leader
        } else {
            self.leader
        }
    }

    #[inline]
    pub fn is_over(&self) -> bool {
        self.table.is_none() && self.hands[0].is_empty() && self.hands[1].is_empty()
    }

    /// Nella briscola si può giocare qualsiasi carta della mano.
    #[inline]
    pub fn legal_moves(&self) -> CardSet {
        self.hands[self.to_move() as usize]
    }

    /// Gioca una carta per chi deve muovere. Restituisce la presa se si è chiusa.
    pub fn play(&mut self, c: Card) -> Option<Trick> {
        let p = self.to_move();
        debug_assert!(self.hands[p as usize].contains(c), "carta non in mano");
        self.hands[p as usize].remove(c);
        match self.table {
            None => {
                self.table = Some(c);
                None
            }
            Some(lead) => {
                let winner = if beats(c, lead, self.trump) { p } else { self.leader };
                let pts = points(lead) + points(c);
                self.points[winner as usize] += pts;
                self.played.insert(lead);
                self.played.insert(c);
                let t = Trick { leader: self.leader, lead, follow: c, winner, points: pts };
                self.table = None;
                self.leader = winner;
                self.draw(winner);
                self.draw(1 - winner);
                Some(t)
            }
        }
    }

    /// Esito per il giocatore `p`: 1 vittoria, 0.5 pareggio (60-60), 0 sconfitta.
    pub fn outcome(&self, p: u8) -> f64 {
        let mine = self.points[p as usize];
        if mine > 60 {
            1.0
        } else if mine == 60 {
            0.5
        } else {
            0.0
        }
    }

    /// Carte ancora nel mazzo (dall'alto verso il basso), per debug e test.
    pub fn deck_cards(&self) -> Vec<Card> {
        self.deck[..self.deck_len as usize].iter().rev().copied().collect()
    }
}

/// Ciò che un giocatore vede della partita: niente mano avversaria, niente ordine del mazzo.
#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub me: u8,
    pub hand: CardSet,
    pub opp_hand_len: u8,
    pub deck_len: u8,
    pub trump_card: Card,
    pub trump: u8,
    pub leader: u8,
    pub table: Option<Card>,
    pub points: [u8; 2],
    pub played: CardSet,
    pub trump_card_owner: Option<u8>,
}

impl Observation {
    pub fn of(g: &Game, me: u8) -> Observation {
        Observation {
            me,
            hand: g.hands[me as usize],
            opp_hand_len: g.hands[1 - me as usize].len() as u8,
            deck_len: g.deck_len,
            trump_card: g.trump_card,
            trump: g.trump,
            leader: g.leader,
            table: g.table,
            points: g.points,
            played: g.played,
            trump_card_owner: g.trump_card_owner,
        }
    }

    /// Vero se tocca a me rispondere a una carta già sul tavolo.
    #[inline]
    pub fn following(&self) -> bool {
        self.table.is_some()
    }

    /// Carte la cui posizione mi è ignota (mano avversaria + mazzo coperto).
    pub fn unknown(&self) -> CardSet {
        let mut u = CardSet::FULL.minus(self.hand).minus(self.played);
        if let Some(t) = self.table {
            u.remove(t);
        }
        // La briscola scoperta è nota: in fondo al mazzo oppure in mano a chi l'ha pescata.
        u.remove(self.trump_card);
        u
    }

    /// Come `determinize`, ma con la mano avversaria data (deve essere compatibile con
    /// quello che so): a caso resta solo l'ordine del mazzo.
    pub fn determinize_given<R: Rng>(&self, opp_hand: CardSet, rng: &mut R) -> Game {
        let mut g = self.determinize(rng);
        let opp = 1 - self.me as usize;
        debug_assert_eq!(opp_hand.len(), g.hands[opp].len());
        let mut rest = self.unknown().minus(opp_hand).to_vec();
        rest.shuffle(rng);
        g.hands[opp] = opp_hand;
        if self.deck_len > 0 {
            g.deck[1..self.deck_len as usize].copy_from_slice(&rest);
        }
        g
    }

    /// Ricostruisce uno stato completo compatibile con quello che so,
    /// distribuendo a caso le carte ignote.
    pub fn determinize<R: Rng>(&self, rng: &mut R) -> Game {
        let me = self.me as usize;
        let opp = 1 - me;
        let mut unknown = self.unknown().to_vec();
        unknown.shuffle(rng);

        let mut hands = [CardSet::EMPTY; 2];
        hands[me] = self.hand;
        let mut need = self.opp_hand_len as usize;
        let trump_in_opp = self.deck_len == 0
            && !self.hand.contains(self.trump_card)
            && !self.played.contains(self.trump_card)
            && self.table != Some(self.trump_card);
        if trump_in_opp {
            hands[opp].insert(self.trump_card);
            need -= 1;
        }
        for &c in &unknown[..need] {
            hands[opp].insert(c);
        }
        let rest = &unknown[need..];
        let mut deck = [0u8; NUM_CARDS];
        if self.deck_len > 0 {
            debug_assert_eq!(rest.len() + 1, self.deck_len as usize);
            deck[0] = self.trump_card;
            deck[1..self.deck_len as usize].copy_from_slice(rest);
        } else {
            debug_assert!(rest.is_empty());
        }
        Game {
            hands,
            deck,
            deck_len: self.deck_len,
            trump_card: self.trump_card,
            trump: self.trump,
            leader: self.leader,
            table: self.table,
            points: self.points,
            played: self.played,
            trump_card_owner: self.trump_card_owner,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::SmallRng;
    use rand::SeedableRng;

    #[test]
    fn full_random_game_sums_to_120() {
        let mut rng = SmallRng::seed_from_u64(1);
        for seed in 0..500u64 {
            let mut g = Game::new(&mut rng, (seed % 2) as u8);
            let mut tricks = 0;
            while !g.is_over() {
                let moves = g.legal_moves().to_vec();
                let c = moves[rng.gen_range(0..moves.len())];
                if g.play(c).is_some() {
                    tricks += 1;
                }
            }
            assert_eq!(tricks, 20);
            assert_eq!(g.points[0] as u32 + g.points[1] as u32, 120);
            assert_eq!(g.played, CardSet::FULL);
        }
    }

    #[test]
    fn determinization_is_consistent() {
        let mut rng = SmallRng::seed_from_u64(7);
        for _ in 0..300 {
            let mut g = Game::new(&mut rng, 0);
            let steps = rng.gen_range(0..39);
            for _ in 0..steps {
                let m = g.legal_moves().to_vec();
                g.play(m[rng.gen_range(0..m.len())]);
            }
            if g.is_over() {
                continue;
            }
            let p = g.to_move();
            let obs = Observation::of(&g, p);
            let d = obs.determinize(&mut rng);
            assert_eq!(d.hands[p as usize], g.hands[p as usize]);
            assert_eq!(d.hands[1 - p as usize].len(), g.hands[1 - p as usize].len());
            assert_eq!(d.deck_len, g.deck_len);
            let all = d.hands[0].union(d.hands[1]).union(d.played)
                .union(d.deck[..d.deck_len as usize].iter().copied().collect())
                .union(d.table.into_iter().collect());
            assert_eq!(all, CardSet::FULL);
        }
    }
}
