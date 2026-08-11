//! Signal analysis: FFT front end, bin-to-bar mapping, and bar/cap motion.

pub mod ballistics;
pub mod mapping;
pub mod spectrum;

#[cfg(test)]
mod pipeline_tests {
    //! End-to-end checks over the whole analysis chain, driven by generated
    //! signals rather than live audio so they are deterministic.

    use super::mapping::{BarMap, DEFAULT_SCALE, bin_for_hz};
    use super::spectrum::Spectrum;
    use crate::testing::AudioGenerator;

    const RATE: u32 = 48_000;
    const BARS: usize = 60;

    /// Analyse `samples` and return the index of the loudest bar.
    fn loudest_bar(samples: &[f32]) -> usize {
        let mut spectrum = Spectrum::new(Spectrum::DEFAULT_SIZE).unwrap();
        let top = bin_for_hz(16_000, RATE, spectrum.bins());
        let map = BarMap::with_top_bin(BARS, spectrum.bins(), top, DEFAULT_SCALE);
        let magnitudes = spectrum.analyse(samples);
        let mut bars = Vec::new();
        map.sample(magnitudes, &mut bars);
        bars.iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap()
    }

    /// Centre frequency a bar corresponds to.
    fn bar_hz(bar: usize) -> f32 {
        let spectrum = Spectrum::new(Spectrum::DEFAULT_SIZE).unwrap();
        let top = bin_for_hz(16_000, RATE, spectrum.bins());
        let map = BarMap::with_top_bin(BARS, spectrum.bins(), top, DEFAULT_SCALE);
        let bin = map.positions()[bar];
        bin * RATE as f32 / spectrum.size() as f32
    }

    #[test]
    fn a_tone_lands_in_a_bar_at_its_own_frequency() {
        // Three semitones is generous, but it is the tolerance that catches an
        // aliased mapping: any band table that folds many bars onto one bin puts
        // a tone octaves away from where it belongs.
        let mut generator = AudioGenerator::new(RATE as f32);
        for hz in [220.0f32, 1000.0, 4000.0] {
            let samples = generator.sine_wave(hz, 1.0, Spectrum::DEFAULT_SIZE);
            let found = bar_hz(loudest_bar(&samples));
            let error = (found / hz).log2().abs() * 12.0; // semitones
            assert!(
                error < 3.0,
                "{hz}Hz landed at {found:.0}Hz ({error:.1} semitones out)"
            );
        }
    }

    #[test]
    fn a_rising_sweep_moves_the_peak_rightwards() {
        let mut generator = AudioGenerator::new(RATE as f32);
        let mut last = 0usize;
        for hz in [100.0f32, 500.0, 2000.0, 8000.0] {
            let samples = generator.sine_wave(hz, 1.0, Spectrum::DEFAULT_SIZE);
            let bar = loudest_bar(&samples);
            assert!(bar >= last, "{hz}Hz went backwards: bar {bar} after {last}");
            last = bar;
        }
    }

    #[test]
    fn silence_produces_no_output_at_all() {
        let generator = AudioGenerator::new(RATE as f32);
        let samples = generator.generate_silence(Spectrum::DEFAULT_SIZE);
        let mut spectrum = Spectrum::new(Spectrum::DEFAULT_SIZE).unwrap();
        assert!(spectrum.analyse(&samples).iter().all(|&m| m == 0.0));
    }

    #[test]
    fn broadband_noise_lights_the_whole_display() {
        // Energy everywhere must light bars everywhere. A mapping that collapses
        // onto a handful of bins fails this while still passing a single-tone
        // check.
        let generator = AudioGenerator::new(RATE as f32);
        let samples = generator.white_noise(1.0, Spectrum::DEFAULT_SIZE);
        let mut spectrum = Spectrum::new(Spectrum::DEFAULT_SIZE).unwrap();
        let top = bin_for_hz(16_000, RATE, spectrum.bins());
        let map = BarMap::with_top_bin(BARS, spectrum.bins(), top, DEFAULT_SCALE);
        let magnitudes = spectrum.analyse(&samples);
        let mut bars = Vec::new();
        map.sample(magnitudes, &mut bars);
        let lit = bars.iter().filter(|&&v| v > 0.0).count();
        assert!(
            lit > BARS * 3 / 4,
            "only {lit} of {BARS} bars had any energy"
        );
    }
}
