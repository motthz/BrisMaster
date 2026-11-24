//! Laboratorio delle partite umane (fase 3 di Massima 2.0).
//!
//! Una persona gioca contro Massima **senza consigli**. Il tavolo fa da arbitro e conosce
//! tutto, ma per ogni carta giocata registra solo quello che vedeva chi l'ha giocata:
//! la sua mano, la carta sul tavolo, la briscola, i punti, la storia pubblica.
//! Il formato è `human_decision_v1` (vedi `docs/FASE3-PARTITE-UMANE.md`): una riga CSV
//! per ogni carta giocata, sia dalla persona sia da Massima, così che ogni partita si possa
//! ricostruire e controllare per intero con [`valida`].
//!
//! Validatore da riga di comando: `cargo run -p briscola-lab --bin lab-valida -- FILE.csv`.

pub mod riga;
pub mod tavolo;
pub mod valida;

pub use riga::{Origine, Riga, INTESTAZIONE, SCHEMA};
pub use tavolo::{Tavolo, Vista};
pub use valida::{valida, Rapporto};

/// Regole codificate: briscola italiana a due, 40 carte, 3 in mano, nessun obbligo di risposta.
pub const REGOLE: &str = "briscola-1v1-v1";
