//! Regole della scopa a due giocatori (scopa classica, partita a 11 punti).
//!
//! Le carte sono quelle del mazzo napoletano della briscola (`briscola_core::cards`):
//! `seme = c / 10`, indice del valore `c % 10` con 0 = Asso ... 9 = Re.
//! Nella scopa l'Asso vale 1, le carte numerali il loro numero, Fante 8, Cavallo 9, Re 10.

pub use briscola_core::cards::{make, name, rank, short, suit, Card, CardSet, NUM_CARDS, RANK_NAMES, SUIT_NAMES};
use rand::seq::SliceRandom;
use rand::Rng;

/// Punti per vincere la partita.
pub const TARGET: u8 = 11;
/// Il sette di denari.
pub const SETTEBELLO: Card = 6;
/// Valori della primiera: 7 = 21, 6 = 18, Asso = 16, 5 = 15, 4 = 14, 3 = 13, 2 = 12, figure = 10.
pub const PRIMIERA: [u16; 10] = [16, 12, 13, 14, 15, 18, 21, 10, 10, 10];

/// Valore di presa: Asso 1 ... Re 10.
#[inline]
pub fn value(c: Card) -> u8 {
    rank(c) + 1
}

#[inline]
pub fn single(c: Card) -> CardSet {
    CardSet(1u64 << c)
}

/// Somma dei valori di presa di un insieme di carte.
pub fn sum(s: CardSet) -> u32 {
    s.iter().map(|c| value(c) as u32).sum()
}

/// Una mossa: la carta giocata e le carte prese dal tavolo (vuoto = la carta resta in tavola).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Move {
    pub card: Card,
    pub take: CardSet,
}

impl Move {
    pub fn describe(&self) -> String {
        if self.take.is_empty() {
            format!("{} (in tavola)", name(self.card))
        } else {
            let taken: Vec<String> = self.take.iter().map(short).collect();
            format!("{} prende {}", name(self.card), taken.join(" + "))
        }
    }
}

/// Chiama `f` per ogni presa possibile giocando `c` sul tavolo `table`.
/// Regola: se in tavola c'è una carta dello stesso valore si deve prendere quella (una sola);
/// altrimenti si può prendere un gruppo di carte la cui somma è il valore della carta giocata.
/// Restituisce falso se non c'è nessuna presa (la carta resta in tavola).
pub fn for_each_capture(table: CardSet, c: Card, mut f: impl FnMut(CardSet)) -> bool {
    let r = rank(c);
    let mut found = false;
    for t in table.iter() {
        if rank(t) == r {
            f(single(t));
            found = true;
        }
    }
    if found {
        return true;
    }
    // Somme di due o più carte, tutte di valore minore della carta giocata.
    let mut cand = [0 as Card; NUM_CARDS];
    let mut n = 0;
    for t in table.iter() {
        if value(t) < value(c) {
            cand[n] = t;
            n += 1;
        }
    }
    fn dfs(
        cand: &[Card],
        start: usize,
        left: u8,
        acc: CardSet,
        depth: u8,
        found: &mut bool,
        f: &mut dyn FnMut(CardSet),
    ) {
        for i in start..cand.len() {
            let v = value(cand[i]);
            if v > left {
                continue;
            }
            let mut s = acc;
            s.insert(cand[i]);
            if v == left {
                if depth >= 1 {
                    f(s);
                    *found = true;
                }
            } else {
                dfs(cand, i + 1, left - v, s, depth + 1, found, f);
            }
        }
    }
    dfs(&cand[..n], 0, value(c), CardSet::EMPTY, 0, &mut found, &mut f);
    found
}

/// Mosse legali con la mano `hand` sul tavolo `table`, aggiunte a `out`.
pub fn moves_for(hand: CardSet, table: CardSet, out: &mut Vec<Move>) {
    for c in hand.iter() {
        let any = for_each_capture(table, c, |take| out.push(Move { card: c, take }));
        if !any {
            out.push(Move { card: c, take: CardSet::EMPTY });
        }
    }
}

