// Quality Optimizer - Generates corrective and preventive actions for quality improvement

use super::{QualityAssessment, QualityControlActions, ColorEnhancement, AnimationAdjustment, AudioOptimization, ParameterChange, QualityTrend, UrgencyLevel};
use tracing::info;

pub struct QualityOptimizer {
    // Optimizer state for learning
    color_enhancement_effectiveness: f32,
    animation_adjustment_effectiveness: f32,
    audio_optimization_effectiveness: f32,
    
    // Adaptation parameters
    min_improvement_threshold: f32,
    aggressive_mode: bool,
}

impl QualityOptimizer {
    pub fn new() -> Self {
        Self {
            color_enhancement_effectiveness: 0.7,
            animation_adjustment_effectiveness: 0.8,
            audio_optimization_effectiveness: 0.6,
            min_improvement_threshold: 0.05,
            aggressive_mode: false,
        }
    }
    
    /// Generate corrective actions based on current quality assessment
    pub fn generate_corrective_actions(&mut self, assessment: &QualityAssessment) -> QualityControlActions {
        let mut actions = QualityControlActions::default();
        
        // Adjust aggressiveness based on urgency
        self.aggressive_mode = matches!(assessment.urgency_level, UrgencyLevel::Critical);
        
        // Generate color enhancements
        if assessment.color_variety_gap > self.min_improvement_threshold {
            actions.color_enhancements = self.generate_color_enhancements(assessment.color_variety_gap);
        }
        
        // Generate animation adjustments
        if assessment.animation_smoothness_gap > self.min_improvement_threshold {
            actions.animation_adjustments = self.generate_animation_adjustments(assessment.animation_smoothness_gap);
        }
        
        // Generate audio optimizations
        if assessment.audio_responsiveness_gap > self.min_improvement_threshold {
            actions.audio_optimizations = self.generate_audio_optimizations(assessment.audio_responsiveness_gap);
        }
        
        // Generate parameter changes for systemic improvements
        actions.parameter_changes = self.generate_parameter_changes(assessment);
        
        info!(
            "🔧 Generated {} corrective actions (Color: {}, Animation: {}, Audio: {}, Params: {})",
            actions.color_enhancements.len() + actions.animation_adjustments.len() + 
            actions.audio_optimizations.len() + actions.parameter_changes.len(),
            actions.color_enhancements.len(),
            actions.animation_adjustments.len(),
            actions.audio_optimizations.len(),
            actions.parameter_changes.len()
        );
        
        actions
    }
    
    /// Generate preventive actions based on predicted issues
    pub fn generate_preventive_actions(&self, predicted_issues: &[PredictedIssue]) -> QualityControlActions {
        let mut actions = QualityControlActions::default();
        
        for issue in predicted_issues {
            match issue.issue_type {
                PredictedIssueType::ColorDegradation => {
                    actions.color_enhancements.push(ColorEnhancement::IncreaseSpectrum);
                    actions.parameter_changes.push(ParameterChange::UpdateColorPalette);
                }
                PredictedIssueType::AnimationStutter => {
                    actions.animation_adjustments.push(AnimationAdjustment::SmoothFrameTiming);
                    actions.parameter_changes.push(ParameterChange::OptimizeBuffering);
                }
                PredictedIssueType::AudioLatency => {
                    actions.audio_optimizations.push(AudioOptimization::ReduceLatency);
                    actions.audio_optimizations.push(AudioOptimization::EnhanceResponseTime);
                }
                PredictedIssueType::PerformanceDrop => {
                    actions.parameter_changes.push(ParameterChange::OptimizeBuffering);
                    actions.animation_adjustments.push(AnimationAdjustment::OptimizeDecay);
                }
            }
        }
        
        actions
    }
    
    /// Generate color enhancement actions
    fn generate_color_enhancements(&self, color_gap: f32) -> Vec<ColorEnhancement> {
        let mut enhancements = Vec::new();
        
        // Prioritize enhancements based on gap size and effectiveness
        if color_gap > 0.4 || self.aggressive_mode {
            // Critical color variety issues - apply multiple enhancements
            enhancements.push(ColorEnhancement::IncreaseSpectrum);
            enhancements.push(ColorEnhancement::AddGradients);
            enhancements.push(ColorEnhancement::BoostSaturation);
        } else if color_gap > 0.2 {
            // Moderate issues - apply targeted enhancements
            enhancements.push(ColorEnhancement::IncreaseSpectrum);
            enhancements.push(ColorEnhancement::EnhanceContrast);
        } else {
            // Minor issues - gentle improvements
            enhancements.push(ColorEnhancement::AddGradients);
        }
        
        enhancements
    }
    
