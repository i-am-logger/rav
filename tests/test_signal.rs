//! Does `--test-audio` draw the right picture?
//!
//! The signal exists so someone with no loopback and no permission can see rav
//! working. That is only worth anything if what they see is *true* - a sweep
//! that lit the wrong bar, or lit it at the wrong height, or arrived late, would
//! be a display agreeing with itself while telling them nothing about rav.
//!
//! So the three things a bar claims are checked separately: **which** bar
//! (frequency), **how tall** (level), and **when** (timing). Ground truth comes
//! from the sweep itself - it can be asked what it is playing at any instant -
//! rather than from a second copy of the curve written out here.

use rav::audio::synthetic::{AMPLITUDE, Sweep};
use rav::signal::mapping::{BarMap, DEFAULT_SCALE, bin_for_hz};
use rav::signal::spectrum::{MAX_HEIGHT, Spectrum};
use rav::testing::AudioGenerator;

const RATE: u32 = 48_000;
/// A display width in the middle of what rav actually runs at.
const BARS: usize = 60;
/// The top of the default frequency range.
const CEILING: u32 = 16_000;

fn mapping() -> (Spectrum, BarMap) {
    let spectrum = Spectrum::new(Spectrum::DEFAULT_SIZE).expect("a power-of-two size");
    let top = bin_for_hz(CEILING, RATE, spectrum.bins());
    let map = BarMap::with_top_bin(BARS, spectrum.bins(), top, DEFAULT_SCALE);
    (spectrum, map)
}

/// The bars a window of samples draws, as fractions of full scale.
fn bars_for(samples: &[f32]) -> Vec<f32> {
    let (mut spectrum, map) = mapping();
    let mut bars = Vec::new();
    map.sample(spectrum.analyse(samples), &mut bars);
    bars.iter().map(|b| b / MAX_HEIGHT).collect()
}

/// The bar a frequency belongs in, derived from the mapping and nothing else.
fn bar_for(hz: f32) -> usize {
    let (spectrum, map) = mapping();
    let centre = |bar: usize| map.positions()[bar] * RATE as f32 / spectrum.size() as f32;
    (0..BARS)
        .min_by(|&a, &b| {
            let (a, b) = ((centre(a) - hz).abs(), (centre(b) - hz).abs());
            a.partial_cmp(&b).expect("no NaN in a bar centre")
        })
        .expect("at least one bar")
}

fn loudest(bars: &[f32]) -> usize {
    bars.iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .expect("no bars at all")
}

/// One analysis window of the sweep, taken `seconds` into a pass, with the
/// frequency it was drawn at in the middle of that window.
fn window_at(seconds: f32) -> (Vec<f32>, f32) {
    let mut sweep = Sweep::new(RATE);
    // Half a window short, so the frequency reported is the one at the centre of
    // what gets analysed rather than at its leading edge.
    let half = Spectrum::DEFAULT_SIZE as f32 / 2.0 / RATE as f32;
    sweep.advance((seconds - half).max(0.0));
    let mut samples = sweep.block(Spectrum::DEFAULT_SIZE / 2);
    let hz = sweep.hz();
    samples.extend(sweep.block(Spectrum::DEFAULT_SIZE / 2));
    (samples, hz)
}

#[test]
fn the_peak_lands_in_the_bar_that_owns_the_frequency() {
    // The claim a spectrum analyser makes. Checked across the pass rather than
    // at one point, because a mapping that is right in the middle and wrong at
    // the ends still looks convincing while it sweeps.
    for seconds in [1.0f32, 3.0, 6.0, 9.0, 11.0] {
        let (samples, hz) = window_at(seconds);
        let bars = bars_for(&samples);
        let lit = loudest(&bars);
        let expected = bar_for(hz);

        assert!(
            lit.abs_diff(expected) <= 1,
            "{seconds}s in, the sweep is playing {hz:.0}Hz - that belongs in bar \
             {expected}, and the loudest bar is {lit}",
        );
    }
}

#[test]
fn the_peak_stands_clear_of_the_bars_either_side_of_it() {
    // A single tone is one bar and its skirts. If the neighbours matched it the
    // display would be showing a smear, and "the right bar is loudest" would be
    // true of a picture nobody could read a frequency off.
    let (samples, hz) = window_at(6.0);
    let bars = bars_for(&samples);
    let lit = loudest(&bars);

    for neighbour in [lit.saturating_sub(2), (lit + 2).min(BARS - 1)] {
        if neighbour == lit {
            continue;
        }
        assert!(
            bars[neighbour] < bars[lit] * 0.5,
            "at {hz:.0}Hz bar {neighbour} is {:.3} against the peak's {:.3} - \
             two bars away should not be half as loud",
            bars[neighbour],
            bars[lit],
        );
    }
}

