//! Regole della smazzata a due giocatori.
//!
//! - 11 carte a testa, due pozzetti da 11, una carta scoperta sugli scarti, il resto è il tallone.
//! - Al proprio turno si pesca una carta dal tallone oppure si raccolgono tutti gli scarti,
//!   si calano o si attaccano combinazioni e si chiude il turno scartando una carta.
//! - Chi finisce le carte prende il pozzetto: "al volo" se le finisce calando (e continua a giocare),
//!   in mano se le finisce con lo scarto.
//! - Si chiude scartando l'ultima carta (non una matta) dopo aver preso il pozzetto e con almeno un burraco.
//! - Quando il tallone finisce, la smazzata termina alla fine del turno, senza chiusura.
//!
//! Punteggio: carte calate +, carte in mano -, burraco pulito 200, sporco 100, chiusura 100,
//! pozzetto non preso -100.

use crate::cards::{self, Cards, Kind, DECK_SIZE};
use crate::melds::Meld;
use rand::seq::SliceRandom;
use rand::Rng;

pub const HAND_SIZE: usize = 11;
pub const POZZETTO_SIZE: usize = 11;
/// Limite di sicurezza sul numero di turni di una smazzata.
pub const MAX_TURNS: u16 = 400;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Il giocatore di turno deve pescare.
    Draw,
    /// Ha pescato: cala, attacca, poi scarta.
    Play,
    Over,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Action {
    DrawStock,
    TakePile,
    /// Nuova combinazione con queste carte della mano.
    Meld(Vec<Kind>),
    /// Carte della mano attaccate alla propria combinazione con questo indice.
    Attach(usize, Vec<Kind>),
    Discard(Kind),
}

#[derive(Clone, Debug)]
pub struct Game {
    pub hands: [Cards; 2],
    pub melds: [Vec<Meld>; 2],
    /// Tallone: si pesca dalla fine.
    pub stock: Vec<Kind>,
    /// Scarti: l'ultima è quella in cima.
    pub pile: Vec<Kind>,
    /// Il pozzetto che ciascun giocatore prenderà.
    pub pozzetti: [Vec<Kind>; 2],
    pub taken: [bool; 2],
    pub to_move: u8,
    pub phase: Phase,
    /// Carta che non si può riscartare subito (raccolta da sola dagli scarti).
    pub no_discard: Option<Kind>,
    /// Carte raccolte dagli scarti e ancora in mano: le vede anche l'avversario.
    pub known: [Cards; 2],
    /// Carte scartate da ciascun giocatore in tutta la smazzata.
    pub discarded: [Cards; 2],
    pub closed_by: Option<u8>,
    pub turns: u16,
    pub first: u8,
}

impl Game {
    /// Mazzo mescolato delle 108 carte.
    pub fn shuffled_deck(rng: &mut impl Rng) -> Vec<Kind> {
        let mut d = Cards::full_deck().to_vec();
        d.shuffle(rng);
        d
    }

    pub fn new(rng: &mut impl Rng, first: u8) -> Game {
        Game::from_deck(&Game::shuffled_deck(rng), first)
    }

    /// Distribuisce un mazzo già mescolato: lo stesso mazzo dà la stessa smazzata.
    pub fn from_deck(deck: &[Kind], first: u8) -> Game {
        assert_eq!(deck.len(), DECK_SIZE);
        let mut it = deck.iter().copied();
        let mut hands = [Cards::EMPTY; 2];
        for _ in 0..HAND_SIZE {
            for h in hands.iter_mut() {
                h.add(it.next().unwrap());
            }
        }
        let pozzetti = [(&mut it).take(POZZETTO_SIZE).collect(), (&mut it).take(POZZETTO_SIZE).collect()];
        let pile = vec![it.next().unwrap()];
        let stock = it.collect();
        Game {
            hands,
            melds: [Vec::new(), Vec::new()],
            stock,
            pile,
            pozzetti,
            taken: [false; 2],
            to_move: first,
            phase: Phase::Draw,
            no_discard: None,
            known: [Cards::EMPTY; 2],
            discarded: [Cards::EMPTY; 2],
            closed_by: None,
            turns: 0,
            first,
        }
    }