/// Punteggio di una smazzata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HandScore {
    pub cards: [u8; 2],
    pub denari: [u8; 2],
    /// Valore di primiera (0 se manca un seme).
    pub primiera: [u16; 2],
    pub settebello: Option<u8>,
    pub scope: [u8; 2],
    /// Chi prende il punto di carte, denari e primiera (nessuno se pari).
    pub cards_to: Option<u8>,
    pub denari_to: Option<u8>,
    pub primiera_to: Option<u8>,
    pub points: [u8; 2],
}

/// Primiera: la carta migliore di ogni seme; 0 se manca un seme.
pub fn primiera(s: CardSet) -> u16 {
    // Miglior valore di primiera per ogni maschera dei 10 valori di un seme.
    static BEST: std::sync::OnceLock<[u16; 1024]> = std::sync::OnceLock::new();
    let best = BEST.get_or_init(|| {
        let mut t = [0u16; 1024];
        for (m, v) in t.iter_mut().enumerate() {
            *v = (0..10).filter(|r| m >> r & 1 == 1).map(|r| PRIMIERA[r]).max().unwrap_or(0);
        }
        t
    });
    let mut total = 0;
    for su in 0..4 {
        let b = best[(s.0 >> (10 * su) & 0x3FF) as usize];
        if b == 0 {
            return 0;
        }
        total += b;
    }
    total
}

fn more<T: PartialOrd>(a: T, b: T) -> Option<u8> {
    if a > b {
        Some(0)
    } else if b > a {
        Some(1)
    } else {
        None
    }
}

pub fn score(captured: [CardSet; 2], scope: [u8; 2]) -> HandScore {
    let mut h = HandScore { scope, ..Default::default() };
    for p in 0..2 {
        h.cards[p] = captured[p].len() as u8;
        h.denari[p] = (captured[p].0 & 0x3FF).count_ones() as u8;
        h.primiera[p] = primiera(captured[p]);
    }
    h.cards_to = more(h.cards[0], h.cards[1]);
    h.denari_to = more(h.denari[0], h.denari[1]);
    h.primiera_to = more(h.primiera[0], h.primiera[1]);
    h.settebello = (0..2u8).find(|&p| captured[p as usize].contains(SETTEBELLO));
    h.points = scope;
    for w in [h.cards_to, h.denari_to, h.primiera_to, h.settebello].into_iter().flatten() {
        h.points[w as usize] += 1;
    }
    h
}

/// Stato completo (informazione perfetta) di una smazzata.
///
/// Il mazzo si pesca dalla fine: `deck[deck_len - 1]` è la prossima carta.
#[derive(Clone, Copy, Debug)]
pub struct Game {
    pub hands: [CardSet; 2],
    pub table: CardSet,
    pub captured: [CardSet; 2],
    pub scope: [u8; 2],
    pub deck: [Card; NUM_CARDS],
    pub deck_len: u8,
    /// Chi ha distribuito: l'altro gioca per primo.
    pub dealer: u8,
    pub to_move: u8,
    /// Ultimo giocatore che ha preso: a fine smazzata prende le carte rimaste in tavola.
    pub last_taker: Option<u8>,
    /// Smazzata finita (carte rimaste già assegnate).
    pub over: bool,
}

impl Game {
    pub fn new<R: Rng>(rng: &mut R, dealer: u8) -> Game {
        let mut order: Vec<Card> = (0..NUM_CARDS as u8).collect();
        order.shuffle(rng);
        Game::from_order(&order, dealer)
    }

