// Cybernetic Quality Control System
// Implements real-time monitoring and improvement of visual quality metrics
// Based on control theory principles for autonomous system optimization

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tracing::info;

pub mod metrics;
pub mod optimizer;
pub mod predictor;

use metrics::VisualQualityMetrics;
use optimizer::QualityOptimizer;
use predictor::QualityPredictor;

/// Cybernetic control loop for visual quality management
pub struct CyberneticQualityController {
    // State tracking
    current_metrics: VisualQualityMetrics,
    metrics_history: VecDeque<VisualQualityMetrics>,
    last_measurement: Instant,
    measurement_interval: Duration,

    // Control components
    optimizer: QualityOptimizer,
    predictor: QualityPredictor,

    // Target quality thresholds
    color_variety_target: f32,
    animation_smoothness_target: f32,
    audio_responsiveness_target: f32,

    // Adaptation parameters
    adaptation_rate: f32,
    improvement_history: VecDeque<f32>,

    // Forward loop for prediction
    prediction_horizon: Duration,
    corrective_actions_taken: u32,
}

impl Default for CyberneticQualityController {
    fn default() -> Self {
        Self {
            current_metrics: VisualQualityMetrics::default(),
            metrics_history: VecDeque::with_capacity(100),
            last_measurement: Instant::now(),
            measurement_interval: Duration::from_millis(100), // 10Hz monitoring

            optimizer: QualityOptimizer::new(),
            predictor: QualityPredictor::new(),

            // Production quality targets from the playbook
            color_variety_target: 0.7,
            animation_smoothness_target: 0.8,
            audio_responsiveness_target: 0.8,

            adaptation_rate: 0.1,
            improvement_history: VecDeque::with_capacity(50),

            prediction_horizon: Duration::from_millis(500), // 500ms prediction
            corrective_actions_taken: 0,
        }
    }
}

impl CyberneticQualityController {
    pub fn new() -> Self {
        info!("🔄 Initializing Cybernetic Quality Control System");
        Self::default()
    }

    /// Main control loop - called every frame to assess and improve quality
    pub fn control_loop(&mut self, current_frame_data: &FrameData) -> QualityControlActions {
        let now = Instant::now();

        // SENSE: Measure current visual quality
        if now.duration_since(self.last_measurement) >= self.measurement_interval {
            self.sense_quality(current_frame_data);
            self.last_measurement = now;
        }

        // COMPARE: Analyze quality gaps
        let quality_assessment = self.compare_against_targets();

        // PREDICT: Anticipate future quality issues
        let predicted_issues = self
            .predictor
            .predict_quality_issues(&self.metrics_history, self.prediction_horizon);

        // DECIDE: Determine corrective actions
        let mut actions = QualityControlActions::default();

        // Handle current quality issues
        if quality_assessment.needs_immediate_action() {
            actions = self
                .optimizer
                .generate_corrective_actions(&quality_assessment);
            self.corrective_actions_taken += 1;
        }

        // Handle predicted future issues (forward loop)
        if !predicted_issues.is_empty() {
            let preventive_actions = self
                .optimizer
                .generate_preventive_actions(&predicted_issues);
            actions.merge_preventive(preventive_actions);
        }

        // ACT: Apply improvements
        self.apply_learning(quality_assessment.overall_improvement());

        // Log significant quality changes
        if actions.has_significant_changes() {
            info!(
                "🎯 Quality Control: Color={:.2}, Animation={:.2}, Audio={:.2} | Actions: {}",
                self.current_metrics.color_variety,
                self.current_metrics.animation_smoothness,
                self.current_metrics.audio_responsiveness,
                actions.summary()
            );
        }

        actions
    }

    /// SENSE: Measure current visual quality metrics
    fn sense_quality(&mut self, frame_data: &FrameData) {
        let new_metrics = VisualQualityMetrics::analyze_frame(frame_data);

        // Update current metrics with smoothing to reduce noise
        self.current_metrics = if self.metrics_history.is_empty() {
            new_metrics
        } else {
            self.current_metrics.smooth_with(&new_metrics, 0.3)
        };

        // Store in history for trend analysis
        self.metrics_history.push_back(self.current_metrics.clone());
        if self.metrics_history.len() > 100 {
            self.metrics_history.pop_front();
        }
    }

