// Advanced visual effects and custom rendering
// Implements sophisticated graphics beyond basic ratatui widgets

use super::themes::Theme;
use ratatui::{
    layout::Rect,
    style::Color,
    text::{Line, Span},
};

/// Custom bar visualization with advanced effects
pub struct NeonBars {
    pub bars: Vec<NeonBar>,
    pub glow_effect: bool,
    pub wave_effect: bool,
}

#[derive(Clone)]
pub struct NeonBar {
    pub height: f32,
    pub target_height: f32,
    pub color: Color,
    pub glow_color: Color,
    pub position: f32,
    pub width: u16,
    pub wave_offset: f32,
}

impl NeonBars {
    pub fn new(count: usize, area_width: u16) -> Self {
        let bar_width = (area_width.saturating_sub(2) / count.max(1) as u16).max(1);
        let bars = (0..count)
            .map(|i| NeonBar {
                height: 0.0,
                target_height: 0.0,
                color: Color::Cyan,
                glow_color: Color::White,
                position: i as f32 / count.max(1) as f32,
                width: bar_width,
                wave_offset: 0.0,
            })
            .collect();

        Self {
            bars,
            glow_effect: true,
            wave_effect: true,
        }
    }

    pub fn update(&mut self, magnitudes: &[f32], theme: Theme, time: f32, delta_time: f32) {
        for (i, bar) in self.bars.iter_mut().enumerate() {
            if i < magnitudes.len() {
                bar.target_height = magnitudes[i];

                // Smooth interpolation
                let lerp_speed = 8.0 * delta_time;
                bar.height = bar.height + (bar.target_height - bar.height) * lerp_speed;

                // Color based on frequency and intensity
                let intensity = bar.height.clamp(0.0, 1.0);
                bar.color = theme.get_color(i % theme.palette().len(), intensity);

                // Glow effect for neon themes
                if theme.use_glow() {
                    bar.glow_color = theme.accent_color();
                }

                // Wave effect
                if self.wave_effect {
                    bar.wave_offset = (time * 2.0 + bar.position * 4.0).sin() * 0.1;
                }
            }
        }
    }

    /// Render bars as text-based graphics
    pub fn render_text(&self, area_height: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let max_height = area_height.saturating_sub(4) as f32;

        // Create visualization lines from bottom to top
        for row in 0..area_height.saturating_sub(4) {
            let y_pos = 1.0 - (row as f32 / max_height);
            let mut spans = Vec::new();

            for bar in &self.bars {
                let bar_height = bar.height + bar.wave_offset;
                let char = if y_pos <= bar_height {
                    if y_pos > bar_height - 0.1 && self.glow_effect {
                        "▄" // Full block for main bar
                    } else if y_pos > bar_height - 0.2 {
                        "▀" // Upper half block for glow
                    } else {
                        "█" // Full block
                    }
                } else {
                    " " // Empty space
                };

                let color = if y_pos <= bar_height {
                    if y_pos > bar_height - 0.1 && self.glow_effect {
                        bar.glow_color
                    } else {
                        bar.color
                    }
                } else {
                    Color::Reset
                };

                spans.push(Span::styled(
                    char.repeat(bar.width as usize),
                    ratatui::style::Style::default().fg(color),
                ));
            }

            lines.push(Line::from(spans));
        }

        lines.reverse(); // Reverse to show from bottom to top
        lines
    }
}

/// Fluid wave visualization
pub struct FluidWave {
    pub points: Vec<f32>,
    pub smoothed_points: Vec<f32>,
    pub wave_resolution: usize,
    pub amplitude: f32,
    pub frequency: f32,
}

impl FluidWave {
    pub fn new(resolution: usize) -> Self {
        Self {
            points: vec![0.0; resolution],
            smoothed_points: vec![0.0; resolution],
            wave_resolution: resolution,
            amplitude: 1.0,
            frequency: 1.0,
        }
    }

    pub fn update(&mut self, magnitudes: &[f32], time: f32, delta_time: f32) {
        // Interpolate magnitudes to wave resolution
        for (i, point) in self.points.iter_mut().enumerate() {
            let mag_index = (i * magnitudes.len()) / self.wave_resolution;
            let mag_index = mag_index.min(magnitudes.len().saturating_sub(1));
            let target = magnitudes[mag_index] * self.amplitude;

            // Add some wave motion
            let wave_offset = (time * self.frequency + (i as f32 * 0.1)).sin() * 0.1;
            *point = target + wave_offset;
        }

        // Smooth the wave
        for i in 0..self.smoothed_points.len() {
            let target = self.points[i];
            let lerp_speed = 10.0 * delta_time;
            self.smoothed_points[i] =
                self.smoothed_points[i] + (target - self.smoothed_points[i]) * lerp_speed;
        }
    }

