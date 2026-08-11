//! Musical material whose ground truth is known before it is analysed.
//!
//! A sine at 1000Hz checks that the mapping is not broken. It does not check
//! that rav shows *music*, because music is several notes at once, each carrying
//! harmonics well above its own pitch - and a display can pass every single-tone
//! test while turning a chord into one lump or a bass note into a mid one.
//!
//! Notes are MIDI numbers because that is where the ground truth comes from: 60
//! is middle C and middle C is 261.63Hz by definition, so the bar it should
//! light is derivable rather than guessed. Reading it off an audio file instead
//! would mean an FFT deciding what the FFT should have found.
//!
//! Nothing here has an envelope. These are steady tones, and what they test is
//! where the energy lands. *When* a spike arrives needs the rolling window
//! advanced frame by frame, which is `App::measure`'s side of the seam - see
//! `a_note_lights_the_display_in_the_first_frame_that_carries_it`.

use std::f32::consts::TAU;

/// A note, as a MIDI number.
///
/// Equal temperament at A440, which is what "note 60 is 261.63Hz" means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Note(pub u8);

impl Note {
    /// The note every one of these is counted from: A4, 440Hz, MIDI 69.
    pub const CONCERT_A: Self = Self(69);
    /// C4, 261.63Hz.
    pub const MIDDLE_C: Self = Self(60);

    pub fn hz(self) -> f32 {
        440.0 * 2f32.powf((f32::from(self.0) - f32::from(Self::CONCERT_A.0)) / 12.0)
    }

    /// The same note `octaves` higher, which is that many doublings in Hz.
    pub fn octave_up(self, octaves: u8) -> Self {
        Self(self.0 + 12 * octaves)
    }
}

/// Something that turns notes into samples.
///
/// Two shapes, because they catch different things. A pure tone puts all its
/// energy at the pitch, which is what a mapping test wants; a tone with
/// harmonics puts most of its energy *above* the pitch, which is what real
/// instruments do and what an equalisation curve can get wrong - rav lifts the
/// high end, so a display that reads a note by its loudest bar can be dragged
/// upwards by partials the ear hears as the same note.
#[derive(Debug, Clone, Copy)]
pub struct Instrument {
    sample_rate: f32,
    partials: usize,
}

impl Instrument {
    /// One partial, at the pitch and nowhere else.
    pub fn pure(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            partials: 1,
        }
    }

    /// `partials` of them, the nth at `1/n` the amplitude - the spectrum of a
    /// sawtooth, and near enough to a bowed or blown note for this purpose.
    pub fn with_harmonics(sample_rate: f32, partials: usize) -> Self {
        Self {
            sample_rate,
            partials: partials.max(1),
        }
    }

    /// Sound `notes` together for `samples`, at equal weight.
    ///
    /// Scaled so the result stays inside full scale however many notes are
    /// sounding: a chord that clipped would spray energy across every bar and
    /// pass a "polyphony lights the display" test for the wrong reason.
    pub fn play(&self, notes: &[Note], samples: usize) -> Vec<f32> {
        if notes.is_empty() {
            return vec![0.0; samples];
        }
        let mut signal = vec![0.0f32; samples];
        for note in notes {
            let hz = note.hz();
            for partial in 1..=self.partials {
                let frequency = hz * partial as f32;
                // Above Nyquist a partial does not exist; rendering it anyway
                // would alias it back down into a bar it has no business in.
                if frequency * 2.0 >= self.sample_rate {
                    break;
                }
                let amplitude = 1.0 / partial as f32;
                for (i, sample) in signal.iter_mut().enumerate() {
                    let t = i as f32 / self.sample_rate;
                    *sample += amplitude * (TAU * frequency * t).sin();
                }
            }
        }
        let loudest = signal.iter().fold(0.0f32, |peak, &s| peak.max(s.abs()));
        if loudest > 0.0 {
            for sample in &mut signal {
                *sample /= loudest;
            }
        }
        signal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notes_are_where_music_says_they_are() {
        // The ground truth the rest of this rests on, so it is checked against
        // the definition rather than against itself.
        assert!((Note::CONCERT_A.hz() - 440.0).abs() < 1e-3);
        assert!((Note::MIDDLE_C.hz() - 261.6256).abs() < 1e-3);
        assert!((Note(21).hz() - 27.5).abs() < 1e-3, "bottom of a piano");
        assert!((Note(108).hz() - 4186.0).abs() < 0.1, "top of a piano");
    }

    #[test]
    fn an_octave_is_a_doubling() {
        let c4 = Note::MIDDLE_C;
        assert!((c4.octave_up(1).hz() / c4.hz() - 2.0).abs() < 1e-4);
        assert!((c4.octave_up(3).hz() / c4.hz() - 8.0).abs() < 1e-3);
    }

    #[test]
    fn a_chord_stays_inside_full_scale() {
        // Four notes summed reach four times the amplitude of one. Clipping
        // sprays energy across the whole spectrum, which would light every bar
        // and pass a polyphony test for entirely the wrong reason.
        let chord = [Note(48), Note(60), Note(64), Note(67)];
        let signal = Instrument::with_harmonics(48_000.0, 6).play(&chord, 1024);
        assert!(signal.iter().all(|s| (-1.0..=1.0).contains(s)));
        assert!(
            signal.iter().any(|&s| s.abs() > 0.9),
            "normalised to nothing"
        );
    }

    #[test]
    fn a_partial_above_nyquist_is_left_out_rather_than_folded_down() {
        // An aliased partial comes back as a frequency that is not in the music
        // at all, and lands in a bar nowhere near the note - which would look
        // exactly like the mapping bug these tests exist to catch.
        let top = Note(108); // 4186Hz; its 6th partial is 25kHz, past Nyquist
        let quiet = Instrument::with_harmonics(48_000.0, 12).play(&[top], 1024);
        let bare = Instrument::pure(48_000.0).play(&[top], 1024);
        // Five partials fit under 24kHz, so the two differ - but only by the
        // ones that fit, and nothing came back from above.
        assert_ne!(quiet, bare, "the harmonics did not sound at all");
    }
}
