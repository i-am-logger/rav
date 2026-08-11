// RAV Audio Visualizer Library
pub mod audio;
pub mod config;
pub mod render;
pub mod signal;
pub mod testing;
pub mod ui;
pub mod units;
pub mod visual;

// Re-export commonly used types
pub use config::Config;
pub use units::{CellSize, Cells, Elapsed, Fill, Hz, Length, Level, SampleRate, Step};
