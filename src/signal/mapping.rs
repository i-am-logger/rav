//! FFT-bin to display-bar mapping.
//!
//! Not a pure logarithmic axis: it blends a logarithmic and a linear one,
//! 91% to 9%, then interpolates between the two adjacent bins.
//!
//! ```text
//! linear = x/(n-1) · (bins-1)
//! log    = 10 ^ (log10(bins) · x/(n-1))
//! index  = 0.09·linear + 0.91·log
//! ```
//!
//! That 9% linear term is not a rounding detail — it is what stops the bottom of
//! the spectrum collapsing. A pure log axis over 512 bins puts the first dozen
//! bars inside a single bin, so they move as one blob; the linear term spreads
//! them back out. It is also why this needs no second bass FFT, no constant-Q
//! and no fractional-octave scheme: the problem is solved on the mapping side
//! rather than by buying more resolution.

/// Bin count the reference maps against. Kept as its own constant so a different
/// FFT size rescales onto the same curve rather than changing its shape.
pub const REFERENCE_BINS: f32 = 512.0;

/// The blend. 0.0 would be fully linear, 1.0 fully logarithmic.
pub const DEFAULT_SCALE: f32 = 0.91;

/// Upper display limits to cycle through, in Hz. `None` is the full spectrum,
/// which is what the original did.
pub const FREQUENCY_LIMITS: [Option<u32>; 5] =
    [Some(8_000), Some(12_000), Some(16_000), Some(20_000), None];

/// Index into [`FREQUENCY_LIMITS`] used by default. 16kHz keeps the near-silent
/// top of the spectrum from occupying a third of the display.
pub const DEFAULT_LIMIT_INDEX: usize = 2;

/// FFT bin that `hz` falls in, for a stream at `sample_rate`.
pub fn bin_for_hz(hz: u32, sample_rate: u32, bins: usize) -> usize {
    if sample_rate == 0 {
        return bins;
    }
    let nyquist = sample_rate as f32 / 2.0;
    let fraction = (hz as f32 / nyquist).clamp(0.0, 1.0);
    ((fraction * bins as f32).round() as usize).clamp(1, bins)
}

/// Fractional bin positions, one per display bar.
#[derive(Debug, Clone)]
pub struct BarMap {
    positions: Vec<f32>,
    bins: usize,
}

impl BarMap {
    /// Build a mapping of `bars` display bars over the whole spectrum.
    pub fn new(bars: usize, bins: usize, scale: f32) -> Self {
        Self::with_top_bin(bars, bins, bins, scale)
    }

    /// Build a mapping that stops at `top_bin` instead of Nyquist.
    ///
    /// The original always spanned the full spectrum, which on a 48kHz stream means the
    /// top third of the display covers 12–24kHz — where most music has almost no
    /// energy, so those bars never move. Capping the range spends the width on
    /// the part of the spectrum that is actually doing something.
    pub fn with_top_bin(bars: usize, bins: usize, top_bin: usize, scale: f32) -> Self {
        // clamp(1, bins) panics when bins == 0, since min > max.
        let top_bin = top_bin.clamp(1, bins.max(1));
        let scale = scale.clamp(0.0, 1.0);
        let log_max = REFERENCE_BINS.log10();
        let stretch = top_bin as f32 / REFERENCE_BINS;
        let last = (bars.saturating_sub(1)).max(1) as f32;

        let positions = (0..bars)
            .map(|x| {
                let t = x as f32 / last;
                let linear = t * (REFERENCE_BINS - 1.0);
                let logarithmic = 10f32.powf(log_max * t);
                let blended = (1.0 - scale) * linear + scale * logarithmic;
                (blended * stretch).clamp(0.0, (bins - 1) as f32)
            })
            .collect();

        Self { positions, bins }
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    pub fn positions(&self) -> &[f32] {
        &self.positions
    }

    /// Sample `magnitudes` at each bar, interpolating between adjacent bins.
    pub fn sample(&self, magnitudes: &[f32], out: &mut Vec<f32>) {
        out.clear();
        out.reserve(self.positions.len());
        if magnitudes.is_empty() {
            out.resize(self.positions.len(), 0.0);
            return;
        }

        for &pos in &self.positions {
            let lo = pos.floor() as usize;
            let hi = (lo + 1).min(self.bins - 1).min(magnitudes.len() - 1);
            let lo = lo.min(magnitudes.len() - 1);
            let frac = pos - pos.floor();
            out.push(magnitudes[lo] * (1.0 - frac) + magnitudes[hi] * frac);
        }
    }
}

/// How many bars are grouped together before drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bandwidth {
    /// The default: average in groups of 4.
    Wide,
    /// One drawn band per sampled bar.
    Thin,
}

impl Bandwidth {
    pub const GROUP: usize = 4;

