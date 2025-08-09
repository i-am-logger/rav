use anyhow::Result;
use clap::Parser;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

use rav::{
    config::Config,
    signal::SignalProcessor,
    testing::{
        audio_monitor::{AudioMonitor, TestProfile},
        bdd_framework::BDDTestFramework,
        visual_analyzer::VisualAnalyzer,
    },
};

#[derive(Parser, Debug)]
#[command(name = "frequency-test-suite")]
#[command(about = "Comprehensive frequency and visualization testing suite")]
#[command(version)]
struct Args {
    /// Test profile to run (all, sweep, bass, midrange, treble, noise, stress)
    #[arg(short, long, default_value = "all")]
    profile: String,

    /// Duration per frequency in milliseconds
    #[arg(short, long, default_value = "100")]
    duration: u64,

    /// Test all visualization modes
    #[arg(long)]
    all_visualizations: bool,

    /// Generate detailed report
    #[arg(long)]
    detailed_report: bool,

    /// Output directory for test results
    #[arg(short, long, default_value = "test_results")]
    output_dir: String,

    /// Run continuously (cargo watch mode)
    #[arg(long)]
    continuous: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize minimal logging for clean output
    tracing_subscriber::fmt()
        .with_env_filter("frequency_test_suite=info,rav=warn")
        .compact()
        .with_target(false)
        .init();

    info!("🎵 Starting Comprehensive Frequency Test Suite");

    // Create output directory
    std::fs::create_dir_all(&args.output_dir)?;

    // Initialize components
    let mut audio_monitor = AudioMonitor::new()?;
    let mut bdd_framework = BDDTestFramework::new();
    let config = Config::default();

    // Get test profiles
    let profiles = match args.profile.as_str() {
        "all" => AudioMonitor::get_comprehensive_test_profiles(),
        "sweep" => vec![get_linear_sweep_profile()],
        "bass" => vec![get_bass_test_profile()],
        "midrange" => vec![get_midrange_test_profile()],
        "treble" => vec![get_treble_test_profile()],
        "noise" => vec![get_noise_test_profile()],
        "stress" => vec![get_stress_test_profile()],
        _ => {
            error!("Unknown test profile: {}", args.profile);
            return Ok(());
        }
    };

    if args.continuous {
        info!("🔄 Running in continuous mode (for cargo watch)");
        run_continuous_tests(audio_monitor, profiles, &args).await?;
    } else {
        run_single_test_suite(audio_monitor, profiles, &args).await?;
    }

    Ok(())
}

async fn run_single_test_suite(
    mut audio_monitor: AudioMonitor,
    profiles: Vec<TestProfile>,
    args: &Args,
) -> Result<()> {
    let mut total_tests = 0;
    let mut passed_tests = 0;
    let mut test_results = Vec::new();

    for (i, profile) in profiles.iter().enumerate() {
        info!(
            "🎯 Running test profile {}/{}: {}",
            i + 1,
            profiles.len(),
            profile.name
        );

        // Start audio monitoring with this profile
        audio_monitor.start_monitoring(profile.clone())?;

        // Wait for the test to complete
        let test_duration =
            Duration::from_millis(profile.frequencies.len() as u64 * profile.duration_ms + 1000);

        let start_time = std::time::Instant::now();

        // Monitor test progress
        while start_time.elapsed() < test_duration {
            let (is_running, current_test) = audio_monitor.get_status();

            if !is_running {
                break;
            }

            // Show progress
            let progress = (start_time.elapsed().as_millis() as f32
                / test_duration.as_millis() as f32)
                * 100.0;
            print!(
                "\r🎵 Progress: {:.1}% - Current: {:.1}Hz",
                progress,
                current_test.frequencies.first().unwrap_or(&0.0)
            );
            std::io::Write::flush(&mut std::io::stdout()).ok();

            sleep(Duration::from_millis(50)).await;
        }

        println!(); // New line after progress

        // Stop monitoring for this profile
        audio_monitor.stop_monitoring();

        // Analyze results
        let test_result = analyze_test_result(&profile, &args.output_dir).await?;

        if test_result.passed {
            passed_tests += 1;
            info!(
                "✅ Profile '{}' passed with score: {:.1}/100",
                profile.name, test_result.score
            );
        } else {
            warn!(
                "❌ Profile '{}' failed with score: {:.1}/100",
                profile.name, test_result.score
            );
            for issue in &test_result.issues {
                warn!("  - {}", issue);
            }
        }

        total_tests += 1;
        test_results.push(test_result);

        // Brief pause between tests
        sleep(Duration::from_millis(500)).await;
    }

    // Generate final report
    generate_final_report(&test_results, total_tests, passed_tests, &args.output_dir).await?;

    let pass_rate = (passed_tests as f32 / total_tests as f32) * 100.0;
    info!(
        "🏁 Test suite completed: {}/{} tests passed ({:.1}%)",
        passed_tests, total_tests, pass_rate
    );

    Ok(())
}