#[test]
fn the_level_follows_the_amplitude_it_was_played_at() {
    // The height of a bar has to mean something. rav's response is linear by
    // default - that is what makes a quiet passage read quiet - so halving the
    // signal has to halve the bar, not shift it by some amount of its own
    // choosing.
    let (samples, hz) = window_at(6.0);
    let bars = bars_for(&samples);
    let lit = loudest(&bars);

    let halved: Vec<f32> = samples.iter().map(|s| s * 0.5).collect();
    let quieter = bars_for(&halved);

    assert!(bars[lit] > 0.0, "the peak has no height at all");
    let ratio = bars[lit] / quieter[lit];
    assert!(
        (ratio - 2.0).abs() < 0.1,
        "half the amplitude drew {:.4} against {:.4} - a ratio of {ratio:.2}, not 2",
        quieter[lit],
        bars[lit],
    );

    // And the sweep is worth exactly what a clean tone of the same pitch and
    // loudness is worth. This is what pins the *generator* rather than the
    // chain: a waveform that had drifted off a sine - harmonics, clipping, a
    // shaped envelope - would put its energy somewhere else and draw a
    // different height, while still passing every ratio above.
    let reference =
        AudioGenerator::new(RATE as f32).sine_wave(hz, AMPLITUDE, Spectrum::DEFAULT_SIZE);
    let clean = bars_for(&reference);
    let ratio = bars[lit] / clean[loudest(&clean)];
    assert!(
        (ratio - 1.0).abs() < 0.05,
        "the sweep drew {:.4} at {hz:.0}Hz where a pure sine of the same \
         amplitude draws {:.4}",
        bars[lit],
        clean[loudest(&clean)],
    );

    // And the same signal twice draws the same bar. A level that wandered would
    // make every assertion above a coincidence.
    let again = bars_for(&samples);
    assert_eq!(bars[lit], again[lit], "the same window drew two heights");
}

#[test]
fn the_peak_travels_up_the_display_as_the_pass_goes_on() {
    // Timing: the picture has to move, in the right direction, at a pace that
    // gets it across the display in one pass. A sweep that reached the top in
    // the first second and sat there would satisfy every frequency check above
    // taken on its own.
    let seconds = [0.5f32, 3.0, 6.0, 9.0, 11.5];
    let walked: Vec<usize> = seconds
        .iter()
        .map(|&s| loudest(&bars_for(&window_at(s).0)))
        .collect();

    for pair in walked.windows(2) {
        assert!(
            pair[1] > pair[0],
            "the peak did not move on: {walked:?} at {seconds:?} seconds",
        );
    }

    // And it crosses the whole display rather than a corner of it: near the
    // bottom early, near the top late.
    assert!(
        walked[0] < BARS / 4,
        "half a second in, the peak is already at bar {} of {BARS}",
        walked[0],
    );
    assert!(
        *walked.last().expect("five readings") > BARS * 3 / 4,
        "at the end of the pass the peak has only reached bar {} of {BARS}",
        walked.last().expect("five readings"),
    );
}

#[test]
fn a_pass_takes_the_time_it_says_and_then_starts_again() {
    // The sweep is a loop, and the seam is where a click would live: the tone
    // has to arrive back at the bottom having gone all the way up, not jump
    // there from wherever it had reached.
    let mut sweep = Sweep::new(RATE);
    let start = sweep.hz();

    sweep.advance(12.0);
    let after_a_pass = sweep.hz();
    assert!(
        (after_a_pass - start).abs() < 1.0,
        "twelve seconds in, the sweep is at {after_a_pass:.0}Hz rather than back \
         at the {start:.0}Hz it started from",
    );

    // Just before the seam it is at the top, so the whole range really is used.
    let mut nearly = Sweep::new(RATE);
    nearly.advance(11.9);
    assert!(
        nearly.hz() > 14_000.0,
        "the pass ends at {:.0}Hz, short of the 16kHz it advertises",
        nearly.hz(),
    );

    // Nothing here checks the wrap for a click, and that is deliberate: a single
    // discontinuity at the seam was measured and it spreads *less* energy than
    // the clean signal does, because at 16kHz there are three samples to a cycle
    // and the tone already swings the full amplitude between neighbours. The
    // discontinuity that would matter is one per sample, from computing
    // `sin(2*pi*f*t)` with a moving `f` - and that is caught by
    // `the_peak_lands_in_the_bar_that_owns_the_frequency`, since it puts the
    // energy at the wrong frequency entirely.
}
