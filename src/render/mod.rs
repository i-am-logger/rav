//! Drawing, independent of where the result is drawn.
//!
//! Nothing here knows about terminals, cells or ratatui. That is the point: the
//! same geometry has to serve a terminal that draws glyphs, a terminal that
//! carries pixels, and a window - see issue #65.

pub mod geometry;
pub mod ink;
pub mod raster;

pub use geometry::{Layout, Rect, Viewport};
pub use ink::{Ink, Rgba};
pub use raster::{Canvas, Ramp};
