//! Same hands and samples on every run; does not modify trained strategy files.
use poker_it::draw::analyze;
use rand::{rngs::SmallRng, seq::SliceRandom, SeedableRng};
use std::{hint::black_box, time::Instant};
fn main() {
    let mut rng = SmallRng::seed_from_u64(20261110);
    let mut deck: Vec<u8> = (0..32).collect();
    let hands: Vec<[u8;5]> = (0..200).map(|_| { deck.shuffle(&mut rng); let mut h = deck[..5].to_vec(); h.sort_unstable(); h.try_into().unwrap() }).collect();
    let _ = analyze(&hands[0]);
    let now = Instant::now();
    let checksum: f32 = hands.iter().map(|h| black_box(analyze(h).max_ev())).sum();
    println!("draw hands=200 elapsed_ms={:.3} checksum={checksum:.9}", now.elapsed().as_secs_f64()*1000.0);
}
