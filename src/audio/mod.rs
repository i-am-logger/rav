//! Where the sound comes in.
//!
//! Three sources, and only two of them are a microphone. This module is the
//! cpal capture stream, which is what Linux uses and what macOS falls back to.
//! `tap` is the CoreAudio process tap macOS prefers, which needs no virtual
//! device and reads ahead of the system volume - named here rather than linked,
//! because it exists only on macOS and this page is built on Linux. And
//! [`synthetic`] is the signal `--test-audio` plays when there is nothing to
//! listen to.
//!
//! # The rule this module is governed by
//!
//! **A capture callback runs on a realtime thread, and must not allocate, lock
//! or block.** Not a style preference: `malloc` can wait on a lock the whole
//! process shares, and a callback that is late does not get to catch up - the
//! samples for that period are simply gone, and what the listener hears is a
//! dropout. Nothing in a callback here may do any of the three.
//!
//! What that comes to in practice, since none of it is visible from the shape
//! of the code:
//!
//! - **The buffers are made once**, at startup, and handed round. A block takes
//!   itself home when it is dropped - see [`AudioData`] - so the consumer needs
//!   to know nothing about it and the callback never asks for memory.
//! - **The accumulation buffer is owned by the callback** and sized for four
//!   blocks. It never grows; a device handing over more than that drops the
//!   period rather than reallocating, which costs one gap in the display and
//!   *shows* a device rav cannot follow.
//! - **The queue is bounded and discards its oldest block.** An unbounded one
//!   becomes a delay line that grows for as long as rav runs, which is what it
//!   used to do; discarding the newest instead makes a full queue permanent.
//! - **Nothing waits.** Every send is a `try_send`, and the tap's callback takes
//!   a `try_lock` it is allowed to lose.
//!
//! The one thing left on that thread is `flume` itself, which takes a brief
//! internal lock. Closing that would mean a lock-free ring, and a dependency.
//!
//! [`synthetic`] is exempt and says so: it paces itself on a thread of its own,
//! answers to nobody, and allocating there costs nothing.
//!
//! # Only one of them runs
//!
//! On macOS the tap replaces this stream entirely - `main` stops the cpal
//! capture once the tap starts, which frees a device and a callback rather than
//! memory, since the queue was bounded anyway. `--test-audio` opens no device at
//! all, which is the whole of its use: a machine with no loopback, no recording
//! permission or no sound card still shows a working display.

pub mod synthetic;
#[cfg(target_os = "macos")]
pub mod tap;

