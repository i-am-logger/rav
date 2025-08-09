// Visual Quality Metrics System
// Real-time analysis of color variety, animation smoothness, and audio responsiveness

use super::FrameData;
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// Comprehensive visual quality metrics
#[derive(Debug, Clone, Default)]
pub struct VisualQualityMetrics {
    pub color_variety: f32,
    pub animation_smoothness: f32,
    pub audio_responsiveness: f32,
    pub render_performance: f32,
    pub timestamp: Option<Instant>,

    // Detailed metrics for analysis
    pub colors_detected: usize,
    pub color_spectrum_coverage: f32,
    pub frame_time_variance: f32,
    pub audio_peak_response: f32,
    pub visual_complexity: f32,
}

impl VisualQualityMetrics {
    /// Analyze a single frame to extract quality metrics
    pub fn analyze_frame(frame_data: &FrameData) -> Self {
        let mut metrics = Self {
            timestamp: Some(frame_data.frame_timestamp),
            ..Default::default()
        };

        // Analyze color variety and spectrum coverage
        metrics.analyze_color_quality(&frame_data.colors_used);

        // Analyze animation smoothness
        metrics.analyze_animation_quality(frame_data.animation_delta, frame_data.render_time);

        // Analyze audio responsiveness
        metrics.analyze_audio_quality(&frame_data.audio_magnitudes);

        // Calculate overall visual complexity
        metrics.calculate_visual_complexity(frame_data);

        metrics
    }

    /// Analyze color quality metrics
    fn analyze_color_quality(&mut self, colors: &[(u8, u8, u8)]) {
        if colors.is_empty() {
            self.color_variety = 0.0;
            self.color_spectrum_coverage = 0.0;
            return;
        }

        // Count unique colors
        let unique_colors: HashSet<_> = colors.iter().collect();
        self.colors_detected = unique_colors.len();

        // Calculate color variety (normalized to 0-1 range)
        // Target: 12+ colors for high variety
        self.color_variety = (unique_colors.len() as f32 / 12.0).min(1.0);

        // Analyze spectrum coverage
        self.color_spectrum_coverage = self.calculate_spectrum_coverage(colors);

        // Adjust color variety based on spectrum coverage
        self.color_variety *= 0.5 + 0.5 * self.color_spectrum_coverage;
    }

    /// Calculate how well colors cover the spectrum
    fn calculate_spectrum_coverage(&self, colors: &[(u8, u8, u8)]) -> f32 {
        if colors.is_empty() {
            return 0.0;
        }

        // Divide spectrum into hue bins
        let mut hue_bins = [false; 12]; // 12 hue regions (30° each)

        for &(r, g, b) in colors {
            // Convert RGB to HSV and get hue bin
            let hue = self.rgb_to_hue(r, g, b);
            let bin = ((hue / 30.0) as usize).min(11);
            hue_bins[bin] = true;
        }

        // Calculate coverage ratio
        let filled_bins = hue_bins.iter().filter(|&&filled| filled).count();
        filled_bins as f32 / 12.0
    }

    /// Simple RGB to Hue conversion
    fn rgb_to_hue(&self, r: u8, g: u8, b: u8) -> f32 {
        let r = r as f32 / 255.0;
        let g = g as f32 / 255.0;
        let b = b as f32 / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        if delta == 0.0 {
            return 0.0; // Grayscale
        }

        let hue = if max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };

