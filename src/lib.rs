// RAV Audio Visualizer Library
pub mod audio;
pub mod config;
pub mod render;
pub mod signal;
pub mod surface;
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
    Bounded, CellSize, Cells, Curve, Elapsed, Fill, Hz, Length, Level, SampleRate, Step,
};
