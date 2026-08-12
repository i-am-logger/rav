//! A complete look and feel, as one thing.
//!
//! "Winamp-ness" is not a theme. It is the fall speeds, the peak decay, the
//! absence of a hold timer, the colours, the shapes, and the response curve -
//! and those travel together. Scattered across four modules they drift, and
//! adding a second visualiser means finding all four.
//!
//! A preset is a `const`. Adding one is writing another; no trait is needed
//! until a preset wants behaviour these fields cannot express, and designing a
//! trait against a single implementation gets it shaped like that
//! implementation.
//!
//! # White labelling
//!
//! This is the unit a vendor overrides. A speaker with a five-light strip sets
//! its own curve so the range lands where the music is, its own colours, and its
//! own skin, without patching a renderer - and gets rav's physics unchanged.

use crate::skin::{self, Skin};
use crate::theme::StaticTheme;
use rav_core::ballistics::{self, Spec};
use rav_core::units::Curve;

/// Everything that makes one visualiser look and feel like itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    /// How bars and caps move. Per second, never per frame - see [`Spec`].
    pub ballistics: Spec,
    /// The colours.
    pub theme: &'static StaticTheme,
    /// The shapes, and how many steps they divide a rung into.
    pub skin: Skin,
    /// How a measured amplitude becomes a level to draw.
    ///
    /// Linear is rav's default and its documented choice - it is what makes a
    /// quiet passage read quiet. A small strip usually wants otherwise, since
    /// four rungs spread linearly put three of them in the top 12 dB.
    pub curve: Curve,
}

/// What rav starts as: its own ramp, block glyphs, a linear response, and the
/// physics it inherited.
///
/// The default, and the one anything reading "the preset rav ships with" wants.
/// [`WINAMP`] is a *different* look and carries a different theme - naming a
/// preset after the physics it borrowed would make wiring the theme from it
/// change rav's default colours by accident.
pub const RAV: Preset = Preset {
    name: "rav",
    ballistics: ballistics::WINAMP,
    theme: &crate::theme::RAV,
    skin: skin::BLOCKS,
    curve: Curve::Linear,
};

/// The transcribed original: same physics and shapes, its own colours.
pub const WINAMP: Preset = Preset {
    name: "winamp",
    ballistics: ballistics::WINAMP,
    theme: &crate::theme::WINAMP,
    skin: skin::BLOCKS,
    curve: Curve::Linear,
};

/// The same instrument on a short LED bar.
///
/// Two departures, both forced by the hardware rather than chosen for taste: a
/// decibel window, because four rungs of linear amplitude leave the strip dark
/// through most music, and a solid skin, because there are no shapes to draw.
pub const STRIP: Preset = Preset {
    name: "strip",
    ballistics: ballistics::WINAMP,
    theme: &crate::theme::RAV,
    skin: skin::PLAIN,
    curve: Curve::Decibel { floor: -48.0 },
};

#[cfg(test)]
mod tests {
    use super::*;
    use rav_core::units::Level;

    #[test]
    fn the_default_preset_describes_what_rav_actually_starts_as() {
        // A preset is a name for what already happens, not a redesign. If any of
        // these drift, rav looks different and nobody decided that.
        assert_eq!(RAV.ballistics, ballistics::WINAMP);
        assert_eq!(RAV.curve, Curve::Linear, "rav measures linearly");
        assert_eq!(RAV.skin.steps(), 8, "eighth blocks");
        assert_eq!(
            RAV.theme.name,
            crate::theme::Theme::default().name,
            "the default preset must carry the default theme"
        );
    }

    #[test]
    fn the_winamp_preset_is_a_different_look_not_the_default() {
        // The trap this exists to close: WINAMP borrows the physics rav
        // inherited, so it reads like "what rav is" - but it carries its own
        // colours. Wiring a theme from it would change rav's default palette
        // without anyone deciding to.
        assert_eq!(WINAMP.theme.name, "winamp");
        assert_ne!(WINAMP.theme.name, RAV.theme.name);
        assert_eq!(WINAMP.ballistics, RAV.ballistics, "same feel");
        assert_eq!(WINAMP.skin, RAV.skin, "same shapes");
    }

    #[test]
    fn a_strip_preset_departs_only_where_the_hardware_forces_it() {
        // Same physics, same colours. What changes is what four lights can say.
        assert_eq!(STRIP.ballistics, RAV.ballistics, "the feel is the same");
        assert_eq!(STRIP.theme.name, RAV.theme.name, "so are the colours");
        assert_ne!(STRIP.curve, RAV.curve, "but not the response");
        assert!(!STRIP.skin.needs_a_shape_per_step(), "and not the shapes");
    }

    #[test]
    fn the_strip_curve_is_what_makes_four_lights_usable() {
        // The measurement behind the departure: at -20 dBFS a linear four-rung
        // bar shows nothing at all, and through the strip's window it shows two.
        //
        // A tenth of full scale rather than a dB figure converted back, because
        // the rung boundaries sit at exact fractions and a level computed to
        // land on one flips between rungs on the last bit of precision.
        let quiet = Level::new(0.1); // -20 dBFS
        assert_eq!(RAV.curve.apply(quiet).fill(4, 1).whole, 0);
        assert_eq!(STRIP.curve.apply(quiet).fill(4, 1).whole, 2);
    }

    #[test]
    fn a_preset_is_const_so_a_board_needs_no_allocator_to_have_one() {
        // The property that makes this usable on hardware: every field is data,
        // so the whole look and feel lives in read-only memory.
        const CHOSEN: Preset = STRIP;
        assert_eq!(CHOSEN.name, "strip");
    }
}
