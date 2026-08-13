//! Drawing, independent of where the result is drawn.
//!
//! Nothing here knows about terminals, cells or ratatui. That is the point: the
//! same geometry has to serve a terminal that draws glyphs, a terminal that
//! carries pixels, and a window - see issue #65.
//!
//! Geometry and capability come from `rav-core`, colour and scenes from
//! `rav-appearance`, both `no_std`. Only [`raster`] is defined here, because it
//! owns a `tiny-skia` pixmap and a pixmap needs an allocator.
//!
//! The re-exports below let a surface reach a scene, a ramp and a rectangle
//! from one place.

pub use rav_appearance::{ink, ramp, scene};
pub use rav_core::{capability, geometry};

pub mod raster;
pub mod sprite;

pub use capability::{Capabilities, Requirements, Shortfall};
pub use geometry::{Anchor, BarLayout, Column, Rectangle, Screen};
pub use ink::{Colour, Ink};
pub use ramp::{Ramp, Stripe};
pub use raster::{Canvas, Draw};
pub use scene::{Band, CapStyle, Scene, Style, StyleId};
