#[cfg(target_os = "macos")]
pub mod tap;

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

/// Identifies a virtual device that loops system output back as an input.
///
/// On macOS, system audio is captured through Background Music
/// (https://github.com/kyleneideck/BackgroundMusic), which must be installed and
/// selected as the output device. It also registers a companion
/// "Background Music (UI Sounds)" device that carries only system alert sounds -
/// selecting that one would look like a silent source, so it is excluded here.
fn is_system_audio_loopback(device_name: &str) -> bool {
    device_name.contains("Background Music") && !device_name.contains("UI Sounds")
}

/// One block of interleaved samples, as the capture callback hands it over.
///
/// The stream's rate and channel count belong to [`AudioCapture`], not to each
/// block: they are fixed for the life of the stream, and a consumer that reads
/// them per block can drift from the one that actually configured it.
#[derive(Clone)]
pub struct AudioData {
    pub samples: AudioBuffer,
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
            Self::find_monitor_device(&host).or_else(|_| -> Result<Device> {
                let device = host
                    .default_input_device()
                    .context("No default input device available")?;
                // A silent source and a misconfigured one look identical once
                // running, so say which happened instead of falling back quietly.
                warn!(
                    "No monitor/loopback device found - capturing from default input '{}'. \
                     System audio will not be visualized, only what this input hears. \
                     On macOS, install Background Music and set it as the output device.",
                    device.name().unwrap_or_else(|_| "unknown".to_string())
                );
                Ok(device)
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
            // The negotiated rate, not the requested one - they differ whenever
            // the device cannot honour what the config asked for.
            sample_rate: actual_sample_rate,
            buffer_size,
            stream: None,
            sender: None,
        })
    }

    /// The sample rate capture actually runs at. Only final after `start()`,
    /// which may renegotiate it if the device rejects the chosen config.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Channels the stream delivers. cpal hands over *interleaved* frames, so a
    /// consumer that ignores this is silently mixing L and R into the time
    /// domain and halving its effective window. Only final after `start()`.
    pub fn channels(&self) -> u16 {
        self.config.channels
    }

    fn find_device_by_name(host: &Host, name: &str) -> Result<Device> {
        // An explicitly requested device wins outright, so match its exact name
        // ahead of any heuristic. Run the monitor/loopback preference first and
        // an unrelated device merely containing "monitor" shadows the one that
        // was actually asked for.
        for device in host.input_devices()? {
            if let Ok(device_name) = device.name() {
                debug!("Checking device: {}", device_name);
                if device_name == name {
                    info!("Found requested device: {}", device_name);
                    return Ok(device);
                }
            }
        }

        // Then fall back to a substring match, so a partial name still works.
        for device in host.input_devices()? {
            if let Ok(device_name) = device.name() {
                if device_name.contains(name) {
                    info!("Found device matching '{}': {}", name, device_name);
                    return Ok(device);
                }
            }
        }

        Err(anyhow::anyhow!("Audio device '{}' not found", name))
    }

    fn find_monitor_device(host: &Host) -> Result<Device> {
        // First pass: virtual loopback devices that carry system audio. cpal
        // enumerates devices in an unspecified order, so this has to be its own
        // pass rather than one more arm of the heuristics below - otherwise a
        // device that merely matches "monitor" could win over the real source.
        for device in host.input_devices()? {
            if let Ok(device_name) = device.name() {
                debug!("Checking device: {}", device_name);
                if is_system_audio_loopback(&device_name) {
                    info!("Found system audio loopback device: {}", device_name);
                    return Ok(device);
                }
            }
        }

        // Second pass: monitor sources as exposed by PulseAudio/PipeWire on Linux.
        for device in host.input_devices()? {
            if let Ok(device_name) = device.name() {
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

        let buffer_size = self.buffer_size;

        let err_fn = |err| {
            error!("Audio stream error: {}", err);
        };

        // Use std::sync::Mutex instead of tokio::sync::Mutex to avoid async in callback
        let sync_buffer =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::with_capacity(buffer_size)));

        // Built through a factory because the callback is consumed by
        // build_input_stream, and the retry below needs a second one.
        let make_data_fn = || {
            let tx = tx.clone();
            let sync_buffer_clone = Arc::clone(&sync_buffer);
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let tx = tx.clone();

                // Use synchronous operations in the audio callback
                if let Ok(mut buf) = sync_buffer_clone.lock() {
                    buf.extend_from_slice(data);

                    // Send data when buffer is full enough
                    if buf.len() >= buffer_size {
                        let audio_data = AudioData {
                            samples: buf.drain(..buffer_size).collect(),
                        };

                        // Use try_send to avoid blocking in the audio callback
                        if tx.try_send(audio_data).is_err() {
                            // Buffer full, skip this frame to avoid blocking audio
                        }
                    }
                }
            }
        };

        // Virtual devices can advertise a sample-rate range they cannot actually
        // honour - Background Music reports 1Hz-1GHz but only runs at whatever
        // rate CoreAudio currently has it at - so if the negotiated config is
        // rejected, retry with the rate the device reports as its own default.
        let stream =
            match self
                .device
                .build_input_stream(&self.config, make_data_fn(), err_fn, None)
            {
                Ok(stream) => stream,
                Err(build_err) => {
                    let fallback = self.device.default_input_config().with_context(|| {
                        format!(
                            "Device rejected {}Hz and reports no default input config: {build_err}",
                            self.sample_rate
                        )
                    })?;
                    warn!(
                        "Device rejected {}Hz ({build_err}); retrying at its default of {}Hz",
                        self.sample_rate,
                        fallback.sample_rate().0
                    );
                    self.sample_rate = fallback.sample_rate().0;
                    self.config = fallback.config();
                    self.device
                        .build_input_stream(&self.config, make_data_fn(), err_fn, None)?
                }
            };

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

#[cfg(test)]
mod tests {
    use super::is_system_audio_loopback;

    #[test]
    fn matches_background_music_device() {
        assert!(is_system_audio_loopback("Background Music"));
    }

    #[test]
    fn skips_background_music_ui_sounds_device() {
        // This device only carries system alert sounds; picking it would look
        // like a silent source while music is playing.
        assert!(!is_system_audio_loopback("Background Music (UI Sounds)"));
    }

    #[test]
    fn ignores_ordinary_input_devices() {
        for name in [
            "MacBook Pro Microphone",
            "Maonocaster E2",
            "Logger Microphone",
        ] {
            assert!(!is_system_audio_loopback(name), "{name} should not match");
        }
    }
}