    /// COMPARE: Analyze quality against targets and historical performance
    fn compare_against_targets(&self) -> QualityAssessment {
        let color_gap = self.color_variety_target - self.current_metrics.color_variety;
        let animation_gap =
            self.animation_smoothness_target - self.current_metrics.animation_smoothness;
        let audio_gap =
            self.audio_responsiveness_target - self.current_metrics.audio_responsiveness;

        // Calculate trend analysis from recent history
        let trend = self.calculate_quality_trend();

        QualityAssessment {
            color_variety_gap: color_gap,
            animation_smoothness_gap: animation_gap,
            audio_responsiveness_gap: audio_gap,
            quality_trend: trend,
            urgency_level: self.calculate_urgency_level(color_gap, animation_gap, audio_gap),
            improvement_potential: self.estimate_improvement_potential(),
        }
    }

    /// Calculate quality trend from recent measurements
    fn calculate_quality_trend(&self) -> QualityTrend {
        if self.metrics_history.len() < 10 {
            return QualityTrend::Stable;
        }

        let recent_metrics: Vec<_> = self.metrics_history.iter().rev().take(10).collect();
        let older_avg = recent_metrics[5..]
            .iter()
            .map(|m| m.overall_score())
            .sum::<f32>()
            / 5.0;
        let recent_avg = recent_metrics[..5]
            .iter()
            .map(|m| m.overall_score())
            .sum::<f32>()
            / 5.0;

        let trend_strength = recent_avg - older_avg;

        if trend_strength > 0.05 {
            QualityTrend::Improving
        } else if trend_strength < -0.05 {
            QualityTrend::Degrading
        } else {
            QualityTrend::Stable
        }
    }

    /// Calculate urgency based on quality gaps
    fn calculate_urgency_level(
        &self,
        color_gap: f32,
        animation_gap: f32,
        audio_gap: f32,
    ) -> UrgencyLevel {
        let max_gap = color_gap.max(animation_gap).max(audio_gap);

        if max_gap > 0.4 {
            UrgencyLevel::Critical
        } else if max_gap > 0.2 {
            UrgencyLevel::High
        } else if max_gap > 0.1 {
            UrgencyLevel::Medium
        } else {
            UrgencyLevel::Low
        }
    }

    /// Estimate potential for improvement based on historical performance
    fn estimate_improvement_potential(&self) -> f32 {
        if self.improvement_history.is_empty() {
            return 0.5; // Neutral potential
        }

        let recent_improvements: f32 = self.improvement_history.iter().rev().take(10).sum();
        let improvement_rate = recent_improvements / 10.0;

        // Scale to 0-1 range where 1 = high improvement potential
        (improvement_rate + 0.1).clamp(0.0, 1.0)
    }

    /// Apply learning from quality improvements
    fn apply_learning(&mut self, improvement: f32) {
        self.improvement_history.push_back(improvement);
        if self.improvement_history.len() > 50 {
            self.improvement_history.pop_front();
        }

        // Adapt control parameters based on effectiveness
        if improvement > 0.05 {
            // Successful improvement - maintain or increase responsiveness
            self.adaptation_rate = (self.adaptation_rate * 1.05).min(0.3);
        } else if improvement < -0.02 {
            // Quality degradation - reduce adaptation rate
            self.adaptation_rate = (self.adaptation_rate * 0.95).max(0.05);
        }
    }

    /// Get current quality metrics for external monitoring
    #[allow(dead_code)]
    pub fn current_metrics(&self) -> &VisualQualityMetrics {
        &self.current_metrics
    }

    /// Get control system status for debugging
    #[allow(dead_code)]
    pub fn control_status(&self) -> ControlStatus {
        ControlStatus {
            actions_taken: self.corrective_actions_taken,
            current_adaptation_rate: self.adaptation_rate,
            quality_trend: self.calculate_quality_trend(),
            target_achievement: self.calculate_target_achievement(),
            prediction_accuracy: self.predictor.get_accuracy(),
        }
    }

    #[allow(dead_code)]
    fn calculate_target_achievement(&self) -> f32 {
        let color_achievement =
            (self.current_metrics.color_variety / self.color_variety_target).min(1.0);
        let animation_achievement =
            (self.current_metrics.animation_smoothness / self.animation_smoothness_target).min(1.0);
        let audio_achievement =
            (self.current_metrics.audio_responsiveness / self.audio_responsiveness_target).min(1.0);

        (color_achievement + animation_achievement + audio_achievement) / 3.0
    }
}

