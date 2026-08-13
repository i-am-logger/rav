//! System audio capture via a CoreAudio process tap (macOS 14.2+).
//!
//! CoreAudio hands over the mixed output of every process, ahead of the hardware
//! volume and with no virtual device installed. A loopback device would sit
//! *after* the volume, so playback level would ride straight into the display.
//!
//! The tapping symbols are declared here rather than taken from a bindgen crate:
//! `AudioHardwareTapping.h` is wrapped in `#ifdef __OBJC__` because one argument
//! is an Objective-C object, so bindgen cannot see it, but the symbols themselves
//! are plain `extern "C"` in CoreAudio.framework. `CATapDescription` is reached
//! through the Objective-C runtime via `objc2`. No C is compiled.
//!
//! The first run raises a system prompt for "System Audio Recording Only". Until
//! it is granted the tap runs but delivers silence, which is indistinguishable
//! from quiet audio - hence [`Tap::has_signal`].

#![cfg(target_os = "macos")]

use anyhow::{Context, Result, bail};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

type OSStatus = i32;
type AudioObjectID = u32;
type AudioDeviceIOProcID = *mut c_void;

/// `AudioObjectPropertyAddress`, laid out exactly as CoreAudio expects.
#[repr(C)]
#[derive(Clone, Copy)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

/// `AudioStreamBasicDescription`.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
struct StreamDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

#[repr(C)]
struct AudioBuffer {
    number_channels: u32,
    data_byte_size: u32,
    data: *mut c_void,
}

/// Variable-length; `buffers` is the first of `number_buffers`.
#[repr(C)]
struct AudioBufferList {
    number_buffers: u32,
    buffers: [AudioBuffer; 1],
}

#[repr(C)]
struct AudioTimeStamp {
    _opaque: [u8; 64],
}

/// Four-character codes, as CoreAudio spells them.
const fn fourcc(code: &[u8; 4]) -> u32 {
    ((code[0] as u32) << 24) | ((code[1] as u32) << 16) | ((code[2] as u32) << 8) | code[3] as u32
}

/// `kAudioFormatFlagIsFloat` / `kAudioFormatFlagIsNonInterleaved`,
/// `CoreAudioBaseTypes.h`.
const FORMAT_FLAG_IS_FLOAT: u32 = 1 << 0;
const FORMAT_FLAG_IS_NON_INTERLEAVED: u32 = 1 << 5;

const TAP_PROPERTY_UID: u32 = fourcc(b"tuid");
const TAP_PROPERTY_FORMAT: u32 = fourcc(b"tfmt");
const PROPERTY_SCOPE_GLOBAL: u32 = fourcc(b"glob");
const PROPERTY_ELEMENT_MAIN: u32 = 0;

