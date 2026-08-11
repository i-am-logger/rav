//! Drawing, independent of where the result is drawn.
//!
//! Nothing here knows about terminals, cells or ratatui. That is the point: the
//! same geometry has to serve a terminal that draws glyphs, a terminal that
//! carries pixels, and a window - see issue #65.

pub mod capability;
pub mod geometry;
pub mod ink;
pub mod raster;

pub use capability::{Capabilities, Requirements, Shortfall};
pub use geometry::{Anchor, BarLayout, Column, Rectangle, Screen};
pub use ink::{Colour, Ink};
pub use raster::{Band, Canvas, CapStyle, Ramp, Scene, Stripe};
