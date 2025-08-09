use crate::audio::AudioData;
use anyhow::Result;
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;
use tracing::debug;

pub struct SignalProcessor {
    #[allow(dead_code)] // Used in future versions for resampling
    sample_rate: u32,
    #[allow(dead_code)] // Used for bounds checking and validation
    frequency_bands: usize,
    #[allow(dead_code)] // Used for frequency filtering
    frequency_range: (f32, f32),
    fft: Arc<dyn RealToComplex<f32>>,
    #[allow(dead_code)] // Used for buffer size calculations
    fft_size: usize,
    spectrum_buffer: Vec<f32>,
    magnitude_buffer: Vec<f32>,
    band_frequencies: Vec<f32>,
    smoothed_magnitudes: Vec<f32>,
    smoothing_factor: f32,
}

impl SignalProcessor {
    pub fn new(sample_rate: u32, frequency_bands: usize, frequency_range: (f32, f32)) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft_size = 1024; // We'll use a fixed FFT size for now
        let fft = planner.plan_fft_forward(fft_size);

        // Calculate the frequencies for each band
        let band_frequencies = Self::calculate_band_frequencies(
            frequency_bands,
            frequency_range,
            sample_rate,
            fft_size,
        );

        debug!(
            "Initialized signal processor with {} bands",
            frequency_bands
        );
        debug!("Band frequencies: {:?}", band_frequencies);

        Self {
            sample_rate,
            frequency_bands,
            frequency_range,
            fft,
            fft_size,
            spectrum_buffer: vec![0.0; fft_size / 2 + 1],
            magnitude_buffer: vec![0.0; frequency_bands],
            band_frequencies,
            smoothed_magnitudes: vec![0.0; frequency_bands],
            smoothing_factor: 0.8,
        }
    }

    fn calculate_band_frequencies(
        frequency_bands: usize,
        frequency_range: (f32, f32),
        sample_rate: u32,
        fft_size: usize,
    ) -> Vec<f32> {
        let (min_freq, max_freq) = frequency_range;
        let nyquist = sample_rate as f32 / 2.0;
        let max_freq = max_freq.min(nyquist);

        // Use logarithmic scale for frequency bands (more natural for audio)
        let log_min = min_freq.ln();
        let log_max = max_freq.ln();
        let log_step = (log_max - log_min) / (frequency_bands - 1) as f32;

        (0..frequency_bands)
            .map(|i| {
                let log_freq = log_min + i as f32 * log_step;
                let freq = log_freq.exp();

                // Convert to FFT bin index
                let bin = (freq * fft_size as f32 / sample_rate as f32).round() as usize;
                bin.min(fft_size / 2) as f32
            })
            .collect()
    }

    pub fn process(&mut self, audio_data: &AudioData) -> Result<Vec<f32>> {
        let samples = &audio_data.samples;
        let fft_size = self.fft.len();

        // Ensure we have enough samples for FFT
        if samples.len() < fft_size {
            return Ok(self.smoothed_magnitudes.clone());
        }

        // Apply Hanning window function to the audio data
        let mut windowed_samples: Vec<f32> = samples
            .iter()
            .take(fft_size)
            .enumerate()
            .map(|(i, &sample)| {
                let window_val = 0.5
                    * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (fft_size - 1) as f32).cos());
                sample * window_val
            })
            .collect();

        // Prepare output buffer for FFT
        let mut spectrum = vec![num_complex::Complex::new(0.0f32, 0.0f32); fft_size / 2 + 1];

        // Perform FFT
        self.fft
            .process(&mut windowed_samples, &mut spectrum)
            .map_err(|e| anyhow::anyhow!("FFT processing failed: {:?}", e))?;

        // Convert complex spectrum to magnitudes
        for (i, complex) in spectrum.iter().enumerate() {
            let magnitude = (complex.re * complex.re + complex.im * complex.im).sqrt();
            if i < self.spectrum_buffer.len() {
                self.spectrum_buffer[i] = magnitude;
            }
        }

        // Map FFT bins to frequency bands
        self.map_to_frequency_bands()?;

        // Apply smoothing to reduce flickering
        self.apply_smoothing();

        Ok(self.smoothed_magnitudes.clone())
    }

    fn map_to_frequency_bands(&mut self) -> Result<()> {
        // Reset magnitude buffer
        self.magnitude_buffer.fill(0.0);

        // For each frequency band, sum the corresponding FFT bins
        for (band_idx, &_start_bin) in self.band_frequencies.iter().enumerate() {
            if band_idx == 0 {
                continue;
            }

            let start_bin = self.band_frequencies[band_idx - 1] as usize;
            let end_bin = self.band_frequencies[band_idx] as usize;
            let end_bin = end_bin.min(self.spectrum_buffer.len() - 1);

            if start_bin <= end_bin {
                let mut sum = 0.0;
                let mut count = 0;

                for bin in start_bin..=end_bin {
                    if bin < self.spectrum_buffer.len() {
                        sum += self.spectrum_buffer[bin];
                        count += 1;
                    }
                }

                if count > 0 {
                    self.magnitude_buffer[band_idx - 1] = sum / count as f32;
                }
            }
        }

        Ok(())
    }

    fn apply_smoothing(&mut self) {
        for (i, &new_magnitude) in self.magnitude_buffer.iter().enumerate() {
            if i < self.smoothed_magnitudes.len() {
                self.smoothed_magnitudes[i] = self.smoothing_factor * self.smoothed_magnitudes[i]
                    + (1.0 - self.smoothing_factor) * new_magnitude;
            }
        }
    }

    #[allow(dead_code)] // Used for runtime configuration adjustments
    pub fn set_smoothing_factor(&mut self, factor: f32) {
        self.smoothing_factor = factor.clamp(0.0, 1.0);
    }

    #[allow(dead_code)] // Used for debugging and configuration validation
    pub fn get_frequency_bands(&self) -> usize {
        self.frequency_bands
    }

    #[allow(dead_code)] // Used for debugging and configuration validation
    pub fn get_frequency_range(&self) -> (f32, f32) {
        self.frequency_range
    }

    /// Normalize the magnitudes to a 0.0-1.0 range
    pub fn normalize_magnitudes(&self, magnitudes: &[f32], sensitivity: f32) -> Vec<f32> {
        if magnitudes.is_empty() {
            return vec![];
        }

        // Find the maximum magnitude for normalization
        let max_magnitude = magnitudes.iter().fold(0.0f32, |max, &val| max.max(val));

        if max_magnitude <= 0.0 {
            return vec![0.0; magnitudes.len()];
        }

        // Apply sensitivity and normalize
        magnitudes
            .iter()
            .map(|&magnitude| {
                let normalized = (magnitude / max_magnitude) * sensitivity;
                normalized.clamp(0.0, 1.0)
            })
            .collect()
    }
}
