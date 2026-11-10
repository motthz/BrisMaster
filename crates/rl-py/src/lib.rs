//! Ambiente di briscola vettoriale per l'allenamento in Python (PyTorch).
//!
//! `VecEnv(n, seed)` gioca `n` partite in parallelo, tutte allo stesso passo: ogni partita
//! dura esattamente 40 mosse, quindi un giro completo è `reset()` + 40 × (`obs()`, `step()`).

use briscola_core::cards::{Card, NUM_CARDS};
use briscola_core::rl::{canon_card, encode, uncanon, RlNet, RL_IN, RL_OUT};
use briscola_core::{Game, Observation};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

#[pyclass]
struct VecEnv {
    games: Vec<Game>,
    hist: Vec<Vec<(u8, Card)>>,
    rng: SmallRng,
}

#[pymethods]
impl VecEnv {
    #[new]
    fn new(n: usize, seed: u64) -> Self {
        let mut rng = SmallRng::seed_from_u64(seed);
        let games = (0..n).map(|_| { let f = rng.gen_range(0..2); Game::new(&mut rng, f) }).collect();
        VecEnv { games, hist: vec![Vec::with_capacity(40); n], rng }
    }

    /// Nuove partite (chi apre a caso).
    fn reset(&mut self) {
        for (g, h) in self.games.iter_mut().zip(self.hist.iter_mut()) {
            let f = self.rng.gen_range(0..2);
            *g = Game::new(&mut self.rng, f);
            h.clear();
        }
    }

    /// Per ogni partita: codifica (n × RL_IN byte), mosse legali (n × 40 byte, semi rinumerati),
    /// giocatore che muove (n byte).
    fn obs<'py>(&self, py: Python<'py>) -> (Bound<'py, PyBytes>, Bound<'py, PyBytes>, Bound<'py, PyBytes>) {
        let n = self.games.len();
        let mut x = vec![0u8; n * RL_IN];
        let mut m = vec![0u8; n * NUM_CARDS];
        let mut p = vec![0u8; n];
        for i in 0..n {
            let g = &self.games[i];
            let me = g.to_move();
            let o = Observation::of(g, me);
            encode(&o, &self.hist[i], &mut x[i * RL_IN..(i + 1) * RL_IN]);
            for c in o.hand.iter() {
                m[i * NUM_CARDS + canon_card(c, g.trump)] = 1;
            }
            p[i] = me;
        }
        (PyBytes::new(py, &x), PyBytes::new(py, &m), PyBytes::new(py, &p))
    }

    /// Gioca una carta (indice con i semi rinumerati) in ogni partita.
    fn step(&mut self, actions: &[u8]) -> PyResult<()> {
        if actions.len() != self.games.len() {
            return Err(PyValueError::new_err("numero di azioni diverso dal numero di partite"));
        }
        for (i, &a) in actions.iter().enumerate() {
            let g = &mut self.games[i];
            let c = uncanon(a as usize, g.trump);
            if !g.legal_moves().contains(c) {
                return Err(PyValueError::new_err(format!("mossa illegale nella partita {i}")));
            }
            self.hist[i].push((g.to_move(), c));
            g.play(c);
        }
        Ok(())
    }

    /// Punti finali (n × 2 byte: giocatore 0, giocatore 1).
    fn points<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        let v: Vec<u8> = self.games.iter().flat_map(|g| g.points).collect();
        PyBytes::new(py, &v)
    }

    fn over(&self) -> bool {
        self.games.iter().all(|g| g.is_over())
    }
}

/// Uscite della rete nel formato del file (inferenza Rust), per controllare che coincida
/// con PyTorch: codifiche n × RL_IN byte → n × RL_OUT f32 (logit e valore già in tanh).
#[pyfunction]
fn rust_forward<'py>(py: Python<'py>, net: &[u8], x: &[u8]) -> PyResult<Bound<'py, PyBytes>> {
    let net = RlNet::from_bytes(net).map_err(PyValueError::new_err)?;
    let mut out = Vec::with_capacity(x.len() / RL_IN * RL_OUT * 4);
    for row in x.chunks_exact(RL_IN) {
        let o = net.forward(row);
        for v in o.logits.iter().chain(std::iter::once(&o.value)) {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    Ok(PyBytes::new(py, &out))
}

#[pymodule]
fn briscola_rl(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<VecEnv>()?;
    m.add_function(wrap_pyfunction!(rust_forward, m)?)?;
    m.add("RL_IN", RL_IN)?;
    m.add("RL_OUT", RL_OUT)?;
    m.add("SCALE", briscola_core::rl::rl_scale().to_vec())?;
    Ok(())
}