    /// Nuova smazzata da un ordine di mazzo dato (si pesca da `order[39]` in giù):
    /// tre carte a testa, cominciando da chi non distribuisce, poi quattro in tavola.
    pub fn from_order(order: &[Card], dealer: u8) -> Game {
        assert_eq!(order.len(), NUM_CARDS);
        let mut deck = [0u8; NUM_CARDS];
        deck.copy_from_slice(order);
        let mut g = Game {
            hands: [CardSet::EMPTY; 2],
            table: CardSet::EMPTY,
            captured: [CardSet::EMPTY; 2],
            scope: [0, 0],
            deck,
            deck_len: NUM_CARDS as u8,
            dealer,
            to_move: 1 - dealer,
            last_taker: None,
            over: false,
        };
        g.deal();
        for _ in 0..4 {
            let c = g.pop();
            g.table.insert(c);
        }
        g
    }

    #[inline]
    fn pop(&mut self) -> Card {
        self.deck_len -= 1;
        self.deck[self.deck_len as usize]
    }

    fn deal(&mut self) {
        let first = 1 - self.dealer;
        for p in [first, 1 - first] {
            for _ in 0..3 {
                let c = self.pop();
                self.hands[p as usize].insert(c);
            }
        }
    }

    #[inline]
    pub fn is_over(&self) -> bool {
        self.over
    }

    pub fn legal_moves(&self, out: &mut Vec<Move>) {
        out.clear();
        moves_for(self.hands[self.to_move as usize], self.table, out);
    }

    /// Vero se la mossa è ammessa dalle regole.
    pub fn is_legal(&self, m: Move) -> bool {
        if !self.hands[self.to_move as usize].contains(m.card) {
            return false;
        }
        let mut ok = false;
        let any = for_each_capture(self.table, m.card, |t| ok |= t == m.take);
        if any {
            ok
        } else {
            m.take.is_empty()
        }
    }

    /// Vero se dopo questa giocata la smazzata finisce (l'ultima carta non fa scopa).
    #[inline]
    pub fn last_play(&self) -> bool {
        self.deck_len == 0 && self.hands[0].len() + self.hands[1].len() == 1
    }

    /// Gioca una mossa (legale) per chi è di turno. Restituisce vero se è scopa.
    pub fn play(&mut self, m: Move) -> bool {
        let p = self.to_move as usize;
        debug_assert!(self.is_legal(m), "mossa illegale {m:?}");
        let last = self.last_play();
        self.hands[p].remove(m.card);
        let mut scopa = false;
        if m.take.is_empty() {
            self.table.insert(m.card);
        } else {
            self.table = self.table.minus(m.take);
            self.captured[p] = self.captured[p].union(m.take);
            self.captured[p].insert(m.card);
            self.last_taker = Some(p as u8);
            if self.table.is_empty() && !last {
                self.scope[p] += 1;
                scopa = true;
            }
        }
        self.to_move = 1 - self.to_move;
        if self.hands[0].is_empty() && self.hands[1].is_empty() {
            if self.deck_len > 0 {
                self.deal();
            } else {
                if let Some(t) = self.last_taker {
                    self.captured[t as usize] = self.captured[t as usize].union(self.table);
                }
                self.table = CardSet::EMPTY;
                self.over = true;
            }
        }
        scopa
    }

    /// Punteggio della smazzata (da chiamare a smazzata finita).
    pub fn score(&self) -> HandScore {
        score(self.captured, self.scope)
    }

    /// Carte non ancora viste da `me`: mano avversaria e mazzo.
    pub fn unseen_by(&self, me: u8) -> CardSet {
        CardSet::FULL.minus(self.hands[me as usize]).minus(self.table).minus(self.captured[0]).minus(self.captured[1])
    }
}

/// Una giocata dell'avversario nel giro in corso, con ciò che c'era prima: serve a dedurre
/// le carte che ha ancora in mano (nel giro le carte non cambiano finché non si gioca).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OppPlay {
    pub table: CardSet,
    /// Carte già prese da entrambi prima della giocata.
    pub seen: CardSet,
    pub mv: Move,
    /// Carte in mano a chi osserva al momento della giocata.
    pub my_len: u8,
}

impl Default for Move {
    fn default() -> Self {
        Move { card: 0, take: CardSet::EMPTY }
    }
}

