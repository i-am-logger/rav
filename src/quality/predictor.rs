// Quality Predictor - Forward-looking system to predict and prevent quality issues

use std::collections::VecDeque;
use std::time::Duration;
use super::metrics::VisualQualityMetrics;
use super::optimizer::{PredictedIssue, PredictedIssueType};

pub struct QualityPredictor {
    // Historical trend analysis
    trend_window_size: usize,
    prediction_accuracy_history: VecDeque<f32>,
    
    // Prediction models
    color_degradation_model: TrendModel,
    animation_stutter_model: TrendModel,
    audio_latency_model: TrendModel,
    performance_model: TrendModel,
    
    // Learning parameters
    learning_rate: f32,
    confidence_threshold: f32,
}

impl QualityPredictor {
    pub fn new() -> Self {
        Self {
            trend_window_size: 20,
            prediction_accuracy_history: VecDeque::with_capacity(100),
            
            color_degradation_model: TrendModel::new(0.05),
            animation_stutter_model: TrendModel::new(0.03),
            audio_latency_model: TrendModel::new(0.04),
            performance_model: TrendModel::new(0.02),
            
            learning_rate: 0.1,
            confidence_threshold: 0.6,
        }
    }
    
    /// Predict potential quality issues based on historical trends
    pub fn predict_quality_issues(
        &mut self,
        metrics_history: &VecDeque<VisualQualityMetrics>,
        prediction_horizon: Duration,
    ) -> Vec<PredictedIssue> {
        let mut predictions = Vec::new();
        
        if metrics_history.len() < self.trend_window_size {
            return predictions; // Not enough data for prediction
        }
        
        // Get recent metrics for trend analysis
        let recent_metrics: Vec<_> = metrics_history.iter()
            .rev()
            .take(self.trend_window_size)
            .collect();
        
        // Predict color degradation
        if let Some(issue) = self.predict_color_degradation(&recent_metrics, prediction_horizon) {
            predictions.push(issue);
        }
        
        // Predict animation stutter
        if let Some(issue) = self.predict_animation_stutter(&recent_metrics, prediction_horizon) {
            predictions.push(issue);
        }
        
        // Predict audio latency issues
        if let Some(issue) = self.predict_audio_latency(&recent_metrics, prediction_horizon) {
            predictions.push(issue);
        }
        
        // Predict performance drops
        if let Some(issue) = self.predict_performance_drop(&recent_metrics, prediction_horizon) {
            predictions.push(issue);
        }
        
        predictions
    }
    
    /// Predict color degradation based on trend analysis
    fn predict_color_degradation(
        &mut self,
        recent_metrics: &[&VisualQualityMetrics],
        horizon: Duration,
    ) -> Option<PredictedIssue> {
        let values: Vec<f32> = recent_metrics.iter()
            .map(|m| m.color_variety)
            .collect();
        
        let prediction = self.color_degradation_model.predict(&values, horizon);
        
        if prediction.confidence >= self.confidence_threshold && prediction.predicted_value < 0.5 {
            Some(PredictedIssue {
                issue_type: PredictedIssueType::ColorDegradation,
                confidence: prediction.confidence,
                severity: (0.7 - prediction.predicted_value).max(0.0),
                time_to_issue: prediction.time_to_threshold,
            })
        } else {
            None
        }
    }
    
    /// Predict animation stutter based on smoothness trends
    fn predict_animation_stutter(
        &mut self,
        recent_metrics: &[&VisualQualityMetrics],
        horizon: Duration,
    ) -> Option<PredictedIssue> {
        let values: Vec<f32> = recent_metrics.iter()
            .map(|m| m.animation_smoothness)
            .collect();
        
        let prediction = self.animation_stutter_model.predict(&values, horizon);
        
        if prediction.confidence >= self.confidence_threshold && prediction.predicted_value < 0.6 {
            Some(PredictedIssue {
                issue_type: PredictedIssueType::AnimationStutter,
                confidence: prediction.confidence,
                severity: (0.8 - prediction.predicted_value).max(0.0),
                time_to_issue: prediction.time_to_threshold,
            })
        } else {
            None
        }
    }
    
    /// Predict audio latency issues
    fn predict_audio_latency(
        &mut self,
        recent_metrics: &[&VisualQualityMetrics],
        horizon: Duration,
    ) -> Option<PredictedIssue> {
        let values: Vec<f32> = recent_metrics.iter()
            .map(|m| m.audio_responsiveness)
            .collect();
        
        let prediction = self.audio_latency_model.predict(&values, horizon);
        
        if prediction.confidence >= self.confidence_threshold && prediction.predicted_value < 0.6 {
            Some(PredictedIssue {
                issue_type: PredictedIssueType::AudioLatency,
                confidence: prediction.confidence,
                severity: (0.8 - prediction.predicted_value).max(0.0),
                time_to_issue: prediction.time_to_threshold,
            })
        } else {
            None
        }
    }
    
    /// Predict performance drops based on render performance trends
    fn predict_performance_drop(
        &mut self,
        recent_metrics: &[&VisualQualityMetrics],
        horizon: Duration,
    ) -> Option<PredictedIssue> {
        let values: Vec<f32> = recent_metrics.iter()
            .map(|m| m.render_performance)
            .collect();
        
        let prediction = self.performance_model.predict(&values, horizon);
        
        if prediction.confidence >= self.confidence_threshold && prediction.predicted_value < 0.8 {
            Some(PredictedIssue {
                issue_type: PredictedIssueType::PerformanceDrop,
                confidence: prediction.confidence,
                severity: (0.9 - prediction.predicted_value).max(0.0),
                time_to_issue: prediction.time_to_threshold,
            })
        } else {
            None
        }
    }
    
