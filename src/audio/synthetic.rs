//! A signal rav can draw when there is nothing to capture.
//!
//! What `--test-audio` plays. A machine with no loopback, no recording
//! permission, or no sound card at all still shows a working display, which is
//! the difference between "rav is broken" and "rav cannot hear anything here" -
//! and on Linux, where routing system output into a capture is genuinely
//! awkward, it is the first thing worth running.
//!
//! A sweep rather than a tone: a single frequency lights one bar and proves
//! almost nothing, while a sweep walks the whole display, so every band, the
//! bar fall and the peak caps are all visible in one pass.

use super::AudioData;
use flume::Receiver;
use std::time::{Duration, Instant};
use tracing::info;

/// Where the sweep starts and ends.
///
/// From the lowest note a small speaker reproduces to the top of the default
/// frequency range, so the peak arrives at the left edge and leaves at the
/// right rather than starting or ending out of view.
const LOWEST: f32 = 40.0;
const HIGHEST: f32 = 16_000.0;

/// How long one pass takes.
///
/// Slow enough to watch a bar rise, hold and fall behind the peak - which is the
/// motion worth looking at - and short enough that the display is not still
/// filling up when someone decides whether rav works.
const SWEEP: Duration = Duration::from_secs(12);

/// Loud, but not against the ceiling: a signal that clipped every bar to full
/// scale would hide exactly the ballistics this exists to show.
const AMPLITUDE: f32 = 0.5;

/// Blocks a second, matching what a capture device delivers.
///
/// The render loop wakes on audio arriving, so this is what sets the frame rate.
/// A hundred a second leaves room for the 60fps cap to be the thing that limits
/// it, as it is with a real device.
const BLOCKS_PER_SECOND: u32 = 100;

/// The frequency `turn` of the way through a pass, where 1.0 is a whole one.
///
/// Logarithmic, so the peak crosses the display at a steady pace: the bars are
/// spaced by octave, not by hertz. A function rather than an expression inside
/// the loop so a test can ask the same question the generator answers - a test
/// that recomputes the curve for itself agrees with itself whatever this does.
fn sweep_hz(turn: f32) -> f32 {
    LOWEST * (HIGHEST / LOWEST).powf(turn.fract())
}

/// Start the test signal, and hand back the channel it feeds.
///
/// One mono channel. The thread ends when the receiver is dropped, so nothing
/// needs to stop it.
pub fn start(sample_rate: u32) -> Receiver<AudioData> {
    let (tx, rx) = flume::bounded(super::QUEUE_BLOCKS);
    let rate = sample_rate.max(1);
    let per_block = (rate / BLOCKS_PER_SECOND).max(1) as usize;
    let block_time = Duration::from_secs_f64(f64::from(per_block as u32) / f64::from(rate));

    info!(
        "Playing the built-in test signal: {LOWEST:.0}Hz to {HIGHEST:.0}Hz every {}s",
        SWEEP.as_secs()
    );

    std::thread::spawn(move || {
        // Phase is integrated rather than computed from the frequency and the
        // time, because a sweep changes frequency every sample: `sin(2*pi*f*t)`
        // with a moving `f` jumps the phase each time it moves, which is a click
        // on every sample and broadband noise across the whole display.
        let mut phase = 0.0f32;
        let mut elapsed = 0.0f32;
        let dt = 1.0 / rate as f32;
        let mut due = Instant::now();

        loop {
            let mut block = Vec::with_capacity(per_block);
            for _ in 0..per_block {
                let hz = sweep_hz(elapsed / SWEEP.as_secs_f32());
                phase = (phase + std::f32::consts::TAU * hz * dt) % std::f32::consts::TAU;
                block.push(AMPLITUDE * phase.sin());
                elapsed += dt;
            }

            if tx.send(AudioData { samples: block }).is_err() {
                // The app has gone. Nothing to clean up: the channel is the only
                // thing this thread holds.
                return;
            }

            // A deadline rather than a sleep of one block, so the signal keeps
            // real time instead of drifting slower by however long the work
            // above took, every block, for as long as rav runs.
            due += block_time;
            if let Some(nap) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(nap);
            } else {
                // Behind schedule: give up the missed time rather than sending a
                // burst, which would arrive as a jump in the display.
                due = Instant::now();
            }
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sweep_covers_the_range_it_advertises() {
        // The frequency at the start, at the end and in the middle of one pass,
        // asked of the generator itself, not recomputed here. A sweep that only
        // reached a third of the way up would still look busy and would leave
        // most of the display dark.
        assert!((sweep_hz(0.0) - LOWEST).abs() < 1.0);
        assert!((sweep_hz(0.999) - HIGHEST).abs() < HIGHEST * 0.01);
        // Halfway through is the geometric mean, not the arithmetic one - which
        // is what puts the peak in the middle of a display spaced by octave.
        assert!((sweep_hz(0.5) - (LOWEST * HIGHEST).sqrt()).abs() < 1.0);
    }

    #[test]
    fn it_delivers_blocks_at_something_like_a_capture_rate() {
        let rx = start(48_000);
        let first = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("no block arrived");
        assert_eq!(first.samples.len(), 480, "48kHz in 100 blocks is 480 each");

        // Not silence, and not clipped: a display fed by this has to show
        // something, and has to show the ballistics rather than a solid wall.
        let peak = first.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.0, "the test signal is silent");
        assert!(
            peak <= AMPLITUDE + 1e-6,
            "louder than it asked to be: {peak}"
        );
    }

    #[test]
    fn what_it_plays_reaches_the_display() {
        // The whole claim of `--test-audio`: someone with no loopback and no
        // permission sees bars move. Blocks that are the right size and the
        // right loudness still prove nothing if the analysis makes nothing of
        // them, so this runs the real chain over the real output.
        use crate::signal::mapping::{BarMap, DEFAULT_SCALE};
        use crate::signal::spectrum::Spectrum;

        let rx = start(48_000);
        let mut window = Vec::new();
        while window.len() < Spectrum::DEFAULT_SIZE {
            let block = rx
                .recv_timeout(Duration::from_secs(2))
                .expect("the signal stopped");
            window.extend_from_slice(&block.samples);
        }
        window.truncate(Spectrum::DEFAULT_SIZE);

        let mut spectrum = Spectrum::new(Spectrum::DEFAULT_SIZE).expect("a power-of-two size");
        let mut bars = Vec::new();
        BarMap::new(60, spectrum.bins(), DEFAULT_SCALE)
            .sample(spectrum.analyse(&window), &mut bars);

        let lit = bars.iter().filter(|b| **b > 0.0).count();
        assert!(lit > 0, "the test signal draws nothing: {bars:?}");

        // And it starts at the bottom, which is where a sweep should begin - a
        // signal that lit the top first would mean the sweep runs backwards.
        let loudest = bars
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .expect("no bars at all");
        assert!(
            loudest < bars.len() / 2,
            "the sweep starts in the upper half: bar {loudest} of {}",
            bars.len(),
        );
    }

    #[test]
    fn the_thread_stops_when_nothing_is_listening() {
        // Otherwise every run of the tests leaves a thread generating audio into
        // a channel nobody reads, for as long as the process lives.
        let rx = start(48_000);
        rx.recv_timeout(Duration::from_secs(2)).expect("no block");
        drop(rx);
        // Nothing to assert but the absence of a hang: the send fails and the
        // thread returns. If it did not, this test would still pass - which is
        // why the `is_err` return above is the thing being read, not timing.
    }
}
