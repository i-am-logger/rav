//! Colour, as a theme states it and as a surface needs it.
//!
//! A theme says either an exact colour or the *name* of one, and the difference
//! is load-bearing rather than a convenience. `#188408` is a promise about the
//! hue; `green` is a deferral - it means "whatever green is here", which is what
//! lets the `terminal` and `mono` themes follow the palette the user already
//! runs.
//!
//! So [`Ink`] keeps the two apart all the way to the surface, and each surface
//! settles it in the way that surface can. A terminal hands the name straight
//! back and lets the terminal paint it. A pixel renderer has nobody to ask, so
//! it resolves through a [`Palette`] and falls back to the standard values.
//!
//! [`Palette`]: crate::palette::Palette

/// A colour as a theme wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// An exact colour, `#rrggbb`.
    Rgb(u8, u8, u8),
    /// One of the sixteen ANSI slots, by index. `0..=7` are the normal colours
    /// and `8..=15` their bright twins, which is the order every terminal uses
    /// and the order a theme author reads off their own configuration.
    Ansi(u8),
}

/// A resolved colour, with the alpha a compositing surface needs.
///
/// Named where a name exists: `Colour::WHITE` says what it is, where three hex
/// bytes only say what it equals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Colour {
    pub const TRANSPARENT: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    };

    pub const BLACK: Self = Self::rgb(0x00, 0x00, 0x00);
    pub const WHITE: Self = Self::rgb(0xff, 0xff, 0xff);
    pub const RED: Self = Self::rgb(0xff, 0x00, 0x00);
    pub const GREEN: Self = Self::rgb(0x00, 0xff, 0x00);
    pub const BLUE: Self = Self::rgb(0x00, 0x00, 0xff);
    pub const YELLOW: Self = Self::rgb(0xff, 0xff, 0x00);
    pub const MAGENTA: Self = Self::rgb(0xff, 0x00, 0xff);
    pub const CYAN: Self = Self::rgb(0x00, 0xff, 0xff);

    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 0xff,
        }
    }

    /// From the `0xrrggbb` a theme file spells out, so a colour lifted from a
    /// design reads the same in code as it does in the TOML.
    pub const fn hex(value: u32) -> Self {
        Self::rgb(
            ((value >> 16) & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
        )
    }

    /// The same hue at a fraction of its brightness.
    ///
    /// All three channels together, which keeps the hue - scaling per channel
    /// drains a saturated colour towards black by way of brown.
    pub fn dimmed(self, keep: f32) -> Self {
        let scale = |c: u8| rav_core::math::round(f32::from(c) * keep).clamp(0.0, 255.0) as u8;
        Self {
            red: scale(self.red),
            green: scale(self.green),
            blue: scale(self.blue),
            alpha: self.alpha,
        }
    }

    pub fn is_transparent(self) -> bool {
        self.alpha == 0
    }

    /// Perceived brightness, by the Rec. 709 weights.
    ///
    /// For a surface that has to render this colour in grey - a monochrome
    /// screen, or a photograph of one.
    ///
    /// **Not for driving a single-colour LED bar.** The eye weights green far
    /// above red, so a green-to-red ramp *falls* from 0.71 to 0.21 as the bar
    /// rises, and peak level would come out the dimmest part of the display.
    /// Intensity on a bar has to follow the step's height rather than its hue;
    /// that is a separate decision from what colour the step is.
    pub fn luminance(self) -> f32 {
        (0.2126 * f32::from(self.red)
            + 0.7152 * f32::from(self.green)
            + 0.0722 * f32::from(self.blue))
            / 255.0
    }
}

/// The sixteen ANSI colours as a terminal that will not say paints them.
///
/// The standard values, so a pixel surface has an answer at all: it has no
/// terminal to interrogate, and leaving a named colour unresolved there would
/// mean not drawing it. A terminal that *does* answer overrides every one of
/// these - this is the floor, not the intent.
pub const DEFAULT_ANSI: [Colour; 16] = [
    Colour::rgb(0x00, 0x00, 0x00), // black
    Colour::rgb(0x80, 0x00, 0x00), // red
    Colour::rgb(0x00, 0x80, 0x00), // green
    Colour::rgb(0x80, 0x80, 0x00), // yellow
    Colour::rgb(0x00, 0x00, 0x80), // blue
    Colour::rgb(0x80, 0x00, 0x80), // magenta
    Colour::rgb(0x00, 0x80, 0x80), // cyan
    Colour::rgb(0xc0, 0xc0, 0xc0), // white
    Colour::rgb(0x80, 0x80, 0x80), // bright-black
    Colour::rgb(0xff, 0x00, 0x00), // bright-red
    Colour::rgb(0x00, 0xff, 0x00), // bright-green
    Colour::rgb(0xff, 0xff, 0x00), // bright-yellow
    Colour::rgb(0x00, 0x00, 0xff), // bright-blue
    Colour::rgb(0xff, 0x00, 0xff), // bright-magenta
    Colour::rgb(0x00, 0xff, 0xff), // bright-cyan
    Colour::rgb(0xff, 0xff, 0xff), // bright-white
];

/// The ANSI names a theme may use, in slot order.
///
/// The names rather than a terminal library's own spelling: `white` is slot 7
/// and `bright-white` is slot 15, which is what a theme author sees in their
/// configuration. A library that calls those `Gray` and `White` is describing
/// its own type, not the terminal.
pub use crate::names::ANSI_NAMES;

