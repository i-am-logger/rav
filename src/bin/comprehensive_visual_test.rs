// Comprehensive visual testing for Speedy v1.0 development
// Tests all visualization modes with various frequency scenarios

use speedy::{
    config::Config,
    testing::{audio_generator::AudioGenerator, VisualTester},
};
use std::fs;

fn main() {
    println!("🎵 Speedy v1.0 Comprehensive Visual Testing Suite");
    println!("================================================\n");

    let test_width = 140; // Wider for better visualization
    let test_height = 50; // Taller for better bars
    let frequency_bands = 64; // More frequency resolution

    let mut tester = VisualTester::new(test_width, test_height);
    let mut audio_gen = AudioGenerator::new(44100.0);

    println!("🔬 Testing all visualization modes with various frequency scenarios...\n");

    // Test scenarios for comprehensive coverage
    let test_scenarios = vec![
        (
            "Silent",
            AudioGenerator::quiet_spectrum_magnitudes(frequency_bands),
        ),
        ("Bass Heavy", generate_bass_heavy_spectrum(frequency_bands)),
        ("Mid Range", generate_mid_range_spectrum(frequency_bands)),
        ("High Treble", generate_treble_spectrum(frequency_bands)),
        (
            "Full Spectrum Rock",
            generate_rock_spectrum(frequency_bands),
        ),
        ("Electronic Dance", generate_edm_spectrum(frequency_bands)),
        (
            "Classical Orchestra",
            generate_classical_spectrum(frequency_bands),
        ),
        ("Vocal Focused", generate_vocal_spectrum(frequency_bands)),
        (
            "Percussion Heavy",
            generate_percussion_spectrum(frequency_bands),
        ),
        (
            "Dynamic Range Test",
            AudioGenerator::dynamic_spectrum_magnitudes(frequency_bands),
        ),
    ];

    let visualization_modes = vec![
        (0, "Neon Bars"),
        (1, "Fluid Wave"),
        (2, "Spectrum Chart"),
        (3, "System Info"),
    ];

    let mut all_results: Vec<(String, Vec<(String, speedy::testing::VisualAnalysis)>)> = Vec::new();
    let mut mode_scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

    // Test each visualization mode with each audio scenario
    for (mode_id, mode_name) in &visualization_modes {
        println!("🎨 Testing {} Mode", mode_name);
        println!("{}", "=".repeat(50));

        let mut mode_total = 0.0;
        let mut scenario_results = Vec::new();

        for (scenario_name, magnitudes) in &test_scenarios {
            let analysis = tester.test_visualization(magnitudes, *mode_id);

            println!(
                "  📊 {}: {:.1}/100",
                scenario_name, analysis.visual_quality_score
            );

            // Detailed analysis for poor scores
            if analysis.visual_quality_score < 70.0 {
                println!("    ⚠️  Issues: {:?}", analysis.issues);
                println!("    💡 Recommendations: {:?}", analysis.recommendations);
            }

            mode_total += analysis.visual_quality_score;
            scenario_results.push((scenario_name.to_string(), analysis));
        }

        let mode_average = mode_total / test_scenarios.len() as f32;
        mode_scores.insert(mode_name.to_string(), mode_average);
        all_results.push((mode_name.to_string(), scenario_results));

        println!("  📈 Mode Average: {:.1}/100\n", mode_average);
    }

    // Frequency response testing
    println!("🎼 Frequency Response Analysis");
    println!("===============================");

    test_frequency_response(&mut tester, &mut audio_gen, frequency_bands);

    // Generate comprehensive report
    generate_comprehensive_report(&all_results, &mode_scores);

    println!("\n✅ Comprehensive visual testing complete!");
    println!("📁 Detailed analysis saved to test_outputs/");
}

fn generate_bass_heavy_spectrum(bands: usize) -> Vec<f32> {
    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;
            if freq_ratio < 0.1 {
                0.9 + 0.1 * (freq_ratio * 20.0).sin() // Strong bass
            } else if freq_ratio < 0.3 {
                0.6 * (1.0 - (freq_ratio - 0.1) * 3.0) // Tapering off
            } else {
                0.1 + 0.1 * (freq_ratio * 10.0).sin() // Minimal higher frequencies
            }
        })
        .collect()
}

