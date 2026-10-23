use burraco::ai::{Heuristic, Player, RandomPlayer, Style};
use burraco::arena::run_deals;

fn style(spec: &str) -> Style {
    let mut s = Style::EXPERT;
    for kv in spec.split(',').filter(|x| !x.is_empty()) {
        let (k, v) = kv.split_once('=').unwrap();
        let f: f32 = v.parse().unwrap();
        match k {
            "pt" => s.pile_threshold = f,
            "pc" => s.pile_card_cost = f,
            "deny" => s.deny_w = f,
            "ew" => s.eager_wilds = f > 0.0,
            "em" => s.eager_melds = f > 0.0,
            "care" => s.care = f,
            "keep" => s.keep_w = f,
            "dump" => s.dump_w = f,
            "cm" => s.close_margin = f as i32,
            "late" => s.late_stock = f as usize,
            "noise" => s.noise = f,
            _ => panic!("{k}"),
        }
    }
    s
}

fn agent(spec: &str) -> Box<dyn Player> {
    match spec {
        "random" => Box::new(RandomPlayer),
        "expert" => Box::new(Heuristic::expert()),
        _ => Box::new(Heuristic::new(spec, style(spec.strip_prefix("h:").unwrap()))),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (from, n): (u64, u64) = match args[1].split_once("..") {
        Some((a, b)) => (a.parse().unwrap(), b.parse().unwrap()),
        None => (0, args[1].parse().unwrap()),
    };
    let b = args[2].clone();
    for a in &args[3..] {
        let t = std::time::Instant::now();
        let st = run_deals(1, from, n, &|| agent(a), &|| agent(&b));
        let (lo, hi) = st.score_ci95();
        println!(
            "{a} vs {b}: score {:.3} [{lo:.3},{hi:.3}] margin {:.0} pts {:.0} closed {:?} {:.1}s",
            st.score(), st.avg_margin(), st.avg_points(), st.closed, t.elapsed().as_secs_f64()
        );
    }
}