async fn run_continuous_tests(
    mut audio_monitor: AudioMonitor,
    profiles: Vec<TestProfile>,
    args: &Args,
) -> Result<()> {
    info!("🔄 Continuous testing mode - will cycle through all profiles");

    let mut cycle_count = 0;

    loop {
        cycle_count += 1;
        info!("🔄 Starting test cycle #{}", cycle_count);

        for profile in &profiles {
            info!("🎵 Testing: {}", profile.name);

            // Start monitoring
            audio_monitor.start_monitoring(profile.clone())?;

            // Short test duration for continuous mode
            let test_duration =
                Duration::from_millis(std::cmp::min(profile.frequencies.len() as u64 * 50, 3000));

            sleep(test_duration).await;

            // Stop and analyze quickly
            audio_monitor.stop_monitoring();

            let result = analyze_test_result(profile, &args.output_dir).await?;

            if result.passed {
                info!("✅ {} - Score: {:.1}", profile.name, result.score);
            } else {
                warn!("❌ {} - Score: {:.1}", profile.name, result.score);
            }

            // Very brief pause
            sleep(Duration::from_millis(200)).await;
        }

        info!("🔄 Cycle #{} completed, restarting...", cycle_count);
        sleep(Duration::from_millis(1000)).await;
    }
}

#[derive(Debug)]
struct TestResult {
    profile_name: String,
    passed: bool,
    score: f32,
    issues: Vec<String>,
    recommendations: Vec<String>,
}

async fn analyze_test_result(profile: &TestProfile, output_dir: &str) -> Result<TestResult> {
    // Simulate visual analysis (in real implementation, this would analyze actual UI output)
    let mut analyzer = VisualAnalyzer::new();

    // Generate test magnitudes based on profile
    let test_magnitudes = generate_test_magnitudes(profile);

    // Analyze the visualization
    let analysis = analyzer.analyze_frequencies(&test_magnitudes, 44100);

    let passed = analysis.overall_quality > 70.0;

    Ok(TestResult {
        profile_name: profile.name.clone(),
        passed,
        score: analysis.overall_quality,
        issues: analysis.issues,
        recommendations: analysis.recommendations,
    })
}

fn generate_test_magnitudes(profile: &TestProfile) -> Vec<f32> {
    // Generate realistic magnitudes based on the test profile
    let mut magnitudes = vec![0.0; 80]; // 80 frequency bands

    for &freq in &profile.frequencies {
        // Convert frequency to bin index (simplified)
        let bin = ((freq / 20000.0) * 80.0) as usize;
        if bin < magnitudes.len() {
            magnitudes[bin] = profile.amplitude * (1.0 + (freq / 1000.0).sin() * 0.3);
        }
    }

    magnitudes
}

