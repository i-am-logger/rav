//! Does the picture match the music?
//!
//! The pipeline tests inside the crate check that a sine lands in a bar at its
//! own frequency. That is the mapping working, not the display showing music -
//! music is several notes at once, each carrying harmonics well above its own
//! pitch, and a chain can pass every single-tone check while turning a chord
//! into one lump or reading a note as one of its own overtones.
//!
//! The ground truth is MIDI note numbers, because that is where it comes from:
//! 60 is middle C and middle C is 261.63Hz by definition, so the bar that should
//! light is derivable. Taking it from an audio file instead would mean running
//! an FFT to decide what the FFT should have found.
//!
//! What is *not* here is timing - when a spike arrives relative to when a note
//! starts. That needs the rolling window advanced frame by frame, and the
//! analysis is currently inline in the render loop with no seam to drive it
//! from. These are steady tones, and what they test is where the energy lands.

use rav::signal::mapping::{BarMap, DEFAULT_SCALE, bin_for_hz};
use rav::signal::spectrum::Spectrum;
use rav::testing::{Instrument, Note};

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

/// What the display would show for `samples`.
fn bars_for(samples: &[f32]) -> Vec<f32> {
    let (mut spectrum, map) = mapping();
    let magnitudes = spectrum.analyse(samples);
    let mut bars = Vec::new();
    map.sample(magnitudes, &mut bars);
    bars
}

/// The bar a note should light, derived from its frequency and nothing else.
fn bar_of(note: Note) -> usize {
    let (spectrum, map) = mapping();
    let hz = |bar: usize| map.positions()[bar] * RATE as f32 / spectrum.size() as f32;
    (0..BARS)
        .min_by(|&a, &b| {
            let (a, b) = ((hz(a) - note.hz()).abs(), (hz(b) - note.hz()).abs());
            a.partial_cmp(&b).expect("no NaN in a bar centre")
        })
        .expect("at least one bar")
}

/// Whether a bar stands above both its neighbours.
///
/// Deliberately without a loudness floor. An early version required a peak to
/// clear 15% of the loudest bar, which is a number chosen to make an answer come
/// out - and removing it changed none of the conclusions, only added leakage
/// peaks up in the empty top of the spectrum that no test here asks about.
fn is_peak(bars: &[f32], at: usize) -> bool {
    at > 0 && at + 1 < bars.len() && bars[at] > bars[at - 1] && bars[at] > bars[at + 1]
}

fn loudest_bar(bars: &[f32]) -> usize {
    (0..bars.len())
        .max_by(|&a, &b| bars[a].partial_cmp(&bars[b]).expect("no NaN in a bar"))
        .expect("at least one bar")
}

#[test]
fn a_chord_lights_a_bar_for_every_note_in_it() {
    // Octaves rather than a triad, because a triad at this pitch is closer
    // together than the analysis can separate - see
    // `the_smallest_interval_the_display_can_separate`, which is where that
    // limit is written down rather than worked around silently.
    let chord = [Note(48), Note(60), Note(72), Note(84)];

    for instrument in [
        Instrument::pure(RATE as f32),
        // And again with six partials each, which puts most of the energy
        // *above* the pitches. Four notes and twenty-four partials is a
        // spectrum busy enough to bury a fundamental if anything is going to.
        Instrument::with_harmonics(RATE as f32, 6),
    ] {
        let bars = bars_for(&instrument.play(&chord, Spectrum::DEFAULT_SIZE));
        for note in chord {
            let bar = bar_of(note);
            assert!(
                is_peak(&bars, bar),
                "note {} ({:.1}Hz) should stand out at bar {bar}: {:?}",
                note.0,
                note.hz(),
                &bars[bar - 1..=bar + 1],
            );
        }
    }
}