    /// Generate animation adjustment actions
    fn generate_animation_adjustments(&self, animation_gap: f32) -> Vec<AnimationAdjustment> {
        let mut adjustments = Vec::new();
        
        if animation_gap > 0.3 || self.aggressive_mode {
            // Critical animation issues
            adjustments.push(AnimationAdjustment::SmoothFrameTiming);
            adjustments.push(AnimationAdjustment::ReduceJitter);
            adjustments.push(AnimationAdjustment::ImproveInterpolation);
        } else if animation_gap > 0.15 {
            // Moderate issues
            adjustments.push(AnimationAdjustment::SmoothFrameTiming);
            adjustments.push(AnimationAdjustment::OptimizeDecay);
        } else {
            // Minor issues
            adjustments.push(AnimationAdjustment::ImproveInterpolation);
        }
        
        adjustments
    }
    
    /// Generate audio optimization actions
    fn generate_audio_optimizations(&self, audio_gap: f32) -> Vec<AudioOptimization> {
        let mut optimizations = Vec::new();
        
        if audio_gap > 0.4 || self.aggressive_mode {
            // Critical audio issues
            optimizations.push(AudioOptimization::IncreaseSensitivity);
            optimizations.push(AudioOptimization::ReduceLatency);
            optimizations.push(AudioOptimization::EnhanceResponseTime);
            optimizations.push(AudioOptimization::ImproveNormalization);
        } else if audio_gap > 0.2 {
            // Moderate issues
            optimizations.push(AudioOptimization::IncreaseSensitivity);
            optimizations.push(AudioOptimization::EnhanceResponseTime);
        } else {
            // Minor issues
            optimizations.push(AudioOptimization::ImproveNormalization);
        }
        
        optimizations
    }
    
    /// Generate parameter change actions
    fn generate_parameter_changes(&self, assessment: &QualityAssessment) -> Vec<ParameterChange> {
        let mut changes = Vec::new();
        
        // Sensitivity adjustments based on audio responsiveness
        if assessment.audio_responsiveness_gap > 0.2 {
            let sensitivity_increase = assessment.audio_responsiveness_gap * 0.5;
            changes.push(ParameterChange::AdjustSensitivity(sensitivity_increase));
        }
        
        // Decay rate adjustments for better animation
        if assessment.animation_smoothness_gap > 0.15 {
            let decay_adjustment = -assessment.animation_smoothness_gap * 0.02; // Slower decay
            changes.push(ParameterChange::ModifyDecayRate(decay_adjustment));
        }
        
        // Color palette updates for variety issues
        if assessment.color_variety_gap > 0.3 {
            changes.push(ParameterChange::UpdateColorPalette);
        }
        
        // Performance optimizations for degrading trends
        if assessment.quality_trend == QualityTrend::Degrading {
            changes.push(ParameterChange::OptimizeBuffering);
        }
        
        // Always consider buffering optimization for high urgency
        if matches!(assessment.urgency_level, UrgencyLevel::Critical) {
            changes.push(ParameterChange::OptimizeBuffering);
        }
        
        changes
    }
    
    /// Update optimizer effectiveness based on results
    pub fn update_effectiveness(&mut self, action_type: ActionType, improvement: f32) {
        let learning_rate = 0.1;
        let effectiveness = match action_type {
            ActionType::Color => &mut self.color_enhancement_effectiveness,
            ActionType::Animation => &mut self.animation_adjustment_effectiveness,
            ActionType::Audio => &mut self.audio_optimization_effectiveness,
        };
        
        // Update effectiveness with exponential moving average
        *effectiveness = *effectiveness * (1.0 - learning_rate) + improvement * learning_rate;
        *effectiveness = effectiveness.clamp(0.1, 1.0);
    }
    
    /// Get current optimizer status
    pub fn status(&self) -> OptimizerStatus {
        OptimizerStatus {
            color_effectiveness: self.color_enhancement_effectiveness,
            animation_effectiveness: self.animation_adjustment_effectiveness,
            audio_effectiveness: self.audio_optimization_effectiveness,
            aggressive_mode: self.aggressive_mode,
        }
    }
}

/// Predicted quality issue types for forward loop
#[derive(Debug)]
pub enum PredictedIssueType {
    ColorDegradation,
    AnimationStutter,
    AudioLatency,
    PerformanceDrop,
}

/// Predicted quality issue with confidence
#[derive(Debug)]
pub struct PredictedIssue {
    pub issue_type: PredictedIssueType,
    pub confidence: f32,
    pub severity: f32,
    pub time_to_issue: std::time::Duration,
}

/// Action types for effectiveness tracking
pub enum ActionType {
    Color,
    Animation,
    Audio,
}

/// Optimizer status for monitoring
pub struct OptimizerStatus {
    pub color_effectiveness: f32,
    pub animation_effectiveness: f32,
    pub audio_effectiveness: f32,
    pub aggressive_mode: bool,
}