/// Ciò che un giocatore vede: niente mano avversaria, niente ordine del mazzo.
/// Le prese di entrambi sono note (le carte prese si vedono quando vengono giocate).
#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub me: u8,
    pub hand: CardSet,
    pub table: CardSet,
    pub opp_hand_len: u8,
    pub deck_len: u8,
    pub captured: [CardSet; 2],
    pub scope: [u8; 2],
    pub dealer: u8,
    pub to_move: u8,
    pub last_taker: Option<u8>,
    /// Punteggio della partita prima di questa smazzata.
    pub match_score: [u8; 2],
    /// Giocate dell'avversario nel giro in corso (al più 2), dalla prima.
    pub opp_plays: [OppPlay; 2],
    pub opp_plays_len: u8,
}

impl Observation {
    pub fn of(g: &Game, me: u8, match_score: [u8; 2]) -> Observation {
        Observation {
            me,
            hand: g.hands[me as usize],
            table: g.table,
            opp_hand_len: g.hands[1 - me as usize].len() as u8,
            deck_len: g.deck_len,
            captured: g.captured,
            scope: g.scope,
            dealer: g.dealer,
            to_move: g.to_move,
            last_taker: g.last_taker,
            match_score,
            opp_plays: [OppPlay::default(); 2],
            opp_plays_len: 0,
        }
    }

    /// Giocate dell'avversario nel giro in corso.
    pub fn opp_plays(&self) -> &[OppPlay] {
        &self.opp_plays[..self.opp_plays_len as usize]
    }

    /// Carte la cui posizione mi è ignota (mano avversaria + mazzo).
    pub fn unknown(&self) -> CardSet {
        CardSet::FULL.minus(self.hand).minus(self.table).minus(self.captured[0]).minus(self.captured[1])
    }

    pub fn legal_moves(&self, out: &mut Vec<Move>) {
        out.clear();
        moves_for(self.hand, self.table, out);
    }

    /// Inizio di smazzata visto da `me`, prima di conoscere tavolo e mano (per l'assistente).
    pub fn new_hand(me: u8, dealer: u8, match_score: [u8; 2]) -> Observation {
        Observation {
            me,
            hand: CardSet::EMPTY,
            table: CardSet::EMPTY,
            opp_hand_len: 3,
            deck_len: (NUM_CARDS - 10) as u8,
            captured: [CardSet::EMPTY; 2],
            scope: [0, 0],
            dealer,
            to_move: 1 - dealer,
            last_taker: None,
            match_score,
            opp_plays: [OppPlay::default(); 2],
            opp_plays_len: 0,
        }
    }

    /// Smazzata finita (carte rimaste in tavola già assegnate).
    pub fn is_over(&self) -> bool {
        self.deck_len == 0 && self.hand.is_empty() && self.opp_hand_len == 0
    }

    /// Mosse legali per chi è di turno: per l'avversario, con le carte che potrebbe avere.
    pub fn is_legal(&self, m: Move) -> bool {
        let can_hold = if self.to_move == self.me { self.hand } else { self.unknown() };
        if !can_hold.contains(m.card) || (self.to_move != self.me && self.opp_hand_len == 0) {
            return false;
        }
        let mut ok = false;
        let any = for_each_capture(self.table, m.card, |t| ok |= t == m.take);
        if any {
            ok
        } else {
            m.take.is_empty()
        }
    }