fn generate_mid_range_spectrum(bands: usize) -> Vec<f32> {
    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;
            if freq_ratio > 0.2 && freq_ratio < 0.7 {
                0.8 + 0.2 * ((freq_ratio - 0.45) * 8.0).sin() // Strong mids
            } else {
                0.2 + 0.1 * (freq_ratio * 5.0).sin() // Minimal bass/treble
            }
        })
        .collect()
}

fn generate_treble_spectrum(bands: usize) -> Vec<f32> {
    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;
            if freq_ratio > 0.6 {
                0.8 + 0.2 * ((1.0 - freq_ratio) * 10.0).sin() // Strong treble
            } else {
                0.1 + 0.05 * (freq_ratio * 3.0).sin() // Minimal lower frequencies
            }
        })
        .collect()
}

fn generate_rock_spectrum(bands: usize) -> Vec<f32> {
    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;
            // Rock: Strong bass, present mids, moderate highs
            if freq_ratio < 0.15 {
                0.85 + 0.15 * (freq_ratio * 12.0).sin() // Powerful bass
            } else if freq_ratio < 0.4 {
                0.7 + 0.2 * ((freq_ratio - 0.2) * 6.0).sin() // Guitar/vocal range
            } else if freq_ratio < 0.7 {
                0.5 + 0.3 * ((freq_ratio - 0.5) * 4.0).sin() // Guitar harmonics
            } else {
                0.3 + 0.2 * (freq_ratio * 8.0).sin() // Cymbals/effects
            }
        })
        .collect()
}

fn generate_edm_spectrum(bands: usize) -> Vec<f32> {
    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;
            // EDM: Massive sub-bass, scooped mids, bright highs
            if freq_ratio < 0.08 {
                1.0 // Massive sub-bass
            } else if freq_ratio < 0.25 {
                0.8 + 0.2 * ((freq_ratio - 0.15) * 15.0).sin() // Bass drops
            } else if freq_ratio < 0.6 {
                0.3 + 0.1 * (freq_ratio * 3.0).sin() // Scooped mids
            } else {
                0.7 + 0.3 * ((freq_ratio - 0.6) * 8.0).sin() // Bright synths
            }
        })
        .collect()
}

fn generate_classical_spectrum(bands: usize) -> Vec<f32> {
    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;
            // Classical: Natural frequency distribution with rich harmonics
            let fundamental = 0.4 + 0.3 * (freq_ratio * 4.0).sin();
            let harmonics = 0.1 * (freq_ratio * 8.0).sin() + 0.05 * (freq_ratio * 16.0).sin();
            (fundamental + harmonics).min(1.0).max(0.0)
        })
        .collect()
}

fn generate_vocal_spectrum(bands: usize) -> Vec<f32> {
    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;
            // Vocal: Focus on 85Hz-255Hz fundamental, 2-5kHz formants
            if freq_ratio > 0.02 && freq_ratio < 0.06 {
                0.8 + 0.2 * (freq_ratio * 50.0).sin() // Vocal fundamental
            } else if freq_ratio > 0.1 && freq_ratio < 0.25 {
                0.9 + 0.1 * ((freq_ratio - 0.175) * 20.0).sin() // Formant region
            } else if freq_ratio > 0.4 && freq_ratio < 0.6 {
                0.6 + 0.2 * (freq_ratio * 10.0).sin() // Higher formants
            } else {
                0.2 + 0.1 * (freq_ratio * 3.0).sin() // Background
            }
        })
        .collect()
}

fn generate_percussion_spectrum(bands: usize) -> Vec<f32> {
    use rand::{thread_rng, Rng};
    let mut rng = thread_rng();

    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;
            // Percussion: Sharp transients across spectrum with emphasis on attack
            let base_energy = if freq_ratio < 0.1 {
                0.9 // Kick drum fundamentals
            } else if freq_ratio < 0.3 {
                0.7 // Snare/tom body
            } else if freq_ratio > 0.7 {
                0.8 // Cymbals/hi-hats
            } else {
                0.4 // Mid-range transients
            };

            // Add randomness for transient character
            let transient = rng.gen::<f32>() * 0.3;
            (base_energy + transient).min(1.0)
        })
        .collect()
}

