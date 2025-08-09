#![allow(dead_code)]
// Advanced color system for professional-grade visualizations
// Implements smooth gradients, HSV color space, and dynamic color generation

use ratatui::style::Color;
use std::f32::consts::PI;

/// Advanced color system for smooth gradients and dynamic colors
#[derive(Debug, Clone)]
pub struct ColorSystem {
    pub hue_offset: f32,
    pub saturation: f32,
    pub brightness: f32,
}

impl Default for ColorSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorSystem {
    pub fn new() -> Self {
        Self {
            hue_offset: 0.0,
            saturation: 0.9,
            brightness: 0.8,
        }
    }

    /// Generate a smooth gradient color based on position and intensity
    pub fn gradient_color(&self, position: f32, intensity: f32, time: f32) -> Color {
        let hue = (self.hue_offset + position * 360.0 + time * 30.0) % 360.0;
        let saturation = (self.saturation * intensity).clamp(0.3, 1.0);
        let value = (self.brightness * intensity).clamp(0.1, 1.0);

        hsv_to_rgb(hue, saturation, value)
    }

    /// Generate a neon-style color with glow effect
    pub fn neon_color(&self, base_hue: f32, intensity: f32, glow: f32) -> Color {
        let hue = (base_hue + self.hue_offset) % 360.0;
        let saturation = (0.9 + glow * 0.1).clamp(0.0, 1.0);
        let value = (intensity * (0.7 + glow * 0.3)).clamp(0.2, 1.0);

        hsv_to_rgb(hue, saturation, value)
    }

    /// Generate a rainbow spectrum color
    pub fn spectrum_color(&self, frequency_ratio: f32, intensity: f32) -> Color {
        // Map frequency to color spectrum (low = red, high = violet)
        let hue = (270.0 + frequency_ratio * 300.0) % 360.0;
        let saturation = 0.9;
        let value = (intensity * 0.8).clamp(0.1, 1.0);

        hsv_to_rgb(hue, saturation, value)
    }

    /// Generate a pulsing color based on time
    pub fn pulse_color(&self, base_color: Color, time: f32, frequency: f32) -> Color {
        let pulse = (time * frequency * 2.0 * PI).sin() * 0.5 + 0.5;
        self.blend_colors(base_color, Color::White, pulse * 0.3)
    }

    /// Blend two colors with a given ratio
    pub fn blend_colors(&self, color1: Color, color2: Color, ratio: f32) -> Color {
        let ratio = ratio.clamp(0.0, 1.0);

        match (color1, color2) {
            (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
                let r = (r1 as f32 * (1.0 - ratio) + r2 as f32 * ratio) as u8;
                let g = (g1 as f32 * (1.0 - ratio) + g2 as f32 * ratio) as u8;
                let b = (b1 as f32 * (1.0 - ratio) + b2 as f32 * ratio) as u8;
                Color::Rgb(r, g, b)
            }
            _ => color1, // Fallback for other color types
        }
    }

    /// Update color system for animations
    pub fn update(&mut self, delta_time: f32) {
        self.hue_offset += delta_time * 60.0; // Rotate hue over time
        if self.hue_offset >= 360.0 {
            self.hue_offset -= 360.0;
        }
    }
}

/// Convert HSV color space to RGB
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color {
    let h = h % 360.0;
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r_prime, g_prime, b_prime) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    let r = ((r_prime + m) * 255.0) as u8;
    let g = ((g_prime + m) * 255.0) as u8;
    let b = ((b_prime + m) * 255.0) as u8;

    Color::Rgb(r, g, b)
}

/// Pre-defined color palettes for quick access
pub struct ColorPalettes;

impl ColorPalettes {
    pub const CYBER_NEON: [Color; 6] = [
        Color::Rgb(0, 255, 255), // Cyan
        Color::Rgb(255, 0, 255), // Magenta
        Color::Rgb(0, 255, 0),   // Lime
        Color::Rgb(255, 255, 0), // Yellow
        Color::Rgb(255, 100, 0), // Orange
        Color::Rgb(200, 0, 255), // Purple
    ];

    pub const OCEAN_DEPTHS: [Color; 6] = [
        Color::Rgb(0, 50, 100),    // Deep blue
        Color::Rgb(0, 100, 150),   // Ocean blue
        Color::Rgb(0, 150, 200),   // Sky blue
        Color::Rgb(100, 200, 255), // Light blue
        Color::Rgb(150, 255, 255), // Cyan
        Color::Rgb(200, 255, 200), // Aqua
    ];

    pub const FIRE_STORM: [Color; 6] = [
        Color::Rgb(50, 0, 0),      // Dark red
        Color::Rgb(100, 20, 0),    // Red
        Color::Rgb(200, 50, 0),    // Orange red
        Color::Rgb(255, 100, 0),   // Orange
        Color::Rgb(255, 200, 0),   // Yellow orange
        Color::Rgb(255, 255, 100), // Yellow
    ];

    pub const FOREST_GLOW: [Color; 6] = [
        Color::Rgb(0, 50, 0),      // Dark green
        Color::Rgb(0, 100, 50),    // Forest green
        Color::Rgb(50, 150, 0),    // Lime green
        Color::Rgb(100, 200, 50),  // Light green
        Color::Rgb(150, 255, 100), // Bright green
        Color::Rgb(200, 255, 200), // Pale green
    ];
}
