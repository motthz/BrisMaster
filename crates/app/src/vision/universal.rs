//! Deck recognition independent of a site's URL. New game adapters require confirmation:
//! pixels identify cards, not the meaning of an opponent's bet, meld or draw.
use super::{
    detect,
    memory::{self, Memory},
    recognize, CardView, Img, Located, CARD_H, CARD_W, WHOLE,
};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeckCard {
    Naples(u8),
    French(u8),
    Unknown,
}

impl DeckCard {
    pub fn label(self) -> String {
        match self {
            Self::Unknown => "Da identificare".into(),
            Self::Naples(c) => briscola_core::cards::name(c),
            Self::French(c) if c >= 52 => "Jolly".into(),
            Self::French(c) => format!(
                "{} di {}",
                ["A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K"][(c % 13) as usize],
                ["cuori", "quadri", "fiori", "picche"][(c / 13) as usize]
            ),
        }
    }
    pub fn french_kind(self) -> Option<u8> {
        match self {
            Self::French(c) if c < 54 => Some(c.min(52)),
            _ => None,
        }
    }
    pub fn poker(self) -> Option<u8> {
        let Self::French(c @ 0..=51) = self else { return None };
        Some(((c % 13 + 12) % 13) * 4 + [1, 2, 3, 0][(c / 13) as usize])
    }
    pub fn poker_it(self) -> Option<u8> {
        let Self::French(c @ 0..=51) = self else { return None };
        let rank = c % 13;
        if rank != 0 && rank < 6 {
            return None;
        }
        Some((if rank == 0 { 7 } else { rank - 6 }) * 4 + [3, 2, 1, 0][(c / 13) as usize])
    }
    pub fn blackjack(self) -> Option<usize> {
        let Self::French(c @ 0..=51) = self else { return None };
        Some((c % 13).min(9) as usize)
    }
}

#[derive(Clone)]
pub struct SeenCard {
    pub card: DeckCard,
    pub image: Img,
    pub at: [f32; 4],
    /// Similarity margin, not a calibrated probability of correct classification.
    pub margin: f32,
    pub visible: [f32; 2],
}
#[derive(Clone, Default)]
pub struct Cards {
    pub cards: Vec<SeenCard>,
}
impl Located for Cards {
    fn spots(&self) -> Vec<[f32; 4]> {
        self.cards.iter().map(|c| c.at).collect()
    }
}

struct Reference {
    full: Vec<u8>,
    corner: Vec<u8>,
}
fn corner(img: &Img) -> Img {
    let mut out = Img::new(CARD_W, CARD_H);
    for y in 0..CARD_H {
        for x in 0..CARD_W {
            out.px[y * CARD_W + x] = img.sample(2.0 + x as f32 * 0.28, 3.0 + y as f32 * 0.34);
        }
    }
    out
}
pub fn reference_image(c: u8) -> Img {
    static SHEET: OnceLock<Img> = OnceLock::new();
    let sheet = SHEET.get_or_init(|| {
        let img = image::load_from_memory(include_bytes!("../../assets/cartes_francesi.png"))
            .expect("French reference deck")
            .to_rgba8();
        Img::from_rgba(img.width() as usize, img.height() as usize, img.as_raw())
    });
    sheet.crop(
        c as usize % 13 * CARD_W,
        c as usize / 13 * CARD_H,
        (c as usize % 13 + 1) * CARD_W,
        (c as usize / 13 + 1) * CARD_H,
    )
}
fn references() -> &'static [Reference] {
    static REFS: OnceLock<Vec<Reference>> = OnceLock::new();
    REFS.get_or_init(|| {
        (0..54)
            .map(|c| {
                let image = reference_image(c);
                Reference { full: memory::thumb(&image), corner: memory::thumb(&corner(&image)) }
            })
            .collect()
    })
}

fn classify(image: &Img, mem: &Memory) -> Option<(u8, f32)> {
    let full = memory::thumb(image);
    let edge = memory::thumb(&corner(image));
    let learned = mem.similarities::<54>(image, WHOLE);
    let mut scores = [f32::NEG_INFINITY; 53];
    for (c, r) in references().iter().enumerate() {
        let mut score = memory::ncc(&full, &r.full, (0, 40));
        if c < 52 {
            score = score.max(memory::ncc(&edge, &r.corner, (0, 40)) * 0.97);
        }
        if let Some(s) = learned[c].filter(|&s| s >= memory::SAME) {
            score = score.max(s + 0.2);
        }
        // Red and black jokers are the same game kind; they must not make each
        // other look ambiguous merely because the artwork differs only in colour.
        scores[c.min(52)] = scores[c.min(52)].max(score);
    }
    let mut first = (0, f32::NEG_INFINITY);
    let mut second = f32::NEG_INFINITY;
    for (c, score) in scores.into_iter().enumerate() {
        if score > first.1 {
            second = first.1;
            first = (c as u8, score);
        } else {
            second = second.max(score);
        }
    }
    (first.1 >= 0.80 && first.1 - second >= 0.025).then_some((first.0, first.1 - second))
}

