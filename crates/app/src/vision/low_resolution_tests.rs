use super::synth::{self, Bg, Deal, Look};
use super::*;

fn read_case(img: &Img, expected: &[Option<Card>; 6]) {
    detect::forget_reading_state();
    forget_focus();
    reference::forget_style();
    let start = std::time::Instant::now();
    let scene = read_briscola(img, &Mutex::new(Memory::in_memory())).unwrap_or_default();
    let mut views: [Option<&CardView>; 6] = [None; 6];
    // Le tre posizioni della mano nel renderer, ordinate da sinistra.
    let width = 160.0 * 0.62 + 12.0;
    for view in &scene.hand {
        let center = (view.at[0] + view.at[2]) * 0.5;
        let index = ((center - 0.5) * 1400.0 / width).round() as i32 + 1;
        if (0..3).contains(&index) {
            views[index as usize] = Some(view);
        }
    }
    views[3] = scene.opp_slot.as_ref();
    views[4] = scene.my_slot.as_ref();
    views[5] = scene.trump.as_ref();
    let roles = ["mano0", "mano1", "mano2", "avversario", "mia", "briscola"];
    for ((role, truth), view) in roles.into_iter().zip(expected).zip(views) {
        let actual = view.and_then(|view| view.best(CardSet::FULL));
        let automatic = actual.is_some_and(|(_, margin)| margin >= 1.4);
        println!("slot {role} attesa={truth:?} letta={:?} auto={automatic}", actual.map(|(card, _)| card));
    }
    println!(
        "extra={} us={}",
        scene.hand.len().saturating_sub(views[..3].iter().flatten().count()),
        start.elapsed().as_micros()
    );
}

fn panel(img: &mut Img, x: usize, y: usize, w: usize, h: usize) {
    for row in 0..h {
        for col in 0..w {
            let pixel = match (row, col) {
                (3..=7, 5..=24) => [31, 84, 117],
                (12..=16, 5..=31) | (22..=25, 5..=27) => [42, 45, 45],
                (32..=37, 13..=32) => [162, 53, 40],
                _ => [241, 239, 229],
            };
            img.px[(y + row) * img.w + x + col] = pixel;
        }
    }
}

/// Riduce il viewport intero, quindi sia le carte sia il testo e lo sfondo
/// perdono dettaglio alla stessa scala. I casi restano identici tra release.
#[test]
#[ignore]
fn banco_bassa_risoluzione() {
    let cases = [
        ("standard", Look::default()),
        ("tavolo-chiaro", Look { bg: Bg::Felt([219, 218, 204]), ..Look::default() }),
        ("loghi-sfocatura", Look { writing: true, logo: 0.7, blur: true, ..Look::default() }),
        ("ventaglio-coperta", Look { fan: 8.0, cover: 0.65, pile: 0.82, ..Look::default() }),
    ];
    for style in 0..2 {
        let drawn: Vec<_> = [0, 12, 25, 37, 4, 17].into_iter().map(|c| reference::card_image(style, c)).collect();
        let expected = [Some(0), Some(12), Some(25), Some(37), Some(4), Some(17)];
        let deal = Deal {
            hand: drawn[..3].iter().collect(),
            opp: Some(&drawn[3]),
            mine: Some(&drawn[4]),
            trump: Some(&drawn[5]),
            deck: true,
        };
        for (label, look) in cases {
            let img = synth::table(&deal, &look);
            for factor in 1..=5 {
                println!("caso {label} stile={style} altezza={}", 160 / factor);
                read_case(&shrink_by(img.clone(), factor), &expected);
            }
        }
    }
    let empty = Deal { hand: Vec::new(), opp: None, mine: None, trump: None, deck: false };
    let mut img = synth::table(&empty, &Look { writing: true, logo: 0.8, ..Look::default() });
    for factor in 1..=5 {
        println!("caso vuoto stile=0 altezza={}", 160 / factor);
        read_case(&shrink_by(img.clone(), factor), &[None; 6]);
    }
    for (x, y, w, h) in [(650, 340, 100, 150), (528, 722, 100, 150), (650, 722, 100, 150), (770, 722, 100, 150)] {
        panel(&mut img, x, y, w, h);
    }
    for factor in 1..=5 {
        println!("caso pulsanti stile=0 altezza={}", 160 / factor);
        read_case(&shrink_by(img.clone(), factor), &[None; 6]);
    }
}

#[test]
fn repeated_score_widgets_are_not_cards() {
    let mut image = Img::new(700, 450);
    image.px.fill([39, 108, 75]);
    let colors = [[70, 126, 184], [175, 39, 47], [222, 170, 62], [44, 116, 94]];
    for (x, y) in [(323, 156), (271, 356), (323, 356), (375, 356)] {
        for row in 0..69 {
            for col in 0..43 {
                image.px[(y + row) * image.w + x + col] = [245, 244, 236];
            }
        }
        for (band, color) in colors.into_iter().enumerate() {
            for row in 8 + band * 14..15 + band * 14 {
                for col in 5..=37 - band % 2 * 5 {
                    image.px[(y + row) * image.w + x + col] = color;
                }
            }
        }
    }
    detect::forget_reading_state();
    let memory = Mutex::new(Memory::in_memory());
    assert!(read_briscola(&image, &memory).is_none(), "un riquadro non è una carta");
}

#[test]
fn recovers_the_first_play_on_a_small_light_table() {
    let cards = [3, 13, 20, 17].map(|id| reference::card_image(1, id));
    let mut deal = Deal {
        hand: cards[..3].iter().collect(),
        opp: None,
        mine: None,
        trump: None,
        deck: false,
    };
    let look = Look { bg: Bg::Felt([220, 215, 200]), ..Look::default() };
    let memory = Mutex::new(Memory::in_memory());
    let read = |deal: &Deal<'_>| {
        read_briscola(&shrink_by(synth::table(deal, &look), 4), &memory).unwrap()
    };
    detect::forget_reading_state();
    let initial = read(&deal);
    assert_eq!(initial.hand.len(), 3);
    assert!(initial.opp_slot.is_none() && initial.my_slot.is_none());
    deal.opp = Some(&cards[3]);
    let result = read(&deal);
    let (card, margin) = result.opp_slot.unwrap().best(CardSet::FULL).unwrap();
    assert_eq!(card, 17);
    assert!(margin >= 1.4);
    assert!(result.my_slot.is_none());
}
