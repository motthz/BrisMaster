//! Strategia media del motore: per ogni situazione astratta, la probabilità di ogni azione.
//! Si salva in un file binario compatto (probabilità a 16 bit).

use crate::abstraction::{self, key, seq_of, Abstraction};
use crate::draw::{strength, summary};
use crate::game::{Observation, Phase};
use std::collections::HashMap;

/// `PKTO01`: astrazione con 16 livelli prima del cambio e 32 dopo (fisso).
/// `PKTO02`: il numero di livelli dopo il cambio è scritto nel file.
/// `PKTO03`: come `PKTO02`, con le chiavi come differenze dalla precedente (varint) e le
/// probabilità a 8 bit: un file circa tre volte più piccolo.
const MAGIC_V1: &[u8; 6] = b"PKTO01";
const MAGIC_V2: &[u8; 6] = b"PKTO02";
const MAGIC: &[u8; 6] = b"PKTO03";

#[derive(Clone, Debug, Default)]
pub struct Strategy {
    pub abs: Abstraction,
    pub iterations: u64,
    keys: Vec<u64>,
    probs: Vec<[f32; 6]>,
}

impl Strategy {
    pub fn from_map(abs: Abstraction, iterations: u64, map: HashMap<u64, [f32; 6]>) -> Strategy {
        let mut v: Vec<(u64, [f32; 6])> = map.into_iter().collect();
        v.sort_unstable_by_key(|e| e.0);
        Strategy { abs, iterations, keys: v.iter().map(|e| e.0).collect(), probs: v.iter().map(|e| e.1).collect() }
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
            Phase::Bet2 => self.abs.post(strength(&obs.hand)),
            _ => self.abs.pre(&summary(&obs.hand)),
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
        out.push(self.abs.post_levels as u8);
        out.extend_from_slice(&(self.abs.thresholds.len() as u32).to_le_bytes());
        for t in &self.abs.thresholds {
            out.extend_from_slice(&t.to_le_bytes());
        }
        out.extend_from_slice(&(self.keys.len() as u32).to_le_bytes());
        let mut prev = 0u64;
        for (&k, p) in self.keys.iter().zip(&self.probs) {
            let mut d = k - prev;
            prev = k;
            loop {
                let byte = (d & 0x7F) as u8;
                d >>= 7;
                if d == 0 {
                    out.push(byte);
                    break;
                }
                out.push(byte | 0x80);
            }
            let n = p.iter().rposition(|&x| x > 0.0).map_or(0, |i| i + 1);
            out.push(n as u8);
            out.extend(p[..n].iter().map(|&x| (x.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
        out
    }

    pub fn from_bytes(b: &[u8]) -> Option<Strategy> {
        let mut r = Reader { b, i: 0 };
        let version = match r.take(6)? {
            m if m == MAGIC_V1 => 1,
            m if m == MAGIC_V2 => 2,
            m if m == MAGIC => 3,
            _ => return None,
        };
        let legacy = version < 3;
        let iterations = u64::from_le_bytes(r.take(8)?.try_into().ok()?);
        let post_levels = if version == 1 { 32 } else { r.take(1)?[0] as usize };
        let nt = r.u32()? as usize;
        let thresholds = (0..nt).map(|_| r.f32()).collect::<Option<Vec<_>>>()?;
        let n = r.u32()? as usize;
        let mut keys = Vec::with_capacity(n);
        let mut probs = Vec::with_capacity(n);
        let mut prev = 0u64;
        for _ in 0..n {
            if legacy {
                keys.push(u64::from_le_bytes(r.take(8)?.try_into().ok()?));
            } else {
                let (mut d, mut shift) = (0u64, 0);
                loop {
                    let byte = r.take(1)?[0];
                    d |= ((byte & 0x7F) as u64) << shift;
                    shift += 7;
                    if byte & 0x80 == 0 {
                        break;
                    }
                }
                prev += d;
                keys.push(prev);
            }
            let m = r.take(1)?[0] as usize;
            let mut p = [0f32; 6];
            for x in p.iter_mut().take(m) {
                *x = if legacy {
                    u16::from_le_bytes(r.take(2)?.try_into().ok()?) as f32 / 65535.0
                } else {
                    r.take(1)?[0] as f32 / 255.0
                };
            }
            probs.push(p);
        }
        Some(Strategy { abs: Abstraction { thresholds, post_levels }, iterations, keys, probs })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_and_loaded() {
        let s = crate::engine::bundled();
        let again = Strategy::from_bytes(&s.to_bytes()).unwrap();
        assert_eq!(again.abs, s.abs);
        assert_eq!(again.keys, s.keys);
        for (a, b) in again.probs.iter().zip(&s.probs) {
            for (x, y) in a.iter().zip(b) {
                assert!((x - y).abs() < 0.003, "{a:?} {b:?}");
            }
        }
    }
}