    /// Render wave as text-based graphics
    pub fn render_text(&self, area: Rect, theme: Theme) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let height = area.height.saturating_sub(4) as f32;
        let width = area.width.saturating_sub(4) as f32;

        for row in 0..area.height.saturating_sub(4) {
            let y = 1.0 - (row as f32 / height);
            let mut spans = Vec::new();

            for col in 0..area.width.saturating_sub(4) {
                let x = col as f32 / width;
                let wave_index = (x * self.smoothed_points.len() as f32) as usize;
                let wave_index = wave_index.min(self.smoothed_points.len().saturating_sub(1));

                let wave_height = (self.smoothed_points[wave_index] + 1.0) / 2.0; // Normalize to 0-1

                let char = if (y - 0.5).abs() <= wave_height * 0.3 {
                    if (y - 0.5).abs() <= wave_height * 0.1 {
                        "█"
                    } else if (y - 0.5).abs() <= wave_height * 0.2 {
                        "▓"
                    } else {
                        "░"
                    }
                } else {
                    " "
                };

                let intensity = wave_height;
                let color = theme.get_color((wave_index / 10) % theme.palette().len(), intensity);

                spans.push(Span::styled(
                    char,
                    ratatui::style::Style::default().fg(color),
                ));
            }

            lines.push(Line::from(spans));
        }

        lines
    }
}

/// Particle system for dynamic effects
pub struct ParticleSystem {
    pub particles: Vec<Particle>,
    pub max_particles: usize,
    pub emission_rate: f32,
    pub gravity: f32,
}

#[derive(Clone)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub life: f32,
    pub max_life: f32,
    pub color: Color,
    pub size: f32,
}

impl ParticleSystem {
    pub fn new(max_particles: usize) -> Self {
        Self {
            particles: Vec::new(),
            max_particles,
            emission_rate: 10.0,
            gravity: -0.5,
        }
    }

    pub fn update(&mut self, magnitudes: &[f32], delta_time: f32, theme: Theme) {
        // Update existing particles
        self.particles.retain_mut(|particle| {
            particle.x += particle.velocity_x * delta_time;
            particle.y += particle.velocity_y * delta_time;
            particle.velocity_y += self.gravity * delta_time;
            particle.life -= delta_time;

            // Fade color as particle dies
            let life_ratio = particle.life / particle.max_life;
            particle.color = theme.get_color(0, life_ratio);

            particle.life > 0.0
        });

        // Emit new particles based on audio intensity
        let total_intensity: f32 = magnitudes.iter().sum();
        let should_emit = total_intensity > 0.1 && self.particles.len() < self.max_particles;

        if should_emit {
            for (i, &magnitude) in magnitudes.iter().enumerate().take(5) {
                if magnitude > 0.3 {
                    let particle = Particle {
                        x: (i as f32 / magnitudes.len() as f32),
                        y: 0.0,
                        velocity_x: (fastrand::f32() - 0.5) * 2.0,
                        velocity_y: fastrand::f32() * 3.0 + magnitude * 2.0,
                        life: 1.0 + magnitude,
                        max_life: 1.0 + magnitude,
                        color: theme.get_color(i % theme.palette().len(), magnitude),
                        size: magnitude,
                    };
                    self.particles.push(particle);
                }
            }
        }
    }

    /// Render particles as text characters
    pub fn render_text(&self, area: Rect) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from(""); area.height.saturating_sub(4) as usize];
        let height = area.height.saturating_sub(4) as f32;
        let width = area.width.saturating_sub(4) as f32;

        for particle in &self.particles {
            let screen_x = (particle.x * width) as usize;
            let screen_y = ((1.0 - particle.y) * height) as usize;

            if screen_y < lines.len() && screen_x < width as usize {
                let char = match particle.size {
                    s if s > 0.7 => "●",
                    s if s > 0.4 => "•",
                    _ => "·",
                };

                // Insert particle character at position
                let line = &mut lines[screen_y];
                let mut spans = vec![Span::raw(" ".repeat(screen_x))];
                spans.push(Span::styled(
                    char,
                    ratatui::style::Style::default().fg(particle.color),
                ));
                if screen_x + 1 < width as usize {
                    spans.push(Span::raw(" ".repeat(width as usize - screen_x - 1)));
                }
                *line = Line::from(spans);
            }
        }

        lines
    }
}

// Add fastrand for simple random numbers (will need to add to Cargo.toml)
// For now, we'll use a simple pseudo-random implementation
mod fastrand {
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEED: AtomicU64 = AtomicU64::new(1);

    pub fn f32() -> f32 {
        let mut seed = SEED.load(Ordering::Relaxed);
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        SEED.store(seed, Ordering::Relaxed);
        seed as f32 / u64::MAX as f32
    }
}
