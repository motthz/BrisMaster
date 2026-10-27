//! Strategia media del motore: per ogni situazione astratta, la probabilità di ogni azione.
//! Si salva in un file binario compatto (probabilità a 16 bit).

use crate::abstraction::{self, key, post_bucket, pre_bucket, seq_of};
use crate::draw::{strength, summary};
use crate::game::{Observation, Phase};
use std::collections::HashMap;

const MAGIC: &[u8; 6] = b"PKTO01";

#[derive(Clone, Debug, Default)]
pub struct Strategy {
    /// Soglie dei 16 livelli del secchio prima del cambio.
    pub thresholds: Vec<f32>,
    pub iterations: u64,
    keys: Vec<u64>,
    probs: Vec<[f32; 6]>,
}

impl Strategy {
    pub fn from_map(thresholds: Vec<f32>, iterations: u64, map: HashMap<u64, [f32; 6]>) -> Strategy {
        let mut v: Vec<(u64, [f32; 6])> = map.into_iter().collect();
        v.sort_unstable_by_key(|e| e.0);
        Strategy { thresholds, iterations, keys: v.iter().map(|e| e.0).collect(), probs: v.iter().map(|e| e.1).collect() }
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn get(&self, key: u64) -> Option<&[f32; 6]> {
        self.keys.binary_search(&key).ok().map(|i| &self.probs[i])
    }

    /// Il secchio della mano di chi deve decidere.
    pub fn bucket(&self, obs: &Observation) -> u8 {
        match obs.phase {
            Phase::Bet2 => post_bucket(strength(&obs.hand)),
            _ => pre_bucket(&summary(&obs.hand), &self.thresholds),
        }
    }

    /// Probabilità delle azioni astratte (`abstraction::actions`) per questa situazione,
    /// se l'allenamento l'ha mai incontrata.
    pub fn probs(&self, obs: &Observation) -> Option<Vec<f32>> {
        let n = abstraction::actions(obs).len();
        let p = self.get(key(seq_of(&obs.history), self.bucket(obs)))?;
        let s: f32 = p[..n].iter().sum();
        (s > 0.0).then(|| p[..n].iter().map(|x| x / s).collect())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.keys.len() * 21);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.iterations.to_le_bytes());
        out.extend_from_slice(&(self.thresholds.len() as u32).to_le_bytes());
        for t in &self.thresholds {
            out.extend_from_slice(&t.to_le_bytes());
        }
        out.extend_from_slice(&(self.keys.len() as u32).to_le_bytes());
        for (k, p) in self.keys.iter().zip(&self.probs) {
            out.extend_from_slice(&k.to_le_bytes());
            let n = p.iter().rposition(|&x| x > 0.0).map_or(0, |i| i + 1);
            out.push(n as u8);
            for &x in &p[..n] {
                out.extend_from_slice(&((x.clamp(0.0, 1.0) * 65535.0).round() as u16).to_le_bytes());
            }
        }
        out
    }

    pub fn from_bytes(b: &[u8]) -> Option<Strategy> {
        let mut r = Reader { b, i: 0 };
        if r.take(6)? != MAGIC {
            return None;
        }
        let iterations = u64::from_le_bytes(r.take(8)?.try_into().ok()?);
        let nt = r.u32()? as usize;
        let thresholds = (0..nt).map(|_| r.f32()).collect::<Option<Vec<_>>>()?;
        let n = r.u32()? as usize;
        let mut keys = Vec::with_capacity(n);
        let mut probs = Vec::with_capacity(n);
        for _ in 0..n {
            keys.push(u64::from_le_bytes(r.take(8)?.try_into().ok()?));
            let m = r.take(1)?[0] as usize;
            let mut p = [0f32; 6];
            for x in p.iter_mut().take(m) {
                *x = u16::from_le_bytes(r.take(2)?.try_into().ok()?) as f32 / 65535.0;
            }
            probs.push(p);
        }
        Some(Strategy { thresholds, iterations, keys, probs })
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.i..self.i + n)?;
        self.i += n;
        Some(s)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
}