/// Frame data structure for quality analysis
pub struct FrameData {
    pub colors_used: Vec<(u8, u8, u8)>,
    pub animation_delta: f32,
    pub audio_magnitudes: Vec<f32>,
    pub render_time: Duration,
    pub frame_timestamp: Instant,
}

/// Quality assessment results
#[derive(Debug)]
pub struct QualityAssessment {
    pub color_variety_gap: f32,
    pub animation_smoothness_gap: f32,
    pub audio_responsiveness_gap: f32,
    pub quality_trend: QualityTrend,
    pub urgency_level: UrgencyLevel,
    #[allow(dead_code)]
    pub improvement_potential: f32,
}

impl QualityAssessment {
    pub fn needs_immediate_action(&self) -> bool {
        matches!(
            self.urgency_level,
            UrgencyLevel::Critical | UrgencyLevel::High
        ) || self.quality_trend == QualityTrend::Degrading
    }

    pub fn overall_improvement(&self) -> f32 {
        // Negative gaps indicate we're above target (good)
        let total_gap =
            self.color_variety_gap + self.animation_smoothness_gap + self.audio_responsiveness_gap;
        -total_gap / 3.0 // Convert to improvement score
    }
}

/// Quality control actions to be applied to the rendering system
#[derive(Debug, Default)]
pub struct QualityControlActions {
    pub color_enhancements: Vec<ColorEnhancement>,
    pub animation_adjustments: Vec<AnimationAdjustment>,
    pub audio_optimizations: Vec<AudioOptimization>,
    pub parameter_changes: Vec<ParameterChange>,
}

impl QualityControlActions {
    pub fn has_significant_changes(&self) -> bool {
        !self.color_enhancements.is_empty()
            || !self.animation_adjustments.is_empty()
            || !self.audio_optimizations.is_empty()
            || !self.parameter_changes.is_empty()
    }

    pub fn merge_preventive(&mut self, preventive: QualityControlActions) {
        self.color_enhancements
            .extend(preventive.color_enhancements);
        self.animation_adjustments
            .extend(preventive.animation_adjustments);
        self.audio_optimizations
            .extend(preventive.audio_optimizations);
        self.parameter_changes.extend(preventive.parameter_changes);
    }

    pub fn summary(&self) -> String {
        format!(
            "Colors: {}, Animation: {}, Audio: {}, Params: {}",
            self.color_enhancements.len(),
            self.animation_adjustments.len(),
            self.audio_optimizations.len(),
            self.parameter_changes.len()
        )
    }
}

#[derive(Debug, Clone)]
pub enum QualityTrend {
    Improving,
    Stable,
    Degrading,
}

impl PartialEq for QualityTrend {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (QualityTrend::Improving, QualityTrend::Improving)
                | (QualityTrend::Stable, QualityTrend::Stable)
                | (QualityTrend::Degrading, QualityTrend::Degrading)
        )
    }
}

#[derive(Debug)]
pub enum UrgencyLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Control system status for monitoring
#[derive(Debug)]
#[allow(dead_code)]
pub struct ControlStatus {
    pub actions_taken: u32,
    pub current_adaptation_rate: f32,
    pub quality_trend: QualityTrend,
    pub target_achievement: f32,
    pub prediction_accuracy: f32,
}

// Action types for specific improvements
#[derive(Debug)]
pub enum ColorEnhancement {
    IncreaseSpectrum,
    AddGradients,
    EnhanceContrast,
    BoostSaturation,
}

#[derive(Debug)]
pub enum AnimationAdjustment {
    SmoothFrameTiming,
    ReduceJitter,
    ImproveInterpolation,
    OptimizeDecay,
}

#[derive(Debug)]
pub enum AudioOptimization {
    IncreaseSensitivity,
    ReduceLatency,
    EnhanceResponseTime,
    ImproveNormalization,
}

#[derive(Debug)]
pub enum ParameterChange {
    AdjustSensitivity(f32),
    ModifyDecayRate(f32),
    UpdateColorPalette,
    OptimizeBuffering,
}
