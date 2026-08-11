//! Drawing, independent of where the result is drawn.
//!
//! Nothing here knows about terminals, cells or ratatui. That is the point: the
//! same geometry has to serve a terminal that draws glyphs, a terminal that
//! carries pixels, and a window - see issue #65.
//!
//! Almost none of it lives here. Geometry and capability come from `rav-core`,
//! colour and scenes from `rav-appearance`, and both are `no_std` - so a
//! microcontroller gets the same ones. What is left is [`raster`], the one part
//! that genuinely cannot go with them: it owns a `tiny-skia` pixmap, and a
//! pixmap needs an allocator.
//!
//! The rest of this module is the re-exports directly below, so a surface can
//! reach a scene, a ramp and a rectangle from one place without knowing which
//! crate each came from.

pub use rav_appearance::{ink, ramp, scene};
pub use rav_core::{capability, geometry};

pub mod raster;

pub use capability::{Capabilities, Requirements, Shortfall};
pub use geometry::{Anchor, BarLayout, Column, Rectangle, Screen};
pub use ink::{Colour, Ink};
pub use ramp::{Ramp, Stripe};
pub use raster::{Canvas, Draw};
pub use scene::{Band, CapStyle, Scene, Style, StyleId};
