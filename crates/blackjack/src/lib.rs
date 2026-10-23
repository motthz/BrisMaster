//! Blackjack: regole configurabili, sabot, calcolo esatto del valore atteso di ogni azione
//! (dipendente dalla composizione del sabot), strategia di base generata dalle regole
//! e simulatore per misurare il rendimento delle strategie contro il banco.

pub mod cards;
pub mod ev;
pub mod rules;
pub mod sim;
pub mod strategy;

pub use cards::{Card, Counts, Shoe};
pub use ev::{ActionEvs, Analyzer};
pub use rules::{DoubleRule, Rules};
pub use strategy::{Action, BasicStrategy, Chart, ChartCell};
