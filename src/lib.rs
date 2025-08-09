// Speedy Audio Visualizer Library
pub mod audio;
pub mod config;
pub mod quality;
pub mod signal;
pub mod testing;
pub mod ui;
pub mod visual;

// Re-export commonly used types
pub use config::Config;
pub use signal::SignalProcessor;
