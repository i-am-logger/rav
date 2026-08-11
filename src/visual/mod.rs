//! Colour data for the visualiser.

pub mod palette;
pub mod theme;

// The palette itself is portable and lives in rav-appearance; only the OSC 4
// conversation that fills one in needs a terminal, and that stays here.
pub use rav_appearance::Palette;
pub use rav_appearance::Theme;
