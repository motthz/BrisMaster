//! Repeated exact EV queries with the same analyzer, as used in solver workloads.
use blackjack_core::{cards::Counts, ev::{Analyzer, Situation}, Rules};
use std::{hint::black_box, time::Instant};
fn main() {
    let rules = Rules::preset(0);
    let mut analyzer = Analyzer::new(rules, 9);
    let mut counts = Counts::full(rules.decks);
    for c in [9,9,5] { counts.remove(c); }
    let sit = Situation { hands: 1, from_split: false };
    let _ = analyzer.analyze(&counts, &[9,5], sit);
    let now = Instant::now();
    let checksum: f64 = (0..10000).map(|_| black_box(analyzer.analyze(&counts, &[9,5], sit).best().1)).sum();
    println!("blackjack queries=10000 elapsed_ms={:.3} checksum={checksum:.12}", now.elapsed().as_secs_f64()*1000.0);
}
