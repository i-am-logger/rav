// Visual testing framework for Speedy audio visualizer
// Uses ratatui's TestBackend to capture and analyze visual output

use ratatui::{backend::TestBackend, style::Color, Terminal};
use std::{collections::HashMap, fmt::Write};

pub mod audio_generator;
pub mod audio_monitor;
pub mod bdd_framework;
pub mod visual_analyzer;

// Re-export commonly used testing types
pub use audio_generator::AudioGenerator;

use crate::{
    config::Config,
    signal::SignalProcessor,
    ui::{draw_main_content, draw_status_bar, draw_tabs},
};

/// Test framework for visual analysis
pub struct VisualTester {
    terminal: Terminal<TestBackend>,
    config: Config,
    signal_processor: SignalProcessor,
}

impl VisualTester {
    pub fn new(width: u16, height: u16) -> Self {
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).unwrap();

        // Create test config
        let config = Config::default();

        // Create signal processor for testing
        let signal_processor = SignalProcessor::new(
            config.audio.sample_rate,
            config.display.frequency_bands,
            config.display.frequency_range.clone(),
        );

        Self {
            terminal,
            config,
            signal_processor,
        }
    }

    /// Test a visualization with given magnitudes and return visual analysis
    pub fn test_visualization(
        &mut self,
        magnitudes: &[f32],
        selected_tab: usize,
    ) -> VisualAnalysis {
        // Normalize magnitudes
        let normalized_magnitudes = self
            .signal_processor
            .normalize_magnitudes(magnitudes, self.config.display.sensitivity);

        // Render the visualization
        self.terminal
            .draw(|f| {
                let chunks = ratatui::layout::Layout::default()
                    .direction(ratatui::layout::Direction::Vertical)
                    .margin(1)
                    .constraints([
                        ratatui::layout::Constraint::Length(3),
                        ratatui::layout::Constraint::Min(0),
                        ratatui::layout::Constraint::Length(3),
                    ])
                    .split(f.area());

                draw_tabs(f, chunks[0], selected_tab);
                draw_main_content(f, chunks[1], selected_tab, &normalized_magnitudes);
                draw_status_bar(f, chunks[2], 60.0, 1.0, normalized_magnitudes.len());
            })
            .unwrap();

        // Analyze the output
        let buffer = self.terminal.backend().buffer();
        VisualAnalysis::analyze(buffer, &normalized_magnitudes)
    }

    /// Generate a text representation of the current display
    pub fn capture_as_text(&self) -> String {
        let buffer = self.terminal.backend().buffer();
        let mut output = String::new();

        for y in 0..buffer.area().height {
            for x in 0..buffer.area().width {
                let cell = &buffer[(x, y)];
                write!(&mut output, "{}", cell.symbol()).unwrap();
            }
            writeln!(&mut output).unwrap();
        }

        output
    }

    /// Generate an ASCII art representation with colors indicated
    pub fn capture_with_colors(&self) -> String {
        let buffer = self.terminal.backend().buffer();
        let mut output = String::new();

        writeln!(&mut output, "=== VISUAL OUTPUT ANALYSIS ===").unwrap();
        writeln!(
            &mut output,
            "Size: {}x{}",
            buffer.area().width,
            buffer.area().height
        )
        .unwrap();
        writeln!(&mut output).unwrap();

        for y in 0..buffer.area().height {
            // Main content line
            for x in 0..buffer.area().width {
                let cell = &buffer[(x, y)];
                write!(&mut output, "{}", cell.symbol()).unwrap();
            }
            write!(&mut output, " |").unwrap();

            // Color information line
            for x in 0..buffer.area().width {
                let cell = &buffer[(x, y)];
                let color_char = match cell.fg {
                    Color::Reset => ' ',
                    Color::Black => 'k',
                    Color::Red => 'r',
                    Color::Green => 'g',
                    Color::Yellow => 'y',
                    Color::Blue => 'b',
                    Color::Magenta => 'm',
                    Color::Cyan => 'c',
                    Color::Gray => 'G',
                    Color::DarkGray => 'd',
                    Color::LightRed => 'R',
                    Color::LightGreen => 'L',
                    Color::LightYellow => 'Y',
                    Color::LightBlue => 'B',
                    Color::LightMagenta => 'M',
                    Color::LightCyan => 'C',
                    Color::White => 'W',
                    Color::Rgb(r, g, b) => {
                        if r > 200 && g < 100 && b < 100 {
                            'R'
                        } else if r < 100 && g > 200 && b < 100 {
                            'G'
                        } else if r < 100 && g < 100 && b > 200 {
                            'B'
                        } else if r > 150 && g > 150 && b < 100 {
                            'Y'
                        } else if r > 150 && g < 100 && b > 150 {
                            'M'
                        } else if r < 100 && g > 150 && b > 150 {
                            'C'
                        } else if r > 200 && g > 200 && b > 200 {
                            'W'
                        } else {
                            '.'
                        }
                    }
                    _ => '?',
                };
                write!(&mut output, "{}", color_char).unwrap();
            }

            writeln!(&mut output).unwrap();
        }

        output
    }
}