    /// Reduce sampled bars to drawn bands.
    ///
    /// The original averages groups of 4 but always divides by 4, so a ragged final
    /// group reads low, and it also drops that group's trailing gap. Neither
    /// quirk is reproduced here: every group divides by its real membership.
    pub fn group(self, samples: &[f32], out: &mut Vec<f32>) {
        out.clear();
        match self {
            Bandwidth::Thin => out.extend_from_slice(samples),
            Bandwidth::Wide => {
                for chunk in samples.chunks(Self::GROUP) {
                    out.push(chunk.iter().sum::<f32>() / chunk.len() as f32);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_the_spectrum_and_increases() {
        let m = BarMap::new(75, 512, DEFAULT_SCALE);
        let p = m.positions();
        assert_eq!(p.len(), 75);
        assert!((p[0] - 0.91).abs() < 0.01, "first bar at {}", p[0]);
        assert!((p[74] - 511.0).abs() < 1.0, "last bar at {}", p[74]);
        for i in 1..p.len() {
            assert!(p[i] > p[i - 1], "not increasing at bar {i}");
        }
    }

    #[test]
    fn the_linear_term_keeps_the_low_end_from_collapsing() {
        // With a pure log axis the first bars share one bin and move as a blob.
        let pure_log = BarMap::new(75, 512, 1.0);
        let blended = BarMap::new(75, 512, DEFAULT_SCALE);
        let log_gap = pure_log.positions()[1] - pure_log.positions()[0];
        let blend_gap = blended.positions()[1] - blended.positions()[0];
        assert!(
            log_gap < 0.2,
            "pure log should be degenerate, got {log_gap}"
        );
        assert!(
            blend_gap > 0.5,
            "blend should separate the low bars, got {blend_gap}"
        );
    }

    #[test]
    fn a_flat_spectrum_samples_flat() {
        let m = BarMap::new(40, 512, DEFAULT_SCALE);
        let mut out = Vec::new();
        m.sample(&vec![1.0; 512], &mut out);
        assert!(out.iter().all(|v| (v - 1.0).abs() < 1e-6));
    }

    #[test]
    fn interpolation_recovers_a_ramp_exactly() {
        // On mag[k] = k, sampling at fractional position p must return p. Catches
        // every off-by-one and sign error in the interpolation in one assertion.
        let m = BarMap::new(60, 512, DEFAULT_SCALE);
        let ramp: Vec<f32> = (0..512).map(|k| k as f32).collect();
        let mut out = Vec::new();
        m.sample(&ramp, &mut out);
        for (got, want) in out.iter().zip(m.positions()) {
            assert!((got - want).abs() < 1e-3, "got {got}, want {want}");
        }
    }

    #[test]
    fn every_bar_is_reachable_and_none_are_out_of_range() {
        for bars in [1, 2, 19, 75, 300, 900] {
            let m = BarMap::new(bars, 512, DEFAULT_SCALE);
            assert_eq!(m.len(), bars);
            assert!(m.positions().iter().all(|&p| (0.0..=511.0).contains(&p)));
            let mut out = Vec::new();
            m.sample(&vec![0.5; 512], &mut out);
            assert_eq!(out.len(), bars);
        }
    }

    #[test]
    fn wide_grouping_averages_by_real_membership() {
        let mut out = Vec::new();
        // 6 values -> groups of 4 and 2; the ragged tail must not read low.
        Bandwidth::Wide.group(&[1.0, 1.0, 1.0, 1.0, 2.0, 2.0], &mut out);
        assert_eq!(out, vec![1.0, 2.0]);

        Bandwidth::Thin.group(&[1.0, 2.0, 3.0], &mut out);
        assert_eq!(out, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn a_frequency_limit_spends_the_width_on_the_lower_spectrum() {
        // Full range puts the top bar at Nyquist; 16kHz on a 48k stream stops at
        // bin 341, which keeps the near-silent 16-24kHz region from eating a
        // third of the display.
        let bins = 512;
        let top = bin_for_hz(16_000, 48_000, bins);
        assert_eq!(top, 341);
        let full = BarMap::new(75, bins, DEFAULT_SCALE);
        let capped = BarMap::with_top_bin(75, bins, top, DEFAULT_SCALE);
        assert!(capped.positions()[74] < full.positions()[74]);
        assert!((capped.positions()[74] - 341.0).abs() < 2.0);
    }

    #[test]
    fn bin_for_hz_clamps_rather_than_overflowing() {
        assert_eq!(bin_for_hz(0, 48_000, 512), 1, "never returns bin 0");
        assert_eq!(bin_for_hz(96_000, 48_000, 512), 512, "above Nyquist clamps");
        assert_eq!(
            bin_for_hz(16_000, 0, 512),
            512,
            "unknown rate is full range"
        );
    }

    #[test]
    fn a_capped_map_still_spans_and_increases() {
        let m = BarMap::with_top_bin(75, 512, 171, DEFAULT_SCALE);
        for i in 1..m.len() {
            assert!(m.positions()[i] > m.positions()[i - 1], "flat at {i}");
        }
    }

    #[test]
    fn handles_empty_input_without_panicking() {
        let m = BarMap::new(10, 512, DEFAULT_SCALE);
        let mut out = Vec::new();
        m.sample(&[], &mut out);
        assert_eq!(out, vec![0.0; 10]);
    }
}
