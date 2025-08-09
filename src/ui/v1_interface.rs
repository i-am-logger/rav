// Speedy v1.0 Professional UI Interface
// Inspired by Spotify's sleek design, CAVA's powerful visualizations, and Winamp's iconic style

use crate::{
    config::Config,
    quality::{
        AnimationAdjustment, AudioOptimization, ColorEnhancement, CyberneticQualityController,
        FrameData, ParameterChange, QualityControlActions,
    },
    signal::SignalProcessor,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::{collections::VecDeque, time::Instant};
use tracing::{debug, info};

/// Professional v1.0 UI State
pub struct SpeedyV1Interface {
    pub(crate) config: Config,
    pub(crate) signal_processor: SignalProcessor,

    // Visual State
    pub current_mode: VisualizationMode,
    pub(crate) normalized_magnitudes: Vec<f32>,
    pub(crate) magnitude_history: VecDeque<Vec<f32>>,

    // UI State
    pub(crate) show_help: bool,
    pub(crate) _show_settings: bool,
    pub(crate) fps: f32,
    pub(crate) peak_hold: Vec<f32>,
    pub(crate) volume_level: f32,

    // Animation State
    pub(crate) animation_time: f32,
    pub(crate) last_update: Instant,

    // Theme
    pub(crate) current_theme: ColorTheme,

    // Quality Control System
    quality_controller: CyberneticQualityController,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum VisualizationMode {
    Bars,      // Main frequency bars (like CAVA)
    Wave,      // Waveform visualization
    Spectrum,  // Spectrum analyzer
    Circle,    // Circular visualization (new)
    Particles, // Particle effect (new)
}

#[derive(Clone, Copy)]
pub struct ColorTheme {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub background: Color,
    pub text: Color,
    #[allow(dead_code)]
    pub glow: Color,
}

impl ColorTheme {
    pub const NEON_CYAN: Self = Self {
        primary: Color::Rgb(0, 255, 255),
        secondary: Color::Rgb(0, 150, 200),
        accent: Color::Rgb(255, 0, 255),
        background: Color::Rgb(5, 5, 15),
        text: Color::Rgb(200, 200, 255),
        glow: Color::Rgb(100, 255, 255),
    };

    pub const NEON_GREEN: Self = Self {
        primary: Color::Rgb(0, 255, 128),
        secondary: Color::Rgb(0, 200, 100),
        accent: Color::Rgb(255, 255, 0),
        background: Color::Rgb(5, 15, 5),
        text: Color::Rgb(200, 255, 200),
        glow: Color::Rgb(100, 255, 150),
    };

    pub const RETRO_AMBER: Self = Self {
        primary: Color::Rgb(255, 191, 0),
        secondary: Color::Rgb(255, 140, 0),
        accent: Color::Rgb(255, 69, 0),
        background: Color::Rgb(15, 10, 5),
        text: Color::Rgb(255, 220, 180),
        glow: Color::Rgb(255, 200, 100),
    };
}

impl SpeedyV1Interface {
    pub fn new(
        config: Config,
        signal_processor: SignalProcessor,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        info!("🔄 Initializing Professional UI with Cybernetic Quality Control");
        Ok(Self {
            config,
            signal_processor,
            current_mode: VisualizationMode::Bars,
            normalized_magnitudes: vec![0.0; 64],
            magnitude_history: VecDeque::with_capacity(100),
            show_help: false,
            _show_settings: false,
            fps: 60.0,
            peak_hold: vec![0.0; 64],
            volume_level: 0.0,
            animation_time: 0.0,
            last_update: Instant::now(),
            current_theme: ColorTheme::NEON_CYAN,
            quality_controller: CyberneticQualityController::new(),
        })
    }

    pub fn update_magnitudes(&mut self, magnitudes: Vec<f32>) {
        // Enhanced amplitude processing with superior audio responsiveness
        let mut enhanced_magnitudes = magnitudes;
        let total_amplitude: f32 = enhanced_magnitudes.iter().sum();
        let avg_amplitude = total_amplitude / enhanced_magnitudes.len() as f32;
        let rms_amplitude = (enhanced_magnitudes.iter().map(|x| x * x).sum::<f32>()
            / enhanced_magnitudes.len() as f32)
            .sqrt();

        // Advanced magnitude enhancement for better audio tracking
        let magnitudes_len = enhanced_magnitudes.len();
        for (i, mag) in enhanced_magnitudes.iter_mut().enumerate() {
            // Apply frequency-specific sensitivity boosting
            let freq_ratio = i as f32 / magnitudes_len as f32;
            let freq_boost = match freq_ratio {
                r if r < 0.1 => 1.2, // Boost sub-bass
                r if r < 0.3 => 1.1, // Slight boost for bass
                r if r < 0.7 => 1.0, // Neutral for mids
                r if r < 0.9 => 1.1, // Boost presence
                _ => 1.3,            // Boost highs
            };

            // Dynamic range compression for better visibility
            *mag = (*mag * freq_boost).powf(0.8); // Gentle compression
        }

        // For silent/very low amplitude scenarios, add sophisticated baseline visualization
        if avg_amplitude < 0.002 || rms_amplitude < 0.001 {
            let time = self.animation_time;
            let magnitudes_len = enhanced_magnitudes.len();
            for (i, mag) in enhanced_magnitudes.iter_mut().enumerate() {
                let freq_factor = i as f32 / magnitudes_len as f32;

                // Multi-layered breathing pattern for natural feel
                let primary_breath = (time * 0.5 + freq_factor * std::f32::consts::PI)
                    .sin()
                    .abs();
                let secondary_breath = (time * 1.2 + freq_factor * 2.5).sin().abs();
                let tertiary_breath = (time * 0.3 + freq_factor * 4.0).cos().abs();

                let combined_baseline = 0.01
                    + 0.006 * primary_breath
                    + 0.003 * secondary_breath
                    + 0.002 * tertiary_breath;

                *mag = (*mag + combined_baseline).min(0.04); // Cap to prevent overwhelming real audio
            }
        }

        self.normalized_magnitudes = self
            .signal_processor
            .normalize_magnitudes(&enhanced_magnitudes, self.config.display.sensitivity);

        // Enhanced peak hold with better decay and minimum values
        for (i, &mag) in self.normalized_magnitudes.iter().enumerate() {
            if i < self.peak_hold.len() {
                if mag > self.peak_hold[i] {
                    self.peak_hold[i] = mag;
                } else {
                    // Improved decay with non-linear falloff
                    let decay_rate = if self.peak_hold[i] > 0.8 { 0.02 } else { 0.008 };
                    self.peak_hold[i] = (self.peak_hold[i] - decay_rate).max(0.0);
                }

                // Ensure minimum visual activity for silent scenarios
                if avg_amplitude < 0.001 && self.peak_hold[i] < 0.01 {
                    self.peak_hold[i] = 0.01;
                }
            }
        }

        // Update history for wave mode with smoothing
        let smoothed_mags = if let Some(last_mags) = self.magnitude_history.back() {
            self.normalized_magnitudes
                .iter()
                .zip(last_mags.iter())
                .map(|(&new, &old)| old * 0.3 + new * 0.7) // Smooth transitions
                .collect()
        } else {
            self.normalized_magnitudes.clone()
        };

        self.magnitude_history.push_back(smoothed_mags);
        if self.magnitude_history.len() > 100 {
            self.magnitude_history.pop_front();
        }

        // Enhanced volume level calculation with better responsiveness
        let new_volume = self.normalized_magnitudes.iter().sum::<f32>()
            / self.normalized_magnitudes.len() as f32;
        self.volume_level = if self.volume_level == 0.0 {
            new_volume
        } else {
            self.volume_level * 0.7 + new_volume * 0.3 // Smooth volume changes
        };
    }

    pub fn cycle_mode(&mut self) {
        self.current_mode = match self.current_mode {
            VisualizationMode::Bars => VisualizationMode::Wave,
            VisualizationMode::Wave => VisualizationMode::Spectrum,
            VisualizationMode::Spectrum => VisualizationMode::Circle,
            VisualizationMode::Circle => VisualizationMode::Particles,
            VisualizationMode::Particles => VisualizationMode::Bars,
        };
    }

    pub fn cycle_theme(&mut self) {
        self.current_theme = match self.current_theme.primary {
            Color::Rgb(0, 255, 255) => ColorTheme::NEON_GREEN,
            Color::Rgb(0, 255, 128) => ColorTheme::RETRO_AMBER,
            _ => ColorTheme::NEON_CYAN,
        };
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn adjust_sensitivity(&mut self, delta: f32) {
        self.config.display.sensitivity = (self.config.display.sensitivity + delta).clamp(0.1, 5.0);
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.animation_time += dt;

        // 🔄 CYBERNETIC QUALITY CONTROL LOOP
        // Extract colors from current rendering for quality analysis
        let colors_used = self.extract_current_colors();
        let frame_data = FrameData {
            colors_used,
            animation_delta: dt,
            audio_magnitudes: self.normalized_magnitudes.clone(),
            render_time: now.duration_since(self.last_update),
            frame_timestamp: now,
        };

        // Run the cybernetic control loop for continuous quality improvement
        let quality_actions = self.quality_controller.control_loop(&frame_data);

        // Apply quality improvements automatically
        self.apply_quality_control_actions(&quality_actions);

        self.last_update = now;

        if self.show_help {
            self.draw_help_overlay(frame);
            return;
        }

        // Main layout - visualization takes most of the screen
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Top bar
                Constraint::Min(0),    // Main visualization
                Constraint::Length(2), // Bottom info bar
            ])
            .split(frame.area());

        self.draw_top_bar(frame, chunks[0]);
        self.draw_main_visualization(frame, chunks[1]);
        self.draw_bottom_bar(frame, chunks[2]);
    }

    fn draw_top_bar(&self, frame: &mut Frame, area: Rect) {
        // Minimal top bar with mode indicator and theme
        let mode_text = match self.current_mode {
            VisualizationMode::Bars => "▄▄▄ BARS",
            VisualizationMode::Wave => "∿∿∿ WAVE",
            VisualizationMode::Spectrum => "📈 SPECTRUM",
            VisualizationMode::Circle => "◯ CIRCLE",
            VisualizationMode::Particles => "✦ PARTICLES",
        };

        let title = format!(" ⚡ RAV v{}  {}  🎵", env!("CARGO_PKG_VERSION"), mode_text);

        let paragraph = Paragraph::new(title)
            .style(
                Style::default()
                    .fg(self.current_theme.primary)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Left);

        frame.render_widget(paragraph, area);
    }

    fn draw_main_visualization(&self, frame: &mut Frame, area: Rect) {
        match self.current_mode {
            VisualizationMode::Bars => self.draw_professional_bars(frame, area),
            VisualizationMode::Wave => self.draw_immersive_wave(frame, area),
            VisualizationMode::Spectrum => self.draw_enhanced_spectrum(frame, area),
            VisualizationMode::Circle => self.draw_circular_visualization(frame, area),
            VisualizationMode::Particles => self.draw_particle_visualization(frame, area),
        }
    }

    fn draw_professional_bars(&self, frame: &mut Frame, area: Rect) {
        // Professional bar visualization inspired by CAVA
        let width = area.width as usize;
        let height = area.height as usize;
        let bar_count = (width / 2).min(self.normalized_magnitudes.len());

        for row in 0..height {
            let y_intensity = 1.0 - (row as f32 / height as f32);
            let mut line_spans = Vec::new();

            for i in 0..bar_count {
                let mag_index = (i * self.normalized_magnitudes.len()) / bar_count;
                let magnitude = self.normalized_magnitudes.get(mag_index).unwrap_or(&0.0);
                let peak = self.peak_hold.get(mag_index).unwrap_or(&0.0);

                // Determine bar characters and colors
                let (char1, char2, color) = if y_intensity <= *magnitude {
                    // Main bar body with frequency-based coloring
                    let freq_ratio = i as f32 / bar_count as f32;
                    let color = self.get_frequency_color(freq_ratio, *magnitude);
                    let intensity = (*magnitude - y_intensity) / *magnitude;

                    if intensity > 0.8 {
                        ("█", "█", color)
                    } else if intensity > 0.6 {
                        ("▓", "▓", color)
                    } else if intensity > 0.4 {
                        ("▒", "▒", color)
                    } else {
                        ("░", "░", color)
                    }
                } else if y_intensity <= *peak {
                    // Peak hold indicators
                    ("▄", "▄", self.current_theme.accent)
                } else {
                    (" ", " ", Color::Reset)
                };

                line_spans.push(Span::styled(char1, Style::default().fg(color)));
                line_spans.push(Span::styled(char2, Style::default().fg(color)));
            }

            let line = Line::from(line_spans);
            let y_pos = area.y + row as u16;
            if y_pos < area.y + area.height {
                let para = Paragraph::new(line);
                frame.render_widget(
                    para,
                    Rect {
                        x: area.x,
                        y: y_pos,
                        width: area.width,
                        height: 1,
                    },
                );
            }
        }
    }

    fn draw_immersive_wave(&self, frame: &mut Frame, area: Rect) {
        // Immersive waveform visualization
        let width = area.width as usize;
        let height = area.height as usize;
        let center_y = height / 2;

        // Create wave data from magnitude history
        for row in 0..height {
            let mut line_spans = Vec::new();

            for x in 0..width {
                let history_index = (x * self.magnitude_history.len()) / width;
                let wave_amplitude =
                    if let Some(magnitudes) = self.magnitude_history.get(history_index) {
                        magnitudes.iter().sum::<f32>() / magnitudes.len() as f32
                    } else {
                        0.0
                    };

                // Add animation
                let animated_amplitude =
                    wave_amplitude + 0.1 * (self.animation_time * 2.0 + x as f32 * 0.1).sin();
                let wave_height = (animated_amplitude * center_y as f32) as usize;

                let distance_from_center = (row as i32 - center_y as i32).unsigned_abs() as usize;
                let (char, color) = if distance_from_center <= wave_height {
                    let intensity = 1.0 - (distance_from_center as f32 / wave_height as f32);
                    let color = self.get_wave_color(x as f32 / width as f32, intensity);

                    if intensity > 0.8 {
                        ("█", color)
                    } else if intensity > 0.6 {
                        ("▓", color)
                    } else if intensity > 0.4 {
                        ("▒", color)
                    } else {
                        ("░", color)
                    }
                } else {
                    (" ", Color::Reset)
                };

                line_spans.push(Span::styled(char, Style::default().fg(color)));
            }

            let line = Line::from(line_spans);
            let para = Paragraph::new(line);
            frame.render_widget(
                para,
                Rect {
                    x: area.x,
                    y: area.y + row as u16,
                    width: area.width,
                    height: 1,
                },
            );
        }
    }

    fn draw_enhanced_spectrum(&self, frame: &mut Frame, area: Rect) {
        // Professional spectrum analyzer with advanced gradient rendering
        let width = area.width as usize;
        let height = area.height as usize;
        let _spectrum_bands = width.min(self.normalized_magnitudes.len());

        // Create 2D spectrum display with frequency-amplitude mapping
        for row in 0..height {
            let y_intensity = 1.0 - (row as f32 / height as f32);
            let mut line_spans = Vec::new();

            for col in 0..width {
                let freq_index = (col * self.normalized_magnitudes.len()) / width;
                let magnitude = self.normalized_magnitudes.get(freq_index).unwrap_or(&0.0);
                let freq_ratio = col as f32 / width as f32;

                // Enhanced amplitude-based rendering with smooth gradients
                let amplitude_factor = *magnitude;
                let visual_height = amplitude_factor;

                let (char, color) = if y_intensity <= visual_height {
                    // Inside the spectrum - use gradient based on intensity and frequency
                    let intensity_in_band = (visual_height - y_intensity) / visual_height;
                    let animation_phase = self.animation_time * 2.0 + freq_ratio * 8.0;
                    let animated_intensity = intensity_in_band + 0.1 * animation_phase.sin().abs();

                    let color = self.get_spectrum_gradient_color(
                        freq_ratio,
                        animated_intensity.clamp(0.0, 1.0),
                    );

                    // Multi-level intensity rendering for smooth gradients
                    let char = match animated_intensity {
                        i if i > 0.9 => "█",  // Solid
                        i if i > 0.75 => "▓", // Heavy shade
                        i if i > 0.5 => "▒",  // Medium shade
                        i if i > 0.25 => "░", // Light shade
                        i if i > 0.1 => "·",  // Dots
                        _ => ".",             // Minimal
                    };

                    (char, color)
                } else if y_intensity <= visual_height + 0.05 {
                    // Glow effect around spectrum peaks
                    let glow_intensity = (visual_height + 0.05 - y_intensity) / 0.05;
                    let glow_color = self.get_spectrum_glow_color(freq_ratio, glow_intensity);
                    ("·", glow_color)
                } else {
                    // Background with subtle grid
                    if row % 4 == 0 && col % 8 == 0 {
                        ("·", Color::Rgb(20, 20, 40)) // Subtle grid
                    } else {
                        (" ", Color::Reset)
                    }
                };

                line_spans.push(Span::styled(char, Style::default().fg(color)));
            }

            let line = Line::from(line_spans);
            let y_pos = area.y + row as u16;
            if y_pos < area.y + area.height {
                let para = Paragraph::new(line);
                frame.render_widget(
                    para,
                    Rect {
                        x: area.x,
                        y: y_pos,
                        width: area.width,
                        height: 1,
                    },
                );
            }
        }

        // Add frequency scale at bottom
        if area.height > 2 {
            let scale_area = Rect {
                x: area.x,
                y: area.y + area.height - 1,
                width: area.width,
                height: 1,
            };

            let mut scale_spans = Vec::new();
            for i in 0..area.width {
                let freq_ratio = i as f32 / area.width as f32;
                let freq_hz = (freq_ratio * 22050.0) as u32; // Assuming 44.1kHz sample rate

                if i % 10 == 0 && freq_hz >= 100 {
                    let freq_text = if freq_hz >= 1000 {
                        format!("{:2}k", freq_hz / 1000)
                    } else {
                        format!("{freq_hz:3}")
                    };

                    for (j, ch) in freq_text.chars().enumerate() {
                        if (i as usize) + j < area.width as usize {
                            scale_spans.push(Span::styled(
                                ch.to_string(),
                                Style::default().fg(self.current_theme.secondary),
                            ));
                        }
                    }
                    // Pad to next position
                    for _ in freq_text.len()..(10.min(area.width as usize - (i as usize))) {
                        scale_spans.push(Span::styled(" ", Style::default()));
                    }
                } else if scale_spans.len() < area.width as usize {
                    scale_spans.push(Span::styled(" ", Style::default()));
                }
            }

            let scale_line = Line::from(scale_spans);
            let scale_para = Paragraph::new(scale_line);
            frame.render_widget(scale_para, scale_area);
        }
    }

    fn draw_circular_visualization(&self, frame: &mut Frame, area: Rect) {
        // Circular visualization (new mode)
        let center_x = area.width / 2;
        let center_y = area.height / 2;
        let max_radius = (center_x.min(center_y) as f32 * 0.8) as usize;

        for row in 0..area.height {
            let mut line_spans = Vec::new();

            for col in 0..area.width {
                let dx = col as i32 - center_x as i32;
                let dy = row as i32 - center_y as i32;
                let distance = ((dx * dx + dy * dy) as f32).sqrt();
                let angle = (dy as f32).atan2(dx as f32) + std::f32::consts::PI;

                let band_index = (angle / (2.0 * std::f32::consts::PI)
                    * self.normalized_magnitudes.len() as f32)
                    as usize;
                let magnitude = self.normalized_magnitudes.get(band_index).unwrap_or(&0.0);
                let target_radius = magnitude * max_radius as f32;

                let (char, color) = if distance <= target_radius {
                    let intensity = 1.0 - (distance / target_radius);
                    let color = self.get_frequency_color(
                        band_index as f32 / self.normalized_magnitudes.len() as f32,
                        intensity,
                    );

                    if intensity > 0.8 {
                        ("●", color)
                    } else if intensity > 0.6 {
                        ("◐", color)
                    } else if intensity > 0.4 {
                        ("◯", color)
                    } else {
                        ("·", color)
                    }
                } else {
                    (" ", Color::Reset)
                };

                line_spans.push(Span::styled(char, Style::default().fg(color)));
            }

            let line = Line::from(line_spans);
            let para = Paragraph::new(line);
            frame.render_widget(
                para,
                Rect {
                    x: area.x,
                    y: area.y + row,
                    width: area.width,
                    height: 1,
                },
            );
        }
    }

    fn draw_particle_visualization(&self, frame: &mut Frame, area: Rect) {
        // Advanced particle effect visualization with anti-flicker technology
        use std::collections::HashMap;
        let mut particle_grid = HashMap::new();

        let width = area.width as usize;
        let height = area.height as usize;
        let center_y = height / 2;

        // Generate particles with stable positioning and smooth transitions
        for (i, &magnitude) in self.normalized_magnitudes.iter().enumerate() {
            if magnitude < 0.01 {
                continue;
            } // Skip very quiet frequencies

            let freq_ratio = i as f32 / self.normalized_magnitudes.len() as f32;
            let base_x = (i * width) / self.normalized_magnitudes.len();

            // Multi-layered particle generation for richness
            let layers = [(1.0, 12.0), (0.7, 8.0), (0.4, 4.0)]; // (intensity_multiplier, max_particles)

            for (intensity_mult, max_particles) in layers.iter() {
                let particle_count = (magnitude * intensity_mult * max_particles) as usize;

                for j in 0..particle_count {
                    // Stable particle positioning with reduced flicker
                    let particle_seed = i as f32 * 7.0 + j as f32 * 3.0; // Stable seed per particle
                    let time_factor = self.animation_time * 0.8; // Slower animation for stability

                    // Smooth orbital motion
                    let orbit_radius = magnitude * 8.0 * intensity_mult;
                    let orbit_speed = 1.0 + freq_ratio * 2.0;
                    let angle = time_factor * orbit_speed + particle_seed;

                    let x_offset = angle.cos() * orbit_radius;
                    let y_offset = (angle * 1.3).sin() * orbit_radius
                        + (time_factor * 0.5 + particle_seed).sin() * magnitude * 5.0;

                    let x = (base_x as f32 + x_offset).max(0.0).min(width as f32 - 1.0) as usize;
                    let y = (center_y as f32 + y_offset)
                        .max(0.0)
                        .min(height as f32 - 1.0) as usize;

                    // Calculate particle properties
                    let age_factor = (time_factor + particle_seed) % std::f32::consts::TAU; // Full cycle
                    let life_intensity =
                        (age_factor.sin() * 0.5 + 0.5) * magnitude * intensity_mult;

                    let color = self.get_particle_color(freq_ratio, life_intensity, age_factor);
                    let particle_char =
                        self.get_particle_character(life_intensity, *intensity_mult);

                    // Anti-overlap system - only place particle if position is empty or this one is brighter
                    if let Some((existing_intensity, _, _)) = particle_grid.get(&(x, y)) {
                        if life_intensity > *existing_intensity {
                            particle_grid.insert((x, y), (life_intensity, color, particle_char));
                        }
                    } else {
                        particle_grid.insert((x, y), (life_intensity, color, particle_char));
                    }
                }
            }
        }

        // Render particle field with smooth gradients
        for row in 0..height {
            let mut line_spans = Vec::new();

            for col in 0..width {
                let (char, color) =
                    if let Some((_, color, particle_char)) = particle_grid.get(&(col, row)) {
                        (*particle_char, *color)
                    } else {
                        // Add subtle background stars for depth
                        if (row + col * 3) % 47 == 0 && self.volume_level > 0.1 {
                            let bg_intensity = self.volume_level * 0.2;
                            let bg_color = match self.current_theme.primary {
                                Color::Rgb(0, 255, 255) => Color::Rgb(
                                    (50.0 * bg_intensity) as u8,
                                    (100.0 * bg_intensity) as u8,
                                    (150.0 * bg_intensity) as u8,
                                ),
                                Color::Rgb(0, 255, 128) => Color::Rgb(
                                    (30.0 * bg_intensity) as u8,
                                    (100.0 * bg_intensity) as u8,
                                    (50.0 * bg_intensity) as u8,
                                ),
                                _ => Color::Rgb(
                                    (100.0 * bg_intensity) as u8,
                                    (80.0 * bg_intensity) as u8,
                                    (30.0 * bg_intensity) as u8,
                                ),
                            };
                            ("·", bg_color)
                        } else {
                            (" ", Color::Reset)
                        }
                    };

                line_spans.push(Span::styled(char, Style::default().fg(color)));
            }

            let line = Line::from(line_spans);
            let para = Paragraph::new(line);
            frame.render_widget(
                para,
                Rect {
                    x: area.x,
                    y: area.y + row as u16,
                    width: area.width,
                    height: 1,
                },
            );
        }
    }

    fn draw_bottom_bar(&self, frame: &mut Frame, area: Rect) {
        // Professional bottom bar with essential info
        let volume_bar = format!(
            "[{}{}]",
            "█".repeat((self.volume_level * 20.0) as usize),
            "░".repeat(20 - (self.volume_level * 20.0) as usize)
        );

        let info = format!(
            " VOL {} | SENS {:.1} | FPS {:.0} | Press H for help ",
            volume_bar, self.config.display.sensitivity, self.fps
        );

        let paragraph = Paragraph::new(info)
            .style(Style::default().fg(self.current_theme.text))
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    fn draw_help_overlay(&self, frame: &mut Frame) {
        // Help overlay with key bindings
        let help_text = vec![
            Line::from(Span::styled(
                format!("⚡ RAV v{} - HELP ⚡", env!("CARGO_PKG_VERSION")),
                Style::default()
                    .fg(self.current_theme.primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "🎵 VISUALIZATION MODES:",
                Style::default()
                    .fg(self.current_theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("  SPACE / TAB    - Cycle visualization modes"),
            Line::from("  1-5            - Select specific mode"),
            Line::from(""),
            Line::from(Span::styled(
                "🎛️ CONTROLS:",
                Style::default()
                    .fg(self.current_theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("  ↑ / ↓          - Adjust sensitivity"),
            Line::from("  T              - Cycle color themes"),
            Line::from("  R              - Reset settings"),
            Line::from(""),
            Line::from(Span::styled(
                "⚙️ SYSTEM:",
                Style::default()
                    .fg(self.current_theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from("  Q / ESC        - Quit"),
            Line::from("  H / ?          - Toggle this help"),
            Line::from(""),
            Line::from(Span::styled(
                "Press any key to return to visualization",
                Style::default()
                    .fg(self.current_theme.text)
                    .add_modifier(Modifier::ITALIC),
            )),
        ];

        let help_area = Rect {
            x: frame.area().width / 4,
            y: frame.area().height / 4,
            width: frame.area().width / 2,
            height: frame.area().height / 2,
        };

        let help_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.current_theme.primary))
            .style(Style::default().bg(self.current_theme.background));

        let help_paragraph = Paragraph::new(help_text)
            .block(help_block)
            .alignment(Alignment::Left);

        frame.render_widget(help_paragraph, help_area);
    }

    fn get_frequency_color(&self, freq_ratio: f32, intensity: f32) -> Color {
        // Enhanced frequency-based color mapping with much wider spectrum and better variety
        let enhanced_colors = match self.current_theme.primary {
            Color::Rgb(0, 255, 255) => [
                // Cyan theme - Extended RGB spectrum for maximum variety
                Color::Rgb(255, 20, 147),  // Deep pink (sub-bass 20-60Hz)
                Color::Rgb(255, 69, 0),    // Red-orange (bass 60-150Hz)
                Color::Rgb(255, 140, 0),   // Orange (low-mid 150-400Hz)
                Color::Rgb(255, 215, 0),   // Gold (mid 400-1kHz)
                Color::Rgb(255, 255, 0),   // Yellow (upper-mid 1-2.5kHz)
                Color::Rgb(154, 255, 0),   // Yellow-green (presence 2.5-5kHz)
                Color::Rgb(0, 255, 100),   // Green (brilliance 5-10kHz)
                Color::Rgb(0, 255, 200),   // Cyan-green (air 10-15kHz)
                Color::Rgb(0, 200, 255),   // Light blue (ultra 15-18kHz)
                Color::Rgb(100, 149, 237), // Cornflower blue (highest 18-20kHz)
                Color::Rgb(147, 112, 219), // Medium purple (super high 20kHz+)
                Color::Rgb(255, 105, 180), // Hot pink (extreme high)
            ],
            Color::Rgb(0, 255, 128) => [
                // Green theme - Natural spectrum mapping
                Color::Rgb(139, 69, 19),  // Brown (sub-bass)
                Color::Rgb(255, 69, 0),   // Red-orange (bass)
                Color::Rgb(255, 140, 0),  // Orange (low-mid)
                Color::Rgb(255, 215, 0),  // Gold (mid)
                Color::Rgb(255, 255, 0),  // Yellow (upper-mid)
                Color::Rgb(154, 255, 0),  // Yellow-green (presence)
                Color::Rgb(0, 255, 0),    // Pure green (brilliance)
                Color::Rgb(0, 255, 127),  // Spring green (air)
                Color::Rgb(0, 191, 255),  // Deep sky blue (ultra)
                Color::Rgb(30, 144, 255), // Dodger blue (highest)
                Color::Rgb(138, 43, 226), // Blue violet (super high)
                Color::Rgb(255, 20, 147), // Deep pink (extreme)
            ],
            _ => [
                // Amber theme - Warm spectrum with good contrast
                Color::Rgb(139, 0, 0),     // Dark red (sub-bass)
                Color::Rgb(220, 20, 60),   // Crimson (bass)
                Color::Rgb(255, 69, 0),    // Red-orange (low-mid)
                Color::Rgb(255, 140, 0),   // Orange (mid)
                Color::Rgb(255, 191, 0),   // Amber (upper-mid)
                Color::Rgb(255, 215, 0),   // Gold (presence)
                Color::Rgb(255, 255, 0),   // Yellow (brilliance)
                Color::Rgb(255, 255, 224), // Light yellow (air)
                Color::Rgb(255, 228, 181), // Moccasin (ultra)
                Color::Rgb(255, 218, 185), // Peach puff (highest)
                Color::Rgb(255, 192, 203), // Pink (super high)
                Color::Rgb(255, 255, 255), // White (extreme)
            ],
        };

        // Enhanced color interpolation for smoother transitions
        let scaled_pos = freq_ratio * (enhanced_colors.len() - 1) as f32;
        let base_idx = scaled_pos.floor() as usize;
        let next_idx = (base_idx + 1).min(enhanced_colors.len() - 1);
        let blend_factor = scaled_pos.fract();

        let base_color = enhanced_colors[base_idx];
        let next_color = enhanced_colors[next_idx];

        // Advanced color blending with intensity scaling
        match (base_color, next_color) {
            (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
                let intensity_boost = (intensity * 0.8 + 0.2).clamp(0.2, 1.0);
                let contrast_boost = 1.0 + (intensity - 0.5).abs() * 0.3; // Boost contrast at extremes

                let r = ((r1 as f32 * (1.0 - blend_factor) + r2 as f32 * blend_factor)
                    * intensity_boost
                    * contrast_boost) as u8;
                let g = ((g1 as f32 * (1.0 - blend_factor) + g2 as f32 * blend_factor)
                    * intensity_boost
                    * contrast_boost) as u8;
                let b = ((b1 as f32 * (1.0 - blend_factor) + b2 as f32 * blend_factor)
                    * intensity_boost
                    * contrast_boost) as u8;

                Color::Rgb(r, g, b)
            }
            _ => base_color,
        }
    }

    fn get_wave_color(&self, position: f32, intensity: f32) -> Color {
        // Position and intensity-based wave coloring
        let hue_shift = position * 360.0;
        let sat_intensity = intensity.clamp(0.5, 1.0);

        // Convert HSV-like values to RGB approximation
        let base = if hue_shift < 120.0 {
            (255, (hue_shift / 120.0 * 255.0) as u8, 0)
        } else if hue_shift < 240.0 {
            (255 - ((hue_shift - 120.0) / 120.0 * 255.0) as u8, 255, 0)
        } else {
            (0, 255, ((hue_shift - 240.0) / 120.0 * 255.0) as u8)
        };

        Color::Rgb(
            (base.0 as f32 * sat_intensity) as u8,
            (base.1 as f32 * sat_intensity) as u8,
            (base.2 as f32 * sat_intensity) as u8,
        )
    }

    fn get_spectrum_gradient_color(&self, freq_ratio: f32, intensity: f32) -> Color {
        // Professional spectrum gradient with smooth transitions
        let base_colors = match self.current_theme.primary {
            Color::Rgb(0, 255, 255) => [
                // Cyan theme - cool spectrum
                Color::Rgb(64, 0, 128),  // Deep purple (low)
                Color::Rgb(128, 0, 255), // Purple
                Color::Rgb(0, 128, 255), // Blue
                Color::Rgb(0, 255, 255), // Cyan (mid)
                Color::Rgb(0, 255, 128), // Green
                Color::Rgb(255, 255, 0), // Yellow
                Color::Rgb(255, 128, 0), // Orange (high)
            ],
            Color::Rgb(0, 255, 128) => [
                // Green theme - nature spectrum
                Color::Rgb(0, 64, 128),  // Deep blue (low)
                Color::Rgb(0, 128, 255), // Blue
                Color::Rgb(0, 255, 255), // Cyan
                Color::Rgb(0, 255, 128), // Green (mid)
                Color::Rgb(128, 255, 0), // Lime
                Color::Rgb(255, 255, 0), // Yellow
                Color::Rgb(255, 200, 0), // Gold (high)
            ],
            _ => [
                // Amber theme - warm spectrum
                Color::Rgb(128, 0, 0),     // Deep red (low)
                Color::Rgb(255, 0, 0),     // Red
                Color::Rgb(255, 69, 0),    // Red-orange
                Color::Rgb(255, 140, 0),   // Orange (mid)
                Color::Rgb(255, 191, 0),   // Amber
                Color::Rgb(255, 215, 0),   // Gold
                Color::Rgb(255, 255, 128), // Light yellow (high)
            ],
        };

        // Smooth interpolation between colors
        let scaled_pos = freq_ratio * (base_colors.len() - 1) as f32;
        let base_index = scaled_pos.floor() as usize;
        let next_index = (base_index + 1).min(base_colors.len() - 1);
        let blend_factor = scaled_pos - base_index as f32;

        let base_color = base_colors[base_index];
        let next_color = base_colors[next_index];

        // Interpolate between the two colors
        let (r1, g1, b1) = match base_color {
            Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
            _ => (0.0, 0.0, 0.0),
        };

        let (r2, g2, b2) = match next_color {
            Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
            _ => (0.0, 0.0, 0.0),
        };

        let r = r1 + (r2 - r1) * blend_factor;
        let g = g1 + (g2 - g1) * blend_factor;
        let b = b1 + (b2 - b1) * blend_factor;

        // Apply intensity scaling with enhanced brightness
        let enhanced_intensity = intensity.clamp(0.1, 1.0).powf(0.7); // Gamma correction for better visibility

        Color::Rgb(
            (r * enhanced_intensity) as u8,
            (g * enhanced_intensity) as u8,
            (b * enhanced_intensity) as u8,
        )
    }

    fn get_spectrum_glow_color(&self, _freq_ratio: f32, glow_intensity: f32) -> Color {
        // Glow effect around spectrum peaks
        let base_glow = match self.current_theme.primary {
            Color::Rgb(0, 255, 255) => Color::Rgb(100, 255, 255), // Cyan glow
            Color::Rgb(0, 255, 128) => Color::Rgb(100, 255, 150), // Green glow
            _ => Color::Rgb(255, 200, 100),                       // Amber glow
        };

        match base_glow {
            Color::Rgb(r, g, b) => {
                let scale = glow_intensity.clamp(0.0, 0.5); // Subtle glow
                Color::Rgb(
                    (r as f32 * scale) as u8,
                    (g as f32 * scale) as u8,
                    (b as f32 * scale) as u8,
                )
            }
            _ => base_glow,
        }
    }

    fn get_particle_color(&self, freq_ratio: f32, life_intensity: f32, age_factor: f32) -> Color {
        // Dynamic particle coloring with lifecycle effects
        let base_color = self.get_frequency_color(freq_ratio, life_intensity);

        // Add pulsing effect based on particle age
        let pulse = (age_factor * 3.0).sin() * 0.2 + 0.8; // Gentle pulsing

        match base_color {
            Color::Rgb(r, g, b) => {
                let pulse_scale = pulse.clamp(0.3, 1.0);
                Color::Rgb(
                    (r as f32 * pulse_scale) as u8,
                    (g as f32 * pulse_scale) as u8,
                    (b as f32 * pulse_scale) as u8,
                )
            }
            _ => base_color,
        }
    }

    fn get_particle_character(&self, life_intensity: f32, layer_multiplier: f32) -> &'static str {
        // Different particle characters based on intensity and layer
        let adjusted_intensity = life_intensity * layer_multiplier;

        match adjusted_intensity {
            i if i > 0.8 => "★", // Bright star
            i if i > 0.6 => "✦", // Sparkle
            i if i > 0.4 => "✧", // Medium sparkle
            i if i > 0.2 => "⋆", // Small star
            i if i > 0.1 => "·", // Dot
            _ => ".",            // Minimal dot
        }
    }

    // 🔄 CYBERNETIC QUALITY CONTROL HELPER FUNCTIONS

    fn extract_current_colors(&self) -> Vec<(u8, u8, u8)> {
        // Extract representative colors from current rendering for quality analysis
        let mut colors = Vec::new();

        // Sample colors from the current theme and visualization state
        match self.current_theme.primary {
            Color::Rgb(r, g, b) => colors.push((r, g, b)),
            _ => colors.push((255, 255, 255)),
        }

        match self.current_theme.secondary {
            Color::Rgb(r, g, b) => colors.push((r, g, b)),
            _ => colors.push((128, 128, 128)),
        }

        match self.current_theme.accent {
            Color::Rgb(r, g, b) => colors.push((r, g, b)),
            _ => colors.push((255, 0, 255)),
        }

        // Add frequency-based colors for variety analysis
        for (i, &magnitude) in self.normalized_magnitudes.iter().enumerate() {
            if magnitude > 0.1 {
                // Only include active frequencies
                let freq_ratio = i as f32 / self.normalized_magnitudes.len() as f32;
                let color = self.get_frequency_color(freq_ratio, magnitude);
                if let Color::Rgb(r, g, b) = color {
                    colors.push((r, g, b))
                }
            }
        }

        // Limit to prevent overload
        colors.truncate(32);
        colors
    }

    fn apply_quality_control_actions(&mut self, actions: &QualityControlActions) {
        // Apply color enhancements
        for enhancement in &actions.color_enhancements {
            match enhancement {
                ColorEnhancement::IncreaseSpectrum => {
                    debug!("🎨 Increasing color spectrum variety");
                    Self::enhance_color(&mut self.current_theme.primary, 1.2);
                    Self::enhance_color(&mut self.current_theme.secondary, 1.1);
                }
                ColorEnhancement::AddGradients => {
                    debug!("🎨 Adding color gradients");
                    // Could implement gradient color transitions
                }
                ColorEnhancement::EnhanceContrast => {
                    debug!("🎨 Enhancing color contrast");
                    Self::enhance_color(&mut self.current_theme.accent, 1.3);
                }
                ColorEnhancement::BoostSaturation => {
                    debug!("🎨 Boosting color saturation");
                    Self::enhance_color(&mut self.current_theme.primary, 1.15);
                    Self::enhance_color(&mut self.current_theme.accent, 1.15);
                }
            }
        }

        // Apply animation adjustments
        for adjustment in &actions.animation_adjustments {
            match adjustment {
                AnimationAdjustment::SmoothFrameTiming => {
                    debug!("🎬 Smoothing frame timing");
                    // Could adjust animation smoothing parameters
                }
                AnimationAdjustment::ReduceJitter => {
                    debug!("🎬 Reducing animation jitter");
                    // Could implement jitter reduction
                }
                AnimationAdjustment::ImproveInterpolation => {
                    debug!("🎬 Improving interpolation");
                    // Could enhance interpolation algorithms
                }
                AnimationAdjustment::OptimizeDecay => {
                    debug!("🎬 Optimizing decay rate");
                    // Could adjust peak hold decay rates
                }
            }
        }

        // Apply audio optimizations
        for optimization in &actions.audio_optimizations {
            match optimization {
                AudioOptimization::IncreaseSensitivity => {
                    debug!("🎵 Increasing audio sensitivity");
                    self.config.display.sensitivity =
                        (self.config.display.sensitivity * 1.1).clamp(0.1, 5.0);
                }
                AudioOptimization::ReduceLatency => {
                    debug!("🎵 Reducing audio latency");
                    // Could optimize buffer sizes
                }
                AudioOptimization::EnhanceResponseTime => {
                    debug!("🎵 Enhancing response time");
                    // Could adjust responsiveness parameters
                }
                AudioOptimization::ImproveNormalization => {
                    debug!("🎵 Improving normalization");
                    // Could enhance magnitude normalization
                }
            }
        }

        // Apply parameter changes
        for change in &actions.parameter_changes {
            match change {
                ParameterChange::AdjustSensitivity(delta) => {
                    debug!("⚙️ Adjusting sensitivity by {}", delta);
                    self.config.display.sensitivity =
                        (self.config.display.sensitivity + delta).clamp(0.1, 5.0);
                }
                ParameterChange::ModifyDecayRate(delta) => {
                    debug!("⚙️ Modifying decay rate by {}", delta);
                    // Could adjust peak hold decay rates
                }
                ParameterChange::UpdateColorPalette => {
                    debug!("⚙️ Updating color palette");
                    // Could cycle to next theme or enhance current one
                    Self::enhance_color(&mut self.current_theme.primary, 1.05);
                }
                ParameterChange::OptimizeBuffering => {
                    debug!("⚙️ Optimizing buffering");
                    // Could adjust buffer parameters
                }
            }
        }
    }

    #[allow(dead_code)]
    fn enhance_theme_color(&mut self, color: &mut Color, boost_factor: f32) {
        // Enhance color saturation and brightness within visual limits
        if let Color::Rgb(r, g, b) = *color {
            let boost = boost_factor.clamp(0.5, 2.0); // Reasonable enhancement range

            let new_r = ((r as f32 * boost).min(255.0)) as u8;
            let new_g = ((g as f32 * boost).min(255.0)) as u8;
            let new_b = ((b as f32 * boost).min(255.0)) as u8;

            *color = Color::Rgb(new_r, new_g, new_b);
        }
    }

    // Static helper method for color enhancement
    fn enhance_color(color: &mut Color, boost_factor: f32) {
        if let Color::Rgb(r, g, b) = *color {
            let boost = boost_factor.clamp(0.5, 2.0); // Reasonable enhancement range

            let new_r = ((r as f32 * boost).min(255.0)) as u8;
            let new_g = ((g as f32 * boost).min(255.0)) as u8;
            let new_b = ((b as f32 * boost).min(255.0)) as u8;

            *color = Color::Rgb(new_r, new_g, new_b);
        }
    }
}
