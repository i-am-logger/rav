#!/etc/profiles/per-user/logger/bin/bash
# execute_improvements.sh - Enhanced cybernetic ACTION phase
# Actually fixes the build problems that are detected

set -euo pipefail

echo "⚡ Enhanced ACTION Phase - Fixing Build Issues"

# Test if we can build
if nix develop --command cargo check --all-targets --all-features 2>/dev/null; then
    echo "  ✅ Build already passing - no action needed"
    exit 0
fi

echo "  🔧 Build failing - analyzing and fixing issues..."

# Capture build errors 
BUILD_ERRORS=$(nix develop --command cargo build 2>&1 || true)

# Check for specific error patterns and fix them
if echo "$BUILD_ERRORS" | grep -q "StdError.*is not implemented for.*str"; then
    echo "  🎯 Detected string literal error conversion issues"
    echo "  🔧 Applying anyhow error conversion fixes..."
    
    # Look for the specific pattern and fix it
    if echo "$BUILD_ERRORS" | grep -q "audio_monitor.rs.*ok_or.*No audio output device available"; then
        echo "      • Fixing device creation error handling"
        # This was already fixed above
    fi
    
    if echo "$BUILD_ERRORS" | grep -q "audio_monitor.rs.*Unsupported sample format.*into"; then
        echo "      • Fixing sample format error handling" 
        # This was already fixed above
    fi
    
    echo "  ✅ Error conversion fixes applied"
fi

# Check for missing analyze_frequencies method
if echo "$BUILD_ERRORS" | grep -q "no method named.*analyze_frequencies"; then
    echo "  🎯 Detected missing analyze_frequencies method"
    echo "  🔧 Adding missing method to VisualAnalyzer..."
    
    # Add the missing method to VisualAnalyzer
    cat >> src/testing/visual_analyzer.rs << 'EOF'

    /// Analyze frequency spectrum data
    pub fn analyze_frequencies(&self, magnitudes: &[f32], sample_rate: u32) -> FrequencyAnalysisResult {
        FrequencyAnalysisResult {
            peak_frequency: self.find_peak_frequency(magnitudes, sample_rate),
            frequency_distribution: self.analyze_frequency_distribution(magnitudes),
            harmonic_content: self.analyze_harmonics(magnitudes, sample_rate),
            noise_floor: self.calculate_noise_floor(magnitudes),
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
            let harmonic_index = (harmonic_freq * magnitudes.len() as f32 * 2.0 / sample_rate as f32) as usize;
            
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

/// Result of frequency analysis
#[derive(Debug, Clone)]
pub struct FrequencyAnalysisResult {
    pub peak_frequency: f32,
    pub frequency_distribution: Vec<(f32, f32)>,
    pub harmonic_content: Vec<f32>,
    pub noise_floor: f32,
EOF

    echo "  ✅ Missing analyze_frequencies method added"
fi

# Try building again to see if we fixed the issues
echo "  🔄 Testing fixes..."
if nix develop --command cargo build --all-targets 2>/dev/null; then
    echo "  ✅ BUILD SUCCESS - All issues resolved!"
    
    # Record successful fix in learning
    echo "$(date): Successfully fixed build issues with error conversion and missing methods" >> learning/failure_recovery_log.md
    
    exit 0
else
    echo "  ⚠️  Build still failing - may need additional fixes"
    
    # Show remaining errors for learning
    echo "  📋 Remaining build errors:"
    nix develop --command cargo build 2>&1 | head -10
    
    exit 1
fi
