//! Fixed workload for comparing solver time without training or rewriting game data.
use rand::{rngs::SmallRng, seq::SliceRandom, SeedableRng};
use scala40_core::{cards, solver};
use std::time::Instant;
fn main() {
    let mut rng = SmallRng::seed_from_u64(20261110);
    let mut deck: Vec<u8> = (0..108).collect();
    let mut hands = Vec::new();
    for _ in 0..200 {
        deck.shuffle(&mut rng);
        hands.push(cards::counts(&deck[..14]));
    }
    let start = Instant::now();
    let checksum: f32 = hands.iter().map(|n| solver::best(n, &solver::OpenValue).score).sum();
    println!("hands={} elapsed_ms={:.3} checksum={checksum}", hands.len(), start.elapsed().as_secs_f64()*1000.0);
}
