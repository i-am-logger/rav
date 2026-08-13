//! The analyser front end: Hann envelope, real FFT, per-bin equalise tilt.
//!
//! Three things about it are worth stating plainly, because they contradict how
//! most spectrum analysers work:
//!
//! **The amplitude response is linear.** The output is
//! `sqrt(re² + im²) * equalize[bin]` — the magnitude is never put through a
//! logarithm. The `log10` appears only while *building* the equalise table, which
//! is a set of per-bin constants. There is no dB window, no compression and no
//! per-frame normalisation; loudness is expressed by bars simply being taller,
//! and the only limiting is a hard clip at full scale.
//!
//! **The tilt does the work a dB curve would.** `equalize` rises from ~0.024 at
//! DC to exactly 1.0 at Nyquist, about +32 dB, which offsets the roughly
//! -6 dB/octave slope of most music and is why the top of the spectrum is
//! visible at all.
//!
//! **The window is a periodic Hann.** The original writes it as
//! `(0.5 + 0.5·sin(2πi/N − π/2))^1`, which is identically `0.5 − 0.5·cos(2πi/N)`.

use anyhow::Result;
use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// The original reads 8-bit samples as `(byte - 128) / 24`. A normalised sample `x`
/// arrives as byte `128 + 128x`, so its FFT input is `x · 128/24`. Applying the
/// same factor to float input puts rav on that amplitude scale.
pub const INPUT_GAIN: f32 = 16.0 / 3.0;

/// Tallest a bar can be drawn, in the original's virtual pixels. Doubles as the clip.
pub const MAX_HEIGHT: f32 = 15.0;

const INITIAL_BIAS: f32 = 0.04;
const BIAS_DECAY: f32 = 1.0025;

/// Periodic Hann window: `0.5 - 0.5·cos(2πi/n)`. Coherent gain is exactly 0.5.
fn hann_envelope(n: usize) -> Vec<f32> {
    let mult = std::f32::consts::TAU / n as f32;
    (0..n)
        .map(|i| 0.5 - 0.5 * (i as f32 * mult).cos())
        .collect()
}

/// The per-bin equalise tilt.
///
/// `bias` decays by `1.0025` per bin, and the span is recomputed from the current
/// bias each step, so this is a recurrence rather than a closed form. The last
/// entry is exactly `log10(10) = 1.0` by construction: at `i = m-1` the argument
/// becomes `1 + bias + (9 - bias) = 10` regardless of `m` or `bias`, which is what
/// lets the shape generalise to other FFT sizes.
fn equalize_table(bins: usize) -> Vec<f32> {
    let mut bias = INITIAL_BIAS;
    // Scale the decay so the curve keeps its shape if the FFT size changes.
    let decay = BIAS_DECAY.powf(512.0 / bins as f32);
    (0..bins)
        .map(|i| {
            let span = (9.0 - bias) / bins as f32;
            let value = (1.0 + bias + (i + 1) as f32 * span).log10();
            bias /= decay;
            value
        })
        .collect()
}

/// Turns a block of mono samples into equalised magnitudes. Allocation-free after
/// construction.
pub struct Spectrum {
    fft: Arc<dyn RealToComplex<f32>>,
    envelope: Vec<f32>,
    equalize: Vec<f32>,
    input: Vec<f32>,
    output: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    magnitudes: Vec<f32>,
}

impl Spectrum {
    /// 1024 samples in, 512 bins out, as the original does.
    pub const DEFAULT_SIZE: usize = 1024;

    /// A spectrum over blocks of `fft_size` samples.
    ///
    /// The size has to be a power of two. `rustfft` behind this one is mixed
    /// radix and would take any even number, but the embedded answer -
    /// `microfft`, which allocates nothing and holds a precomputed sine table -
    /// is radix-2 and cannot. Saying so here makes it an API constraint rather
    /// than something found on hardware, which is the whole reason the two
    /// portable crates exist.
    ///
    /// It also gives this `Result` something to return: before, every size was
    /// accepted, `0` included, and a spectrum with no bins was an `Ok`.
    pub fn new(fft_size: usize) -> Result<Self> {
        // Both, and in this order. `1` is a power of two, so the check below
        // lets it through - and `bins` is `fft_size / 2`, which makes it the
        // same empty spectrum `0` was. The two smallest cases are the ones a
        // power-of-two test reads as fine.
        anyhow::ensure!(
            fft_size >= 2,
            "an FFT size has to leave at least one bin, and {fft_size} leaves none",
        );
        anyhow::ensure!(
            fft_size.is_power_of_two(),
            "an FFT size has to be a power of two, and {fft_size} is not",
        );
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(fft_size);
        let bins = fft_size / 2;
        Ok(Self {
            input: fft.make_input_vec(),
            output: fft.make_output_vec(),
            // realfft's `process` allocates a scratch vec on every call; holding
            // our own and using `process_with_scratch` keeps the render path free
            // of per-frame allocation.
            scratch: fft.make_scratch_vec(),
            envelope: hann_envelope(fft_size),
            equalize: equalize_table(bins),
            magnitudes: vec![0.0; bins],
            fft,
        })
    }

    pub fn size(&self) -> usize {
        self.envelope.len()
    }

    pub fn bins(&self) -> usize {
        self.magnitudes.len()
    }