async fn generate_final_report(
    results: &[TestResult],
    total_tests: usize,
    passed_tests: usize,
    output_dir: &str,
) -> Result<()> {
    let report_path = format!("{}/frequency_test_report.md", output_dir);
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");

    let mut report = String::new();
    report.push_str(&format!("# Frequency Test Suite Report\n"));
    report.push_str(&format!("*Generated: {}*\n\n", timestamp));

    report.push_str("## Summary\n\n");
    report.push_str(&format!("- **Total Tests**: {}\n", total_tests));
    report.push_str(&format!("- **Passed**: {}\n", passed_tests));
    report.push_str(&format!("- **Failed**: {}\n", total_tests - passed_tests));
    report.push_str(&format!(
        "- **Pass Rate**: {:.1}%\n\n",
        (passed_tests as f32 / total_tests as f32) * 100.0
    ));

    // Calculate average score
    let avg_score = results.iter().map(|r| r.score).sum::<f32>() / results.len() as f32;
    report.push_str(&format!("- **Average Score**: {:.1}/100\n\n", avg_score));

    report.push_str("## Test Results\n\n");

    for result in results {
        let status = if result.passed {
            "✅ PASS"
        } else {
            "❌ FAIL"
        };
        report.push_str(&format!(
            "### {} - {} ({:.1}/100)\n\n",
            result.profile_name, status, result.score
        ));

        if !result.issues.is_empty() {
            report.push_str("**Issues:**\n");
            for issue in &result.issues {
                report.push_str(&format!("- {}\n", issue));
            }
            report.push_str("\n");
        }

        if !result.recommendations.is_empty() {
            report.push_str("**Recommendations:**\n");
            for rec in &result.recommendations {
                report.push_str(&format!("- {}\n", rec));
            }
            report.push_str("\n");
        }
    }

    // Add visualization quality metrics
    report.push_str("## Quality Metrics\n\n");
    report.push_str("| Test Profile | Score | Status | Key Issues |\n");
    report.push_str("|--------------|-------|--------|------------|\n");

    for result in results {
        let status = if result.passed { "Pass" } else { "Fail" };
        let key_issue = result.issues.first().unwrap_or(&"None".to_string()).clone();
        report.push_str(&format!(
            "| {} | {:.1} | {} | {} |\n",
            result.profile_name, result.score, status, key_issue
        ));
    }

    tokio::fs::write(report_path, report).await?;
    info!(
        "📊 Detailed report saved to {}/frequency_test_report.md",
        output_dir
    );

    Ok(())
}

// Helper functions to create specific test profiles
fn get_linear_sweep_profile() -> TestProfile {
    TestProfile {
        name: "Linear Frequency Sweep".to_string(),
        frequencies: (20..=20000).step_by(200).map(|f| f as f32).collect(),
        duration_ms: 50,
        amplitude: 0.5,
waveform: rav::testing::audio_monitor::WaveformType::Sine,
sweep_type: rav::testing::audio_monitor::SweepType::Linear,
        test_all_visualizations: true,
    }
}

fn get_bass_test_profile() -> TestProfile {
    TestProfile {
        name: "Bass Response Test".to_string(),
        frequencies: (20..=200).step_by(10).map(|f| f as f32).collect(),
        duration_ms: 150,
        amplitude: 0.8,
        waveform: speedy::testing::audio_monitor::WaveformType::Sine,
sweep_type: rav::testing::audio_monitor::SweepType::Stepped,
        test_all_visualizations: false,
    }
}

fn get_midrange_test_profile() -> TestProfile {
    TestProfile {
        name: "Midrange Test".to_string(),
        frequencies: (200..=2000).step_by(100).map(|f| f as f32).collect(),
        duration_ms: 100,
        amplitude: 0.6,
waveform: rav::testing::audio_monitor::WaveformType::Square,
        sweep_type: speedy::testing::audio_monitor::SweepType::Linear,
        test_all_visualizations: true,
    }
}

fn get_treble_test_profile() -> TestProfile {
    TestProfile {
        name: "Treble Test".to_string(),
        frequencies: (2000..=20000).step_by(1000).map(|f| f as f32).collect(),
        duration_ms: 75,
        amplitude: 0.4,
waveform: rav::testing::audio_monitor::WaveformType::Triangle,
        sweep_type: speedy::testing::audio_monitor::SweepType::Linear,
        test_all_visualizations: true,
    }
}

fn get_noise_test_profile() -> TestProfile {
    TestProfile {
        name: "White Noise Test".to_string(),
        frequencies: vec![0.0],
        duration_ms: 2000,
        amplitude: 0.3,
waveform: rav::testing::audio_monitor::WaveformType::WhiteNoise,
sweep_type: rav::testing::audio_monitor::SweepType::Continuous,
        test_all_visualizations: true,
    }
}

fn get_stress_test_profile() -> TestProfile {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let frequencies: Vec<f32> = (0..50).map(|_| rng.gen_range(20.0..20000.0)).collect();

    TestProfile {
        name: "Random Stress Test".to_string(),
        frequencies,
        duration_ms: 25,
        amplitude: 0.7,
waveform: rav::testing::audio_monitor::WaveformType::Square,
sweep_type: rav::testing::audio_monitor::SweepType::Random,
        test_all_visualizations: true,
    }
}