        if hue < 0.0 {
            hue + 360.0
        } else {
            hue
        }
    }

    /// Analyze animation smoothness
    fn analyze_animation_quality(&mut self, animation_delta: f32, render_time: Duration) {
        // Calculate render performance
        let target_frame_time = Duration::from_millis(16); // 60fps target
        self.render_performance = if render_time <= target_frame_time {
            1.0
        } else {
            target_frame_time.as_secs_f32() / render_time.as_secs_f32()
        };

        // Analyze animation smoothness based on delta consistency
        // Smooth animation should have consistent, reasonable deltas
        self.frame_time_variance = self.calculate_frame_variance(animation_delta);

        // Animation smoothness combines render performance and delta consistency
        self.animation_smoothness = self.render_performance * (1.0 - self.frame_time_variance);
    }

    /// Calculate frame timing variance (lower is better)
    fn calculate_frame_variance(&self, animation_delta: f32) -> f32 {
        // Ideal delta is around 0.016s (60fps)
        let ideal_delta = 0.016;
        let variance = (animation_delta - ideal_delta).abs() / ideal_delta;
        variance.min(1.0) // Clamp to [0, 1]
    }

    /// Analyze audio responsiveness quality
    fn analyze_audio_quality(&mut self, magnitudes: &[f32]) {
        if magnitudes.is_empty() {
            self.audio_responsiveness = 0.0;
            return;
        }

        // Calculate peak audio response
        self.audio_peak_response = magnitudes.iter().fold(0.0f32, |a, &b| a.max(b));

        // Calculate dynamic range
        let min_magnitude = magnitudes.iter().fold(1.0f32, |a, &b| a.min(b));
        let dynamic_range = self.audio_peak_response - min_magnitude;

        // Calculate audio responsiveness based on:
        // 1. Peak response strength (should be > 0.1 for good audio)
        // 2. Dynamic range (difference between loud and quiet)
        // 3. Frequency distribution (not all in one band)
        let peak_score = (self.audio_peak_response * 2.0).min(1.0);
        let range_score = (dynamic_range * 3.0).min(1.0);
        let distribution_score = self.calculate_frequency_distribution(magnitudes);

        self.audio_responsiveness = (peak_score + range_score + distribution_score) / 3.0;
    }

    /// Calculate how well audio is distributed across frequencies
    fn calculate_frequency_distribution(&self, magnitudes: &[f32]) -> f32 {
        let len = magnitudes.len();
        if len < 4 {
            return 0.5; // Neutral score for insufficient data
        }

        // Divide into frequency bands
        let band_size = len / 4;
        let bands: Vec<f32> = (0..4)
            .map(|i| {
                let start = i * band_size;
                let end = if i == 3 { len } else { (i + 1) * band_size };
                magnitudes[start..end].iter().sum::<f32>() / (end - start) as f32
            })
            .collect();

        // Good distribution means multiple bands have significant energy
        let active_bands = bands.iter().filter(|&&band| band > 0.05).count();
        (active_bands as f32 / 4.0).min(1.0)
    }

    /// Calculate overall visual complexity
    fn calculate_visual_complexity(&mut self, frame_data: &FrameData) {
        // Visual complexity combines multiple factors:
        // 1. Color variety
        // 2. Audio activity level
        // 3. Animation amount

        let color_complexity = self.color_variety;
        let audio_complexity = self.audio_responsiveness;
        let animation_complexity = frame_data.animation_delta.min(1.0);

        self.visual_complexity = (color_complexity + audio_complexity + animation_complexity) / 3.0;
    }

    /// Smooth current metrics with new metrics to reduce noise
    pub fn smooth_with(&self, new_metrics: &VisualQualityMetrics, smoothing_factor: f32) -> Self {
        let alpha = smoothing_factor;
        let beta = 1.0 - alpha;

        VisualQualityMetrics {
            color_variety: beta * self.color_variety + alpha * new_metrics.color_variety,
            animation_smoothness: beta * self.animation_smoothness
                + alpha * new_metrics.animation_smoothness,
            audio_responsiveness: beta * self.audio_responsiveness
                + alpha * new_metrics.audio_responsiveness,
            render_performance: beta * self.render_performance
                + alpha * new_metrics.render_performance,
            timestamp: new_metrics.timestamp,

            colors_detected: new_metrics.colors_detected,
            color_spectrum_coverage: beta * self.color_spectrum_coverage
                + alpha * new_metrics.color_spectrum_coverage,
            frame_time_variance: beta * self.frame_time_variance
                + alpha * new_metrics.frame_time_variance,
            audio_peak_response: beta * self.audio_peak_response
                + alpha * new_metrics.audio_peak_response,
            visual_complexity: beta * self.visual_complexity
                + alpha * new_metrics.visual_complexity,
        }
    }

    /// Calculate overall quality score
    pub fn overall_score(&self) -> f32 {
        // Weighted average of key metrics
        let weights = [0.3, 0.3, 0.3, 0.1]; // color, animation, audio, performance
        let values = [
            self.color_variety,
            self.animation_smoothness,
            self.audio_responsiveness,
            self.render_performance,
        ];

        weights.iter().zip(values.iter()).map(|(w, v)| w * v).sum()
    }

    /// Check if metrics meet production quality standards
    #[allow(dead_code)]
    pub fn meets_production_quality(&self) -> bool {
        self.color_variety >= 0.7
            && self.animation_smoothness >= 0.8
            && self.audio_responsiveness >= 0.8
            && self.render_performance >= 0.9
    }

    /// Get quality grade as a letter
    #[allow(dead_code)]
    pub fn quality_grade(&self) -> char {
        let score = self.overall_score();
        if score >= 0.9 {
            'A'
        } else if score >= 0.8 {
            'B'
        } else if score >= 0.7 {
            'C'
        } else if score >= 0.6 {
            'D'
        } else {
            'F'
        }
    }

    /// Generate detailed quality report
    #[allow(dead_code)]
    pub fn detailed_report(&self) -> String {
        format!(
            "Quality Report [Grade: {}]\n\
             Overall Score: {:.3}\n\
             ├─ Color Variety: {:.3} ({} colors, {:.1}% spectrum)\n\
             ├─ Animation: {:.3} (variance: {:.3}, perf: {:.3})\n\
             ├─ Audio: {:.3} (peak: {:.3})\n\
             └─ Visual Complexity: {:.3}\n\
             Production Ready: {}",
            self.quality_grade(),
            self.overall_score(),
            self.color_variety,
            self.colors_detected,
            self.color_spectrum_coverage * 100.0,
            self.animation_smoothness,
            self.frame_time_variance,
            self.render_performance,
            self.audio_responsiveness,
            self.audio_peak_response,
            self.visual_complexity,
            if self.meets_production_quality() {
                "✅ Yes"
            } else {
                "❌ No"
            }
        )
    }
}
