#![allow(dead_code)]
// Theme system for consistent visual aesthetics
// Each theme defines colors, styles, and visual effects

use super::colors::ColorPalettes;
use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Theme {
    CyberNeon,
    OceanDeep,
    FireStorm,
    ForestGlow,
    Monochrome,
    RetroWave,
}

impl Theme {
    /// Get the primary color palette for this theme
    pub fn palette(self) -> &'static [Color; 6] {
        match self {
            Theme::CyberNeon => &ColorPalettes::CYBER_NEON,
            Theme::OceanDeep => &ColorPalettes::OCEAN_DEPTHS,
            Theme::FireStorm => &ColorPalettes::FIRE_STORM,
            Theme::ForestGlow => &ColorPalettes::FOREST_GLOW,
            Theme::Monochrome => &MONOCHROME_PALETTE,
            Theme::RetroWave => &RETRO_WAVE_PALETTE,
        }
    }

    /// Get the background color for this theme
    pub fn background_color(self) -> Color {
        match self {
            Theme::CyberNeon => Color::Rgb(10, 10, 20),
            Theme::OceanDeep => Color::Rgb(5, 15, 25),
            Theme::FireStorm => Color::Rgb(20, 5, 5),
            Theme::ForestGlow => Color::Rgb(5, 15, 5),
            Theme::Monochrome => Color::Rgb(15, 15, 15),
            Theme::RetroWave => Color::Rgb(20, 10, 30),
        }
    }

    /// Get the accent color for highlights
    pub fn accent_color(self) -> Color {
        match self {
            Theme::CyberNeon => Color::Rgb(0, 255, 255),
            Theme::OceanDeep => Color::Rgb(100, 200, 255),
            Theme::FireStorm => Color::Rgb(255, 100, 0),
            Theme::ForestGlow => Color::Rgb(100, 255, 50),
            Theme::Monochrome => Color::Rgb(200, 200, 200),
            Theme::RetroWave => Color::Rgb(255, 100, 200),
        }
    }

    /// Get the text color for this theme
    pub fn text_color(self) -> Color {
        match self {
            Theme::CyberNeon => Color::Rgb(200, 200, 255),
            Theme::OceanDeep => Color::Rgb(200, 220, 255),
            Theme::FireStorm => Color::Rgb(255, 220, 200),
            Theme::ForestGlow => Color::Rgb(220, 255, 220),
            Theme::Monochrome => Color::Rgb(180, 180, 180),
            Theme::RetroWave => Color::Rgb(255, 200, 255),
        }
    }

    /// Get the border color for this theme
    pub fn border_color(self) -> Color {
        match self {
            Theme::CyberNeon => Color::Rgb(0, 150, 150),
            Theme::OceanDeep => Color::Rgb(50, 100, 150),
            Theme::FireStorm => Color::Rgb(150, 50, 0),
            Theme::ForestGlow => Color::Rgb(50, 150, 50),
            Theme::Monochrome => Color::Rgb(100, 100, 100),
            Theme::RetroWave => Color::Rgb(150, 50, 150),
        }
    }

    /// Get the name of the theme
    pub fn name(self) -> &'static str {
        match self {
            Theme::CyberNeon => "Cyber Neon",
            Theme::OceanDeep => "Ocean Deep",
            Theme::FireStorm => "Fire Storm",
            Theme::ForestGlow => "Forest Glow",
            Theme::Monochrome => "Monochrome",
            Theme::RetroWave => "Retro Wave",
        }
    }

    /// Cycle to the next theme
    pub fn next(self) -> Self {
        match self {
            Theme::CyberNeon => Theme::OceanDeep,
            Theme::OceanDeep => Theme::FireStorm,
            Theme::FireStorm => Theme::ForestGlow,
            Theme::ForestGlow => Theme::Monochrome,
            Theme::Monochrome => Theme::RetroWave,
            Theme::RetroWave => Theme::CyberNeon,
        }
    }

    /// Get a color from the theme palette by index
    pub fn get_color(self, index: usize, intensity: f32) -> Color {
        let palette = self.palette();
        let base_color = palette[index % palette.len()];

        // Adjust intensity
        match base_color {
            Color::Rgb(r, g, b) => {
                let r = ((r as f32 * intensity).clamp(0.0, 255.0)) as u8;
                let g = ((g as f32 * intensity).clamp(0.0, 255.0)) as u8;
                let b = ((b as f32 * intensity).clamp(0.0, 255.0)) as u8;
                Color::Rgb(r, g, b)
            }
            _ => base_color,
        }
    }

    /// Check if theme should use glow effects
    pub fn use_glow(self) -> bool {
        match self {
            Theme::CyberNeon | Theme::RetroWave => true,
            _ => false,
        }
    }

    /// Get the glow intensity for effects
    pub fn glow_intensity(self) -> f32 {
        match self {
            Theme::CyberNeon => 0.8,
            Theme::RetroWave => 0.6,
            Theme::FireStorm => 0.4,
            _ => 0.2,
        }
    }
}

// Additional theme palettes
const MONOCHROME_PALETTE: [Color; 6] = [
    Color::Rgb(80, 80, 80),
    Color::Rgb(120, 120, 120),
    Color::Rgb(160, 160, 160),
    Color::Rgb(200, 200, 200),
    Color::Rgb(240, 240, 240),
    Color::Rgb(255, 255, 255),
];

const RETRO_WAVE_PALETTE: [Color; 6] = [
    Color::Rgb(255, 20, 147),  // Deep pink
    Color::Rgb(255, 100, 200), // Pink
    Color::Rgb(200, 50, 255),  // Purple
    Color::Rgb(100, 100, 255), // Blue
    Color::Rgb(0, 200, 255),   // Cyan
    Color::Rgb(255, 200, 0),   // Gold
];