    pub fn is_over(&self) -> bool {
        self.phase == Phase::Over
    }

    pub fn has_burraco(&self, p: u8) -> bool {
        self.melds[p as usize].iter().any(|m| m.is_burraco())
    }

    /// Punti del giocatore se la smazzata finisse adesso.
    pub fn score(&self, p: u8) -> i32 {
        let i = p as usize;
        let table: i32 = self.melds[i].iter().map(|m| m.value() + m.bonus()).sum();
        let close = if self.closed_by == Some(p) { 100 } else { 0 };
        let pozzetto = if self.taken[i] { 0 } else { -100 };
        table + close + pozzetto - self.hands[i].value()
    }

    /// Una mano con cui il giocatore può finire il turno senza restare bloccato.
    pub fn hand_ok(&self, p: u8, hand: &Cards, burraco: bool) -> bool {
        if !self.taken[p as usize] {
            return true;
        }
        match hand.len() {
            0 => false,
            1 => burraco && !cards::is_wild(hand.kinds().next().unwrap()),
            _ => true,
        }
    }

    /// Si può scartare questa carta adesso?
    pub fn can_discard(&self, k: Kind) -> bool {
        let p = self.to_move as usize;
        let h = &self.hands[p];
        if self.phase != Phase::Play || !h.has(k) {
            return false;
        }
        if self.no_discard == Some(k) && h.kinds().any(|o| o != k) {
            return false;
        }
        if h.len() == 1 && self.taken[p] {
            return self.has_burraco(self.to_move) && !cards::is_wild(k);
        }
        true
    }

    fn forget_known(&mut self, p: usize, ks: &[Kind]) {
        for &k in ks {
            self.known[p].remove(k);
        }
    }

    /// Mano vuota prima del pozzetto: lo si prende.
    fn take_pozzetto(&mut self, p: usize) {
        self.taken[p] = true;
        let pz = std::mem::take(&mut self.pozzetti[p]);
        self.hands[p] = Cards::from_slice(&pz);
    }