/// Analysis results for visual quality
#[derive(Debug)]
pub struct VisualAnalysis {
    pub color_distribution: HashMap<String, u32>,
    pub filled_percentage: f32,
    pub color_variety: u32,
    pub has_gradients: bool,
    pub visual_quality_score: f32,
    pub issues: Vec<String>,
    pub recommendations: Vec<String>,
}

impl VisualAnalysis {
    pub fn analyze(buffer: &ratatui::buffer::Buffer, magnitudes: &[f32]) -> Self {
        let mut color_distribution = HashMap::new();
        let mut total_cells = 0u32;
        let mut filled_cells = 0u32;
        let mut unique_colors = std::collections::HashSet::new();

        // Analyze each cell
        for y in 0..buffer.area().height {
            for x in 0..buffer.area().width {
                let cell = &buffer[(x, y)];
                total_cells += 1;

                // Count filled vs empty cells
                if !cell.symbol().trim().is_empty() && cell.symbol() != " " {
                    filled_cells += 1;
                }

                // Analyze colors
                let color_name = match cell.fg {
                    Color::Reset => "reset",
                    Color::Black => "black",
                    Color::Red => "red",
                    Color::Green => "green",
                    Color::Yellow => "yellow",
                    Color::Blue => "blue",
                    Color::Magenta => "magenta",
                    Color::Cyan => "cyan",
                    Color::White => "white",
                    Color::Rgb(r, g, b) => {
                        unique_colors.insert(format!("rgb({},{},{})", r, g, b));
                        "rgb"
                    }
                    _ => "other",
                };

                *color_distribution
                    .entry(color_name.to_string())
                    .or_insert(0) += 1;
            }
        }

        let filled_percentage = (filled_cells as f32 / total_cells as f32) * 100.0;
        let color_variety = color_distribution.len() as u32 + unique_colors.len() as u32;

        // Detect gradients (simplified)
        let has_gradients = unique_colors.len() > 5;

        // Calculate visual quality score
        let mut quality_score = 0.0;

        // Good fill ratio (not too sparse, not too dense)
        if filled_percentage > 20.0 && filled_percentage < 80.0 {
            quality_score += 25.0;
        }

        // Good color variety
        if color_variety > 3 {
            quality_score += 25.0;
        }

        // Gradients add sophistication
        if has_gradients {
            quality_score += 20.0;
        }

        // Audio responsiveness
        let max_magnitude = magnitudes.iter().fold(0.0f32, |a, &b| a.max(b));
        if max_magnitude > 0.1 {
            quality_score += 15.0;
        }

        // Dynamic range
        let min_magnitude = magnitudes.iter().fold(1.0f32, |a, &b| a.min(b));
        if (max_magnitude - min_magnitude) > 0.3 {
            quality_score += 15.0;
        }

        // Identify issues
        let mut issues = Vec::new();
        let mut recommendations = Vec::new();

        if filled_percentage < 10.0 {
            issues.push("Visualization appears too sparse/empty".to_string());
            recommendations.push("Increase bar width or add glow effects".to_string());
        }

        if color_variety < 3 {
            issues.push("Limited color variety makes visualization monotonous".to_string());
            recommendations.push("Add frequency-based color mapping or gradients".to_string());
        }

        if !has_gradients {
            issues.push("Lacks smooth color transitions".to_string());
            recommendations.push("Implement gradient effects for smoother appearance".to_string());
        }

        if max_magnitude < 0.05 {
            issues.push("Very low audio response".to_string());
            recommendations.push("Check audio input levels and sensitivity settings".to_string());
        }

        Self {
            color_distribution,
            filled_percentage,
            color_variety,
            has_gradients,
            visual_quality_score: quality_score,
            issues,
            recommendations,
        }
    }

    pub fn report(&self) -> String {
        let mut report = String::new();

        writeln!(&mut report, "=== VISUAL QUALITY ANALYSIS ===").unwrap();
        writeln!(
            &mut report,
            "Quality Score: {:.1}/100",
            self.visual_quality_score
        )
        .unwrap();
        writeln!(
            &mut report,
            "Filled Percentage: {:.1}%",
            self.filled_percentage
        )
        .unwrap();
        writeln!(&mut report, "Color Variety: {}", self.color_variety).unwrap();
        writeln!(&mut report, "Has Gradients: {}", self.has_gradients).unwrap();
        writeln!(&mut report).unwrap();

        writeln!(&mut report, "Color Distribution:").unwrap();
        for (color, count) in &self.color_distribution {
            writeln!(&mut report, "  {}: {}", color, count).unwrap();
        }
        writeln!(&mut report).unwrap();

        if !self.issues.is_empty() {
            writeln!(&mut report, "Issues Found:").unwrap();
            for issue in &self.issues {
                writeln!(&mut report, "  ❌ {}", issue).unwrap();
            }
            writeln!(&mut report).unwrap();
        }

        if !self.recommendations.is_empty() {
            writeln!(&mut report, "Recommendations:").unwrap();
            for rec in &self.recommendations {
                writeln!(&mut report, "  💡 {}", rec).unwrap();
            }
            writeln!(&mut report).unwrap();
        }

        report
    }
}
