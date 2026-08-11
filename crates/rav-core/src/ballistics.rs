//! How a bar and its cap move over time, as numbers.
//!
//! The parameters, not the integrator. What a level *does* between frames is
//! mechanics and belongs here; running it needs somewhere to keep the state, and
//! that is the analyser's business.
//!
//! # The invariant that makes these portable
//!
//! **Time-based, never frame-based.** Every quantity below is per *second*, and
//! anything consuming them integrates a `dt`. That is the only reason one set of
//! numbers can drive four LEDs and a 16K display: a bar that fell by a fixed
//! amount per frame would fall at whatever rate the display happened to manage,
//! and the same preset would feel different on every target.
//!
//! [`reference_fps`](Spec::reference_fps) does not break that. It is how the
//! per-frame constants rav inherited are *read* into per-second ones, stated
//! once, rather than a frame rate anything is paced to.

/// Bar and cap motion for one preset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    /// How fast a bar falls, in pixels-per-frame numerators over 16.
    pub bar_fall: f32,
    /// The cap's own multiplier, applied on top of the bar's.
    pub peak_fall: f32,
    /// The frame rate the two above are interpreted against.
    ///
    /// Not a rate anything runs at. The numbers rav inherited are per-frame, and
    /// this is what turns them into a speed - so 60 drops a full-scale cap in
    /// about 0.86s and 30 stretches the same curve to 1.72s. A genuine choice
    /// rather than a fact, which is why it is a field.
    pub reference_fps: f32,
}

impl Spec {
    /// How far a bar falls in one second.
    pub fn bar_fall_per_second(&self) -> f32 {
        self.bar_fall * self.reference_fps
    }

    /// How far a cap falls in one second.
    pub fn peak_fall_per_second(&self) -> f32 {
        self.bar_fall * self.peak_fall * self.reference_fps
    }
}

/// The fall speeds a user can cycle through, slowest first.
///
/// Bound to a key; the cap's own multiplier is not, so it keeps its default
/// whichever of these is chosen.
pub const FALL_SPEEDS: [f32; 5] = [3.0, 6.0, 12.0, 16.0, 32.0];

/// What rav has always done: instant attack, exponential fall, and a cap that
/// only appears to hang.
///
/// There is no hold timer here and that is deliberate. The cap looks like it
/// hangs because it is re-seeded with a velocity so small that it takes dozens
/// of frames to become visible motion. Coding an explicit hold followed by a
/// linear fall gives a visibly different feel - which is a fine thing for
/// another preset to choose, and not this one.
pub const WINAMP: Spec = Spec {
    bar_fall: 12.0,
    peak_fall: 1.1,
    reference_fps: 60.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_winamp_spec_is_what_rav_already_does() {
        // Named numbers rather than a redesign: these are the constants the
        // analyser has always used, and a preset that changed them would change
        // how rav feels without anyone deciding to.
        assert_eq!(WINAMP.bar_fall, 12.0);
        assert_eq!(WINAMP.peak_fall, 1.1);
        assert_eq!(WINAMP.reference_fps, 60.0);
        assert!(
            FALL_SPEEDS.contains(&WINAMP.bar_fall),
            "the default is on the dial"
        );
    }

    #[test]
    fn a_cap_falls_more_slowly_than_its_bar() {
        // The whole point of a peak cap: it lingers where the bar has left.
        assert!(WINAMP.peak_fall_per_second() > WINAMP.bar_fall_per_second());
    }

    #[test]
    fn the_dial_runs_slowest_to_fastest() {
        // So cycling it is a direction rather than a shuffle.
        for pair in FALL_SPEEDS.windows(2) {
            assert!(pair[1] > pair[0], "{:?} then {:?}", pair[0], pair[1]);
        }
    }

    #[test]
    fn reading_the_reference_rate_scales_the_speed_and_nothing_else() {
        // Halving it stretches the same curve over twice the time, which is what
        // makes it a reading of the inherited constants rather than a pace.
        let slower = Spec {
            reference_fps: 30.0,
            ..WINAMP
        };
        assert_eq!(
            slower.bar_fall_per_second() * 2.0,
            WINAMP.bar_fall_per_second()
        );
        // And the cap keeps its ratio to the bar at either reading.
        assert_eq!(
            slower.peak_fall_per_second() / slower.bar_fall_per_second(),
            WINAMP.peak_fall_per_second() / WINAMP.bar_fall_per_second()
        );
    }
}