    /// Applica la mossa di chi è di turno. Se le mani finiscono e il mazzo no, l'avversario
    /// riceve 3 carte e la mano di `me` resta vuota finché non si indicano le carte pescate.
    /// Restituisce vero se è scopa.
    pub fn apply(&mut self, m: Move) -> bool {
        debug_assert!(self.is_legal(m));
        let p = self.to_move as usize;
        if self.to_move == self.me {
            self.hand.remove(m.card);
        } else {
            if (self.opp_plays_len as usize) < self.opp_plays.len() {
                self.opp_plays[self.opp_plays_len as usize] = OppPlay {
                    table: self.table,
                    seen: self.captured[0].union(self.captured[1]),
                    mv: m,
                    my_len: self.hand.len() as u8,
                };
                self.opp_plays_len += 1;
            }
            self.opp_hand_len -= 1;
        }
        let last = self.deck_len == 0 && self.hand.is_empty() && self.opp_hand_len == 0;
        let mut scopa = false;
        if m.take.is_empty() {
            self.table.insert(m.card);
        } else {
            self.table = self.table.minus(m.take);
            self.captured[p] = self.captured[p].union(m.take);
            self.captured[p].insert(m.card);
            self.last_taker = Some(p as u8);
            if self.table.is_empty() && !last {
                self.scope[p] += 1;
                scopa = true;
            }
        }
        self.to_move = 1 - self.to_move;
        if self.hand.is_empty() && self.opp_hand_len == 0 {
            if self.deck_len > 0 {
                self.deck_len -= 6;
                self.opp_hand_len = 3;
                self.opp_plays_len = 0;
            } else {
                if let Some(t) = self.last_taker {
                    self.captured[t as usize] = self.captured[t as usize].union(self.table);
                }
                self.table = CardSet::EMPTY;
            }
        }
        scopa
    }

    pub fn score(&self) -> HandScore {
        score(self.captured, self.scope)
    }

    /// Uno stato completo compatibile con quello che so, con le carte ignote distribuite a caso.
    pub fn determinize<R: Rng>(&self, rng: &mut R) -> Game {
        let me = self.me as usize;
        let mut unknown = self.unknown().to_vec();
        unknown.shuffle(rng);
        let need = self.opp_hand_len as usize;
        debug_assert_eq!(unknown.len(), need + self.deck_len as usize);
        let mut hands = [CardSet::EMPTY; 2];
        hands[me] = self.hand;
        hands[1 - me] = unknown[..need].iter().copied().collect();
        let mut deck = [0u8; NUM_CARDS];
        deck[..self.deck_len as usize].copy_from_slice(&unknown[need..]);
        Game {
            hands,
            table: self.table,
            captured: self.captured,
            scope: self.scope,
            deck,
            deck_len: self.deck_len,
            dealer: self.dealer,
            to_move: self.to_move,
            last_taker: self.last_taker,
            over: false,
        }
    }
}

