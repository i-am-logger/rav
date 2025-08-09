use crate::testing::audio_generator::AudioGenerator;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleFormat, Stream, StreamConfig, SupportedStreamConfig};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info};

/// Custom audio monitor device for comprehensive testing
pub struct AudioMonitor {
    generator: AudioGenerator,
    _host: Host,
    device: Option<Device>,
    stream: Option<Stream>,
    config: StreamConfig,
    supported_config: SupportedStreamConfig,
    is_running: Arc<Mutex<bool>>,
    current_test: Arc<Mutex<TestProfile>>,
}

/// Test profile for different audio scenarios
#[derive(Clone, Debug)]
pub struct TestProfile {
    pub name: String,
    pub frequencies: Vec<f32>,
    pub duration_ms: u64,
    pub amplitude: f32,
    pub waveform: WaveformType,
    pub sweep_type: SweepType,
    pub test_all_visualizations: bool,
}

#[derive(Clone, Debug)]
pub enum WaveformType {
    Sine,
    Square,
    Triangle,
    Sawtooth,
    WhiteNoise,
    PinkNoise,
    Chirp,
    MultiTone(Vec<f32>),
}

#[derive(Clone, Debug)]
pub enum SweepType {
    Linear,
    Logarithmic,
    Random,
    Stepped,
    Continuous,
}

impl AudioMonitor {
    pub fn new() -> anyhow::Result<Self> {
        let generator = AudioGenerator::new(44100.0);
        let host = cpal::default_host();

        // Try to create a virtual device or use default
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No audio output device available"))?;

        let config = device.default_output_config()?;
        let stream_config = StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(44100),
            buffer_size: cpal::BufferSize::Fixed(1024),
        };

