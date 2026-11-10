//! Deterministic river workload: exact evaluator and the app's equity entry point.
use poker_core::{
    cards::Card,
    equity::{equity_vs_random, river_hs},
};
use rand::{rngs::SmallRng, seq::SliceRandom, SeedableRng};
use std::{hint::black_box, time::Instant};
fn main() {
    let mut rng = SmallRng::seed_from_u64(20261110);
    let mut deck: Vec<Card> = (0..52).collect();
    let hands: Vec<_> = (0..200)
        .map(|_| {
            deck.shuffle(&mut rng);
            deck[..7].to_vec()
        })
        .collect();
    let now = Instant::now();
    let exact: f64 = hands.iter().map(|h| black_box(river_hs([h[0], h[1]], &h[2..]))).sum();
    println!("river_exact hands=200 elapsed_ms={:.3} checksum={exact:.12}", now.elapsed().as_secs_f64() * 1000.0);
    let now = Instant::now();
    let app: f64 = hands.iter().map(|h| black_box(equity_vs_random([h[0], h[1]], &h[2..], 4000, &mut rng))).sum();
    println!("river_app hands=200 elapsed_ms={:.3} checksum={app:.12}", now.elapsed().as_secs_f64() * 1000.0);
}
