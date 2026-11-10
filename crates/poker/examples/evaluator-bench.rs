//! Fixed inputs; timings are kernel measurements, not playing-strength ratings.
use poker_core::{cards::{eval, Card}, equity::equity_vs_random};
use rand::{rngs::SmallRng, seq::SliceRandom, SeedableRng};
use std::{hint::black_box, time::Instant};
fn main() {
    let mut rng = SmallRng::seed_from_u64(20261111);
    let mut deck: Vec<Card> = (0..52).collect();
    let deals: Vec<[Card; 7]> = (0..200_000).map(|_| {
        deck.partial_shuffle(&mut rng, 7);
        deck[..7].try_into().unwrap()
    }).collect();
    let now = Instant::now();
    let checksum: u64 = deals.iter().map(|d| black_box(eval(black_box(d))) as u64).sum();
    println!("eval7 deals={} elapsed_ms={:.3} checksum={checksum}", deals.len(), now.elapsed().as_secs_f64()*1000.0);
    for (name, count) in [("preflop", 0), ("flop", 3), ("turn", 4), ("river", 5)] {
        let mut rng = SmallRng::seed_from_u64(20261112);
        let now = Instant::now();
        let checksum: f64 = deals[..200].iter().map(|d| black_box(equity_vs_random([d[0],d[1]], &d[2..2+count], 4000, &mut rng))).sum();
        println!("{name} deals=200 elapsed_ms={:.3} checksum={checksum:.12}", now.elapsed().as_secs_f64()*1000.0);
    }
}