fn test_frequency_response(
    tester: &mut VisualTester,
    _audio_gen: &mut AudioGenerator,
    bands: usize,
) {
    println!("🎯 Testing individual frequency responses:");

    // Test specific frequency ranges
    let frequency_tests = vec![
        (
            "Sub-Bass (20-60Hz)",
            generate_frequency_range_test(0.0, 0.03, bands),
        ),
        (
            "Bass (60-250Hz)",
            generate_frequency_range_test(0.03, 0.12, bands),
        ),
        (
            "Low-Mid (250Hz-1kHz)",
            generate_frequency_range_test(0.12, 0.3, bands),
        ),
        (
            "Mid (1-4kHz)",
            generate_frequency_range_test(0.3, 0.6, bands),
        ),
        (
            "High-Mid (4-8kHz)",
            generate_frequency_range_test(0.6, 0.8, bands),
        ),
        (
            "High (8-20kHz)",
            generate_frequency_range_test(0.8, 1.0, bands),
        ),
    ];

    for (range_name, magnitudes) in frequency_tests {
        let analysis = tester.test_visualization(&magnitudes, 0); // Test with bars
        println!(
            "  {}: {:.1}/100 (Fill: {:.1}%)",
            range_name, analysis.visual_quality_score, analysis.filled_percentage
        );
    }
}

fn generate_frequency_range_test(start_ratio: f32, end_ratio: f32, bands: usize) -> Vec<f32> {
    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;
            if freq_ratio >= start_ratio && freq_ratio <= end_ratio {
                0.8 + 0.2 * ((freq_ratio - start_ratio) * 20.0).sin()
            } else {
                0.05 // Minimal energy outside range
            }
        })
        .collect()
}

fn generate_comprehensive_report(
    results: &[(String, Vec<(String, speedy::testing::VisualAnalysis)>)],
    mode_scores: &std::collections::HashMap<String, f32>,
) {
    let mut report = String::new();

    report.push_str("# Speedy v1.0 Comprehensive Visual Analysis Report\n\n");

    // Overall scores
    report.push_str("## Overall Mode Performance\n\n");
    let mut sorted_modes: Vec<_> = mode_scores.iter().collect();
    sorted_modes.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());

    for (mode, score) in sorted_modes {
        let status = if *score >= 90.0 {
            "🟢 Excellent"
        } else if *score >= 70.0 {
            "🟡 Good"
        } else if *score >= 50.0 {
            "🟠 Needs Work"
        } else {
            "🔴 Poor"
        };
        report.push_str(&format!("- **{}**: {:.1}/100 {}\n", mode, score, status));
    }

    report.push_str("\n## Detailed Analysis by Mode\n\n");

    for (mode_name, scenarios) in results {
        report.push_str(&format!("### {} Mode\n\n", mode_name));

        for (scenario, analysis) in scenarios {
            report.push_str(&format!("#### {} Scenario\n", scenario));
            report.push_str(&format!(
                "- Quality Score: {:.1}/100\n",
                analysis.visual_quality_score
            ));
            report.push_str(&format!(
                "- Fill Percentage: {:.1}%\n",
                analysis.filled_percentage
            ));
            report.push_str(&format!("- Color Variety: {}\n", analysis.color_variety));
            report.push_str(&format!("- Has Gradients: {}\n", analysis.has_gradients));

            if !analysis.issues.is_empty() {
                report.push_str("- Issues:\n");
                for issue in &analysis.issues {
                    report.push_str(&format!("  - ❌ {}\n", issue));
                }
            }

            if !analysis.recommendations.is_empty() {
                report.push_str("- Recommendations:\n");
                for rec in &analysis.recommendations {
                    report.push_str(&format!("  - 💡 {}\n", rec));
                }
            }
            report.push_str("\n");
        }
    }

    // Save report
    if let Err(e) = fs::create_dir_all("test_outputs") {
        eprintln!("Warning: Could not create test_outputs directory: {}", e);
        return;
    }

    if let Err(e) = fs::write("test_outputs/comprehensive_analysis.md", report) {
        eprintln!("Warning: Could not save comprehensive report: {}", e);
    } else {
        println!("📊 Comprehensive analysis saved to test_outputs/comprehensive_analysis.md");
    }
}
