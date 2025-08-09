// Audio signal generators for testing visualizations
use std::f32::consts::PI;

/// Generator for various test audio signals
#[derive(Clone)]
pub struct AudioGenerator {
    sample_rate: f32,
    time: f32,
}

impl AudioGenerator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            time: 0.0,
        }
    }

    // Convenience methods for BDD testing
    pub fn generate_silence(&self, samples: usize) -> Vec<f32> {
        vec![0.0; samples]
    }

    pub fn generate_sine_wave(&mut self, frequency: f32, samples: usize) -> Vec<f32> {
        self.sine_wave(frequency, 0.7, samples)
    }

    pub fn generate_white_noise(&self, samples: usize) -> Vec<f32> {
        self.white_noise(0.5, samples)
    }

    pub fn generate_pink_noise(&self, samples: usize) -> Vec<f32> {
        self.pink_noise(0.5, samples)
    }

    pub fn generate_music_spectrum(&self, samples: usize) -> Vec<f32> {
        self.realistic_music_spectrum("rock".to_string(), samples)
    }

    /// Generate a pure sine wave at the given frequency
    pub fn sine_wave(&mut self, frequency: f32, amplitude: f32, samples: usize) -> Vec<f32> {
        let mut signal = Vec::with_capacity(samples);
        let dt = 1.0 / self.sample_rate;

        for _ in 0..samples {
            let sample = amplitude * (2.0 * PI * frequency * self.time).sin();
            signal.push(sample);
            self.time += dt;
        }

        signal
    }

    /// Generate white noise
    pub fn white_noise(&self, amplitude: f32, samples: usize) -> Vec<f32> {
        use rand::{thread_rng, Rng};
        let mut rng = thread_rng();

        (0..samples)
            .map(|_| amplitude * (rng.gen::<f32>() * 2.0 - 1.0))
            .collect()
    }

    /// Generate a frequency sweep (chirp)
    pub fn frequency_sweep(
        &mut self,
        start_freq: f32,
        end_freq: f32,
        amplitude: f32,
        samples: usize,
    ) -> Vec<f32> {
        let mut signal = Vec::with_capacity(samples);
        let dt = 1.0 / self.sample_rate;
        let duration = samples as f32 * dt;

        for i in 0..samples {
            let t = i as f32 * dt;
            let freq = start_freq + (end_freq - start_freq) * t / duration;
            let sample = amplitude * (2.0 * PI * freq * t).sin();
            signal.push(sample);
        }

        signal
    }

    /// Generate a multi-frequency signal (mixture of sine waves)
    pub fn multi_tone(
        &mut self,
        frequencies: &[f32],
        amplitudes: &[f32],
        samples: usize,
    ) -> Vec<f32> {
        assert_eq!(frequencies.len(), amplitudes.len());

        let mut signal = vec![0.0; samples];
        let dt = 1.0 / self.sample_rate;

        for (freq, amp) in frequencies.iter().zip(amplitudes.iter()) {
            let mut time = self.time;
            for i in 0..samples {
                signal[i] += amp * (2.0 * PI * freq * time).sin();
                time += dt;
            }
        }

        self.time += samples as f32 * dt;
        signal
    }

    /// Generate pink noise (1/f noise)
    pub fn pink_noise(&self, amplitude: f32, samples: usize) -> Vec<f32> {
        use rand::{thread_rng, Rng};
        let mut rng = thread_rng();

        // Simple pink noise approximation using multiple octaves
        let mut signal = vec![0.0; samples];
        let octaves = 8;

        for octave in 0..octaves {
            let freq_mult = 2.0_f32.powi(octave);
            let amp_mult = 1.0 / freq_mult.sqrt();

            for i in 0..samples {
                let noise_val = rng.gen::<f32>() * 2.0 - 1.0;
                signal[i] += amplitude * amp_mult * noise_val;
            }
        }

        // Normalize
        let max_val = signal.iter().fold(0.0f32, |max, &val| max.max(val.abs()));
        if max_val > 0.0 {
            for sample in &mut signal {
                *sample = (*sample / max_val) * amplitude;
            }
        }

        signal
    }

    /// Generate a simple drum hit simulation
    pub fn drum_hit(&mut self, fundamental: f32, decay: f32, samples: usize) -> Vec<f32> {
        let mut signal = Vec::with_capacity(samples);
        let dt = 1.0 / self.sample_rate;

        for i in 0..samples {
            let t = i as f32 * dt;
            let envelope = (-decay * t).exp();

            // Mix fundamental with harmonics
            let sample = envelope
                * (0.8 * (2.0 * PI * fundamental * t).sin()
                    + 0.3 * (2.0 * PI * fundamental * 2.0 * t).sin()
                    + 0.1 * (2.0 * PI * fundamental * 3.0 * t).sin());

            signal.push(sample);
        }

        signal
    }

    /// Generate test magnitudes that simulate typical audio spectrum
    pub fn test_spectrum_magnitudes(frequency_bands: usize) -> Vec<f32> {
        let mut magnitudes = Vec::with_capacity(frequency_bands);

        for i in 0..frequency_bands {
            let freq_ratio = i as f32 / frequency_bands as f32;

            // Simulate typical music spectrum: strong low end, moderate mids, lower highs
            let magnitude = if freq_ratio < 0.1 {
                // Bass region - strong
                0.8 + 0.2 * (freq_ratio * 10.0 * PI).sin()
            } else if freq_ratio < 0.3 {
                // Low mids - moderate to strong
                0.6 + 0.3 * ((freq_ratio - 0.1) * 5.0 * PI).sin()
            } else if freq_ratio < 0.7 {
                // Mids - variable
                0.4 + 0.4 * ((freq_ratio - 0.3) * 2.5 * PI).sin()
            } else {
                // Highs - moderate to weak
                0.2 + 0.2 * ((freq_ratio - 0.7) * 3.3 * PI).sin()
            };

            magnitudes.push(magnitude.max(0.0));
        }

        magnitudes
    }

    /// Generate test magnitudes for a quiet scene
    pub fn quiet_spectrum_magnitudes(frequency_bands: usize) -> Vec<f32> {
        use rand::{thread_rng, Rng};
        let mut rng = thread_rng();

        (0..frequency_bands)
            .map(|_| rng.gen::<f32>() * 0.1) // Very low random values
            .collect()
    }

    /// Generate test magnitudes for a loud/dynamic scene
    pub fn dynamic_spectrum_magnitudes(frequency_bands: usize) -> Vec<f32> {
        use rand::{thread_rng, Rng};
        let mut rng = thread_rng();

        let mut magnitudes = Vec::with_capacity(frequency_bands);

        for i in 0..frequency_bands {
            let _freq_ratio = i as f32 / frequency_bands as f32;
            let base = Self::test_spectrum_magnitudes(frequency_bands)[i];

            // Add random variation
            let variation = (rng.gen::<f32>() - 0.5) * 0.4;
            let magnitude = (base + variation).clamp(0.0, 1.0);

            magnitudes.push(magnitude);
        }

        magnitudes
    }

    /// Generate realistic music spectrum based on genre
    pub fn realistic_music_spectrum(&self, _genre: String, frequency_bands: usize) -> Vec<f32> {
        // For now, just return the standard test spectrum
        Self::test_spectrum_magnitudes(frequency_bands)
    }

    /// Reset the internal time counter
    pub fn reset(&mut self) {
        self.time = 0.0;
    }

    // AudioMonitor compatible methods that fill buffers directly
    pub fn fill_sine_wave(&mut self, data: &mut [f32], frequency: f32, amplitude: f32) {
        let dt = 1.0 / self.sample_rate;

        for sample in data.iter_mut() {
            *sample = amplitude * (2.0 * PI * frequency * self.time).sin();
            self.time += dt;
        }
    }

    pub fn fill_square_wave(&mut self, data: &mut [f32], frequency: f32, amplitude: f32) {
        let dt = 1.0 / self.sample_rate;

        for sample in data.iter_mut() {
            let sine_val = (2.0 * PI * frequency * self.time).sin();
            *sample = amplitude * if sine_val >= 0.0 { 1.0 } else { -1.0 };
            self.time += dt;
        }
    }

    pub fn fill_triangle_wave(&mut self, data: &mut [f32], frequency: f32, amplitude: f32) {
        let dt = 1.0 / self.sample_rate;
        let period = 1.0 / frequency;

        for sample in data.iter_mut() {
            let t_in_period = self.time % period;
            let normalized_t = t_in_period / period;

            *sample = amplitude
                * if normalized_t < 0.5 {
                    4.0 * normalized_t - 1.0
                } else {
                    3.0 - 4.0 * normalized_t
                };

            self.time += dt;
        }
    }

    pub fn fill_white_noise(&self, data: &mut [f32], amplitude: f32) {
        use rand::{thread_rng, Rng};
        let mut rng = thread_rng();

        for sample in data.iter_mut() {
            *sample = amplitude * (rng.gen::<f32>() * 2.0 - 1.0);
        }
    }

    pub fn fill_multi_tone(&mut self, data: &mut [f32], frequencies: &[f32], amplitude: f32) {
        data.fill(0.0);

        let dt = 1.0 / self.sample_rate;
        let base_amplitude = amplitude / frequencies.len() as f32;

        for &freq in frequencies {
            let mut time = self.time;
            for sample in data.iter_mut() {
                *sample += base_amplitude * (2.0 * PI * freq * time).sin();
                time += dt;
            }
        }

        self.time += data.len() as f32 * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sine_wave_generation() {
        let mut generator = AudioGenerator::new(44100.0);
        let signal = generator.sine_wave(440.0, 1.0, 100);

        assert_eq!(signal.len(), 100);
        assert!(signal[0].abs() < 0.1); // Should start near zero
        assert!(signal.iter().any(|&x| x > 0.5)); // Should have positive peaks
        assert!(signal.iter().any(|&x| x < -0.5)); // Should have negative peaks
    }

    #[test]
    fn test_white_noise_generation() {
        let generator = AudioGenerator::new(44100.0);
        let signal = generator.white_noise(1.0, 1000);

        assert_eq!(signal.len(), 1000);

        // Check that we have both positive and negative values
        assert!(signal.iter().any(|&x| x > 0.0));
        assert!(signal.iter().any(|&x| x < 0.0));

        // Check amplitude bounds
        assert!(signal.iter().all(|&x| x.abs() <= 1.0));
    }

    #[test]
    fn test_spectrum_magnitudes() {
        let magnitudes = AudioGenerator::test_spectrum_magnitudes(32);

        assert_eq!(magnitudes.len(), 32);
        assert!(magnitudes.iter().all(|&x| x >= 0.0 && x <= 1.0));

        // Bass should generally be stronger than highs
        let bass_avg = magnitudes[0..3].iter().sum::<f32>() / 3.0;
        let treble_avg = magnitudes[24..32].iter().sum::<f32>() / 8.0;
        assert!(bass_avg > treble_avg);
    }
}
