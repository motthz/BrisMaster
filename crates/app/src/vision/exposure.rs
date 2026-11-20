//! Compensa una gamma RGB compressa prima di cercare i contorni chiari delle carte.
use super::Img;

pub(super) fn recover(frame: &Img) -> Option<Img> {
    let mut hist = [[0usize; 256]; 3];
    let step = (frame.px.len() / 8192).max(1);
    let mut n = 0;
    for rgb in frame.px.iter().step_by(step) {
        for channel in 0..3 {
            hist[channel][rgb[channel] as usize] += 1;
        }
        n += 1;
    }
    if n < 128 { return None; }
    let bounds = hist.map(|bins| {
        let q = |from_high: bool| {
            let mut count = 0;
            for i in 0..256 {
                let v = if from_high { 255 - i } else { i };
                count += bins[v];
                if count > n / 500 { return v; }
            }
            0
        };
        (q(false), q(true))
    });
    // Con pochi livelli osservati l'espansione amplificherebbe solo il rumore.
    if bounds.iter().any(|&(lo, hi)| hi - lo < 28) { return None; }
    let washed = bounds.iter().all(|&(lo, _)| lo >= 78);
    if !washed && bounds.iter().all(|&(_, hi)| hi >= 115) { return None; }
    let lut = bounds.map(|(lo, hi)| {
        let low = if washed || hi < 115 { lo } else { 0 };
        let high = if washed || hi < 115 { hi } else { 255 };
        std::array::from_fn::<_, 256, _>(|i: usize| {
            ((i.saturating_sub(low) * 255 + (high - low) / 2) / (high - low)).min(255) as u8
        })
    });
    let px = frame.px.iter().map(|rgb| std::array::from_fn(|i| lut[i][rgb[i] as usize])).collect();
    Some(Img { w: frame.w, h: frame.h, px })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_fields_and_full_range_are_untouched() {
        let mut frame = Img::new(64, 64);
        for color in [[0; 3], [38; 3], [193; 3], [12, 97, 151]] {
            frame.px.fill(color);
            assert!(recover(&frame).is_none());
        }
        for (i,p) in frame.px.iter_mut().enumerate() { *p = [(i % 256) as u8; 3]; }
        assert!(recover(&frame).is_none());
        for p in &mut frame.px { p[0] /= 5; }
        let restored = recover(&frame).unwrap();
        assert_eq!(restored.at(63, 3), [255; 3]);
        assert!(restored.px.iter().zip(&frame.px).all(|(a,b)| a[1..] == b[1..]));
    }
}