    /// Esegue un'azione del giocatore di turno.
    pub fn apply(&mut self, a: &Action) -> Result<(), &'static str> {
        let p = self.to_move as usize;
        match (self.phase, a) {
            (Phase::Over, _) => Err("la smazzata è finita"),
            (Phase::Draw, Action::DrawStock) => {
                let k = self.stock.pop().ok_or("il tallone è finito")?;
                self.hands[p].add(k);
                self.phase = Phase::Play;
                Ok(())
            }
            (Phase::Draw, Action::TakePile) => {
                if self.pile.is_empty() {
                    return Err("non ci sono scarti da raccogliere");
                }
                self.no_discard = (self.pile.len() == 1).then(|| self.pile[0]);
                for k in self.pile.drain(..) {
                    self.hands[p].add(k);
                    self.known[p].add(k);
                }
                self.phase = Phase::Play;
                Ok(())
            }
            (Phase::Draw, _) => Err("prima bisogna pescare"),
            (Phase::Play, Action::Meld(ks)) => {
                let m = Meld::new(ks).ok_or("combinazione non valida")?;
                self.lay(p, ks, m, None)
            }
            (Phase::Play, Action::Attach(i, ks)) => {
                let m = self.melds[p].get(*i).ok_or("combinazione inesistente")?;
                let m = m.with(ks).ok_or("le carte non si attaccano a questa combinazione")?;
                self.lay(p, ks, m, Some(*i))
            }
            (Phase::Play, Action::Discard(k)) => {
                if !self.can_discard(*k) {
                    return Err("scarto non consentito");
                }
                self.hands[p].remove(*k);
                self.forget_known(p, &[*k]);
                self.discarded[p].add(*k);
                self.pile.push(*k);
                self.no_discard = None;
                if self.hands[p].is_empty() {
                    if self.taken[p] {
                        self.closed_by = Some(p as u8);
                        self.phase = Phase::Over;
                        return Ok(());
                    }
                    self.take_pozzetto(p);
                }
                self.turns += 1;
                self.to_move = 1 - self.to_move;
                self.phase = if self.turns >= MAX_TURNS || self.stock.is_empty() { Phase::Over } else { Phase::Draw };
                Ok(())
            }
            (Phase::Play, _) => Err("hai già pescato"),
        }
    }

    fn lay(&mut self, p: usize, ks: &[Kind], m: Meld, at: Option<usize>) -> Result<(), &'static str> {
        let mut hand = self.hands[p];
        if !ks.iter().all(|&k| hand.remove(k)) {
            return Err("carte non in mano");
        }
        let burraco = m.is_burraco() || self.has_burraco(p as u8);
        if !self.hand_ok(p as u8, &hand, burraco) {
            return Err("devi tenere almeno una carta per lo scarto");
        }
        self.hands[p] = hand;
        self.forget_known(p, ks);
        match at {
            Some(i) => self.melds[p][i] = m,
            None => self.melds[p].push(m),
        }
        if self.hands[p].is_empty() {
            self.take_pozzetto(p);
        }
        Ok(())
    }

    /// Carte di cui il giocatore `me` non conosce la posizione: mano avversaria (tranne le note),
    /// tallone e pozzetti non ancora presi.
    pub fn unseen(&self, me: u8) -> Cards {
        let mut seen = self.hands[me as usize];
        for ms in &self.melds {
            for m in ms {
                for k in m.cards() {
                    seen.add(k);
                }
            }
        }
        for &k in &self.pile {
            seen.add(k);
        }
        seen.add_all(&self.known[1 - me as usize]);
        Cards::full_deck().minus(&seen)
    }

    /// Ridistribuisce a caso le carte nascoste al giocatore `me`, tenendo ferme quelle che conosce.
    pub fn determinize(&mut self, me: u8, rng: &mut impl Rng) {
        let o = 1 - me as usize;
        let mut pool = self.unseen(me).to_vec();
        pool.shuffle(rng);
        let mut it = pool.into_iter();
        let opp_unknown = self.hands[o].len() - self.known[o].len();
        let mut h = self.known[o];
        for _ in 0..opp_unknown {
            h.add(it.next().unwrap());
        }
        self.hands[o] = h;
        for i in 0..2 {
            let n = self.pozzetti[i].len();
            self.pozzetti[i] = (&mut it).take(n).collect();
        }
        let n = self.stock.len();
        self.stock = (&mut it).take(n).collect();
        debug_assert!(it.next().is_none());
    }

    /// Carte in gioco in tutto (sempre 108): per i controlli.
    pub fn total_cards(&self) -> usize {
        let melded: usize = self.melds.iter().flatten().map(|m| m.len()).sum();
        self.hands[0].len()
            + self.hands[1].len()
            + self.stock.len()
            + self.pile.len()
            + self.pozzetti[0].len()
            + self.pozzetti[1].len()
            + melded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::parse;
    use rand::rngs::SmallRng;
    use rand::SeedableRng;

    fn ks(s: &str) -> Vec<Kind> {
        s.split_whitespace().map(|c| parse(c).unwrap()).collect()
    }

    #[test]
    fn deal_and_conservation() {
        let mut rng = SmallRng::seed_from_u64(1);
        let g = Game::new(&mut rng, 0);
        assert_eq!(g.hands[0].len(), 11);
        assert_eq!(g.stock.len(), 108 - 22 - 22 - 1);
        assert_eq!(g.total_cards(), 108);
    }

    #[test]
    fn pozzetto_in_hand_and_on_the_fly() {
        let mut rng = SmallRng::seed_from_u64(2);
        let mut g = Game::new(&mut rng, 0);
        g.apply(&Action::DrawStock).unwrap();
        g.hands[0] = Cards::from_slice(&ks("3c 4c 5c Kp Kq"));
        g.apply(&Action::Meld(ks("3c 4c 5c"))).unwrap();
        g.apply(&Action::Discard(parse("Kp").unwrap())).unwrap();
        assert_eq!(g.hands[0].len(), 1);
        g.apply(&Action::DrawStock).unwrap();
        let k = g.hands[1].kinds().next().unwrap();
        g.apply(&Action::Discard(k)).unwrap();
        g.apply(&Action::DrawStock).unwrap();
        g.hands[0] = Cards::from_slice(&ks("Kq 6c"));
        g.apply(&Action::Attach(0, ks("6c"))).unwrap();
        // Mano finita con lo scarto: pozzetto in mano e turno all'avversario.
        g.apply(&Action::Discard(parse("Kq").unwrap())).unwrap();
        assert!(g.taken[0]);
        assert_eq!(g.hands[0].len(), 11);
        assert_eq!(g.to_move, 1);
        // L'avversario finisce le carte calando: pozzetto al volo, e continua a giocare.
        g.apply(&Action::DrawStock).unwrap();
        g.hands[1] = Cards::from_slice(&ks("8f 9f 10f"));
        g.apply(&Action::Meld(ks("8f 9f 10f"))).unwrap();
        assert!(g.taken[1]);
        assert_eq!(g.hands[1].len(), 11);
        assert_eq!(g.phase, Phase::Play);
        assert_eq!(g.score(1), 30 - g.hands[1].value());
    }

    #[test]
    fn cannot_close_without_burraco() {
        let mut rng = SmallRng::seed_from_u64(3);
        let mut g = Game::new(&mut rng, 0);
        g.taken[0] = true;
        g.hands[0] = Cards::from_slice(&ks("3c 4c 5c Kp"));
        g.apply(&Action::DrawStock).unwrap();
        g.hands[0] = Cards::from_slice(&ks("3c 4c 5c Kp Kq"));
        g.apply(&Action::Meld(ks("3c 4c 5c"))).unwrap();
        g.apply(&Action::Discard(parse("Kp").unwrap())).unwrap();
        assert_eq!(g.hands[0].len(), 1);
        g.apply(&Action::DrawStock).unwrap();
        let k = g.hands[1].kinds().next().unwrap();
        g.apply(&Action::Discard(k)).unwrap();
        g.apply(&Action::DrawStock).unwrap();
        g.hands[0] = Cards::from_slice(&ks("Kq 6c"));
        // Attaccare il 6 lascerebbe una carta sola senza burraco: vietato.
        assert!(g.apply(&Action::Attach(0, ks("6c"))).is_err());
        g.melds[0][0] = Meld::new(&ks("3c 4c 5c 6c 7c 8c 9c")).unwrap();
        g.hands[0] = Cards::from_slice(&ks("Kq 10c"));
        g.apply(&Action::Attach(0, ks("10c"))).unwrap();
        g.apply(&Action::Discard(parse("Kq").unwrap())).unwrap();
        assert!(g.is_over());
        assert_eq!(g.closed_by, Some(0));
        assert_eq!(g.score(0), 5 * 5 + 10 * 3 + 200 + 100);
    }

    #[test]
    fn take_pile_and_determinize() {
        let mut rng = SmallRng::seed_from_u64(4);
        let mut g = Game::new(&mut rng, 0);
        let top = g.pile[0];
        g.apply(&Action::TakePile).unwrap();
        assert!(g.known[0].has(top));
        // La carta raccolta da sola non si riscarta subito.
        if g.hands[0].kinds().any(|k| k != top) {
            assert!(!g.can_discard(top));
        }
        let other = g.hands[0].kinds().find(|&k| k != top).unwrap();
        g.apply(&Action::Discard(other)).unwrap();
        let before = g.clone();
        g.determinize(1, &mut rng);
        assert_eq!(g.total_cards(), 108);
        assert!(g.hands[0].has(top));
        assert_eq!(g.hands[1], before.hands[1]);
        assert_eq!(g.hands[0].len(), before.hands[0].len());
    }
}
