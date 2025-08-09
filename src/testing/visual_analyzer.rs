// Visual analysis tools for measuring visualization quality
use ratatui::{buffer::Buffer, style::Color};
use std::collections::HashMap;

/// Analyzes visual elements for quality metrics
pub struct VisualAnalyzer;

impl Default for VisualAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl VisualAnalyzer {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze_frames(&self, frames: &[ratatui::buffer::Buffer]) -> VisualAnalysisResult {
        if frames.is_empty() {
            return VisualAnalysisResult {
                overall_score: 0.0,
                color_variety: 0,
                fill_percentage: 0.0,
                gradient_quality: 0.0,
                animation_smoothness: 0.0,
                issues: vec!["No frames to analyze".to_string()],
                recommendations: vec!["Ensure visualization is rendering properly".to_string()],
            };
        }

        let frame = &frames[0];
        let mut score = 0.0;
        let mut color_count = std::collections::HashSet::new();
        let mut filled_cells = 0;
        let total_cells = frame.area().width as usize * frame.area().height as usize;

        // Analyze frame content
        for y in 0..frame.area().height {
            for x in 0..frame.area().width {
                let cell = &frame[(x, y)];
                if !cell.symbol().trim().is_empty() && cell.symbol() != " " {
                    filled_cells += 1;
                }
                if let ratatui::style::Color::Rgb(r, g, b) = cell.fg {
                    color_count.insert((r, g, b));
                }
            }
        }

        let fill_percentage = filled_cells as f32 / total_cells as f32;
        let color_variety = color_count.len();

        // Score based on various factors
        if fill_percentage > 0.1 {
            score += 25.0;
        }
        if color_variety > 3 {
            score += 25.0;
        }
        if fill_percentage > 0.2 && fill_percentage < 0.8 {
            score += 25.0;
        }
        if color_variety > 5 {
            score += 25.0;
        }

