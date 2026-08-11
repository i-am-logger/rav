//! Deterministic-shape signal sources for the pipeline tests.
//!
//! Only what the tests actually drive: a tone to check that a frequency lands in
//! the right bar, broadband noise to check that the mapping spreads across the
//! display, and silence to check that nothing invents energy.

use std::f32::consts::PI;

pub struct AudioGenerator {
    sample_rate: f32,
    /// Phase carries across calls, so consecutive blocks join without a click.
    time: f32,
}

impl AudioGenerator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            time: 0.0,
        }
    }

    pub fn generate_silence(&self, samples: usize) -> Vec<f32> {
        vec![0.0; samples]
    }

    /// A pure sine at `frequency`, continuing from wherever the last call left
    /// the phase.
    pub fn sine_wave(&mut self, frequency: f32, amplitude: f32, samples: usize) -> Vec<f32> {
        let dt = 1.0 / self.sample_rate;
        let mut signal = Vec::with_capacity(samples);
        for _ in 0..samples {
            signal.push(amplitude * (2.0 * PI * frequency * self.time).sin());
            self.time += dt;
        }
        signal
    }

    /// White noise: every frequency carries the same energy, on average.
    ///
    /// What "does the whole display light up" wants, and the reason it is white
    /// rather than pink - pink falls 3dB per octave, so it would put least
    /// energy exactly where a spectrum analyser has the most bars.
    ///
    /// This was called `pink_noise` and was not pink. It summed eight octaves of
    /// white noise at falling amplitudes, which is a description of the
    /// Voss-McCartney algorithm with the part that makes it work left out: each
    /// term was independent white noise over the whole band, so the sum was
    /// white noise with a larger variance and no 1/f tilt anywhere. Measured
    /// against a known white source through the same analysis, the two had the
    /// same spectral shape to within 2% across every octave - a constant ratio,
    /// which is what "identical shape" looks like.
    ///
    /// Seeded rather than drawn from the thread's generator, so a failure is
    /// reproducible. A test source that cannot be replayed turns a rare failure
    /// into a mystery, and this module's whole claim is to be deterministic.
    pub fn white_noise(&self, amplitude: f32, samples: usize) -> Vec<f32> {
        // A plain LCG, written out rather than pulled in: the numbers are
        // Numerical Recipes', the sequence has to be the same everywhere, and
        // nothing here needs the quality of a real generator.
        let mut seed: u32 = 0x5eed_1234;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            // The high bits are the good ones in an LCG; the low bits cycle.
            (seed >> 8) as f32 / 8_388_608.0 - 1.0
        };
        (0..samples).map(|_| amplitude * next()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_swings_both_ways_and_starts_at_zero() {
        let mut generator = AudioGenerator::new(44100.0);
        let signal = generator.sine_wave(440.0, 1.0, 100);

        assert_eq!(signal.len(), 100);
        assert!(signal[0].abs() < 0.1);
        assert!(signal.iter().any(|&x| x > 0.5));
        assert!(signal.iter().any(|&x| x < -0.5));
    }

    #[test]
    fn successive_blocks_continue_the_same_wave() {
        // Restarting the phase each call would put a step at every block
        // boundary, which shows up in the FFT as broadband energy that is not
        // in the signal.
        let mut generator = AudioGenerator::new(48_000.0);
        let first = generator.sine_wave(1000.0, 1.0, 48);
        let second = generator.sine_wave(1000.0, 1.0, 48);
        assert_ne!(first, second, "phase did not advance between calls");
    }

    #[test]
    fn white_noise_is_bounded_and_two_sided() {
        let generator = AudioGenerator::new(44100.0);
        let signal = generator.white_noise(1.0, 1000);

        assert_eq!(signal.len(), 1000);
        assert!(signal.iter().any(|&x| x > 0.0));
        assert!(signal.iter().any(|&x| x < 0.0));
        assert!(signal.iter().all(|&x| x.abs() <= 1.0));
    }

    #[test]
    fn the_same_noise_comes_back_every_time() {
        // The module's claim, and it was not true while the noise came from the
        // thread's generator: a test that fails once in a thousand runs is a
        // mystery rather than a bug report if the input cannot be replayed.
        let generator = AudioGenerator::new(48_000.0);
        assert_eq!(
            generator.white_noise(1.0, 256),
            generator.white_noise(1.0, 256),
        );
        assert_eq!(
            AudioGenerator::new(44_100.0).white_noise(1.0, 256),
            generator.white_noise(1.0, 256),
            "the rate does not change the noise, only how long it lasts",
        );
    }

    #[test]
    fn white_noise_is_flat_rather_than_tilted() {
        // The property the old name got wrong. Split the signal in two halves by
        // the sign of a coarse high-pass - if one end carried more energy than
        // the other the noise would be tinted, and a display test built on it
        // would be asserting the tint rather than the display.
        let signal = AudioGenerator::new(48_000.0).white_noise(1.0, 8192);
        // The difference of neighbouring samples isolates the high end and
        // their sum isolates the low. For independent samples both come to
        // twice the variance, so the ratio is 1 - measured at 0.9992 here - and
        // noise tilted towards the low end drives it below that.
        let (mut high, mut low) = (0.0f64, 0.0f64);
        for pair in signal.windows(2) {
            high += f64::from(pair[1] - pair[0]).powi(2);
            low += f64::from(pair[1] + pair[0]).powi(2);
        }
        let ratio = high / low;
        assert!(
            (0.8..=1.25).contains(&ratio),
            "the noise is tinted: high/low power {ratio:.3}, flat is 1.0",
        );
    }

    #[test]
    fn silence_is_silent() {
        let generator = AudioGenerator::new(44100.0);
        assert!(generator.generate_silence(64).iter().all(|&x| x == 0.0));
    }
}
