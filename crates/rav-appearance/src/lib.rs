//! What rav's abstract steps look like.
//!
//! [`rav_core`] answers *how many* and *which one* - stop eleven of sixteen,
//! step five of eight - and never learns what any of them look like. This crate
//! is the other half of that: it turns an index into a colour, a shape, or a
//! brightness, and it is the reason an ESP32 with a screen looks like rav rather
//! than merely working.
//!
//! It is a layer beside the core rather than inside it, because appearance has
//! to be portable in its own right. One theme dresses a glyph grid today, and
//! is meant to dress a kitty image, a window and an LED matrix on the same
//! terms; only the last mile differs.
//!
//! # The awkward case that shapes the API
//!
//! A terminal is the one surface that **cannot** accept a resolved colour. The
//! `terminal` and `mono` themes exist to defer to whatever palette the user
//! runs, so an [`Ink::Ansi`] must reach it *unresolved* or those themes stop
//! following the user's colours. Every other target - pixels, a window, an RGB
//! LED - wants the resolved value. So this layer hands back an [`Ink`] as
//! readily as a [`Colour`], and each surface settles it the only way it can.
//!
//! # Why no rasteriser
//!
//! Because a crate cannot pick one up by accident and a comment saying so can
//! be ignored. Nothing here draws: [`Ramp`] hands back the stripes a screen
//! should be painted in and never paints them, so the same ramp answers a
//! rasteriser, a glyph grid and a strip of LEDs - none of which agree on what
//! painting means. `tiny-skia` lives one crate up, with the surface that owns a
//! pixmap.
//!
//! [`Ramp`]: ramp::Ramp

#![no_std]

// `Ramp` owns its stops, so this crate needs an allocator where the core does
// not. That is honest rather than ideal: a target with no allocator at all
// wants a `Ramp` that borrows its stops from a `const`, which is a change to
// make when there is hardware to check it against rather than before.
extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod ink;
mod names;
pub mod palette;
pub mod preset;
pub mod ramp;
pub mod scene;
pub mod skin;
pub mod theme;

pub use ink::{ANSI_NAMES, Colour, DEFAULT_ANSI, Ink};
pub use palette::Palette;
pub use preset::Preset;
pub use ramp::{Ramp, Stripe};
pub use scene::{Band, CapStyle, Scene, Style, StyleId, View};
pub use skin::{Rung, Shapes, Skin, ladders};
pub use theme::{SCOPE_LEVELS, STOPS, StaticTheme, Theme};
