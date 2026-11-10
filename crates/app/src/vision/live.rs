//! Independent, full-pixel card tracking between recognizer results.
//! Samples keep the original appearance: occlusion must not teach a different card.
use super::Img;

#[derive(Clone)]
pub struct CardTrack {
    pub source: [f32; 4],
    pub current: Option<[f32; 4]>,
    last: [f32; 4],
    samples: Vec<(f32, f32, [u8; 3])>,
}

impl CardTrack {
    pub fn new(img: &Img, source: [f32; 4]) -> Option<Self> {
        if img.w < 2
            || img.h < 2
            || source.iter().any(|v| !v.is_finite())
            || source[2] <= source[0]
            || source[3] <= source[1]
        {
            return None;
        }
        let mut points = Vec::new();
        for y in 0..18 {
            for x in 0..12 {
                points.push(((x as f32 + 0.5) / 12.0, (y as f32 + 0.5) / 18.0));
            }
        }
        // Samples on both sides of each edge lock the contour, not just the picture inside it.
        let ex = 1.5 / ((source[2] - source[0]) * img.w as f32);
        let ey = 1.5 / ((source[3] - source[1]) * img.h as f32);
        for i in 1..10 {
            let u = i as f32 / 10.0;
            points.extend([
                (-ex, u),
                (ex, u),
                (1.0 - ex, u),
                (1.0 + ex, u),
                (u, -ey),
                (u, ey),
                (u, 1.0 - ey),
                (u, 1.0 + ey),
            ]);
        }
        let samples: Vec<_> = points.into_iter().map(|(u, v)| (u, v, sample(img, source, u, v))).collect();
        let mean = samples.iter().map(|p| luminance(p.2)).sum::<f32>() / samples.len() as f32;
        let contrast =
            (samples.iter().map(|p| (luminance(p.2) - mean).powi(2)).sum::<f32>() / samples.len() as f32).sqrt();
        (contrast >= 10.0).then_some(Self { source, current: Some(source), last: source, samples })
    }

    fn score(&self, img: &Img, rect: [f32; 4], stride: usize, limit: f32) -> f32 {
        if rect[0] < 0.0 || rect[1] < 0.0 || rect[2] > 1.0 || rect[3] > 1.0 {
            return f32::INFINITY;
        }
        let count = self.samples.len().div_ceil(stride);
        let mut error = 0.0;
        for &(u, v, old) in self.samples.iter().step_by(stride) {
            let now = sample(img, rect, u, v);
            error += (0..3).map(|c| (now[c] as f32 - old[c] as f32).abs()).sum::<f32>() / 3.0;
            if error > limit * count as f32 {
                return f32::INFINITY;
            }
        }
        error / count as f32
    }

    pub fn follow(&mut self, img: &Img, shift_hint: [f32; 2]) {
        if img.w < 2 || img.h < 2 {
            self.current = None;
            return;
        }
        let hint = self.current.unwrap_or([
            self.source[0] + shift_hint[0],
            self.source[1] + shift_hint[1],
            self.source[2] + shift_hint[0],
            self.source[3] + shift_hint[1],
        ]);
        let base = if self.current.is_some() { self.last } else { hint };
        let translate = |r: [f32; 4], dx: f32, dy: f32| {
            [r[0] + dx / img.w as f32, r[1] + dy / img.h as f32, r[2] + dx / img.w as f32, r[3] + dy / img.h as f32]
        };
        let mut best = (self.score(img, base, 1, 24.0), base);
        if best.0 > 0.5 {
            // Pixel search near the last position avoids quantization even for tiny
            // details that disappear in a coarse pyramid. Early-out bounds its cost.
            for dy in -16..=16 {
                for dx in -16..=16 {
                    let rect = translate(base, dx as f32, dy as f32);
                    let score = self.score(img, rect, 4, best.0.min(32.0));
                    if score < best.0 {
                        best = (score, rect);
                    }
                }
            }
            if best.0 > 18.0 {
                for dy in (-48..=48).step_by(4) {
                    for dx in (-48..=48).step_by(4) {
                        let rect = translate(base, dx as f32, dy as f32);
                        let score = self.score(img, rect, 4, best.0.min(32.0));
                        if score < best.0 {
                            best = (score, rect);
                        }
                    }
                }
            }
            let rough = best.1;
            best.0 = self.score(img, rough, 1, 24.0);
            for dy in -6..=6 {
                for dx in -6..=6 {
                    let rect = translate(rough, dx as f32, dy as f32);
                    let score = self.score(img, rect, 1, best.0.min(24.0));
                    if score < best.0 {
                        best = (score, rect);
                    }
                }
            }
            // Search size separately from translation so zoom cannot choose a
            // false translation first and drag the scale search away from the card.
            for scale in [0.96, 0.98, 1.02, 1.04] {
                let w = ((base[2] - base[0]) * img.w as f32 * scale).floor() / img.w as f32;
                let h = ((base[3] - base[1]) * img.h as f32 * scale).floor() / img.h as f32;
                let scaled = [base[0], base[1], base[0] + w, base[1] + h];
                let mut coarse = (f32::INFINITY, scaled);
                for dy in -6..=6 {
                    for dx in -6..=6 {
                        let rect = translate(scaled, dx as f32, dy as f32);
                        let score = self.score(img, rect, 4, coarse.0.min(32.0));
                        if score < coarse.0 {
                            coarse = (score, rect);
                        }
                    }
                }
                let score = self.score(img, coarse.1, 1, best.0.min(24.0));
                if score < best.0 {
                    best = (score, coarse.1);
                }
            }
        }
        // Never paint a stale box after losing or covering this individual card.
        self.current = (best.0 <= 18.0).then_some(best.1);
        if let Some(rect) = self.current {
            self.last = rect;
        }
    }
}

