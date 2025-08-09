// Visual test runner for Speedy audio visualizer
// Run with: cargo run --bin visual_test

use rav::{
    config::Config,
    testing::{audio_generator::AudioGenerator, VisualTester},
};
use std::fs;

fn main() {
    println!("🎵 Speedy Visual Testing Framework");
    println!("==================================\n");

    // Test parameters
    let test_width = 120;
    let test_height = 40;
    let frequency_bands = 32;

    // Create visual tester
    let mut tester = VisualTester::new(test_width, test_height);

    println!("📊 Running visual tests...\n");

    // Test 1: Typical music spectrum
    println!("Test 1: Typical Music Spectrum");
    println!("------------------------------");
    let typical_mags = AudioGenerator::test_spectrum_magnitudes(frequency_bands);
    let analysis1 = tester.test_visualization(&typical_mags, 0); // Bars tab

    println!("{}", analysis1.report());
    println!("Visual Output:");
    println!("{}", tester.capture_with_colors());
    println!();

    // Test 2: Quiet scene
    println!("Test 2: Quiet Scene");
    println!("-------------------");
    let quiet_mags = AudioGenerator::quiet_spectrum_magnitudes(frequency_bands);
    let analysis2 = tester.test_visualization(&quiet_mags, 0);

    println!("{}", analysis2.report());
    println!();

    // Test 3: Dynamic/loud scene
    println!("Test 3: Dynamic Scene");
    println!("--------------------");
    let dynamic_mags = AudioGenerator::dynamic_spectrum_magnitudes(frequency_bands);
    let analysis3 = tester.test_visualization(&dynamic_mags, 0);

    println!("{}", analysis3.report());
    println!();

    // Test 4: Wave visualization
    println!("Test 4: Wave Visualization");
    println!("-------------------------");
    let analysis4 = tester.test_visualization(&typical_mags, 1); // Wave tab

    println!("{}", analysis4.report());
    println!();

    // Test 5: Spectrum visualization
    println!("Test 5: Spectrum Visualization");
    println!("------------------------------");
    let analysis5 = tester.test_visualization(&typical_mags, 2); // Spectrum tab

    println!("{}", analysis5.report());
    println!();

    // Generate comprehensive report
    println!("📈 COMPREHENSIVE ANALYSIS");
    println!("==========================\n");

    let analyses = vec![
        ("Bars - Typical", &analysis1),
        ("Bars - Quiet", &analysis2),
        ("Bars - Dynamic", &analysis3),
        ("Wave - Typical", &analysis4),
        ("Spectrum - Typical", &analysis5),
    ];

    // Overall scores
    println!("Quality Scores:");
    for (name, analysis) in &analyses {
        println!("  {}: {:.1}/100", name, analysis.visual_quality_score);
    }
    println!();

    // Common issues across all tests
    let mut all_issues = std::collections::HashMap::new();
    let mut all_recommendations = std::collections::HashMap::new();

    for (_, analysis) in &analyses {
        for issue in &analysis.issues {
            *all_issues.entry(issue.clone()).or_insert(0) += 1;
        }
        for rec in &analysis.recommendations {
            *all_recommendations.entry(rec.clone()).or_insert(0) += 1;
        }
    }

    println!("Most Common Issues:");
    let mut sorted_issues: Vec<_> = all_issues.iter().collect();
    sorted_issues.sort_by(|a, b| b.1.cmp(a.1));
    for (issue, count) in sorted_issues.iter().take(3) {
        println!("  ❌ {} (appears in {} tests)", issue, count);
    }
    println!();

    println!("Top Recommendations:");
    let mut sorted_recs: Vec<_> = all_recommendations.iter().collect();
    sorted_recs.sort_by(|a, b| b.1.cmp(a.1));
    for (rec, count) in sorted_recs.iter().take(3) {
        println!("  💡 {} (suggested {} times)", rec, count);
    }
    println!();

    // Save detailed outputs to files
    save_test_results(&analyses, &tester);

    println!("✅ Visual testing complete!");
    println!("📁 Detailed results saved to test_outputs/");
}

fn save_test_results(analyses: &[(&str, &rav::testing::VisualAnalysis)], tester: &VisualTester) {
    // Create output directory
    if let Err(e) = fs::create_dir_all("test_outputs") {
        eprintln!("Warning: Could not create test_outputs directory: {}", e);
        return;
    }

    // Save summary report
    let mut summary = String::new();
    summary.push_str("# Speedy Visual Test Results\n\n");

    for (name, analysis) in analyses {
        summary.push_str(&format!("## {}\n\n", name));
        summary.push_str(&analysis.report());
        summary.push_str("\n");
    }

    if let Err(e) = fs::write("test_outputs/summary.md", summary) {
        eprintln!("Warning: Could not save summary report: {}", e);
    }

    // Save visual captures
    let visual_output = tester.capture_with_colors();
    if let Err(e) = fs::write("test_outputs/latest_visual.txt", visual_output) {
        eprintln!("Warning: Could not save visual capture: {}", e);
    }

    // Save raw text output
    let text_output = tester.capture_as_text();
    if let Err(e) = fs::write("test_outputs/latest_raw.txt", text_output) {
        eprintln!("Warning: Could not save raw text capture: {}", e);
    }

    println!("💾 Test results saved successfully!");
}