#[test]
fn a_note_reads_as_its_pitch_rather_than_as_one_of_its_own_harmonics() {
    // Not free, and the reason it is worth asserting: rav lifts the high end of
    // the spectrum deliberately (`equalize_table`), and a real note puts most of
    // its energy above its pitch. Tilt the curve far enough and the tallest bar
    // stops being the note being played and becomes an overtone of it - the
    // display would still look busy and would be reporting the wrong pitch.
    let instrument = Instrument::with_harmonics(RATE as f32, 6);
    for note in [Note(48), Note(60), Note(72), Note(84)] {
        let bars = bars_for(&instrument.play(&[note], Spectrum::DEFAULT_SIZE));
        assert_eq!(
            loudest_bar(&bars),
            bar_of(note),
            "note {} ({:.1}Hz) was read as something else",
            note.0,
            note.hz(),
        );
    }
}

#[test]
fn a_bass_note_and_a_lead_note_are_nowhere_near_each_other() {
    // The coarsest thing a listener would notice: bass on the left, melody in
    // the middle. A mapping that compressed the low end onto a couple of bars
    // would pass a single-tone test and still show a bass line and a lead line
    // on top of one another.
    let bass = bar_of(Note(36)); // C2, 65.4Hz
    let lead = bar_of(Note(84)); // C6, 1046.5Hz
    assert!(
        lead > bass + BARS / 4,
        "C2 at bar {bass} and C6 at bar {lead} are {} bars apart of {BARS}",
        lead - bass,
    );
}

#[test]
fn the_smallest_interval_the_display_can_separate() {
    // rav's resolution, in the units it is heard in. A 1024-point FFT at 48kHz
    // has 46.875Hz bins, which is a fixed distance in hertz and therefore a
    // *shrinking* musical interval as pitch rises - so the display gets sharper
    // as it goes up, and the bass, where the bins are widest relative to the
    // notes, is where it is blurriest.
    //
    // Measured, an octave at a time:
    //
    // | root | pitch | resolves from |
    // |---|---|---|
    // | C3 | 130.8Hz | a minor seventh |
    // | C4 | 261.6Hz | a fifth |
    // | C5 | 523.3Hz | a major third |
    // | C6 | 1046.5Hz | a minor third |
    //
    // So a triad in the middle of a piano is beyond it, which is a real limit
    // and is written here rather than discovered by someone wondering why a
    // chord shows two bars. Doubling the FFT size would roughly halve every
    // number in this table, and this test is what would say so.
    //
    // This is the one test here whose pass rests on a `>` between two floats
    // rather than on an inequality with room in it, so the room was measured.
    // At the tightest boundary - C3 at a minor seventh - the deciding bar
    // stands **1.0% above its neighbour, about 80,000 ULP**; the nearest miss
    // is 3.1% the other way. rustfft dispatches on CPU features, so an x86 CI
    // machine and this arm64 one do not agree bit for bit, but they disagree by
    // single-digit ULP and not by a percent. It will not flake.
    //
    // A semitone of tolerance anyway, for a mapping change too small to move
    // the table. Note it does *not* cover C3's boundary moving: resolution is
    // not monotonic in interval there - 10 semitones separates and 11 does not,
    // because the pair happens to land in bars 4/7 and then 4/8 - so a flip at
    // 10 would jump the answer to 12. That is a real change in what the display
    // resolves and should fail rather than be absorbed.
    let expected = [(48u8, 10u8), (60, 7), (72, 4), (84, 3)];
    let instrument = Instrument::pure(RATE as f32);

    for (root, want) in expected {
        let root = Note(root);
        let separated = (1..=24u8)
            .find(|&semitones| {
                let other = Note(root.0 + semitones);
                // Both must reach *distinct* bars, or "two peaks" is one peak
                // counted twice - which is how a first attempt at this
                // measurement reported that C3 resolves a semitone.
                let (low, high) = (bar_of(root), bar_of(other));
                if low == high {
                    return false;
                }
                let bars = bars_for(&instrument.play(&[root, other], Spectrum::DEFAULT_SIZE));
                is_peak(&bars, low) && is_peak(&bars, high)
            })
            .expect("two notes two octaves apart are separable or something is very wrong");

        assert!(
            separated.abs_diff(want) <= 1,
            "at {:.1}Hz the display separates {separated} semitones, not {want}",
            root.hz(),
        );
    }
}
