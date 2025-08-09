#![allow(dead_code)]
// Animation engine for smooth transitions and visual effects
// Provides easing functions, interpolation, and time-based animations

use std::f32::consts::PI;
use std::time::Instant;

/// Animation engine for smooth transitions and effects
#[derive(Debug, Clone)]
pub struct AnimationEngine {
    last_update: Instant,
    pub delta_time: f32,
    pub total_time: f32,
    pub smoothing_factor: f32,
    pub transition_speed: f32,
}

impl AnimationEngine {
    pub fn new() -> Self {
        Self {
            last_update: Instant::now(),
            delta_time: 0.0,
            total_time: 0.0,
            smoothing_factor: 0.1,
            transition_speed: 1.0,
        }
    }

    /// Update the animation engine and calculate delta time
    pub fn update(&mut self) {
        let now = Instant::now();
        self.delta_time = now.duration_since(self.last_update).as_secs_f32();
        self.total_time += self.delta_time;
        self.last_update = now;
    }

    /// Smooth interpolation between current and target values
    pub fn smooth_lerp(&self, current: f32, target: f32) -> f32 {
        current + (target - current) * self.smoothing_factor
    }

    /// Linear interpolation between two values
    pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t.clamp(0.0, 1.0)
    }

    /// Smooth step interpolation (smoother than linear)
    pub fn smooth_step(t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// Elastic easing function for bouncy effects
    pub fn elastic_ease_out(t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if t == 0.0 || t == 1.0 {
            t
        } else {
            2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * (2.0 * PI / 3.0)).sin() + 1.0
        }
    }

    /// Bounce easing function
    pub fn bounce_ease_out(t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if t < 1.0 / 2.75 {
            7.5625 * t * t
        } else if t < 2.0 / 2.75 {
            let t = t - 1.5 / 2.75;
            7.5625 * t * t + 0.75
        } else if t < 2.5 / 2.75 {
            let t = t - 2.25 / 2.75;
            7.5625 * t * t + 0.9375
        } else {
            let t = t - 2.625 / 2.75;
            7.5625 * t * t + 0.984375
        }
    }

    /// Sine wave oscillation
    pub fn sine_wave(&self, frequency: f32, amplitude: f32, phase: f32) -> f32 {
        amplitude * (self.total_time * frequency * 2.0 * PI + phase).sin()
    }

    /// Cosine wave oscillation
    pub fn cosine_wave(&self, frequency: f32, amplitude: f32, phase: f32) -> f32 {
        amplitude * (self.total_time * frequency * 2.0 * PI + phase).cos()
    }

    /// Pulsing effect based on sine wave
    pub fn pulse(&self, frequency: f32, min_value: f32, max_value: f32) -> f32 {
        let normalized = (self.sine_wave(frequency, 1.0, 0.0) + 1.0) / 2.0;
        Self::lerp(min_value, max_value, normalized)
    }

    /// Create a sawtooth wave pattern
    pub fn sawtooth(&self, frequency: f32) -> f32 {
        let cycle_time = 1.0 / frequency;
        let position = (self.total_time % cycle_time) / cycle_time;
        position * 2.0 - 1.0
    }

    /// Create a triangle wave pattern
    pub fn triangle_wave(&self, frequency: f32) -> f32 {
        let sawtooth = self.sawtooth(frequency);
        if sawtooth >= 0.0 {
            1.0 - sawtooth
        } else {
            1.0 + sawtooth
        }
    }

    /// Exponential decay for fade effects
    pub fn exponential_decay(&self, start_time: f32, decay_rate: f32) -> f32 {
        let elapsed = self.total_time - start_time;
        if elapsed < 0.0 {
            1.0
        } else {
            (-decay_rate * elapsed).exp()
        }
    }

    /// Smooth array interpolation for magnitude transitions
    pub fn smooth_magnitude_array(&self, current: &mut [f32], target: &[f32]) {
        let len = current.len().min(target.len());
        for i in 0..len {
            current[i] = self.smooth_lerp(current[i], target[i]);
        }
    }

    /// Apply peak decay to magnitude values
    pub fn apply_peak_decay(&self, current: &mut [f32], decay_rate: f32) {
        let decay_factor = 1.0 - decay_rate * self.delta_time;
        for value in current.iter_mut() {
            *value *= decay_factor;
        }
    }

    /// Get a time-based offset for staggered animations
    pub fn staggered_time(&self, index: usize, stagger_amount: f32) -> f32 {
        self.total_time + (index as f32) * stagger_amount
    }

    /// Get a normalized time value (0-1) over a given period
    pub fn normalized_time(&self, period: f32) -> f32 {
        (self.total_time % period) / period
    }

    /// Apply time-based jitter for randomness
    pub fn time_jitter(&self, amplitude: f32, frequency: f32) -> f32 {
        let noise1 = (self.total_time * frequency).sin();
        let noise2 = (self.total_time * frequency * 1.3 + 0.5).cos();
        (noise1 + noise2 * 0.5) * amplitude
    }

    /// Smooth color transition
    pub fn interpolate_colors(&self, colors: &[(f32, f32, f32)], position: f32) -> (f32, f32, f32) {
        if colors.is_empty() {
            return (0.0, 0.0, 0.0);
        }

        if colors.len() == 1 {
            return colors[0];
        }

        let position = position.clamp(0.0, 1.0);
        let segment_size = 1.0 / (colors.len() - 1) as f32;
        let segment_index = (position / segment_size).floor() as usize;
        let segment_index = segment_index.min(colors.len() - 2);

        let local_position = (position % segment_size) / segment_size;
        let smoothed_position = Self::smooth_step(local_position);

        let color1 = colors[segment_index];
        let color2 = colors[segment_index + 1];

        (
            Self::lerp(color1.0, color2.0, smoothed_position),
            Self::lerp(color1.1, color2.1, smoothed_position),
            Self::lerp(color1.2, color2.2, smoothed_position),
        )
    }
}
