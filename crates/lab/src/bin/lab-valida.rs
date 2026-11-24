//! Valida uno o più CSV `human_decision_v1` e stampa il rapporto.
//!
//! cargo run --release -p briscola-lab --bin lab-valida -- partite.csv [altre.csv ...]

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("uso: lab-valida FILE.csv [FILE.csv ...]");
        std::process::exit(2);
    }
    let mut testo = String::new();
    for f in &files {
        match std::fs::read_to_string(f) {
            Ok(t) => {
                testo += &t;
                testo.push('\n');
            }
            Err(e) => {
                eprintln!("{f}: {e}");
                std::process::exit(2);
            }
        }
    }
    let r = briscola_lab::valida(&testo);
    print!("{}", r.testo());
    if r.partite_valide == 0 {
        std::process::exit(1);
    }
}