    /// Analyse exactly `size()` mono samples in `-1.0..=1.0`, newest last.
    pub fn analyse(&mut self, frames: &[f32]) -> &[f32] {
        let n = self.input.len();
        for i in 0..n {
            let sample = frames.get(i).copied().unwrap_or(0.0);
            self.input[i] = sample * INPUT_GAIN * self.envelope[i];
        }

        if self
            .fft
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .is_err()
        {
            self.magnitudes.fill(0.0);
            return &self.magnitudes;
        }

        for (i, m) in self.magnitudes.iter_mut().enumerate() {
            *m = self.output[i].norm() * self.equalize[i];
        }
        &self.magnitudes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_is_a_periodic_hann() {
        let w = hann_envelope(1024);
        assert!((w[0] - 0.0).abs() < 1e-6, "starts at zero");
        assert!((w[512] - 1.0).abs() < 1e-6, "peaks at the centre");
        // Coherent gain 0.5 => the window sums to exactly half its length.
        let sum: f32 = w.iter().sum();
        assert!((sum - 512.0).abs() < 0.01, "sum was {sum}");
    }

    #[test]
    fn equalize_ends_at_exactly_one() {
        // Structural: the argument reaches exactly 10 at the last bin, for any size.
        for bins in [256, 512, 2048] {
            let eq = equalize_table(bins);
            assert!(
                (eq[bins - 1] - 1.0).abs() < 1e-6,
                "bins={bins} gave {}",
                eq[bins - 1]
            );
        }
    }

    #[test]
    fn equalize_rises_monotonically_from_a_small_floor() {
        let eq = equalize_table(512);
        assert!(eq[0] > 0.02 && eq[0] < 0.03, "DC bin was {}", eq[0]);
        for i in 1..eq.len() {
            assert!(eq[i] > eq[i - 1], "not monotonic at {i}");
        }
        // ~+32 dB of tilt across the spectrum.
        let tilt_db = 20.0 * (eq[511] / eq[0]).log10();
        assert!((tilt_db - 32.3).abs() < 1.0, "tilt was {tilt_db:.2} dB");
    }

    #[test]
    fn a_full_scale_sine_lands_in_its_own_bin() {
        let mut s = Spectrum::new(1024).unwrap();
        let bin = 64;
        let samples: Vec<f32> = (0..1024)
            .map(|i| (std::f32::consts::TAU * bin as f32 * i as f32 / 1024.0).sin())
            .collect();
        let mag = s.analyse(&samples);
        let peak = mag
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        assert_eq!(peak, bin, "energy should concentrate in bin {bin}");
    }

    #[test]
    fn amplitude_response_is_linear_not_logarithmic() {
        // The defining property: halving the input halves the output. A dB curve
        // would compress this to a fixed offset instead.
        let mut s = Spectrum::new(1024).unwrap();
        let tone = |amp: f32| -> Vec<f32> {
            (0..1024)
                .map(|i| amp * (std::f32::consts::TAU * 64.0 * i as f32 / 1024.0).sin())
                .collect()
        };
        let loud = s.analyse(&tone(1.0))[64];
        let quiet = s.analyse(&tone(0.5))[64];
        assert!(
            (loud / quiet - 2.0).abs() < 0.01,
            "expected a 2:1 ratio, got {:.4}",
            loud / quiet
        );
    }

    #[test]
    fn silence_produces_exactly_zero() {
        let mut s = Spectrum::new(1024).unwrap();
        assert!(s.analyse(&vec![0.0; 1024]).iter().all(|&m| m == 0.0));
    }

    #[test]
    fn short_input_is_zero_padded_rather_than_panicking() {
        let mut s = Spectrum::new(1024).unwrap();
        assert_eq!(s.analyse(&[0.1, 0.2, 0.3]).len(), 512);
    }
    #[test]
    fn an_fft_size_is_a_power_of_two_with_a_bin_in_it() {
        for size in [256, 512, 1024, 2048] {
            assert!(
                Spectrum::new(size).is_ok(),
                "{size} is a power of two and was refused"
            );
        }

        // 1000 is the one that matters: `rustfft` plans it perfectly well, so
        // nothing here would have complained, and the radix-2 crate an
        // embedded target needs cannot do it at all. 0 was an `Ok` with no
        // bins in it.
        // Two bins is the smallest that is not degenerate, and the smallest
        // this accepts - `bins` is `fft_size / 2`.
        assert_eq!(Spectrum::new(2).map(|s| s.bins()).ok(), Some(1));

        // `1` is in here because it is a power of two: a check written only
        // against that reads it as fine, and it builds the same empty spectrum
        // `0` did.
        //
        // `let Err(..) else`, not `expect_err`, which wants the `Ok` side to be
        // `Debug` and a `Spectrum` holds a planner that is not.
        for size in [0, 1, 3, 1000, 1023] {
            let Err(refused) = Spectrum::new(size) else {
                // Not "is not a power of two": 1 is one, and is refused for
                // having no bins. A message that names the wrong reason sends
                // the next reader to the wrong check.
                panic!("{size} is not a usable FFT size and was accepted");
            };
            assert!(
                refused.to_string().contains(&size.to_string()),
                "the error does not say which size: {refused}"
            );
        }
    }
}
