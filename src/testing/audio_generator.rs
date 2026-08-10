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

    /// Pink (1/f) noise, summed over octaves and normalised to `amplitude`.
    ///
    /// Approximate rather than spectrally exact - the tests ask only that every
    /// part of the spectrum carries energy, not how much.
    pub fn pink_noise(&self, amplitude: f32, samples: usize) -> Vec<f32> {
        use rand::{Rng, thread_rng};
        let mut rng = thread_rng();

        let mut signal = vec![0.0; samples];
        for octave in 0..8 {
            let amp_mult = 1.0 / 2.0_f32.powi(octave).sqrt();
            for sample in signal.iter_mut() {
                *sample += amplitude * amp_mult * (rng.r#gen::<f32>() * 2.0 - 1.0);
            }
        }

        let max_val = signal.iter().fold(0.0f32, |max, &val| max.max(val.abs()));
        if max_val > 0.0 {
            for sample in &mut signal {
                *sample = (*sample / max_val) * amplitude;
            }
        }

        signal
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
    fn pink_noise_is_bounded_and_two_sided() {
        let generator = AudioGenerator::new(44100.0);
        let signal = generator.pink_noise(1.0, 1000);

        assert_eq!(signal.len(), 1000);
        assert!(signal.iter().any(|&x| x > 0.0));
        assert!(signal.iter().any(|&x| x < 0.0));
        assert!(signal.iter().all(|&x| x.abs() <= 1.0));
    }

    #[test]
    fn silence_is_silent() {
        let generator = AudioGenerator::new(44100.0);
        assert!(generator.generate_silence(64).iter().all(|&x| x == 0.0));
    }
}