/// `AudioHardwareCreateProcessTap` and its destructor are resolved with `dlsym`
/// rather than linked.
///
/// They are `API_AVAILABLE(macos(14.2))`, and a direct `extern` declaration
/// produces a *non-weak* import: on 11.0-14.1 the symbol is absent and dyld kills
/// the process at first call. That makes the documented fallback unreachable -
/// worse, `CATapDescription` is available from 12.0, so the description builds
/// fine and control reaches the call before dying. Looking the symbols up lets
/// the fallback actually happen.
mod dynamic {
    use super::{AudioObjectID, OSStatus};
    use objc2::runtime::AnyObject;
    use std::ffi::{c_char, c_void};
    use std::sync::OnceLock;

    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }
    /// `dlfcn.h:319` - `((void *) -2)`. Not NULL: passing NULL here silently
    /// finds nothing, which looks exactly like "this macOS is too old".
    const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

    pub type CreateTap = unsafe extern "C" fn(*mut AnyObject, *mut AudioObjectID) -> OSStatus;
    pub type DestroyTap = unsafe extern "C" fn(AudioObjectID) -> OSStatus;

    fn lookup(name: &[u8]) -> Option<*mut c_void> {
        // SAFETY: `name` is NUL-terminated; dlsym only reads it.
        let p = unsafe { dlsym(RTLD_DEFAULT, name.as_ptr().cast()) };
        (!p.is_null()).then_some(p)
    }

    pub fn create() -> Option<CreateTap> {
        static F: OnceLock<Option<usize>> = OnceLock::new();
        let addr =
            *F.get_or_init(|| lookup(b"AudioHardwareCreateProcessTap\0").map(|p| p as usize));
        // SAFETY: the symbol is that function; its signature is from the SDK header.
        addr.map(|a| unsafe { std::mem::transmute::<usize, CreateTap>(a) })
    }

    pub fn destroy() -> Option<DestroyTap> {
        static F: OnceLock<Option<usize>> = OnceLock::new();
        let addr =
            *F.get_or_init(|| lookup(b"AudioHardwareDestroyProcessTap\0").map(|p| p as usize));
        // SAFETY: as above.
        addr.map(|a| unsafe { std::mem::transmute::<usize, DestroyTap>(a) })
    }
}

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {

    fn AudioHardwareCreateAggregateDevice(
        description: *const c_void,
        out_device: *mut AudioObjectID,
    ) -> OSStatus;
    fn AudioHardwareDestroyAggregateDevice(device: AudioObjectID) -> OSStatus;

    fn AudioObjectGetPropertyData(
        object: AudioObjectID,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> OSStatus;

    fn AudioDeviceCreateIOProcID(
        device: AudioObjectID,
        proc_: extern "C" fn(
            AudioObjectID,
            *const AudioTimeStamp,
            *const AudioBufferList,
            *const AudioTimeStamp,
            *mut AudioBufferList,
            *const AudioTimeStamp,
            *mut c_void,
        ) -> OSStatus,
        client_data: *mut c_void,
        out_proc: *mut AudioDeviceIOProcID,
    ) -> OSStatus;
    fn AudioDeviceDestroyIOProcID(device: AudioObjectID, proc_: AudioDeviceIOProcID) -> OSStatus;
    fn AudioDeviceStart(device: AudioObjectID, proc_: AudioDeviceIOProcID) -> OSStatus;
    fn AudioDeviceStop(device: AudioObjectID, proc_: AudioDeviceIOProcID) -> OSStatus;
}

/// Cap on the queued backlog, so a stalled reader cannot grow it without bound.
const MAX_QUEUED: usize = 48_000;

/// Samples the tap has delivered, newest last. Shared with the render thread.
#[derive(Default)]
pub struct TapBuffer {
    pub samples: Vec<f32>,
    /// Set once any non-zero sample arrives. Distinguishes "permission denied or
    /// nothing playing" from "the tap never started".
    pub saw_signal: bool,
}

/// The buffer, plus the means to say something has arrived in it.
///
/// The signal is what lets the render loop wait on the audio device rather than
/// poll it on a timer. `notify_one` is not strictly realtime-safe - it can take
/// a short internal lock when a waiter is parked - and it is used anyway,
/// deliberately: this IOProc is a *tap*, reading a copy of what other processes
/// play. It is in nobody's output path, so being late costs rav a frame and
/// costs no one else a glitch. The callback already takes a `try_lock` on the
/// same reasoning.
#[derive(Default)]
pub struct TapChannel {
    pub buffer: Mutex<TapBuffer>,
    pub ready: Notify,
}

type Shared = Arc<TapChannel>;

/// A running process tap and the private aggregate device that reads it.
pub struct Tap {
    tap_id: AudioObjectID,
    device_id: AudioObjectID,
    proc_id: AudioDeviceIOProcID,
    /// The `Arc` handed to the IOProc as its context, reclaimed on teardown.
    context: *const TapChannel,
    buffer: Shared,
    sample_rate: u32,
    channels: u16,
}

impl Tap {
    /// Create a tap over every process' output and start reading it.
    pub fn start() -> Result<Self> {
        let description = stereo_global_tap()?;

        let create = dynamic::create()
            .context("this macOS has no AudioHardwareCreateProcessTap (needs 14.2 or newer)")?;

        let mut tap_id: AudioObjectID = 0;
        // SAFETY: `description` is a live CATapDescription; `tap_id` is ours.
        let status = unsafe { create(Retained::as_ptr(&description) as *mut _, &mut tap_id) };
        if status != 0 || tap_id == 0 {
            bail!("AudioHardwareCreateProcessTap failed (OSStatus {status})");
        }

        let guard = TapGuard(tap_id);

        let uid = tap_uid(tap_id).context("reading the tap's UID")?;
        let format = tap_format(tap_id).context("reading the tap's format")?;
        // The callback reinterprets the buffer as f32. Nothing else checks that,
        // and a mismatch is not a crash - it is a silently wrong display, which
        // is worse. Reject anything we cannot actually read.
        if format.format_id != fourcc(b"lpcm") {
            bail!("tap delivers a non-PCM format ({:#x})", format.format_id);
        }
        if format.format_flags & FORMAT_FLAG_IS_FLOAT == 0 || format.bits_per_channel != 32 {
            bail!(
                "tap delivers {}-bit non-float samples; only f32 is supported",
                format.bits_per_channel
            );
        }
        if format.format_flags & FORMAT_FLAG_IS_NON_INTERLEAVED != 0 {
            bail!("tap delivers non-interleaved audio, which is not handled");
        }
        let sample_rate = format.sample_rate.round().max(1.0) as u32;
        let channels = format.channels_per_frame.max(1) as u16;

        let (device_id, proc_id, buffer, context) = aggregate_reading(&uid)?;

        std::mem::forget(guard); // ownership passes to the returned Tap
        Ok(Self {
            tap_id,
            device_id,
            proc_id,
            context,
            buffer,
            sample_rate,
            channels,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn buffer(&self) -> Shared {
        Arc::clone(&self.buffer)
    }

    /// Whether any non-zero sample has arrived. False for a long stretch while
    /// audio is playing means the recording permission was refused.
    pub fn has_signal(&self) -> bool {
        self.buffer
            .buffer
            .lock()
            .map(|b| b.saw_signal)
            .unwrap_or(false)
    }

    /// Take everything captured since the last call.
    pub fn drain(&self, out: &mut Vec<f32>) {
        if let Ok(mut b) = self.buffer.buffer.lock() {
            out.clear();
            out.append(&mut b.samples);
        }
    }

    /// Resolves when the device has delivered another buffer.
    ///
    /// The render loop's clock. `Notify` holds one permit, so a buffer that
    /// arrives while the loop is busy is not missed - the next wait returns at
    /// once. It does not count, which is right here: the loop drains everything
    /// queued, so two buffers and one wake are the same amount of work.
    pub async fn ready(&self) {
        self.buffer.ready.notified().await;
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        // Ordering matters: stop the IOProc, then tear down the device, then the
        // tap. Destroying the tap first leaves the aggregate reading a dead
        // object.
        unsafe {
            AudioDeviceStop(self.device_id, self.proc_id);
            // The real synchronisation point: after this the IOProc is retired
            // and cannot be running, so the context can be reclaimed. Doing it
            // after AudioDeviceStop alone would race an in-flight callback.
            AudioDeviceDestroyIOProcID(self.device_id, self.proc_id);
            drop(Arc::from_raw(self.context));
            AudioHardwareDestroyAggregateDevice(self.device_id);
            if let Some(destroy) = dynamic::destroy() {
                destroy(self.tap_id);
            }
        }
    }
}

/// Destroys a tap if we bail out between creating it and taking ownership.
struct TapGuard(AudioObjectID);
impl Drop for TapGuard {
    fn drop(&mut self) {
        if let Some(destroy) = dynamic::destroy() {
            unsafe { destroy(self.0) };
        }
    }
}

/// `[[CATapDescription alloc] initStereoGlobalTapButExcludeProcesses:@[]]`.
///
/// An empty exclusion list means every process, which is what "system audio"
/// means here. The tap is marked private so it does not appear to other apps,
/// and left unmuted so playback still reaches the speakers.
fn stereo_global_tap() -> Result<Retained<AnyObject>> {
    let class = class!(CATapDescription);
    let empty: Retained<NSArray<NSNumber>> = NSArray::new();

    // SAFETY: the selector and its argument type match the header. The init
    // family returns +1, so the raw pointer is adopted rather than retained.
    let description = unsafe {
        let allocated: *mut AnyObject = msg_send![class, alloc];
        let initialised: *mut AnyObject =
            msg_send![allocated, initStereoGlobalTapButExcludeProcesses: &*empty];
        Retained::from_raw(initialised)
    }
    .context("CATapDescription init returned nil")?;

    let name = NSString::from_str("rav");
    unsafe {
        let _: () = msg_send![&*description, setName: &*name];
        let _: () = msg_send![&*description, setPrivate: true];
    }
    Ok(description)
}

fn tap_uid(tap: AudioObjectID) -> Result<Retained<NSString>> {
    let address = PropertyAddress {
        selector: TAP_PROPERTY_UID,
        scope: PROPERTY_SCOPE_GLOBAL,
        element: PROPERTY_ELEMENT_MAIN,
    };
    let mut size = std::mem::size_of::<*const c_void>() as u32;
    let mut value: *const c_void = std::ptr::null();
    // CFStringRef is toll-free bridged to NSString, so the returned +1 reference
    // can be adopted directly.
    let status = unsafe {
        AudioObjectGetPropertyData(
            tap,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            &mut value as *mut _ as *mut c_void,
        )
    };
    if status != 0 || value.is_null() {
        bail!("reading kAudioTapPropertyUID failed (OSStatus {status})");
    }
    // SAFETY: `kAudioTapPropertyUID` hands back a CFString the caller owns -
    // `AudioHardware.h` says so in as many words: "The caller is responsible
    // for releasing the returned CFObject." So it arrives at +1, and
    // `from_raw` is the constructor that takes ownership *without* retaining
    // again, which is what balances it. `Retained` releases on drop.
    //
    // The two ways to get this wrong are a leak and a double free, and neither
    // shows up in a test - which is why the header is quoted rather than
    // paraphrased. `retain` here would leak; `from_raw` on a +0 object, as the
    // Get Rule properties hand back, would over-release.
    //
    // Null is checked above, and `from_raw` returns `None` for it anyway.
    let uid = unsafe { Retained::from_raw(value as *mut NSString) }.context("tap UID was nil")?;
    Ok(uid)
}

fn tap_format(tap: AudioObjectID) -> Result<StreamDescription> {
    let address = PropertyAddress {
        selector: TAP_PROPERTY_FORMAT,
        scope: PROPERTY_SCOPE_GLOBAL,
        element: PROPERTY_ELEMENT_MAIN,
    };
    let mut format = StreamDescription::default();
    let mut size = std::mem::size_of::<StreamDescription>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            tap,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            &mut format as *mut _ as *mut c_void,
        )
    };
    if status != 0 {
        bail!("reading kAudioTapPropertyFormat failed (OSStatus {status})");
    }
    // A short write leaves the tail at Default (zero), so sample_rate and
    // channels would silently fall back to 1 rather than failing.
    if size as usize != std::mem::size_of::<StreamDescription>() {
        bail!("kAudioTapPropertyFormat returned {size} bytes, expected 40");
    }
    Ok(format)
}

/// Build a private aggregate device wrapping the tap, and start reading it.
///
/// A tap is not itself readable; it has to be a sub-device of an aggregate. The
/// dictionary keys are the string literals from `AudioHardware.h` - `"taps"`,
/// `"uid"`, `"tapautostart"` - and NSDictionary is toll-free bridged to
/// CFDictionaryRef, so it can be passed straight to CoreAudio.
#[allow(clippy::type_complexity)]
fn aggregate_reading(
    tap_uid: &NSString,
) -> Result<(
    AudioObjectID,
    AudioDeviceIOProcID,
    Shared,
    *const TapChannel,
)> {
    let sub_tap =
        NSDictionary::from_slices(&[&*NSString::from_str("uid")], &[tap_uid as &AnyObject]);
    let taps = NSArray::from_slice(&[&*sub_tap as &AnyObject]);

    let unique = format!("com.i-am-logger.rav.tap.{}", std::process::id());
    let keys: [&NSString; 5] = [
        &NSString::from_str("uid"),
        &NSString::from_str("name"),
        &NSString::from_str("private"),
        &NSString::from_str("taps"),
        &NSString::from_str("tapautostart"),
    ];
    let values: [&AnyObject; 5] = [
        &*NSString::from_str(&unique) as &AnyObject,
        &*NSString::from_str("rav") as &AnyObject,
        &*NSNumber::new_i32(1) as &AnyObject,
        &*taps as &AnyObject,
        &*NSNumber::new_i32(1) as &AnyObject,
    ];
    let description = NSDictionary::from_slices(&keys, &values);

    let mut device_id: AudioObjectID = 0;
    let status = unsafe {
        AudioHardwareCreateAggregateDevice(
            Retained::as_ptr(&description) as *const c_void,
            &mut device_id,
        )
    };
    if status != 0 || device_id == 0 {
        bail!("AudioHardwareCreateAggregateDevice failed (OSStatus {status})");
    }

    // Preallocated so the realtime callback never grows the Vec. Allocating on
    // the audio thread can take the allocator lock, which is exactly what a
    // realtime callback must not do.
    //
    // The drain counts the arriving samples before it runs, so after appending
    // the length is *exactly* `MAX_QUEUED` for any buffer no larger than the
    // cap - which is every buffer a device delivers. One times the cap carries
    // that on its own.
    //
    // The second cap is for the case the drain cannot reach: a single buffer
    // longer than the whole queue empties it and then becomes it, so the length
    // is that buffer's. `MAX_QUEUED` is a second of audio at 48kHz, so this is
    // headroom against a device nobody has, not against the ordinary path.
    let buffer: Shared = Arc::new(TapChannel {
        buffer: Mutex::new(TapBuffer {
            samples: Vec::with_capacity(MAX_QUEUED * 2),
            saw_signal: false,
        }),
        ready: Notify::new(),
    });
    // Leaked deliberately: the IOProc runs on a realtime thread that outlives
    // this call, and the pointer is reclaimed when the Tap is dropped.
    let context = Arc::into_raw(Arc::clone(&buffer)) as *mut c_void;

    let mut proc_id: AudioDeviceIOProcID = std::ptr::null_mut();
    let status = unsafe { AudioDeviceCreateIOProcID(device_id, io_proc, context, &mut proc_id) };
    // A null proc with noErr would start the hardware with nothing registered:
    // the tap would appear to run and deliver silence forever, which is the one
    // failure `has_signal` exists to tell apart.
    if status != 0 || proc_id.is_null() {
        unsafe { AudioHardwareDestroyAggregateDevice(device_id) };
        bail!("AudioDeviceCreateIOProcID failed (OSStatus {status})");
    }

    let status = unsafe { AudioDeviceStart(device_id, proc_id) };
    if status != 0 {
        unsafe {
            AudioDeviceDestroyIOProcID(device_id, proc_id);
            AudioHardwareDestroyAggregateDevice(device_id);
        }
        bail!("AudioDeviceStart failed (OSStatus {status})");
    }

    Ok((device_id, proc_id, buffer, context as *const TapChannel))
}

/// Realtime callback. Copies interleaved float samples into the shared buffer.
///
/// Must not allocate beyond the `Vec` push, block, or log. The buffer is capped
/// so a stalled reader cannot grow it without bound.
extern "C" fn io_proc(
    _device: AudioObjectID,
    _now: *const AudioTimeStamp,
    input: *const AudioBufferList,
    _input_time: *const AudioTimeStamp,
    _output: *mut AudioBufferList,
    _output_time: *const AudioTimeStamp,
    context: *mut c_void,
) -> OSStatus {
    if input.is_null() || context.is_null() {
        return 0;
    }
    // SAFETY: `context` is the Arc we passed to AudioDeviceCreateIOProcID and is
    // valid until the Tap is dropped, which happens after AudioDeviceStop.
    let shared = unsafe { &*(context as *const TapChannel) };

    let list = unsafe { &*input };
    if list.number_buffers == 0 {
        return 0;
    }
    let buffer = unsafe { &*list.buffers.as_ptr() };
    if buffer.data.is_null() {
        return 0;
    }
    let count = buffer.data_byte_size as usize / std::mem::size_of::<f32>();
    // Alignment is an obligation of `from_raw_parts` and CoreAudio does not
    // promise it for `mData` - it is a `void*`, and the guarantee is nowhere in
    // `CoreAudioTypes.h`. In practice every buffer is at least as aligned as
    // the allocator makes it, so this never fires; checking is two instructions
    // and the alternative is undefined behaviour on the realtime thread if it
    // ever does. Dropping the buffer beats reading it wrongly.
    if !(buffer.data as usize).is_multiple_of(std::mem::align_of::<f32>()) {
        return 0;
    }
    // SAFETY: the most dangerous line here, so the obligations are named rather
    // than left to be re-derived. `data` is non-null, checked above, and
    // aligned for `f32`, checked directly above. The length comes from the
    // buffer's own `data_byte_size` rather than from a frame or channel count,
    // so it cannot claim more than CoreAudio wrote - deriving it any other way
    // is how this reads off the end. The format is checked to be 32-bit float
    // when the tap is created, so `f32` is the element type. CoreAudio owns the
    // memory for the length of this call and the slice does not outlive it:
    // everything below copies out of it before returning.
    let samples = unsafe { std::slice::from_raw_parts(buffer.data as *const f32, count) };

    if let Ok(mut out) = shared.buffer.try_lock() {
        if !out.saw_signal && samples.iter().any(|s| *s != 0.0) {
            out.saw_signal = true;
        }
        let queued = out.samples.len();
        if queued + samples.len() > MAX_QUEUED {
            let overflow = (queued + samples.len() - MAX_QUEUED).min(queued);
            out.samples.drain(..overflow);
        }
        out.samples.extend_from_slice(samples);
    }
    // Outside the lock, so the woken loop never arrives to find it still held.
    // A wake with nothing behind it - the `try_lock` above having failed - costs
    // one drain of an empty buffer, which is cheaper than missing a buffer.
    shared.ready.notify_one();
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourcc_matches_coreaudios_spelling() {
        // 'tuid', 'tfmt' and 'glob' as CoreAudio writes them in AudioHardware.h.
        assert_eq!(TAP_PROPERTY_UID, 0x74756964);
        assert_eq!(TAP_PROPERTY_FORMAT, 0x74666d74);
        assert_eq!(PROPERTY_SCOPE_GLOBAL, 0x676c6f62);
    }

    #[test]
    fn format_flags_match_coreaudios_values() {
        // kAudioFormatFlagIsFloat = 1<<0, kAudioFormatFlagIsNonInterleaved = 1<<5.
        assert_eq!(FORMAT_FLAG_IS_FLOAT, 1);
        assert_eq!(FORMAT_FLAG_IS_NON_INTERLEAVED, 32);
        assert_eq!(fourcc(b"lpcm"), 0x6c70636d);
    }

    #[test]
    fn structs_match_the_c_layout() {
        // AudioStreamBasicDescription is 40 bytes: f64 + 8 u32s.
        assert_eq!(std::mem::size_of::<StreamDescription>(), 40);
        assert_eq!(std::mem::size_of::<PropertyAddress>(), 12);
    }

    /// End-to-end, against the real system: creates a tap, reads it for a few
    /// seconds and reports what arrived. Ignored by default because it needs the
    /// "System Audio Recording Only" permission and audio actually playing.
    ///
    /// `cargo test --lib -- --ignored --nocapture tap_captures`
    #[test]
    #[ignore]
    fn tap_captures_system_audio() {
        let tap = Tap::start().expect("tap should start");
        println!(
            "tap running: {} Hz, {} channel(s)",
            tap.sample_rate(),
            tap.channels()
        );
        let mut total = 0usize;
        let mut peak = 0.0f32;
        let mut out = Vec::new();
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            tap.drain(&mut out);
            total += out.len();
            for s in &out {
                peak = peak.max(s.abs());
            }
        }
        println!(
            "captured {total} samples, peak {peak:.4}, signal={}",
            tap.has_signal()
        );
        assert!(
            total > 0,
            "no samples arrived - is the tap permission granted?"
        );
    }

    #[test]
    fn a_tap_description_can_be_built() {
        // Does not create a tap, so it needs no permission - it only proves the
        // Objective-C class and selector resolve at runtime.
        assert!(stereo_global_tap().is_ok());
    }
}
