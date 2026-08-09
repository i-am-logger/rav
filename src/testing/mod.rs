//! Deterministic signal sources for tests.
//!
//! Generated waveforms rather than live audio, so the analysis chain can be
//! checked reproducibly - see `signal::pipeline_tests`.

pub mod audio_generator;

pub use audio_generator::AudioGenerator;