fn sample(img: &Img, r: [f32; 4], u: f32, v: f32) -> [u8; 3] {
    img.sample((r[0] + u * (r[2] - r[0])) * img.w as f32, (r[1] + v * (r[3] - r[1])) * img.h as f32)
}
fn luminance(p: [u8; 3]) -> f32 {
    p[0] as f32 * 0.3 + p[1] as f32 * 0.6 + p[2] as f32 * 0.1
}

#[cfg(test)]
mod tests {
    use super::*;
    fn card(img: &mut Img, x: usize, y: usize, w: usize, h: usize, seed: usize) {
        for j in 0..h {
            for i in 0..w {
                let sx = i * 80 / w;
                let sy = j * 120 / h;
                let gx = sx / 5;
                let gy = sy / 5;
                let value = if sx < 3 || sy < 3 || sx > 76 || sy > 116 {
                    240
                } else {
                    ((gx * 17 + gy * 29 + gx * gy * seed) % 190 + 40) as u8
                };
                img.px[(y + j) * img.w + x + i] = [value, value, 240u8.saturating_sub(value / 2)];
            }
        }
    }
    fn frame(a: (usize, usize), b: (usize, usize), scale: f32) -> Img {
        let mut img = Img::new(640, 400);
        img.px.fill([20, 80, 45]);
        card(&mut img, a.0, a.1, (80.0 * scale) as usize, (120.0 * scale) as usize, 3);
        card(&mut img, b.0, b.1, 80, 120, 7);
        img
    }
    fn rect(x: usize, y: usize) -> [f32; 4] {
        [x as f32 / 640.0, y as f32 / 400.0, (x + 80) as f32 / 640.0, (y + 120) as f32 / 400.0]
    }
    fn error(r: [f32; 4], want: [f32; 4]) -> f32 {
        r.iter()
            .zip(want)
            .enumerate()
            .map(|(i, (a, b))| (a - b).abs() * if i % 2 == 0 { 640.0 } else { 400.0 })
            .fold(0.0, f32::max)
    }

    #[test]
    fn cards_follow_independent_odd_pixel_motion_and_stop_immediately() {
        let start = frame((100, 180), (350, 160), 1.0);
        let mut a = CardTrack::new(&start, rect(100, 180)).unwrap();
        let mut b = CardTrack::new(&start, rect(350, 160)).unwrap();
        for step in 1..6 {
            let image = frame((100 + step * 7, 180 - step * 3), (350 - step * 5, 160 + step), 1.0);
            a.follow(&image, [0.0, 0.0]);
            b.follow(&image, [0.0, 0.0]);
            assert!(
                error(a.current.expect("first card"), rect(100 + step * 7, 180 - step * 3)) <= 1.01,
                "{:?}",
                a.current
            );
            assert!(
                error(b.current.expect("second card"), rect(350 - step * 5, 160 + step)) <= 1.01,
                "{:?}",
                b.current
            );
        }
        let stopped = a.current.unwrap();
        a.follow(&frame((135, 165), (325, 165), 1.0), [0.0, 0.0]);
        assert!(error(a.current.unwrap(), stopped) < 0.01);
    }

    #[test]
    fn hidden_card_does_not_leave_a_stale_outline_or_hide_its_neighbour() {
        let start = frame((100, 180), (350, 160), 1.0);
        let mut a = CardTrack::new(&start, rect(100, 180)).unwrap();
        let mut b = CardTrack::new(&start, rect(350, 160)).unwrap();
        let mut covered = frame((100, 180), (350, 160), 1.0);
        for y in 175..305 {
            for x in 95..185 {
                covered.px[y * 640 + x] = [20, 80, 45];
            }
        }
        a.follow(&covered, [0.0, 0.0]);
        b.follow(&covered, [0.0, 0.0]);
        assert!(a.current.is_none());
        assert!(b.current.is_some());
    }

    #[test]
    fn follows_a_small_resize() {
        let mut a = CardTrack::new(&frame((100, 180), (350, 160), 1.0), rect(100, 180)).unwrap();
        a.follow(&frame((102, 178), (350, 160), 1.04), [0.0, 0.0]);
        let expected = [102.0 / 640.0, 178.0 / 400.0, 185.0 / 640.0, 302.0 / 400.0];
        assert!(error(a.current.expect("resized card"), expected) <= 2.0, "{:?}", a.current);
    }
}
