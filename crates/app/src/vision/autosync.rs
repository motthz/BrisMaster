//! Stable, role-aware observations and multiset reconciliation. No screen coordinates
//! are guessed from the name of a site, and no betting/action events are invented.
use super::{
    screen::Area,
    universal::{Cards, DeckCard},
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Hand,
    Dealer,
    Board,
    Discard,
    Covered,
    Observed,
}
impl Role {
    pub const ALL: [Self; 6] = [Self::Hand, Self::Dealer, Self::Board, Self::Discard, Self::Covered, Self::Observed];
    pub fn label(self) -> &'static str {
        match self {
            Self::Hand => "La mia mano",
            Self::Dealer => "Banco",
            Self::Board => "Board",
            Self::Discard => "Scarto in cima",
            Self::Covered => "La mia coperta",
            Self::Observed => "Altre carte viste",
        }
    }
}
impl Default for Role {
    fn default() -> Self {
        Self::Hand
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Zone {
    pub role: Role,
    pub rect: [f32; 4],
}
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Layout {
    pub area: Option<Area>,
    pub zones: Vec<Zone>,
}
impl Layout {
    pub fn validate(&self, area: Area) -> Result<(), String> {
        if self.area != Some(area) {
            return Err("L'area del tavolo è cambiata: ridisegna le zone".into());
        }
        for (i, z) in self.zones.iter().enumerate() {
            let [l, t, r, b] = z.rect;
            if !z.rect.iter().all(|x| x.is_finite()) || l < 0.0 || t < 0.0 || r > 1.0 || b > 1.0 || r <= l || b <= t {
                return Err("Zona del tavolo non valida".into());
            }
            for other in &self.zones[..i] {
                let [a, c, d, e] = other.rect;
                if other.role == z.role || (l.max(a) < r.min(d) && t.max(c) < b.min(e)) {
                    return Err("Le zone del tavolo si sovrappongono".into());
                }
            }
        }
        Ok(())
    }
    pub fn role_at(&self, at: [f32; 4]) -> Option<Role> {
        let (x, y) = ((at[0] + at[2]) * 0.5, (at[1] + at[3]) * 0.5);
        let mut matches =
            self.zones.iter().filter(|z| x >= z.rect[0] && x < z.rect[2] && y >= z.rect[1] && y < z.rect[3]);
        let role = matches.next()?.role;
        matches.next().is_none().then_some(role)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    groups: [Vec<DeckCard>; 6],
}
impl Observation {
    pub fn from_scene(scene: &Cards, layout: &Layout, area: Area) -> Result<Self, String> {
        layout.validate(area)?;
        let mut groups: [Vec<DeckCard>; 6] = Default::default();
        let mut ordered: Vec<_> = scene.cards.iter().collect();
        ordered.sort_by(|a, b| a.at[0].total_cmp(&b.at[0]).then(a.at[1].total_cmp(&b.at[1])));
        for c in ordered {
            let Some(role) = layout.role_at(c.at) else { continue };
            let threshold = match c.card {
                DeckCard::Naples(_) => 1.4,
                _ => 0.06,
            };
            if !c.at.iter().all(|v| v.is_finite())
                || c.at[2] <= c.at[0]
                || c.at[3] <= c.at[1]
                || !c.visible.iter().all(|v| v.is_finite())
                || c.card == DeckCard::Unknown
                || !c.margin.is_finite()
                || c.margin < threshold
                || c.visible[1] - c.visible[0] < 0.8
            {
                return Err(format!(
                    "Carta incerta nella zona «{}»: conferma o correggi il riconoscimento",
                    role.label()
                ));
            }
            groups[role as usize].push(c.card);
        }
        // Position/order is not a game event: reshuffling a hand must not duplicate it.
        for cards in &mut groups {
            cards.sort_by_key(|&c| raw_key(c));
        }
        Ok(Self { groups })
    }
    pub fn cards(&self, role: Role) -> &[DeckCard] {
        &self.groups[role as usize]
    }
}
fn raw_key(card: DeckCard) -> u16 {
    match card {
        DeckCard::Naples(c) => c as u16,
        DeckCard::French(c) => 64 + c.min(52) as u16,
        DeckCard::Unknown => 255,
    }
}
#[derive(Clone, Copy)]
pub enum Domain {
    Naples,
    French,
    French52,
    ShortFrench,
    Blackjack,
}
#[derive(Clone)]
pub struct Request {
    pub role: Role,
    pub existing: Vec<DeckCard>,
    pub min_total: usize,
    pub max_total: usize,
    pub max_copies: usize,
    pub domain: Domain,
    pub replace: bool,
}
impl Request {
    pub fn known(role: Role, domain: Domain, existing: Vec<DeckCard>, max_copies: usize) -> Self {
        let n = existing.len();
        Self {
            role,
            domain,
            existing,
            min_total: if role == Role::Observed { 0 } else { n },
            max_total: if role == Role::Observed { 40 } else { n },
            max_copies,
            replace: false,
        }
    }
    fn key(&self, c: DeckCard) -> Result<u16, String> {
        match (self.domain, c) {
            (Domain::Naples, DeckCard::Naples(c @ 0..=39)) => Ok(c as u16),
            (Domain::French, DeckCard::French(c @ 0..=53)) => Ok(c.min(52) as u16),
            (Domain::French52, DeckCard::French(c @ 0..=51)) => Ok(c as u16),
            (Domain::ShortFrench, c) if c.poker_it().is_some() => Ok(c.poker_it().unwrap() as u16),
            (Domain::Blackjack, c) if c.blackjack().is_some() => Ok(c.blackjack().unwrap() as u16),
            _ => Err("Carta incompatibile con il mazzo del gioco".into()),
        }
    }
    pub fn additions(&self, visible: &[DeckCard]) -> Result<Vec<DeckCard>, String> {
        if visible.len() < self.min_total {
            return Ok(Vec::new());
        }
        if visible.len() > self.max_total {
            return Err(format!("Troppe carte nella zona «{}»: verifica il turno o i confini", self.role.label()));
        }
        let mut copies = std::collections::HashMap::new();
        for &c in visible {
            self.key(c)?;
            let n = copies.entry(raw_key(c)).or_insert(0usize);
            *n += 1;
            let limit = if matches!(c, DeckCard::French(52..=53)) { self.max_copies * 2 } else { self.max_copies };
            if *n > limit {
                return Err("Copie della stessa carta incompatibili con il mazzo".into());
            }
        }
        if self.replace {
            if visible.len() != 1 {
                return Err("Per lo scarto serve una sola carta visibile".into());
            }
            return if self.existing.first().map(|&c| self.key(c)).transpose()? == Some(self.key(visible[0])?) {
                Ok(Vec::new())
            } else {
                Ok(visible.to_vec())
            };
        }
        let mut remaining: Vec<_> = self.existing.iter().map(|&c| self.key(c)).collect::<Result<_, _>>()?;
        let mut added = Vec::new();
        for &c in visible {
            let key = self.key(c)?;
            if let Some(i) = remaining.iter().position(|&k| k == key) {
                remaining.remove(i);
            } else {
                added.push(c);
            }
        }
        if !remaining.is_empty() && self.role != Role::Observed {
            return Err("Carte già inserite non più visibili: verifica la fase prima di sincronizzare".into());
        }
        Ok(added)
    }
}
#[derive(Default)]
pub struct Reconciler {
    candidate: Option<(Observation, Instant, u64)>,
    last_frame: Option<u64>,
    stable: Option<Observation>,
    consumed: [Option<Vec<DeckCard>>; 6],
}
impl Reconciler {
    pub fn interrupt(&mut self) {
        self.candidate = None;
        self.stable = None;
    }
    pub fn observe(&mut self, observation: Observation, frame: u64, now: Instant) {
        if self.last_frame.is_some_and(|last| frame <= last) {
            return;
        }
        self.last_frame = Some(frame);
        match &self.candidate {
            Some((old, since, old_frame)) if old == &observation && frame > *old_frame => {
                if now.saturating_duration_since(*since) >= Duration::from_millis(200) {
                    self.stable = Some(observation);
                }
            }
            Some((_, _, old_frame)) if frame <= *old_frame => {}
            _ => {
                self.stable = None;
                self.candidate = Some((observation, now, frame));
            }
        }
    }
    pub fn plan(&self, request: &Request) -> Result<Vec<DeckCard>, String> {
        let Some(stable) = &self.stable else { return Ok(Vec::new()) };
        if self.consumed[request.role as usize].as_deref() == Some(stable.cards(request.role)) {
            return Ok(Vec::new());
        }
        request.additions(stable.cards(request.role))
    }
    pub fn commit(&mut self, role: Role) {
        if let Some(stable) = &self.stable {
            self.consumed[role as usize] = Some(stable.cards(role).to_vec());
        }
    }
    pub fn acknowledge_existing(&mut self, request: &Request) {
        let Some(stable) = &self.stable else { return };
        let visible = stable.cards(request.role);
        if (visible.len() == request.existing.len() || request.role == Role::Observed)
            && visible.len() >= request.min_total
            && request.additions(visible).is_ok_and(|added| added.is_empty())
        {
            self.commit(request.role);
        }
    }
    pub fn has_uncommitted(&self) -> bool {
        self.stable.as_ref().is_none_or(|stable| {
            Role::ALL.iter().any(|&role| {
                let cards = stable.cards(role);
                !cards.is_empty() && self.consumed[role as usize].as_deref() != Some(cards)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observation(cards: Vec<DeckCard>) -> Observation {
        let mut groups = Default::default();
        let g: &mut [Vec<DeckCard>; 6] = &mut groups;
        g[0] = cards;
        Observation { groups }
    }
    fn request() -> Request {
        Request {
            role: Role::Hand,
            existing: vec![],
            min_total: 2,
            max_total: 2,
            max_copies: 1,
            domain: Domain::ShortFrench,
            replace: false,
        }
    }
    #[test]
    fn two_distinct_frames_and_real_time_are_required() {
        let mut sync = Reconciler::default();
        let now = Instant::now();
        let obs = observation(vec![DeckCard::French(0), DeckCard::French(6)]);
        sync.observe(obs.clone(), 1, now);
        sync.observe(obs.clone(), 1, now + Duration::from_secs(1));
        assert!(sync.plan(&request()).unwrap().is_empty());
        sync.observe(obs.clone(), 2, now + Duration::from_millis(100));
        assert!(sync.plan(&request()).unwrap().is_empty());
        sync.observe(obs, 3, now + Duration::from_millis(250));
        assert_eq!(sync.plan(&request()).unwrap().len(), 2);
        sync.commit(Role::Hand);
        assert!(sync.plan(&request()).unwrap().is_empty());
        sync.interrupt();
        assert!(sync.plan(&request()).unwrap().is_empty());
    }
    #[test]
    fn duplicate_copies_are_counted_and_a_reordered_hand_is_not_a_draw() {
        let a = DeckCard::French(6);
        let b = DeckCard::French(7);
        let mut r = request();
        r.domain = Domain::French;
        r.max_total = 3;
        r.existing = vec![a, b];
        assert!(r.additions(&[b, a]).unwrap().is_empty());
        assert!(r.additions(&[a, a, b]).is_err());
        r.max_copies = 2;
        assert_eq!(r.additions(&[a, a, b]).unwrap(), vec![a]);
        assert!(r.additions(&[a, a]).is_err());
        assert!(r.additions(&[a, DeckCard::Unknown]).is_err());
    }
    #[test]
    fn blackjack_compares_values_and_short_deck_rejects_twos() {
        let mut r = request();
        r.domain = Domain::Blackjack;
        r.existing = vec![DeckCard::French(9)];
        assert_eq!(r.additions(&[DeckCard::French(12), DeckCard::French(0)]).unwrap(), vec![DeckCard::French(0)]);
        r.domain = Domain::ShortFrench;
        r.existing.clear();
        assert!(r.additions(&[DeckCard::French(1), DeckCard::French(0)]).is_err());
    }
    #[test]
    fn undo_is_not_immediately_reversed_by_the_same_screen() {
        let mut sync = Reconciler::default();
        let now = Instant::now();
        let obs = observation(vec![DeckCard::French(0), DeckCard::French(6)]);
        sync.observe(obs.clone(), 1, now);
        sync.observe(obs.clone(), 2, now + Duration::from_secs(1));
        sync.commit(Role::Hand);
        sync.observe(obs, 3, now + Duration::from_secs(2));
        assert!(sync.plan(&request()).unwrap().is_empty());
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::vision::{universal::SeenCard, Img, CARD_H, CARD_W, WHOLE};
    #[test]
    fn ambiguous_overlapping_and_changed_capture_areas_are_rejected() {
        let area = Area::default();
        let mut layout =
            Layout { area: Some(area), zones: vec![Zone { role: Role::Hand, rect: [0.1, 0.5, 0.9, 0.9] }] };
        assert!(layout.validate(area).is_ok());
        layout.zones.push(Zone { role: Role::Board, rect: [0.2, 0.4, 0.8, 0.7] });
        assert!(layout.validate(area).is_err());
        layout.zones.pop();
        layout.zones[0].rect[2] = f32::NAN;
        assert!(layout.validate(area).is_err());
        layout.zones[0].rect[2] = 0.9;
        assert!(layout.validate(Area { monitor: 1, ..area }).is_err());
    }
    #[test]
    fn confidence_and_partial_cards_block_a_whole_observation() {
        let area = Area::default();
        let layout = Layout { area: Some(area), zones: vec![Zone { role: Role::Hand, rect: [0.1, 0.5, 0.9, 0.9] }] };
        let card = SeenCard {
            card: DeckCard::French(0),
            image: Img::new(CARD_W, CARD_H),
            at: [0.2, 0.6, 0.3, 0.8],
            margin: 0.2,
            visible: WHOLE,
        };
        let mut cards = Cards { cards: vec![card.clone()] };
        assert_eq!(Observation::from_scene(&cards, &layout, area).unwrap().cards(Role::Hand).len(), 1);
        cards.cards[0].margin = 0.01;
        assert!(Observation::from_scene(&cards, &layout, area).is_err());
        cards.cards[0] = card;
        cards.cards[0].visible = [0.0, 0.4];
        assert!(Observation::from_scene(&cards, &layout, area).is_err());
        cards.cards[0].at = [0.0, 0.0, 0.1, 0.1]; // uncertainty outside configured zones is irrelevant
        assert!(Observation::from_scene(&cards, &layout, area).is_ok());
    }
    #[test]
    fn joker_copies_and_partial_deals_respect_the_actual_deck() {
        let mut r = Request {
            role: Role::Hand,
            existing: vec![],
            min_total: 1,
            max_total: 5,
            max_copies: 2,
            domain: Domain::French,
            replace: false,
        };
        r.domain = Domain::French;
        r.min_total = 1;
        r.max_total = 5;
        r.max_copies = 2;
        assert!(r.additions(&[DeckCard::French(52); 4]).is_ok());
        assert!(r.additions(&[DeckCard::French(52); 5]).is_err());
        assert!(r.additions(&[DeckCard::French(6); 3]).is_err());
    }
}

#[cfg(test)]
mod continuity_tests {
    use super::*;
    fn obs(hand: Vec<DeckCard>, board: Vec<DeckCard>) -> Observation {
        let mut groups: [Vec<DeckCard>; 6] = Default::default();
        groups[Role::Hand as usize] = hand;
        groups[Role::Board as usize] = board;
        Observation { groups }
    }
    #[test]
    fn unrelated_board_changes_and_capture_gaps_cannot_reinsert_an_undone_card() {
        let old = vec![DeckCard::French(6), DeckCard::French(7)];
        let known = Request::known(Role::Hand, Domain::French52, old.clone(), 1);
        let mut sync = Reconciler::default();
        let now = Instant::now();
        sync.observe(obs(old.clone(), vec![]), 1, now);
        sync.observe(obs(old.clone(), vec![]), 2, now + Duration::from_secs(1));
        sync.acknowledge_existing(&known);
        let after_undo = Request { existing: vec![old[0]], min_total: 1, ..known.clone() };
        sync.interrupt();
        sync.acknowledge_existing(&after_undo); // no stable snapshot must not clear consumption
        let changed = obs(old, vec![DeckCard::French(8)]);
        sync.observe(changed.clone(), 3, now + Duration::from_secs(2));
        sync.acknowledge_existing(&after_undo);
        sync.observe(changed, 4, now + Duration::from_secs(3));
        assert!(sync.plan(&after_undo).unwrap().is_empty());
        assert!(sync.has_uncommitted(), "new board must invalidate old guidance until recorded");
    }
    #[test]
    fn observed_history_accumulates_new_cards_without_requiring_old_cards_to_stay_visible() {
        let request = Request {
            role: Role::Observed,
            existing: vec![DeckCard::Naples(1), DeckCard::Naples(2)],
            min_total: 1,
            max_total: 39,
            max_copies: 1,
            domain: Domain::Naples,
            replace: false,
        };
        assert_eq!(request.additions(&[DeckCard::Naples(3)]).unwrap(), vec![DeckCard::Naples(3)]);
        assert!(request.additions(&[DeckCard::Naples(1)]).unwrap().is_empty());
    }
}
