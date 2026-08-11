//! The mechanics behind rav, and nothing else.
//!
//! rav runs on anything from a single bar of four single-colour LEDs to a 16K
//! truecolour display. What survives that range is arithmetic: how loud a band
//! is, which of a skin's steps that lands on, where a bar stands in the area it
//! is given, and whether a display can show a visualisation at all.
//!
//! What does *not* survive is everything concrete. This crate never learns what
//! a glyph looks like, what colour stop eleven is, or that a terminal exists.
//! It answers **how many** and **which one**; the appearance layer says what
//! those look like and a surface puts them somewhere.
//!
//! # Why a crate and not a module
//!
//! Because the boundary has to hold mechanically. It was a module with a comment
//! saying "nothing here knows about terminals" for exactly one day before two
//! implementations of the same rule drifted apart - one for the glyph grid, one
//! for the rasteriser - and disagreed on 679 of 19980 cases with nothing to say
//! so. `#![no_std]` here is not a claim about embedded targets; it is the thing
//! that makes such a drift fail to compile.
//!
//! # What that costs
//!
//! No allocator, so nothing returns a `Vec`; geometry hands back one rectangle
//! at a time and iterators do the rest. No clock, so anything with memory takes
//! an [`Elapsed`] and the caller keeps the clock. No floating-point maths beyond
//! what `core` provides - [`Curve`]'s logarithm and power forms are the one
//! exception, and they say so.
//!
//! [`Elapsed`]: units::Elapsed
//! [`Curve`]: units::Curve

#![no_std]

// Tests want `Vec` and the assertion machinery; the library itself must not.
#[cfg(test)]
extern crate std;

pub mod math;

pub mod ballistics;
pub mod capability;
pub mod geometry;
pub mod units;

pub use capability::{Capabilities, Requirements, Shortfall};
pub use geometry::{Anchor, BarLayout, Column, Rectangle, Screen};
pub use units::{
    Bounded, CellSize, Cells, Curve, Elapsed, Fill, Frames, Hz, Length, Level, SampleRate, Step,
};