/// Esito della partita, se finita: chi supera 11 punti, il più alto se ci arrivano entrambi
/// (a pari punteggio sopra l'11 si gioca un'altra smazzata).
pub fn match_winner(score: [u8; 2]) -> Option<u8> {
    if score[0].max(score[1]) >= TARGET && score[0] != score[1] {
        Some(if score[0] > score[1] { 0 } else { 1 })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::SmallRng;
    use rand::SeedableRng;

    fn set(cs: &[Card]) -> CardSet {
        cs.iter().copied().collect()
    }

    #[test]
    fn capture_rules() {
        // Tavolo: 2 di Coppe, 3 di Spade, 5 di Bastoni, 5 di Denari.
        let table = set(&[make(1, 1), make(2, 2), make(3, 4), make(0, 4)]);
        // Con un 5 si deve prendere un 5 (non 2+3).
        let mut v = vec![];
        for_each_capture(table, make(2, 4), |t| v.push(t));
        assert_eq!(v.len(), 2);
        assert!(v.iter().all(|t| t.len() == 1));
        // Con un 7 si prende 2+5 (due modi).
        let mut v = vec![];
        for_each_capture(table, make(1, 6), |t| v.push(t));
        assert_eq!(v.len(), 2);
        assert!(v.iter().all(|t| sum(*t) == 7 && t.len() == 2));
        // Con un Re (10): 2+3+5 in due modi, 5+5.
        let mut v = vec![];
        for_each_capture(table, make(1, 9), |t| v.push(t));
        assert_eq!(v.len(), 3);
        // Con un Asso non si prende niente.
        assert!(!for_each_capture(table, make(1, 0), |_| {}));
    }

    #[test]
    fn primiera_and_score() {
        let all = CardSet::FULL;
        assert_eq!(primiera(all), 84);
        assert_eq!(primiera(set(&[make(0, 6), make(1, 6), make(2, 6)])), 0);
        // Tutte le carte a un giocatore: 4 punti più le scope.
        let h = score([all, CardSet::EMPTY], [2, 1]);
        assert_eq!(h.points, [6, 1]);
        // 20 carte a testa: punto di carte a nessuno.
        let half = CardSet((1u64 << 20) - 1);
        let h = score([half, CardSet::FULL.minus(half)], [0, 0]);
        assert_eq!(h.cards_to, None);
        assert_eq!(h.denari_to, Some(0));
        assert_eq!(h.settebello, Some(0));
    }

    #[test]
    fn random_hands_are_consistent() {
        let mut rng = SmallRng::seed_from_u64(1);
        let mut moves = Vec::new();
        for i in 0..2000 {
            let mut g = Game::new(&mut rng, (i % 2) as u8);
            let mut plies = 0;
            while !g.is_over() {
                g.legal_moves(&mut moves);
                assert!(!moves.is_empty());
                let m = moves[rng.gen_range(0..moves.len())];
                assert!(g.is_legal(m));
                g.play(m);
                plies += 1;
            }
            assert_eq!(plies, 36);
            assert_eq!(g.captured[0].union(g.captured[1]), CardSet::FULL);
            assert_eq!(g.captured[0].0 & g.captured[1].0, 0);
            let s = g.score();
            let base = s.points[0] + s.points[1] - s.scope[0] - s.scope[1];
            assert!(base <= 4);
        }
    }

    #[test]
    fn determinization_is_consistent() {
        let mut rng = SmallRng::seed_from_u64(7);
        let mut moves = Vec::new();
        for _ in 0..500 {
            let mut g = Game::new(&mut rng, 0);
            let steps = rng.gen_range(0..35);
            for _ in 0..steps {
                g.legal_moves(&mut moves);
                g.play(moves[rng.gen_range(0..moves.len())]);
            }
            let p = g.to_move;
            let obs = Observation::of(&g, p, [0, 0]);
            let d = obs.determinize(&mut rng);
            assert_eq!(d.hands[p as usize], g.hands[p as usize]);
            assert_eq!(d.hands[1 - p as usize].len(), g.hands[1 - p as usize].len());
            let deck: CardSet = d.deck[..d.deck_len as usize].iter().copied().collect();
            let all = d.hands[0].union(d.hands[1]).union(d.table).union(d.captured[0]).union(d.captured[1]).union(deck);
            assert_eq!(all, CardSet::FULL);
        }
    }

    #[test]
    fn observation_follows_the_game() {
        let mut rng = SmallRng::seed_from_u64(9);
        let mut moves = Vec::new();
        for i in 0..300 {
            let mut g = Game::new(&mut rng, (i % 2) as u8);
            let mut o = Observation::new_hand(0, g.dealer, [0, 0]);
            o.table = g.table;
            o.hand = g.hands[0];
            while !g.is_over() {
                assert_eq!(o.hand, g.hands[0]);
                g.legal_moves(&mut moves);
                let m = moves[rng.gen_range(0..moves.len())];
                assert!(o.is_legal(m));
                assert_eq!(o.apply(m), g.play(m));
                if o.hand.is_empty() && o.opp_hand_len == 3 {
                    o.hand = g.hands[0];
                }
                let n = Observation::of(&g, 0, [0, 0]);
                assert_eq!(
                    (o.table, o.captured, o.scope, o.deck_len, o.to_move),
                    (n.table, n.captured, n.scope, n.deck_len, n.to_move)
                );
            }
            assert!(o.is_over());
            assert_eq!(o.score(), g.score());
        }
    }

    #[test]
    fn match_end() {
        assert_eq!(match_winner([10, 9]), None);
        assert_eq!(match_winner([11, 9]), Some(0));
        assert_eq!(match_winner([12, 13]), Some(1));
        assert_eq!(match_winner([12, 12]), None);
    }
}
