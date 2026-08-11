//! A real-time audio spectrum analyser and oscilloscope.
//!
//! The binary is argument parsing and wiring; everything else is here, so the
//! tests and `rav` compile one copy of the tree rather than two.
//!
//! Capture is in [`audio`], analysis in [`signal`], and what a level looks like
//! in [`visual`] and [`render`]. [`ui`] holds the event loop and the widgets.
//! The portable half - levels, geometry, colours, themes - lives in `rav-core`
//! and `rav-appearance`, re-exported below.

pub mod audio;
pub mod config;
pub mod render;
pub mod signal;
pub mod testing;
pub mod ui;
pub mod visual;

/// The mechanics rav is built on, from the core crate.
///
/// Re-exported so callers need not know `rav-core` exists, while the crate
/// boundary keeps doing the work a module never did: it is `no_std`,
/// allocation-free, and cannot acquire a terminal, a clock or an allocator by
/// accident.
pub use rav_core::units;

// Re-export commonly used types
pub use config::Config;
pub use rav_core::{
    Bounded, CellSize, Cells, Curve, Elapsed, Fill, Frames, Hz, Length, Level, SampleRate, Step,
};
