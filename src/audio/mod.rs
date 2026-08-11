#[cfg(target_os = "macos")]
pub mod tap;

use anyhow::{Context, Result};
use cpal::{
    Device, Host, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use flume::{Receiver, Sender};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

pub type AudioSample = f32;
pub type AudioBuffer = Vec<AudioSample>;

/// Blocks the capture queue holds before it starts discarding.
///
/// Bounded rather than unbounded on purpose. The analyser only ever draws the
/// newest block, so a queue is a shock absorber for a late frame and nothing
/// more; an unbounded one silently becomes a delay line that grows for as long
/// as rav runs, which is exactly what it used to do. Eight blocks of 1024 is
/// ~185 ms at 44.1 kHz - room for a stalled frame or two, short enough that a
/// full queue is not audible as lag.
const QUEUE_BLOCKS: usize = 8;

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

/// What to do when no loopback was found, for the platform saying so.
///
/// Split by platform because the advice is entirely different, and the wrong
/// one is worse than none: on Linux the search cannot succeed at all - see
/// [`is_loopback_capture`] - so telling a Linux user to install a macOS virtual
/// device reads as "rav is broken" and sends them looking in the wrong place.
#[cfg(target_os = "macos")]
const NO_LOOPBACK_REMEDY: &str = "Install Background Music and set it as the output device.";
#[cfg(not(target_os = "macos"))]
const NO_LOOPBACK_REMEDY: &str = "A PipeWire/PulseAudio monitor source is not an ALSA device, so rav cannot find one by \
     name and -d cannot reach one either. Route this capture at your sink's monitor instead: \
     PIPEWIRE_NODE=<sink>.monitor rav, or a patchbay such as qpwgraph. See docs/audio.md.";

/// Identifies a capture that carries system output back in, by name.
///
/// Deliberately narrower than a bare `contains("monitor")`. A studio-monitor
/// interface and an HDMI display both put that word in their device name, and
/// choosing one of those over the real source fails *silently*: the display
/// still moves, so nothing looks broken, it is just showing the wrong thing.
/// The three forms below each belong to a loopback and to nothing else.
///
/// This is also the pass that cannot succeed on a stock Linux box. `.monitor`
/// and "Monitor of" are PulseAudio *source* names, which the ALSA backend cpal
/// uses on Linux never enumerates; what remains is `snd-aloop`, whose PCMs do
/// appear as `…CARD=Loopback`.
fn is_loopback_capture(device_name: &str) -> bool {
    device_name.contains(".monitor")
        || device_name.contains("Monitor of")
        || device_name.to_lowercase().contains("loopback")
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
                     System audio will not be visualized, only what this input hears. {}",
                    device.name().unwrap_or_else(|_| "unknown".to_string()),
                    NO_LOOPBACK_REMEDY
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

    /// What the display is being fed from, for saying so on screen.
    ///
    /// The name the device reports, not the one that was asked for: `-d` takes
    /// a partial name, and rav falls back to the default input when it finds no
    /// loopback, so the two differ exactly when it matters.
    pub fn device_name(&self) -> String {
        self.device
            .name()
            .unwrap_or_else(|_| "an unnamed device".to_string())
    }

    /// Every input device with its name, enumerated **once**.
    ///
    /// Worth its own function because enumeration is not cheap: cpal's ALSA
    /// backend opens every PCM its hint list names to answer this -
    /// `surround51:CARD=…`, `iec958:…`, `dmix` and the rest, most of which
    /// cannot be opened at all - which costs a quarter of a second per pass and
    /// a screenful of libasound complaints on stderr. The searches below want
    /// to walk the same list more than once, so pay for it once and scan
    /// strings after that.
    fn input_devices(host: &Host) -> Result<Vec<(Device, String)>> {
        let mut devices = Vec::new();
        for device in host.input_devices()? {
            // A device that will not say what it is called cannot be matched by
            // any of the searches below, so it is dropped here rather than
            // carried as an unnameable entry.
            if let Ok(name) = device.name() {
                debug!("Checking device: {}", name);
                devices.push((device, name));
            }
        }
        Ok(devices)
    }

    fn find_device_by_name(host: &Host, name: &str) -> Result<Device> {
        let devices = Self::input_devices(host)?;

        // An explicitly requested device wins outright, so match its exact name
        // ahead of any heuristic. Run the monitor/loopback preference first and
        // an unrelated device merely containing "monitor" shadows the one that
        // was actually asked for.
        if let Some((device, device_name)) = devices.iter().find(|(_, n)| n == name) {
            info!("Found requested device: {}", device_name);
            return Ok(device.clone());
        }

        // Then fall back to a substring match, so a partial name still works.
        if let Some((device, device_name)) = devices.iter().find(|(_, n)| n.contains(name)) {
            info!("Found device matching '{}': {}", name, device_name);
            return Ok(device.clone());
        }

        // Naming what is there rather than only what is not: the names carry
        // punctuation and capitals nobody guesses, and a partial one matches, so
        // the list is usually the whole answer.
        Err(anyhow::anyhow!(
            "no capture device matching '{name}'. rav can see: {}",
            devices
                .iter()
                .map(|(_, n)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    fn find_monitor_device(host: &Host) -> Result<Device> {
        let devices = Self::input_devices(host)?;

        // First pass: virtual loopback devices that carry system audio. cpal
        // enumerates devices in an unspecified order, so this has to be its own
        // pass rather than one more arm of the heuristics below - otherwise a
        // device that merely matches "monitor" could win over the real source.
        // Now that the list is enumerated once, a pass costs a string scan, so
        // there is nothing to gain by narrowing it to the platform that has the
        // device.
        if let Some((device, name)) = devices.iter().find(|(_, n)| is_system_audio_loopback(n)) {
            info!("Found system audio loopback device: {}", name);
            return Ok(device.clone());
        }

        // Second pass: a capture that carries system output back in, under
        // whatever name the backend gives it.
        //
        // Note what this cannot do. On Linux cpal uses ALSA, which enumerates
        // ALSA PCM names - `default`, `pipewire`, `front:CARD=…`. A PulseAudio
        // or PipeWire monitor *source* is not an ALSA PCM, so it has no name
        // here to match and never will: on a stock PipeWire box this pass finds
        // nothing, and `-d` cannot reach one either, because it searches the
        // same list. docs/audio.md says what to do instead.
        if let Some((device, name)) = devices.iter().find(|(_, n)| is_loopback_capture(n)) {
            info!("Found monitor device: {}", name);
            return Ok(device.clone());
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
        let (tx, rx) = flume::bounded(QUEUE_BLOCKS);
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
            // Only ever used to discard, so the queue can drop its oldest block
            // rather than refuse the newest. flume is MPMC, so a second handle
            // costs nothing and the consumer keeps the one `start` returns.
            let spill = rx.clone();
            let sync_buffer_clone = Arc::clone(&sync_buffer);
            // A capture stream that is running but delivering silence is
            // indistinguishable from a quiet room, and on Linux the device is
            // whatever the ALSA `default` PCM happens to be routed to - so say
            // once what level is actually arriving. The same report the macOS
            // tap makes, for the same reason.
            let mut reported = false;
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !reported {
                    let peak = data.iter().fold(0.0f32, |a, s| a.max(s.abs()));
                    if peak > 0.0 {
                        info!("Capture delivering audio (peak {peak:.3})");
                        reported = true;
                    }
                }

                // Use synchronous operations in the audio callback
                let Ok(mut buf) = sync_buffer_clone.lock() else {
                    return;
                };
                buf.extend_from_slice(data);

                // A `while`, not an `if`. The period the device hands over is
                // not rav's block size and need not be smaller than it: through
                // PipeWire's ALSA plugin it is 1102 frames against a block of
                // 1024. Emitting one block per callback then leaves a remainder
                // that nothing ever catches up on - the display advances slower
                // than real time, drifts further behind with every callback, and
                // the buffer grows for as long as rav runs. macOS never saw it,
                // because the process tap replaces this stream entirely.
                while buf.len() >= buffer_size {
                    let block = AudioData {
                        samples: buf.drain(..buffer_size).collect(),
                    };

                    // Drop the *oldest* queued block to make room, as the macOS
                    // tap does. Discarding the newest instead turns a full queue
                    // into a permanent delay line; dropping the oldest costs
                    // latency once and then recovers.
                    match tx.try_send(block) {
                        Ok(()) => {}
                        Err(flume::TrySendError::Full(block)) => {
                            let _ = spill.try_recv();
                            if tx.try_send(block).is_err() {
                                return;
                            }
                        }
                        Err(flume::TrySendError::Disconnected(_)) => return,
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
    use super::{is_loopback_capture, is_system_audio_loopback};

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

    #[test]
    fn matches_the_loopback_forms_that_carry_system_output() {
        for name in [
            "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor",
            "Monitor of Built-in Audio Analog Stereo",
            "sysdefault:CARD=Loopback",
            "hw:Loopback,1,0",
        ] {
            assert!(is_loopback_capture(name), "{name} should match");
        }
    }

    #[test]
    fn a_device_merely_named_monitor_is_not_a_loopback() {
        // The reason this predicate is not `contains("monitor")`. Both of these
        // are ordinary inputs, and picking one fails silently: the bars still
        // move, they are just showing the wrong source.
        for name in [
            "Studio Monitor Controller",
            "HDMI Monitor Audio",
            "sysdefault:CARD=Monitor",
        ] {
            assert!(!is_loopback_capture(name), "{name} should not match");
        }
    }
}
