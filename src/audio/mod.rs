use anyhow::{Context, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Host, Stream, StreamConfig,
};
use flume::{Receiver, Sender};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

pub type AudioSample = f32;
pub type AudioBuffer = Vec<AudioSample>;

#[derive(Clone)]
pub struct AudioData {
    pub samples: AudioBuffer,
    #[allow(dead_code)] // Will be used in future versions for processing
    pub sample_rate: u32,
    #[allow(dead_code)] // Will be used in future versions for timing analysis
    pub timestamp: std::time::Instant,
}

pub struct AudioCapture {
    device: Device,
    config: StreamConfig,
    sample_rate: u32,
    buffer_size: usize,
    stream: Option<Stream>,
    sender: Option<Sender<AudioData>>,
}

impl AudioCapture {
    pub async fn new(
        device_name: Option<&str>,
        sample_rate: u32,
        buffer_size: usize,
    ) -> Result<Self> {
        let host = cpal::default_host();

        let device = if let Some(name) = device_name {
            Self::find_device_by_name(&host, name)?
        } else {
            // Try to find a monitor device first for better audio visualization
            Self::find_monitor_device(&host).or_else(|_| {
                host.default_input_device()
                    .context("No default input device available")
            })?
        };

        info!("Using audio device: {}", device.name()?);

        let supported_configs = device.supported_input_configs()?.collect::<Vec<_>>();

        debug!("Supported input configs: {:#?}", supported_configs);

        // Find a suitable F32 config - prioritize F32 format
        let config = supported_configs
            .into_iter()
            // First try to find F32 format with our desired sample rate
            .find(|config| {
                config.sample_format() == cpal::SampleFormat::F32
                    && config.min_sample_rate().0 <= sample_rate
                    && config.max_sample_rate().0 >= sample_rate
            })
            .or_else(|| {
                // Fallback: find any F32 config regardless of sample rate
                device
                    .supported_input_configs()
                    .ok()?
                    .find(|config| config.sample_format() == cpal::SampleFormat::F32)
            })
            .context("No F32 audio input configuration found. Please check your audio setup.")?;

        // Use the config's preferred sample rate if our desired rate isn't supported
        let actual_sample_rate = if config.min_sample_rate().0 <= sample_rate
            && config.max_sample_rate().0 >= sample_rate
        {
            sample_rate
        } else {
            // Use a common sample rate that's supported
            if config.min_sample_rate().0 <= 44100 && config.max_sample_rate().0 >= 44100 {
                44100
            } else {
                config
                    .max_sample_rate()
                    .0
                    .min(48000)
                    .max(config.min_sample_rate().0)
            }
        };

        let config = config.with_sample_rate(cpal::SampleRate(actual_sample_rate));

        info!("Using audio config: {:?}", config);

        Ok(Self {
            device,
            config: config.into(),
            sample_rate,
            buffer_size,
            stream: None,
            sender: None,
        })
    }

    fn find_device_by_name(host: &Host, name: &str) -> Result<Device> {
        let devices = host.input_devices()?;

        // First, try to find monitor/loopback device
        for device in devices {
            if let Ok(device_name) = device.name() {
                debug!("Checking device: {}", device_name);
                // Look for monitor sources (common in PulseAudio/PipeWire)
                if device_name.contains(".monitor")
                    || device_name.contains("Monitor of")
                    || device_name.contains("monitor")
                    || device_name.contains(name)
                {
                    info!("Found potential monitor device: {}", device_name);
                    return Ok(device);
                }
            }
        }

        // If no monitor device found, try default approach
        for device in host.input_devices()? {
            if let Ok(device_name) = device.name() {
                if device_name.contains(name) {
                    return Ok(device);
                }
            }
        }

        Err(anyhow::anyhow!("Audio device '{}' not found", name))
    }

    fn find_monitor_device(host: &Host) -> Result<Device> {
        let devices = host.input_devices()?;

        // Look for monitor/loopback devices
        for device in devices {
            if let Ok(device_name) = device.name() {
                debug!("Checking device: {}", device_name);
                // Look for monitor sources (common in PulseAudio/PipeWire)
                if device_name.contains(".monitor")
                    || device_name.contains("Monitor of")
                    || device_name.to_lowercase().contains("loopback")
                    || device_name.to_lowercase().contains("monitor")
                {
                    info!("Found monitor device: {}", device_name);
                    return Ok(device);
                }
            }
        }

        Err(anyhow::anyhow!("No monitor/loopback device found"))
    }

    pub async fn list_devices() -> Result<()> {
        let host = cpal::default_host();

        println!("🎵 Available audio input devices:");

        let default_device = host.default_input_device();

        for device in host.input_devices()? {
            let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
            let is_default = default_device
                .as_ref()
                .map(|d| d.name().ok() == Some(name.clone()))
                .unwrap_or(false);

            let marker = if is_default { " (default)" } else { "" };
            println!("  • {name}{marker}");

            if let Ok(configs) = device.supported_input_configs() {
                for config in configs.take(3) {
                    println!(
                        "    - {} {:?} ({}Hz - {}Hz)",
                        config.channels(),
                        config.sample_format(),
                        config.min_sample_rate().0,
                        config.max_sample_rate().0
                    );
                }
            }
        }

        Ok(())
    }

    pub async fn start(&mut self) -> Result<Receiver<AudioData>> {
        let (tx, rx) = flume::unbounded();
        self.sender = Some(tx.clone());

        let sample_rate = self.sample_rate;
        let buffer_size = self.buffer_size;

        let err_fn = |err| {
            error!("Audio stream error: {}", err);
        };

        // Use std::sync::Mutex instead of tokio::sync::Mutex to avoid async in callback
        let sync_buffer =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::with_capacity(buffer_size)));
        let sync_buffer_clone = Arc::clone(&sync_buffer);

        let data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let tx = tx.clone();
            let timestamp = std::time::Instant::now();

            // Use synchronous operations in the audio callback
            if let Ok(mut buf) = sync_buffer_clone.lock() {
                buf.extend_from_slice(data);

                // Send data when buffer is full enough
                if buf.len() >= buffer_size {
                    let audio_data = AudioData {
                        samples: buf.drain(..buffer_size).collect(),
                        sample_rate,
                        timestamp,
                    };

                    // Use try_send to avoid blocking in the audio callback
                    if tx.try_send(audio_data).is_err() {
                        // Buffer full, skip this frame to avoid blocking audio
                    }
                }
            }
        };

        let stream = self
            .device
            .build_input_stream(&self.config, data_fn, err_fn, None)?;

        stream.play()?;
        self.stream = Some(stream);

        info!(
            "Audio stream started with sample rate: {}Hz, buffer size: {}",
            self.sample_rate, self.buffer_size
        );

        Ok(rx)
    }

    pub fn stop(&mut self) {
        if let Some(stream) = self.stream.take() {
            // Try to pause the stream before dropping to avoid ALSA assertion failures
            if let Err(e) = stream.pause() {
                warn!("Failed to pause audio stream: {}", e);
            }
            // Give the stream a moment to stop cleanly
            std::thread::sleep(std::time::Duration::from_millis(10));
            drop(stream);
            info!("Audio stream stopped");
        }
        self.sender = None;
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        // Don't call stop() in Drop to avoid potential double-free issues
        // The stream will be cleaned up automatically when dropped
        info!("AudioCapture dropped");
        self.sender = None;
    }
}