impl Ink {
    /// The slot an ANSI name refers to.
    pub fn from_name(name: &str) -> Option<Self> {
        ANSI_NAMES
            .iter()
            .position(|&n| n == name)
            .map(|i| Self::Ansi(i as u8))
    }

    /// This colour without asking anyone - the standard value for a name.
    ///
    /// Only for a surface with no terminal to ask. Anything that *can* ask
    /// should, or the `terminal` theme stops following the user's colours,
    /// which is the whole reason that theme exists.
    pub fn resolved(self) -> Colour {
        match self {
            Self::Rgb(r, g, b) => Colour::rgb(r, g, b),
            Self::Ansi(i) => DEFAULT_ANSI[usize::from(i) & 0x0f],
        }
    }

    /// Whether this colour is exact, rather than deferred to the terminal.
    pub fn is_exact(self) -> bool {
        matches!(self, Self::Rgb(..))
    }

    /// This ink as a theme's `darken` asks for it.
    ///
    /// `keep` is `None` when the theme gave **no instruction**, which is not the
    /// same as keeping all of the brightness. A theme that says nothing gets its
    /// ink handed on untouched, so a named colour stays named and the terminal
    /// still paints it from the user's palette - `mono` declares no `darken` and
    /// a `grid` of `bright-black`, and folding "absent" into `Some(1.0)` would
    /// resolve it against a snapshot of the palette taken at startup.
    ///
    /// `known` is what the terminal said this ink is, if it said anything. There
    /// is nothing to scale otherwise, and inventing a value would replace the
    /// theme rather than dim it.
    ///
    /// Returns an `Ink` rather than a `Colour` so each surface still resolves at
    /// its own boundary: the terminal hands a name back as a name, a pixel
    /// surface falls back to the standard values.
    pub fn dimmed(self, keep: Option<f32>, known: Option<(u8, u8, u8)>) -> Self {
        match (keep, known) {
            (None, _) | (Some(_), None) => self,
            (Some(keep), Some((r, g, b))) => {
                // All three channels together, which keeps the hue. Per channel
                // drains a saturated colour to black by way of brown.
                let scale =
                    |c: u8| rav_core::math::round(f32::from(c) * keep).clamp(0.0, 255.0) as u8;
                Self::Rgb(scale(r), scale(g), scale(b))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `no_std` drops the prelude, so a test that builds a string says so.
    use alloc::format;

    #[test]
    fn a_rising_ramp_loses_luminance_which_is_why_brightness_is_not_derived_from_it() {
        // The trap this test exists to pin. A green-to-red ramp reads as rising
        // to the eye, but green weighs more than red in perceived brightness, so
        // its luminance falls - and a single-colour LED bar driven from these
        // numbers would be dimmest at peak level.
        let floor = Colour::GREEN.luminance();
        let ceiling = Colour::RED.luminance();
        assert!(
            floor > ceiling,
            "green {floor} should out-weigh red {ceiling}"
        );
        assert!(Colour::WHITE.luminance() > 0.99);
        assert!(Colour::BLACK.luminance() < 0.01);
    }

    #[test]
    fn a_name_round_trips_through_its_slot() {
        for (i, name) in ANSI_NAMES.iter().enumerate() {
            assert_eq!(
                Ink::from_name(name),
                Some(Ink::Ansi(i as u8)),
                "{name} should be slot {i}"
            );
        }
    }

    #[test]
    fn the_bright_twins_sit_eight_slots_above_their_own_colour() {
        // Every terminal lays the sixteen out this way, and a theme's `grid`
        // ladder relies on it: the dark twin of the colour a row lights up in
        // is the same index minus eight.
        for slot in 0..8u8 {
            let name = ANSI_NAMES[usize::from(slot)];
            let bright = format!("bright-{name}");
            assert_eq!(Ink::from_name(&bright), Some(Ink::Ansi(slot + 8)));
        }
    }

    #[test]
    fn an_unknown_name_is_not_a_colour() {
        assert_eq!(Ink::from_name("puce"), None);
        assert_eq!(Ink::from_name("bright-puce"), None);
        // Case matters: themes are written in lower case and a near miss should
        // say so rather than silently pick something.
        assert_eq!(Ink::from_name("Green"), None);
    }

    #[test]
    fn an_exact_colour_resolves_to_itself() {
        assert_eq!(
            Ink::Rgb(0x18, 0x84, 0x08).resolved(),
            Colour::rgb(0x18, 0x84, 0x08)
        );
        assert!(Ink::Rgb(0, 0, 0).is_exact());
        assert!(!Ink::Ansi(2).is_exact());
    }

    #[test]
    fn every_slot_has_a_fallback() {
        // A pixel surface has no terminal to ask, so a name with no answer
        // would mean not drawing at all.
        for slot in 0..16u8 {
            assert_eq!(Ink::Ansi(slot).resolved(), DEFAULT_ANSI[usize::from(slot)]);
        }
        // The index is masked rather than trusted, so a malformed slot cannot
        // panic in the middle of a frame.
        assert_eq!(Ink::Ansi(200).resolved(), DEFAULT_ANSI[8]);
    }
}
