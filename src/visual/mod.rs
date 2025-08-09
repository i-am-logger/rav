// Modern visual system for the Speedy audio visualizer
// Implements professional-grade graphics with smooth animations,
// gradient effects, and responsive design principles

use std::time::Instant;

pub mod animations;
pub mod colors;
pub mod effects;
pub mod themes;

use animations::AnimationEngine;
use colors::ColorSystem;
use themes::Theme;

/// Main visual rendering system
pub struct VisualEngine {
    pub theme: Theme,
    pub color_system: ColorSystem,
    pub animation_engine: AnimationEngine,
    pub start_time: Instant,
    pub current_mode: VisualizationMode,
    pub history: Vec<Vec<f32>>, // Keep history for smooth transitions
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VisualizationMode {
    NeonBars,
    FluidWave,
    Particles,
    Matrix,
    Spectrum3D,
    Radial,
}

impl VisualEngine {
    pub fn new() -> Self {
        Self {
            theme: Theme::CyberNeon,
            color_system: ColorSystem::new(),
            animation_engine: AnimationEngine::new(),
            start_time: Instant::now(),
            current_mode: VisualizationMode::NeonBars,
            history: Vec::new(),
        }
    }

    /// Update the engine with new audio data
    pub fn update(&mut self, magnitudes: &[f32]) {
        self.history.push(magnitudes.to_vec());
        // Keep only last N frames for smooth transitions
        if self.history.len() > 10 {
            self.history.remove(0);
        }

        self.animation_engine.update();
    }

    /// Cycle to next visualization mode
    pub fn next_mode(&mut self) {
        use VisualizationMode::*;
        self.current_mode = match self.current_mode {
            NeonBars => FluidWave,
            FluidWave => Particles,
            Particles => Matrix,
            Matrix => Spectrum3D,
            Spectrum3D => Radial,
            Radial => NeonBars,
        };
    }

    /// Cycle to next theme
    pub fn next_theme(&mut self) {
        self.theme = self.theme.next();
    }

    /// Get elapsed time since start for animations
    pub fn elapsed_time(&self) -> f32 {
        self.start_time.elapsed().as_secs_f32()
    }
}

impl Default for VisualEngine {
    fn default() -> Self {
        Self::new()
    }
}