use anyhow::{Context, Result};
use cpal::{
    Device, Host, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use flume::{Receiver, Sender};
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
pub struct AudioData {
    pub samples: AudioBuffer,
    /// Where the buffer goes once this block is finished with.
    ///
    /// A capture callback runs on a realtime thread and must not allocate:
    /// `malloc` can block on a lock the whole process shares, and a block of
    /// samples arriving every ten milliseconds is not where you want to find
    /// that out. So the buffers are made once, at startup, and handed round -
    /// the consumer drops the block, the buffer goes home, and the callback
    /// picks it up again.
    ///
    /// `None` for a source with no realtime thread behind it, which is the
    /// synthetic generator: it paces itself and has nothing to lose by
    /// allocating.
    home: Option<Sender<AudioBuffer>>,
}

impl AudioData {
    /// A block that owns its buffer outright, for a source that can afford to
    /// allocate one.
    pub fn loose(samples: AudioBuffer) -> Self {
        Self {
            samples,
            home: None,
        }
    }

    /// A block borrowed from a pool, which goes back when it is dropped.
    fn borrowed(samples: AudioBuffer, home: Sender<AudioBuffer>) -> Self {
        Self {
            samples,
            home: Some(home),
        }
    }
}

impl Drop for AudioData {
    fn drop(&mut self) {
        let Some(home) = self.home.take() else {
            return;
        };
        let mut spent = std::mem::take(&mut self.samples);
        spent.clear();
        // A full or closed pool means the buffer is simply freed, which is how
        // an ordinary shutdown ends.
        let _ = home.try_send(spent);
    }
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

        // Every buffer the callback will ever send, made here where allocating
        // is free. Two more than the queue holds, so a block in flight and a
        // block being read still leave one to fill.
        let (home, spare) = flume::bounded::<AudioBuffer>(QUEUE_BLOCKS + 2);
        for _ in 0..QUEUE_BLOCKS + 2 {
            let _ = home.try_send(Vec::with_capacity(buffer_size));
        }

        // Built through a factory because the callback is consumed by
        // build_input_stream, and the retry below needs a second one.
        let make_data_fn = || {
            let tx = tx.clone();
            // Only ever used to discard, so the queue can drop its oldest block
            // rather than refuse the newest. flume is MPMC, so a second handle
            // costs nothing and the consumer keeps the one `start` returns.
            let spill = rx.clone();
            let home = home.clone();
            let spare = spare.clone();
            // Owned by the callback, which is the only thing that ever touches
            // it - so there is nothing for a lock to guard, and no lock on the
            // realtime thread. Room for four blocks, and it never grows: see
            // below for what happens when a device hands over more than that.
            let mut pending: AudioBuffer = Vec::with_capacity(buffer_size * 4);
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

                // A period longer than four blocks would grow this, and growing
                // it means `malloc` on the realtime thread. Dropping the period
                // instead costs one gap in the display; a device that does it
                // every time is a device rav cannot follow, and quietly falling
                // behind for the rest of the run would hide that rather than
                // show it.
                if pending.len() + data.len() > pending.capacity() {
                    pending.clear();
                    return;
                }
                pending.extend_from_slice(data);

                // A `while`, not an `if`. The period the device hands over is
                // not rav's block size and need not be smaller than it: through
                // PipeWire's ALSA plugin it is 1102 frames against a block of
                // 1024. Emitting one block per callback then leaves a remainder
                // that nothing ever catches up on - the display advances slower
                // than real time, drifts further behind with every callback, and
                // the buffer grows for as long as rav runs. macOS never saw it,
                // because the process tap replaces this stream entirely.
                while pending.len() >= buffer_size {
                    let Ok(mut buffer) = spare.try_recv().or_else(|_| {
                        // Every buffer is out with the consumer, which means it
                        // is behind. Dropping the oldest queued block sends its
                        // buffer home - the same choice a full queue makes, for
                        // the same reason.
                        drop(spill.try_recv());
                        spare.try_recv()
                    }) else {
                        return;
                    };
                    buffer.clear();
                    buffer.extend_from_slice(&pending[..buffer_size]);
                    pending.drain(..buffer_size);
                    let block = AudioData::borrowed(buffer, home.clone());

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

#[cfg(test)]
mod pooling {
    use super::*;

    #[test]
    fn a_finished_block_sends_its_buffer_home_to_be_filled_again() {
        // The whole of what keeps `malloc` off the realtime thread. A capture
        // callback that allocates can block on a lock the rest of the process
        // shares, and a block of samples arriving every ten milliseconds is not
        // where anyone wants to find that out.
        let (home, spare) = flume::bounded::<AudioBuffer>(2);
        let buffer = Vec::with_capacity(1024);
        let address = buffer.as_ptr();

        let block = AudioData::borrowed(buffer, home.clone());
        assert!(spare.try_recv().is_err(), "it went home before it was used");
        drop(block);

        let back = spare.try_recv().expect("the buffer never came home");
        assert!(
            back.is_empty(),
            "it came home with the last block still in it"
        );
        assert_eq!(
            back.capacity(),
            1024,
            "it came home without the room it was given, so the next fill allocates",
        );
        assert_eq!(back.as_ptr(), address, "that is a different buffer");
    }

    #[test]
    fn a_block_from_a_source_with_no_pool_simply_ends() {
        // The synthetic generator paces itself on a thread of its own and has
        // nothing to lose by allocating, so it does not carry a pool around.
        let block = AudioData::loose(vec![0.5; 8]);
        assert_eq!(block.samples.len(), 8);
        drop(block);
    }

    #[test]
    fn a_pool_nobody_is_listening_to_frees_the_buffer_rather_than_blocking() {
        // How an ordinary shutdown ends: the stream outlives the consumer for a
        // moment, and a block finishing then must not wait for room that is
        // never coming.
        let (home, spare) = flume::bounded::<AudioBuffer>(1);
        drop(spare);
        drop(AudioData::borrowed(Vec::with_capacity(4), home));
    }
}