    /// Update prediction accuracy based on actual outcomes
    pub fn update_accuracy(&mut self, predicted_issues: &[PredictedIssue], actual_outcome: f32) {
        if predicted_issues.is_empty() {
            return;
        }
        
        // Calculate prediction accuracy
        let average_confidence: f32 = predicted_issues.iter()
            .map(|issue| issue.confidence)
            .sum::<f32>() / predicted_issues.len() as f32;
        
        // Compare prediction confidence with actual outcome
        let accuracy = 1.0 - (average_confidence - actual_outcome).abs();
        
        // Update accuracy history
        self.prediction_accuracy_history.push_back(accuracy);
        if self.prediction_accuracy_history.len() > 100 {
            self.prediction_accuracy_history.pop_front();
        }
        
        // Update prediction models based on accuracy
        self.update_model_parameters(accuracy);
    }
    
    /// Update prediction model parameters based on accuracy
    fn update_model_parameters(&mut self, accuracy: f32) {
        let adjustment_factor = if accuracy > 0.8 {
            1.05 // Increase sensitivity for accurate models
        } else if accuracy < 0.5 {
            0.95 // Decrease sensitivity for inaccurate models
        } else {
            1.0  // No change
        };
        
        self.color_degradation_model.update_sensitivity(adjustment_factor);
        self.animation_stutter_model.update_sensitivity(adjustment_factor);
        self.audio_latency_model.update_sensitivity(adjustment_factor);
        self.performance_model.update_sensitivity(adjustment_factor);
    }
    
    /// Get current prediction accuracy
    pub fn get_accuracy(&self) -> f32 {
        if self.prediction_accuracy_history.is_empty() {
            return 0.5; // Neutral accuracy
        }
        
        self.prediction_accuracy_history.iter().sum::<f32>() 
            / self.prediction_accuracy_history.len() as f32
    }
    
    /// Get predictor status
    pub fn status(&self) -> PredictorStatus {
        PredictorStatus {
            accuracy: self.get_accuracy(),
            predictions_made: self.prediction_accuracy_history.len(),
            confidence_threshold: self.confidence_threshold,
            model_sensitivity: self.color_degradation_model.sensitivity,
        }
    }
}

/// Simple trend prediction model
struct TrendModel {
    sensitivity: f32,
    trend_decay: f32,
}

impl TrendModel {
    fn new(sensitivity: f32) -> Self {
        Self {
            sensitivity,
            trend_decay: 0.1,
        }
    }
    
    /// Predict future value based on current trend
    fn predict(&self, values: &[f32], horizon: Duration) -> TrendPrediction {
        if values.len() < 3 {
            return TrendPrediction::default();
        }
        
        // Calculate linear trend
        let trend = self.calculate_linear_trend(values);
        let current_value = values[0]; // Most recent value
        
        // Predict future value
        let horizon_seconds = horizon.as_secs_f32();
        let predicted_value = current_value + trend * horizon_seconds * self.sensitivity;
        
        // Calculate confidence based on trend consistency
        let confidence = self.calculate_trend_confidence(values, trend);
        
        // Estimate time to reach critical threshold
        let threshold = 0.5; // Generic threshold
        let time_to_threshold = if trend < 0.0 && current_value > threshold {
            Duration::from_secs_f32((current_value - threshold) / (-trend * self.sensitivity))
        } else {
            Duration::from_secs(u64::MAX) // Never reach threshold
        };
        
        TrendPrediction {
            predicted_value,
            confidence,
            time_to_threshold,
        }
    }
    
    /// Calculate linear trend from recent values
    fn calculate_linear_trend(&self, values: &[f32]) -> f32 {
        let n = values.len() as f32;
        let sum_x = (0..values.len()).map(|i| i as f32).sum::<f32>();
        let sum_y = values.iter().sum::<f32>();
        let sum_xy = values.iter().enumerate()
            .map(|(i, &y)| i as f32 * y)
            .sum::<f32>();
        let sum_x2 = (0..values.len()).map(|i| (i * i) as f32).sum::<f32>();
        
        // Linear regression slope
        (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x * sum_x)
    }
    
    /// Calculate confidence in trend prediction
    fn calculate_trend_confidence(&self, values: &[f32], trend: f32) -> f32 {
        if values.len() < 2 {
            return 0.5;
        }
        
        // Calculate R-squared for trend line fit
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let mut ss_tot = 0.0;
        let mut ss_res = 0.0;
        
        for (i, &value) in values.iter().enumerate() {
            let predicted = values[0] + trend * i as f32;
            ss_tot += (value - mean).powi(2);
            ss_res += (value - predicted).powi(2);
        }
        
        let r_squared = if ss_tot > 0.0 {
            1.0 - (ss_res / ss_tot)
        } else {
            0.0
        };
        
        r_squared.clamp(0.0, 1.0)
    }
    
    /// Update model sensitivity based on accuracy
    fn update_sensitivity(&mut self, adjustment_factor: f32) {
        self.sensitivity = (self.sensitivity * adjustment_factor).clamp(0.01, 0.2);
    }
}

/// Prediction result structure
#[derive(Default)]
struct TrendPrediction {
    predicted_value: f32,
    confidence: f32,
    time_to_threshold: Duration,
}

/// Predictor status for monitoring
pub struct PredictorStatus {
    pub accuracy: f32,
    pub predictions_made: usize,
    pub confidence_threshold: f32,
    pub model_sensitivity: f32,
}