        VisualAnalysisResult {
            overall_score: score,
            color_variety,
            fill_percentage: fill_percentage * 100.0,
            gradient_quality: if color_variety > 5 { 0.8 } else { 0.4 },
            animation_smoothness: 0.7, // Default for now
            issues: vec![],
            recommendations: if score < 70.0 {
                vec!["Improve visual richness and color variety".to_string()]
            } else {
                vec![]
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct VisualAnalysisResult {
    pub overall_score: f32,
    pub color_variety: usize,
    pub fill_percentage: f32,
    pub gradient_quality: f32,
    pub animation_smoothness: f32,
    pub issues: Vec<String>,
    pub recommendations: Vec<String>,
}

impl VisualAnalyzer {
    /// Calculate color contrast ratio between two colors
    pub fn color_contrast(color1: Color, color2: Color) -> f32 {
        let (r1, g1, b1) = Self::color_to_rgb(color1);
        let (r2, g2, b2) = Self::color_to_rgb(color2);

        let l1 = Self::relative_luminance(r1, g1, b1);
        let l2 = Self::relative_luminance(r2, g2, b2);

        if l1 > l2 {
            (l1 + 0.05) / (l2 + 0.05)
        } else {
            (l2 + 0.05) / (l1 + 0.05)
        }
    }

    /// Convert ratatui Color to RGB values
    fn color_to_rgb(color: Color) -> (u8, u8, u8) {
        match color {
            Color::Reset => (128, 128, 128),
            Color::Black => (0, 0, 0),
            Color::Red => (255, 0, 0),
            Color::Green => (0, 255, 0),
            Color::Yellow => (255, 255, 0),
            Color::Blue => (0, 0, 255),
            Color::Magenta => (255, 0, 255),
            Color::Cyan => (0, 255, 255),
            Color::Gray => (128, 128, 128),
            Color::DarkGray => (64, 64, 64),
            Color::LightRed => (255, 128, 128),
            Color::LightGreen => (128, 255, 128),
            Color::LightYellow => (255, 255, 128),
            Color::LightBlue => (128, 128, 255),
            Color::LightMagenta => (255, 128, 255),
            Color::LightCyan => (128, 255, 255),
            Color::White => (255, 255, 255),
            Color::Rgb(r, g, b) => (r, g, b),
            _ => (128, 128, 128),
        }
    }

    /// Calculate relative luminance for contrast calculations
    fn relative_luminance(r: u8, g: u8, b: u8) -> f32 {
        let r = r as f32 / 255.0;
        let g = g as f32 / 255.0;
        let b = b as f32 / 255.0;

        let r = if r <= 0.03928 {
            r / 12.92
        } else {
            ((r + 0.055) / 1.055).powf(2.4)
        };
        let g = if g <= 0.03928 {
            g / 12.92
        } else {
            ((g + 0.055) / 1.055).powf(2.4)
        };
        let b = if b <= 0.03928 {
            b / 12.92
        } else {
            ((b + 0.055) / 1.055).powf(2.4)
        };

        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// Analyze visual density and distribution
    pub fn analyze_visual_density(buffer: &Buffer) -> VisualDensityMetrics {
        let total_cells = buffer.area().width as usize * buffer.area().height as usize;
        let mut filled_cells = 0;
        let mut color_regions = HashMap::new();
        let mut vertical_distribution = vec![0; buffer.area().height as usize];
        let mut horizontal_distribution = vec![0; buffer.area().width as usize];

        for y in 0..buffer.area().height {
            for x in 0..buffer.area().width {
                let cell = &buffer[(x, y)];

                // Count non-empty cells
                if !cell.symbol().trim().is_empty() && cell.symbol() != " " {
                    filled_cells += 1;
                    vertical_distribution[y as usize] += 1;
                    horizontal_distribution[x as usize] += 1;
                }

                // Track color regions
                let color_key = format!("{:?}", cell.fg);
                *color_regions.entry(color_key).or_insert(0) += 1;
            }
        }

        let fill_percentage = (filled_cells as f32 / total_cells as f32) * 100.0;

        // Calculate distribution evenness
        let vertical_evenness = Self::calculate_distribution_evenness(&vertical_distribution);
        let horizontal_evenness = Self::calculate_distribution_evenness(&horizontal_distribution);

        VisualDensityMetrics {
            total_cells,
            filled_cells,
            fill_percentage,
            color_regions,
            vertical_evenness,
            horizontal_evenness,
        }
    }

    /// Calculate how evenly distributed values are (0 = all in one place, 1 = perfectly even)
    fn calculate_distribution_evenness(distribution: &[usize]) -> f32 {
        let total: usize = distribution.iter().sum();
        if total == 0 {
            return 1.0;
        }

        let mean = total as f32 / distribution.len() as f32;
        let variance: f32 = distribution
            .iter()
            .map(|&count| {
                let diff = count as f32 - mean;
                diff * diff
            })
            .sum::<f32>()
            / distribution.len() as f32;

        // Convert variance to evenness score (lower variance = more even)
        let max_variance =
            mean * mean * (distribution.len() as f32 - 1.0) / distribution.len() as f32;
        if max_variance == 0.0 {
            1.0
        } else {
            (1.0 - variance / max_variance).max(0.0)
        }
    }

    /// Detect visual patterns and rhythm
    pub fn analyze_visual_patterns(buffer: &Buffer) -> VisualPatternMetrics {
        let mut patterns = HashMap::new();
        #[allow(unused_assignments)]
        let mut rhythm_score = 0.0;
        let mut repetitive_elements = 0;

        // Analyze horizontal patterns (common in bar charts)
        for y in 0..buffer.area().height {
            let mut row_pattern = String::new();
            for x in 0..buffer.area().width {
                let cell = &buffer[(x, y)];
                row_pattern.push_str(cell.symbol());
            }

            // Look for repeating segments
            let segments = Self::find_repeating_segments(&row_pattern, 2);
            repetitive_elements += segments.len();

            // Store pattern
            *patterns.entry(row_pattern).or_insert(0) += 1;
        }

        // Calculate rhythm score based on pattern regularity
        rhythm_score = Self::calculate_rhythm_score(&patterns);

        VisualPatternMetrics {
            unique_patterns: patterns.len(),
            repetitive_elements,
            rhythm_score,
            pattern_frequency: patterns,
        }
    }

    /// Find repeating segments in a string
    fn find_repeating_segments(s: &str, min_length: usize) -> Vec<String> {
        let mut segments = Vec::new();

        for len in min_length..s.len() / 2 {
            for start in 0..s.len() - len {
                let segment = &s[start..start + len];
                let remaining = &s[start + len..];

                if remaining.starts_with(segment) {
                    segments.push(segment.to_string());
                }
            }
        }

        segments.sort();
        segments.dedup();
        segments
    }

    /// Calculate rhythm score from pattern analysis
    fn calculate_rhythm_score(patterns: &HashMap<String, usize>) -> f32 {
        if patterns.is_empty() {
            return 0.0;
        }

        let total_patterns: usize = patterns.values().sum();
        let unique_count = patterns.len();

        // Good rhythm has some repetition but not too much
        let repetition_ratio = (total_patterns - unique_count) as f32 / total_patterns as f32;

        // Score peaks around 0.3-0.7 repetition ratio
        if repetition_ratio < 0.3 {
            repetition_ratio / 0.3
        } else if repetition_ratio > 0.7 {
            (1.0 - repetition_ratio) / 0.3
        } else {
            1.0
        }
    }

    /// Analyze color harmony and aesthetic appeal
    pub fn analyze_color_harmony(buffer: &Buffer) -> ColorHarmonyMetrics {
        let mut color_counts = HashMap::new();
        let mut rgb_colors = Vec::new();

        // Collect all colors
        for y in 0..buffer.area().height {
            for x in 0..buffer.area().width {
                let cell = &buffer[(x, y)];
                let color = cell.fg;

                *color_counts.entry(format!("{color:?}")).or_insert(0) += 1;

                if let Color::Rgb(r, g, b) = color {
                    rgb_colors.push((r, g, b));
                }
            }
        }

        // Calculate color diversity
        let total_cells = buffer.area().width as usize * buffer.area().height as usize;
        let unique_colors = color_counts.len();
        let color_diversity = unique_colors as f32 / total_cells as f32;

        // Analyze RGB color relationships
        let harmony_score = Self::calculate_color_harmony_score(&rgb_colors);

        // Calculate color temperature (warm vs cool)
        let temperature_score = Self::calculate_color_temperature(&rgb_colors);

        ColorHarmonyMetrics {
            unique_colors,
            color_diversity,
            harmony_score,
            temperature_score,
            dominant_colors: Self::find_dominant_colors(&color_counts, 5),
        }
    }

    /// Calculate color harmony score based on color wheel relationships
    fn calculate_color_harmony_score(rgb_colors: &[(u8, u8, u8)]) -> f32 {
        if rgb_colors.is_empty() {
            return 0.0;
        }

        let mut hue_angles = Vec::new();

        // Convert RGB to HSV and extract hue angles
        for &(r, g, b) in rgb_colors {
            let (h, _, _) = Self::rgb_to_hsv(r, g, b);
            hue_angles.push(h);
        }

        // Analyze hue distribution for harmony
        hue_angles.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mut harmony_score = 0.0;
        let mut harmony_count = 0;

        // Check for complementary colors (180° apart)
        // Check for triadic colors (120° apart)
        // Check for analogous colors (close together)
        for i in 0..hue_angles.len() {
            for j in i + 1..hue_angles.len() {
                let diff = (hue_angles[j] - hue_angles[i]).abs();
                let complement_diff = (diff - 180.0).abs();
                let triadic_diff = (diff - 120.0).abs().min((diff - 240.0).abs());

                if complement_diff < 30.0 {
                    harmony_score += 1.0; // Complementary
                } else if triadic_diff < 30.0 {
                    harmony_score += 0.8; // Triadic
                } else if diff < 30.0 {
                    harmony_score += 0.6; // Analogous
                }

                harmony_count += 1;
            }
        }

        if harmony_count > 0 {
            harmony_score / harmony_count as f32
        } else {
            0.0
        }
    }

    /// Convert RGB to HSV
    fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
        let r = r as f32 / 255.0;
        let g = g as f32 / 255.0;
        let b = b as f32 / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let h = if delta == 0.0 {
            0.0
        } else if max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if max == g {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };

        let s = if max == 0.0 { 0.0 } else { delta / max };
        let v = max;

        (h, s, v)
    }

    /// Calculate color temperature (warm = 1.0, cool = -1.0, neutral = 0.0)
    fn calculate_color_temperature(rgb_colors: &[(u8, u8, u8)]) -> f32 {
        if rgb_colors.is_empty() {
            return 0.0;
        }

        let mut temperature_sum = 0.0;

        for &(r, g, b) in rgb_colors {
            // Simple temperature calculation: more red/yellow = warm, more blue = cool
            let warm_score = (r as f32 + g as f32 * 0.5) / 255.0;
            let cool_score = b as f32 / 255.0;
            temperature_sum += warm_score - cool_score;
        }

        (temperature_sum / rgb_colors.len() as f32).clamp(-1.0, 1.0)
    }

    /// Find most frequently used colors
    fn find_dominant_colors(
        color_counts: &HashMap<String, usize>,
        limit: usize,
    ) -> Vec<(String, usize)> {
        let mut colors: Vec<_> = color_counts.iter().collect();
        colors.sort_by(|a, b| b.1.cmp(a.1));
        colors
            .into_iter()
            .take(limit)
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    /// Analyze frequency spectrum data
    pub fn analyze_frequencies(
        &self,
        magnitudes: &[f32],
        sample_rate: u32,
    ) -> FrequencyAnalysisResult {
        let peak_frequency = self.find_peak_frequency(magnitudes, sample_rate);
        let noise_floor = self.calculate_noise_floor(magnitudes);
        let harmonic_content = self.analyze_harmonics(magnitudes, sample_rate);

        // Calculate overall quality based on various factors
        let mut quality_score: f32 = 50.0; // Base score

        // Good peak frequency range (20Hz - 20kHz)
        if (20.0..=20000.0).contains(&peak_frequency) {
            quality_score += 20.0;
        }

        // Low noise floor is good
        if noise_floor < 0.1 {
            quality_score += 20.0;
        } else if noise_floor < 0.3 {
            quality_score += 10.0;
        }

        // Rich harmonic content is good
        if harmonic_content.len() >= 3 {
            quality_score += 10.0;
        }

        let mut issues = Vec::new();
        let mut recommendations = Vec::new();

        if !(20.0..=20000.0).contains(&peak_frequency) {
            issues.push(format!(
                "Peak frequency {peak_frequency}Hz is outside audible range"
            ));
            recommendations.push("Check audio input source".to_string());
        }

        if noise_floor > 0.5 {
            issues.push("High noise floor detected".to_string());
            recommendations
                .push("Improve audio input quality or reduce background noise".to_string());
        }

        if harmonic_content.is_empty() {
            issues.push("No harmonics detected".to_string());
            recommendations.push("Check for musical or tonal content".to_string());
        }

        FrequencyAnalysisResult {
            peak_frequency,
            frequency_distribution: self.analyze_frequency_distribution(magnitudes),
            harmonic_content,
            noise_floor,
            overall_quality: quality_score.min(100.0),
            issues,
            recommendations,
        }
    }

    fn find_peak_frequency(&self, magnitudes: &[f32], sample_rate: u32) -> f32 {
        let peak_index = magnitudes
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);

        (peak_index as f32 * sample_rate as f32) / (magnitudes.len() as f32 * 2.0)
    }

    fn analyze_frequency_distribution(&self, magnitudes: &[f32]) -> Vec<(f32, f32)> {
        magnitudes
            .iter()
            .enumerate()
            .map(|(i, &magnitude)| (i as f32, magnitude))
            .collect()
    }

    fn analyze_harmonics(&self, magnitudes: &[f32], sample_rate: u32) -> Vec<f32> {
        // Simple harmonic analysis - find peaks at multiples of fundamental
        let fundamental = self.find_peak_frequency(magnitudes, sample_rate);
        let mut harmonics = Vec::new();

        for harmonic in 2..=8 {
            let harmonic_freq = fundamental * harmonic as f32;
            let harmonic_index =
                (harmonic_freq * magnitudes.len() as f32 * 2.0 / sample_rate as f32) as usize;

            if harmonic_index < magnitudes.len() {
                harmonics.push(magnitudes[harmonic_index]);
            }
        }

        harmonics
    }

    fn calculate_noise_floor(&self, magnitudes: &[f32]) -> f32 {
        let mut sorted_mags: Vec<f32> = magnitudes.to_vec();
        sorted_mags.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // Use lower 10% as noise floor estimate
        let noise_samples = sorted_mags.len() / 10;
        sorted_mags[..noise_samples].iter().sum::<f32>() / noise_samples as f32
    }
}

/// Metrics for visual density analysis
#[derive(Debug)]
pub struct VisualDensityMetrics {
    pub total_cells: usize,
    pub filled_cells: usize,
    pub fill_percentage: f32,
    pub color_regions: HashMap<String, usize>,
    pub vertical_evenness: f32,
    pub horizontal_evenness: f32,
}

/// Metrics for visual pattern analysis
#[derive(Debug)]
pub struct VisualPatternMetrics {
    pub unique_patterns: usize,
    pub repetitive_elements: usize,
    pub rhythm_score: f32,
    pub pattern_frequency: HashMap<String, usize>,
}

/// Metrics for color harmony analysis
#[derive(Debug)]
pub struct ColorHarmonyMetrics {
    pub unique_colors: usize,
    pub color_diversity: f32,
    pub harmony_score: f32,
    pub temperature_score: f32,
    pub dominant_colors: Vec<(String, usize)>,
}

/// Result of frequency analysis
#[derive(Debug, Clone)]
pub struct FrequencyAnalysisResult {
    pub peak_frequency: f32,
    pub frequency_distribution: Vec<(f32, f32)>,
    pub harmonic_content: Vec<f32>,
    pub noise_floor: f32,
    pub overall_quality: f32,
    pub issues: Vec<String>,
    pub recommendations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_contrast_calculation() {
        let contrast = VisualAnalyzer::color_contrast(Color::White, Color::Black);
        assert!(contrast > 10.0); // Should be high contrast

        let low_contrast = VisualAnalyzer::color_contrast(Color::Gray, Color::DarkGray);
        assert!(low_contrast < 5.0); // Should be lower contrast
    }

    #[test]
    fn test_rgb_to_hsv_conversion() {
        let (h, s, v) = VisualAnalyzer::rgb_to_hsv(255, 0, 0); // Pure red
        assert!((h - 0.0).abs() < 1.0); // Hue should be near 0
        assert!((s - 1.0).abs() < 0.1); // Should be fully saturated
        assert!((v - 1.0).abs() < 0.1); // Should be full value
    }

    #[test]
    fn test_distribution_evenness() {
        // Perfectly even distribution
        let even_dist = vec![10, 10, 10, 10, 10];
        let evenness = VisualAnalyzer::calculate_distribution_evenness(&even_dist);
        assert!(evenness > 0.9);

        // Uneven distribution
        let uneven_dist = vec![50, 0, 0, 0, 0];
        let unevenness = VisualAnalyzer::calculate_distribution_evenness(&uneven_dist);
        assert!(unevenness < 0.5);
    }
}