        Ok(AudioMonitor {
            generator,
            _host: host,
            device: Some(device),
            stream: None,
            config: stream_config,
            supported_config: config,
            is_running: Arc::new(Mutex::new(false)),
            current_test: Arc::new(Mutex::new(Self::default_test_profile())),
        })
    }

    /// Start monitoring with a specific test profile
    pub fn start_monitoring(&mut self, profile: TestProfile) -> anyhow::Result<()> {
        *self.current_test.lock().unwrap() = profile.clone();
        *self.is_running.lock().unwrap() = true;

        info!("🎵 Starting audio monitor with profile: {}", profile.name);
        debug!("Profile details: {:?}", profile);

        if let Some(device) = &self.device {
            let generator = Arc::new(Mutex::new(self.generator.clone()));
            let is_running = Arc::clone(&self.is_running);
            let current_test = Arc::clone(&self.current_test);

            let stream = match self.supported_config.sample_format() {
                SampleFormat::F32 => device.build_output_stream(
                    &self.config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        Self::fill_audio_buffer_f32(data, &generator, &is_running, &current_test);
                    },
                    |err| error!("Audio stream error: {}", err),
                    None,
                )?,
                _ => return Err(anyhow::anyhow!("Unsupported sample format")),
            };

            stream.play()?;
            self.stream = Some(stream);

            // Start the test sequence in a separate thread
            self.spawn_test_sequence(profile);
        }

        Ok(())
    }

    /// Stop monitoring
    pub fn stop_monitoring(&mut self) {
        *self.is_running.lock().unwrap() = false;
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        info!("🔇 Audio monitor stopped");
    }

    /// Get comprehensive test profiles
    pub fn get_comprehensive_test_profiles() -> Vec<TestProfile> {
        vec![
            // Basic frequency sweep
            TestProfile {
                name: "Linear Frequency Sweep".to_string(),
                frequencies: (20..=20000).step_by(100).map(|f| f as f32).collect(),
                duration_ms: 100,
                amplitude: 0.3,
                waveform: WaveformType::Sine,
                sweep_type: SweepType::Linear,
                test_all_visualizations: true,
            },
            // Logarithmic sweep
            TestProfile {
                name: "Logarithmic Frequency Sweep".to_string(),
                frequencies: Self::generate_log_frequencies(20.0, 20000.0, 200),
                duration_ms: 50,
                amplitude: 0.5,
                waveform: WaveformType::Sine,
                sweep_type: SweepType::Logarithmic,
                test_all_visualizations: true,
            },
            // Bass test
            TestProfile {
                name: "Bass Response Test".to_string(),
                frequencies: (20..=200).step_by(5).map(|f| f as f32).collect(),
                duration_ms: 200,
                amplitude: 0.8,
                waveform: WaveformType::Sine,
                sweep_type: SweepType::Stepped,
                test_all_visualizations: false,
            },
            // Midrange test
            TestProfile {
                name: "Midrange Clarity Test".to_string(),
                frequencies: (200..=2000).step_by(50).map(|f| f as f32).collect(),
                duration_ms: 150,
                amplitude: 0.6,
                waveform: WaveformType::Square,
                sweep_type: SweepType::Stepped,
                test_all_visualizations: true,
            },
            // Treble test
            TestProfile {
                name: "Treble Response Test".to_string(),
                frequencies: (2000..=20000).step_by(500).map(|f| f as f32).collect(),
                duration_ms: 100,
                amplitude: 0.4,
                waveform: WaveformType::Triangle,
                sweep_type: SweepType::Linear,
                test_all_visualizations: true,
            },
            // Multi-tone test
            TestProfile {
                name: "Multi-Tone Harmony Test".to_string(),
                frequencies: vec![440.0, 880.0, 1320.0, 1760.0], // A notes
                duration_ms: 500,
                amplitude: 0.5,
                waveform: WaveformType::MultiTone(vec![440.0, 880.0, 1320.0, 1760.0]),
                sweep_type: SweepType::Continuous,
                test_all_visualizations: true,
            },
            // Noise tests
            TestProfile {
                name: "White Noise Test".to_string(),
                frequencies: vec![0.0], // Special case for noise
                duration_ms: 2000,
                amplitude: 0.3,
                waveform: WaveformType::WhiteNoise,
                sweep_type: SweepType::Continuous,
                test_all_visualizations: true,
            },
            // Chirp test
            TestProfile {
                name: "Chirp Sweep Test".to_string(),
                frequencies: vec![20.0, 20000.0], // Start and end frequencies
                duration_ms: 3000,
                amplitude: 0.5,
                waveform: WaveformType::Chirp,
                sweep_type: SweepType::Continuous,
                test_all_visualizations: true,
            },
            // Random stress test
            TestProfile {
                name: "Random Stress Test".to_string(),
                frequencies: Self::generate_random_frequencies(100),
                duration_ms: 50,
                amplitude: 0.7,
                waveform: WaveformType::Square,
                sweep_type: SweepType::Random,
                test_all_visualizations: true,
            },
        ]
    }

    /// Default test profile for quick testing
    fn default_test_profile() -> TestProfile {
        TestProfile {
            name: "Quick Test".to_string(),
            frequencies: vec![440.0, 880.0, 1760.0],
            duration_ms: 1000,
            amplitude: 0.5,
            waveform: WaveformType::Sine,
            sweep_type: SweepType::Stepped,
            test_all_visualizations: false,
        }
    }

    /// Generate logarithmically spaced frequencies
    fn generate_log_frequencies(start: f32, end: f32, count: usize) -> Vec<f32> {
        let log_start = start.ln();
        let log_end = end.ln();
        let step = (log_end - log_start) / (count - 1) as f32;

        (0..count)
            .map(|i| (log_start + step * i as f32).exp())
            .collect()
    }

    /// Generate random frequencies for stress testing
    fn generate_random_frequencies(count: usize) -> Vec<f32> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..count).map(|_| rng.gen_range(20.0..20000.0)).collect()
    }

    /// Fill audio buffer with generated audio
    fn fill_audio_buffer_f32(
        data: &mut [f32],
        generator: &Arc<Mutex<AudioGenerator>>,
        is_running: &Arc<Mutex<bool>>,
        current_test: &Arc<Mutex<TestProfile>>,
    ) {
        if !*is_running.lock().unwrap() {
            data.fill(0.0);
            return;
        }

        let mut gen = generator.lock().unwrap();
        let test = current_test.lock().unwrap().clone();

        // Generate audio based on current test profile
        match test.waveform {
            WaveformType::Sine => {
                if let Some(&freq) = test.frequencies.first() {
                    gen.fill_sine_wave(data, freq, test.amplitude);
                }
            }
            WaveformType::Square => {
                if let Some(&freq) = test.frequencies.first() {
                    gen.fill_square_wave(data, freq, test.amplitude);
                }
            }
            WaveformType::Triangle => {
                if let Some(&freq) = test.frequencies.first() {
                    gen.fill_triangle_wave(data, freq, test.amplitude);
                }
            }
            WaveformType::WhiteNoise => {
                gen.fill_white_noise(data, test.amplitude);
            }
            WaveformType::MultiTone(ref freqs) => {
                gen.fill_multi_tone(data, freqs, test.amplitude);
            }
            _ => {
                // Default to sine wave
                if let Some(&freq) = test.frequencies.first() {
                    gen.fill_sine_wave(data, freq, test.amplitude);
                }
            }
        }
    }

    /// Spawn test sequence that cycles through frequencies
    fn spawn_test_sequence(&self, profile: TestProfile) {
        let current_test = Arc::clone(&self.current_test);
        let is_running = Arc::clone(&self.is_running);

        thread::spawn(move || {
            info!("🔄 Starting test sequence: {}", profile.name);

            for (i, &frequency) in profile.frequencies.iter().enumerate() {
                if !*is_running.lock().unwrap() {
                    break;
                }

                // Update current frequency
                {
                    let mut test = current_test.lock().unwrap();
                    test.frequencies = vec![frequency];
                }

                debug!(
                    "🎵 Testing frequency: {}Hz ({}/{})",
                    frequency,
                    i + 1,
                    profile.frequencies.len()
                );

                thread::sleep(Duration::from_millis(profile.duration_ms));
            }

            info!("✅ Test sequence completed: {}", profile.name);
        });
    }

    /// Get current test status
    pub fn get_status(&self) -> (bool, TestProfile) {
        let is_running = *self.is_running.lock().unwrap();
        let current_test = self.current_test.lock().unwrap().clone();
        (is_running, current_test)
    }
}

impl Drop for AudioMonitor {
    fn drop(&mut self) {
        self.stop_monitoring();
    }
}