pub fn read_french(img: &Img, mem: &Mutex<Memory>) -> Option<Cards> {
    let mem = mem.lock().unwrap_or_else(|e| e.into_inner()).isolated();
    let mut cards = Vec::new();
    for rect in detect::card_regions(img) {
        let image = detect::warp(img, &rect);
        let mut reverse = image.clone();
        reverse.px.reverse();
        let best = [(image, false), (reverse, true)]
            .into_iter()
            .filter_map(|(im, _)| classify(&im, &mem).map(|(c, margin)| (im, c, margin)))
            .max_by(|a, b| a.2.total_cmp(&b.2));
        if let Some((image, c, margin)) = best {
            cards.push(SeenCard { card: DeckCard::French(c), image, margin, at: rect.bounds(img), visible: WHOLE });
        } else {
            // A new site's artwork can be taught explicitly rather than silently
            // assigned the least-bad label. Unknown cards never guide the overlay.
            cards.push(SeenCard {
                card: DeckCard::Unknown,
                image: detect::warp(img, &rect),
                margin: 0.0,
                at: rect.bounds(img),
                visible: WHOLE,
            });
        }
    }
    cards.sort_by(|a, b| a.at[1].total_cmp(&b.at[1]).then(a.at[0].total_cmp(&b.at[0])));
    (!cards.is_empty()).then_some(Cards { cards })
}

pub fn read_naples(img: &Img, mem: &Mutex<Memory>) -> Option<Cards> {
    let mem = mem.lock().unwrap_or_else(|e| e.into_inner()).isolated();
    let cards = detect::analyze(img)
        .cards
        .into_iter()
        .filter_map(|found| {
            let mut view: CardView = found.view;
            recognize(&mut view, &mem);
            let (c, margin) = view.best(briscola_core::cards::CardSet::FULL)?;
            Some(SeenCard { card: DeckCard::Naples(c), image: view.img, at: view.at, margin, visible: view.vis })
        })
        .collect::<Vec<_>>();
    (!cards.is_empty()).then_some(Cards { cards })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_screen_detection_preserves_two_copies_of_the_same_card() {
        let mut table = Img::new(640, 400);
        table.px.fill([18, 80, 57]);
        let card = reference_image(12);
        for left in [60, 240] {
            for y in 0..CARD_H {
                for x in 0..CARD_W {
                    table.px[(y + 120) * table.w + left + x] = card.at(x, y);
                }
            }
        }
        let result = read_french(&table, &Mutex::new(Memory::in_memory())).expect("French cards on table");
        assert_eq!(result.cards.len(), 2);
        assert!(result.cards.iter().all(|c| c.card == DeckCard::French(12)));
        assert!(result.cards.iter().all(|c| c.at[2] > c.at[0] && c.at[3] > c.at[1]));
        // White panels without rank/suit evidence must not turn into cards.
        table.px.fill([255, 255, 255]);
        assert!(read_french(&table, &Mutex::new(Memory::in_memory())).is_none());
    }

    #[test]
    fn every_french_card_and_joker_is_distinguished() {
        let mem = Memory::in_memory();
        for c in 0..54 {
            let image = reference_image(c);
            assert_eq!(classify(&image, &mem).map(|r| r.0), Some(c.min(52)), "card {c}");
        }
        assert!(classify(&Img::new(CARD_W, CARD_H), &mem).is_none());
    }
    #[test]
    fn deck_conversions_preserve_ranks_suits_and_duplicates() {
        let mut poker = std::collections::HashSet::new();
        let mut italian = std::collections::HashSet::new();
        for c in 0..52 {
            let card = DeckCard::French(c);
            let p = card.poker().unwrap();
            assert_eq!((p / 4 + 1) % 13, c % 13);
            poker.insert(p);
            if let Some(p) = card.poker_it() {
                italian.insert(p);
            }
        }
        assert_eq!(poker.len(), 52);
        assert_eq!(italian.len(), 32);
        assert_eq!(DeckCard::French(52).french_kind(), Some(52));
        assert_eq!(DeckCard::French(53).french_kind(), Some(52));
        assert_eq!(DeckCard::French(0).blackjack(), Some(0));
        assert_eq!(DeckCard::French(12).blackjack(), Some(9));
        assert!(DeckCard::French(52).poker().is_none());
    }
}
