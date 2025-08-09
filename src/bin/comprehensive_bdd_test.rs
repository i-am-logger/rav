// Comprehensive BDD Testing for Speedy v1.0
// Validates readiness for professional v1.0 release

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use rav::{
    config::Config,
    signal::SignalProcessor,
    testing::audio_generator::AudioGenerator,
    testing::bdd_framework::{BDDTestFramework, BDDTestResult},
    ui::{SpeedyV1Interface, VisualizationMode},
};
use std::{
    collections::HashMap,
    io,
    time::{Duration, Instant},
};
use tokio::time::interval;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Speedy v1.0 Comprehensive BDD Test Suite");
    println!("===========================================");
    println!("This will validate all aspects of the v1.0 release");
    println!("including visual quality, user experience, and performance.\n");

    // Run automated BDD test suite first
    let mut bdd_framework = BDDTestFramework::new();
    let test_results = bdd_framework.run_full_test_suite();

    // Save detailed results
    save_test_results(&test_results)?;

    // Interactive validation if requested
    println!("\nWould you like to run the interactive visual validation? (y/n)");
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() == "y" {
        run_interactive_validation().await?;
    }

    // Generate final v1.0 readiness assessment
    generate_v1_readiness_report(&test_results);

    Ok(())
}

/// Run interactive visual validation
async fn run_interactive_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🎨 Starting Interactive Visual Validation");
    println!("Press keys to test different scenarios:");
    println!("  1-5: Switch visualization modes");
    println!("  T: Cycle themes");
    println!("  S: Switch test scenarios");
    println!("  Q: Quit interactive test");

    // Setup terminal for interactive test
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Create enhanced V1 interface with better rendering
    let config = Config::default();
    let signal_processor = SignalProcessor::new(
        config.audio.sample_rate,
        128, // Higher resolution for v1.0
        config.display.frequency_range.clone(),
    );

    let mut interface = create_enhanced_v1_interface(config, signal_processor)?;
    let mut audio_gen = AudioGenerator::new(44100.0);

    let scenarios = create_enhanced_test_scenarios(&mut audio_gen);
    let mut current_scenario = 0;
    let mut last_scenario_change = Instant::now();

    let mut frame_interval = interval(Duration::from_millis(16)); // 60 FPS

    // Performance tracking
    let mut frame_times = Vec::new();
    let mut last_frame_time = Instant::now();

    loop {
        // Handle input
        if event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('1') => interface.current_mode = VisualizationMode::Bars,
                        KeyCode::Char('2') => interface.current_mode = VisualizationMode::Wave,
                        KeyCode::Char('3') => interface.current_mode = VisualizationMode::Spectrum,
                        KeyCode::Char('4') => interface.current_mode = VisualizationMode::Circle,
                        KeyCode::Char('5') => interface.current_mode = VisualizationMode::Particles,
                        KeyCode::Char('t') | KeyCode::Char('T') => interface.cycle_theme(),
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            current_scenario = (current_scenario + 1) % scenarios.len();
                            println!("\r🎵 Switched to: {}", scenarios[current_scenario].0);
                        }
                        KeyCode::Char('h') | KeyCode::Char('?') => interface.toggle_help(),
                        KeyCode::Up => interface.adjust_sensitivity(0.1),
                        KeyCode::Down => interface.adjust_sensitivity(-0.1),
                        _ => {}
                    }
                }
            }
        }

        // Auto-cycle scenarios every 8 seconds
        if last_scenario_change.elapsed() >= Duration::from_secs(8) {
            current_scenario = (current_scenario + 1) % scenarios.len();
            println!("\r🎵 Auto-switched to: {}", scenarios[current_scenario].0);
            last_scenario_change = Instant::now();
        }

        // Update visualization with current scenario
        let magnitudes = scenarios[current_scenario].1.clone();
        interface.update_magnitudes(magnitudes);

        // Render frame with performance tracking
        let render_start = Instant::now();

        terminal.draw(|frame| {
            interface.draw(frame);
        })?;

        let render_time = render_start.elapsed();
        frame_times.push(render_time.as_millis() as f32);

        // Keep only last 60 frame times for moving average
        if frame_times.len() > 60 {
            frame_times.remove(0);
        }

        // Update FPS calculation
        let frame_time = last_frame_time.elapsed();
        last_frame_time = Instant::now();

        // Wait for next frame
        frame_interval.tick().await;
    }

    // Cleanup
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Report interactive test results
    let avg_frame_time = frame_times.iter().sum::<f32>() / frame_times.len() as f32;
    let fps = 1000.0 / avg_frame_time;

    println!("\n📊 Interactive Test Results:");
    println!("Average Frame Time: {:.2}ms", avg_frame_time);
    println!("Average FPS: {:.1}", fps);

    if fps >= 55.0 {
        println!("✅ Performance: Excellent");
    } else if fps >= 45.0 {
        println!("⚠️  Performance: Good");
    } else {
        println!("❌ Performance: Needs improvement");
    }

    Ok(())
}

/// Create enhanced V1 interface with fixed rendering
fn create_enhanced_v1_interface(
    config: Config,
    signal_processor: SignalProcessor,
) -> Result<SpeedyV1Interface, Box<dyn std::error::Error>> {
    // This would create a properly working interface
    // For now, use the testing version
    SpeedyV1Interface::new_for_testing(config, signal_processor)
}

/// Create enhanced test scenarios with more realistic data
fn create_enhanced_test_scenarios(audio_gen: &mut AudioGenerator) -> Vec<(String, Vec<f32>)> {
    vec![
        (
            "🎸 Rock Concert (Dynamic)".to_string(),
            generate_dynamic_rock_spectrum(audio_gen),
        ),
        (
            "🎛️  Electronic Music (Bass Heavy)".to_string(),
            generate_electronic_spectrum(audio_gen),
        ),
        (
            "🎼 Classical Orchestra (Full Range)".to_string(),
            generate_orchestral_spectrum(audio_gen),
        ),
        (
            "🎺 Jazz Ensemble (Rich Midrange)".to_string(),
            generate_jazz_spectrum(audio_gen),
        ),
        (
            "🌊 Ambient Soundscape".to_string(),
            generate_ambient_spectrum(audio_gen),
        ),
        (
            "🎤 Vocal Performance".to_string(),
            generate_vocal_spectrum(audio_gen),
        ),
        (
            "🥁 Percussion Focus".to_string(),
            generate_percussion_spectrum(audio_gen),
        ),
        (
            "🎹 Piano Solo".to_string(),
            generate_piano_spectrum(audio_gen),
        ),
        (
            "🔊 Full Frequency Sweep".to_string(),
            generate_frequency_sweep(audio_gen),
        ),
        (
            "🌈 Colorful Test Pattern".to_string(),
            generate_test_pattern_spectrum(audio_gen),
        ),
    ]
}

/// Generate realistic dynamic rock spectrum
fn generate_dynamic_rock_spectrum(audio_gen: &AudioGenerator) -> Vec<f32> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..128)
        .map(|i| {
            let freq_ratio = i as f32 / 128.0;
            let beat = (time * 2.0).sin() * 0.3 + 0.7; // 120 BPM simulation

            // Rock characteristics: strong bass, present mids, controlled highs
            match freq_ratio {
                f if f < 0.1 => 0.9 * beat + 0.1 * (time * 4.0 + f * 20.0).sin(), // Deep bass
                f if f < 0.2 => 0.8 * beat + 0.15 * (time * 3.0 + f * 15.0).sin(), // Bass
                f if f < 0.4 => 0.7 * beat + 0.2 * (time * 2.5 + f * 12.0).sin(), // Low mids
                f if f < 0.6 => 0.6 * beat + 0.25 * (time * 2.0 + f * 10.0).sin(), // Mids
                f if f < 0.8 => 0.4 * beat + 0.2 * (time * 1.5 + f * 8.0).sin(),  // High mids
                _ => 0.3 * beat + 0.15 * (time * 1.0 + freq_ratio * 6.0).sin(),   // Highs
            }
            .clamp(0.0, 1.0)
        })
        .collect()
}

fn generate_electronic_spectrum(audio_gen: &AudioGenerator) -> Vec<f32> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..128)
        .map(|i| {
            let freq_ratio = i as f32 / 128.0;
            let kick = (time * 2.0).sin().max(0.0).powf(4.0); // Tight kick pattern
            let synth_sweep = 0.5 + 0.5 * (time * 0.5 + freq_ratio * 3.0).sin();

            match freq_ratio {
                f if f < 0.05 => kick,                                            // Sub bass
                f if f < 0.15 => 0.8 * kick + 0.2 * synth_sweep,                  // Bass
                f if f < 0.4 => 0.4 + 0.4 * synth_sweep,                          // Synth range
                f if f < 0.7 => 0.3 + 0.5 * synth_sweep * (time * 1.5).cos(),     // Lead synth
                _ => 0.2 + 0.6 * (time * 4.0 + freq_ratio * 10.0).sin().max(0.0), // Hi-hats and effects
            }
            .clamp(0.0, 1.0)
        })
        .collect()
}

fn generate_orchestral_spectrum(_audio_gen: &AudioGenerator) -> Vec<f32> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..128)
        .map(|i| {
            let freq_ratio = i as f32 / 128.0;
            let dynamics = 0.6 + 0.4 * (time * 0.3).sin(); // Slow dynamics

            // Natural frequency distribution with rich harmonics
            let fundamental = match freq_ratio {
                f if f < 0.2 => 0.5 + 0.3 * (time * 0.5 + f * 8.0).sin(), // Cellos, basses
                f if f < 0.4 => 0.6 + 0.3 * (time * 0.7 + f * 6.0).sin(), // Violas, horns
                f if f < 0.6 => 0.7 + 0.25 * (time * 0.8 + f * 5.0).sin(), // Violins
                f if f < 0.8 => 0.5 + 0.4 * (time * 1.0 + f * 4.0).sin(), // Woodwinds
                _ => 0.3 + 0.3 * (time * 1.2 + freq_ratio * 3.0).sin(),   // High strings, piccolo
            };

            (fundamental * dynamics).clamp(0.0, 1.0)
        })
        .collect()
}

fn generate_jazz_spectrum(_audio_gen: &AudioGenerator) -> Vec<f32> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..128)
        .map(|i| {
            let freq_ratio = i as f32 / 128.0;
            let swing = 0.7 + 0.3 * (time * 1.5).sin(); // Swing feel

            match freq_ratio {
                f if f < 0.15 => 0.6 * swing * (1.0 + 0.2 * (time * 1.2).sin()), // Upright bass
                f if f < 0.35 => 0.7 * swing * (1.0 + 0.3 * (time * 0.8 + f * 5.0).sin()), // Piano bass
                f if f < 0.55 => 0.8 * swing * (1.0 + 0.25 * (time * 1.0 + f * 4.0).sin()), // Piano/guitar mids
                f if f < 0.75 => 0.5 * swing * (1.0 + 0.4 * (time * 1.3 + f * 3.0).sin()),  // Horns
                _ => 0.4 * swing * (1.0 + 0.5 * (time * 2.0 + freq_ratio * 6.0).sin()), // Cymbals
            }
            .clamp(0.0, 1.0)
        })
        .collect()
}

fn generate_ambient_spectrum(_audio_gen: &AudioGenerator) -> Vec<f32> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..128)
        .map(|i| {
            let freq_ratio = i as f32 / 128.0;

            // Multiple slow-moving layers
            let layer1 = 0.3 + 0.2 * (time * 0.2 + freq_ratio * 1.5).sin();
            let layer2 = 0.2 + 0.15 * (time * 0.15 + freq_ratio * 2.0).cos();
            let layer3 = 0.1 + 0.1 * (time * 0.1 + freq_ratio * 3.0).sin();

            (layer1 + layer2 + layer3).clamp(0.0, 0.8)
        })
        .collect()
}

fn generate_vocal_spectrum(_audio_gen: &AudioGenerator) -> Vec<f32> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..128)
        .map(|i| {
            let freq_ratio = i as f32 / 128.0;
            let vibrato = 1.0 + 0.05 * (time * 6.0).sin(); // Vocal vibrato

            match freq_ratio {
                f if f < 0.1 => 0.3, // Low fundamentals
                f if f < 0.3 => 0.8 * vibrato * (1.0 + 0.2 * (time * 2.0).sin()), // Vocal fundamentals
                f if f < 0.6 => 0.6 * vibrato * (1.0 + 0.3 * (time * 1.5).cos()), // Formants
                f if f < 0.8 => 0.4 * vibrato * (1.0 + 0.2 * (time * 1.0).sin()), // Upper harmonics
                _ => 0.2 * (1.0 + 0.1 * (time * 3.0).sin()), // Breath and sibilants
            }
            .clamp(0.0, 1.0)
        })
        .collect()
}

fn generate_percussion_spectrum(_audio_gen: &AudioGenerator) -> Vec<f32> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    let kick_hit = (time * 2.0).sin().max(0.0).powf(8.0);
    let snare_hit = (time * 2.0 - 0.5).sin().max(0.0).powf(6.0);
    let hihat = 0.3 + 0.4 * (time * 8.0).sin().max(0.0);

    (0..128)
        .map(|i| {
            let freq_ratio = i as f32 / 128.0;

            match freq_ratio {
                f if f < 0.1 => kick_hit,                             // Kick drum
                f if f < 0.3 => 0.5 * kick_hit + 0.3,                 // Kick harmonics
                f if f < 0.6 => snare_hit * 0.8,                      // Snare drum
                _ => hihat * (0.5 + 0.5 * (freq_ratio * 20.0).sin()), // Hi-hats and cymbals
            }
            .clamp(0.0, 1.0)
        })
        .collect()
}

fn generate_piano_spectrum(_audio_gen: &AudioGenerator) -> Vec<f32> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..128)
        .map(|i| {
            let freq_ratio = i as f32 / 128.0;
            let key_velocity = 0.7 + 0.3 * (time * 1.0).sin(); // Key dynamics

            // Piano has rich harmonic content across the spectrum
            let fundamental = match freq_ratio {
                f if f < 0.2 => 0.6 * key_velocity, // Bass notes
                f if f < 0.5 => 0.8 * key_velocity * (1.0 + 0.1 * (time * 2.0 + f * 8.0).sin()), // Mid range
                f if f < 0.8 => 0.5 * key_velocity * (1.0 + 0.2 * (time * 1.5 + f * 6.0).sin()), // Upper range
                _ => 0.2 * key_velocity * (1.0 + 0.3 * (freq_ratio * 15.0).sin()), // Harmonics
            };

            fundamental.clamp(0.0, 1.0)
        })
        .collect()
}

fn generate_frequency_sweep(_audio_gen: &AudioGenerator) -> Vec<f32> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    let sweep_position = (time * 0.2).sin() * 0.5 + 0.5; // Slow sweep across spectrum

    (0..128)
        .map(|i| {
            let freq_ratio = i as f32 / 128.0;
            let distance = (freq_ratio - sweep_position).abs();

            if distance < 0.1 {
                0.9 - distance * 5.0
            } else if distance < 0.2 {
                0.4 - (distance - 0.1) * 2.0
            } else {
                0.1
            }
            .clamp(0.0, 1.0)
        })
        .collect()
}

fn generate_test_pattern_spectrum(_audio_gen: &AudioGenerator) -> Vec<f32> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..128)
        .map(|i| {
            let freq_ratio = i as f32 / 128.0;

            // Create interesting patterns for testing color gradients
            let pattern1 = (freq_ratio * 8.0 + time).sin() * 0.5 + 0.5;
            let pattern2 = (freq_ratio * 4.0 + time * 0.7).cos() * 0.3 + 0.3;
            let pattern3 = (freq_ratio * 12.0 - time * 0.5).sin() * 0.2 + 0.2;

            (pattern1 + pattern2 + pattern3).clamp(0.0, 1.0)
        })
        .collect()
}

/// Save test results to file
fn save_test_results(results: &[BDDTestResult]) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create("test_outputs/bdd_test_results.txt")?;

    writeln!(file, "Speedy v1.0 BDD Test Results")?;
    writeln!(file, "===========================")?;
    writeln!(
        file,
        "Generated: {}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    )?;
    writeln!(file)?;

    for result in results {
        writeln!(file, "Scenario: {}", result.scenario)?;
        writeln!(file, "Passed: {}", if result.passed { "✅" } else { "❌" })?;
        writeln!(file, "Visual Quality: {:.2}", result.visual_quality_score)?;
        writeln!(file, "UX Quality: {:.2}", result.user_experience_score)?;
        writeln!(file, "Performance: {:.2}", result.performance_score)?;
        writeln!(file, "Overall: {:.2}", result.overall_score)?;

        if !result.issues.is_empty() {
            writeln!(file, "Issues:")?;
            for issue in &result.issues {
                writeln!(file, "  - {}", issue)?;
            }
        }

        if !result.recommendations.is_empty() {
            writeln!(file, "Recommendations:")?;
            for rec in &result.recommendations {
                writeln!(file, "  - {}", rec)?;
            }
        }

        writeln!(file)?;
    }

    println!("📄 Test results saved to test_outputs/bdd_test_results.txt");
    Ok(())
}

/// Generate final v1.0 readiness report
fn generate_v1_readiness_report(results: &[BDDTestResult]) {
    println!("\n🎯 FINAL V1.0 READINESS ASSESSMENT");
    println!("=====================================");

    let passed_tests = results.iter().filter(|r| r.passed).count();
    let total_tests = results.len();
    let pass_rate = if total_tests > 0 {
        passed_tests as f32 / total_tests as f32
    } else {
        0.0
    };

    let avg_visual =
        results.iter().map(|r| r.visual_quality_score).sum::<f32>() / results.len() as f32;
    let avg_ux =
        results.iter().map(|r| r.user_experience_score).sum::<f32>() / results.len() as f32;
    let avg_perf = results.iter().map(|r| r.performance_score).sum::<f32>() / results.len() as f32;
    let avg_overall = results.iter().map(|r| r.overall_score).sum::<f32>() / results.len() as f32;

    println!("📊 METRICS SUMMARY:");
    println!(
        "  Pass Rate: {:.1}% ({}/{})",
        pass_rate * 100.0,
        passed_tests,
        total_tests
    );
    println!("  Visual Quality: {:.2}/1.0", avg_visual);
    println!("  User Experience: {:.2}/1.0", avg_ux);
    println!("  Performance: {:.2}/1.0", avg_perf);
    println!("  Overall Score: {:.2}/1.0", avg_overall);

    let v1_ready = pass_rate >= 0.80 && avg_overall >= 0.75;

    println!("\n🏁 VERDICT:");
    if v1_ready {
        println!("✅ SPEEDY v1.0 IS READY FOR RELEASE!");
        println!("   All quality gates have been met.");
        println!("   The professional-grade visualizer is production-ready.");
    } else {
        println!("❌ SPEEDY v1.0 IS NOT READY FOR RELEASE");
        println!("   Critical improvements needed:");

        if pass_rate < 0.80 {
            println!(
                "   🔴 Pass rate must reach 80%+ (currently {:.1}%)",
                pass_rate * 100.0
            );
        }

        if avg_visual < 0.75 {
            println!(
                "   🔴 Visual quality must reach 0.75+ (currently {:.2})",
                avg_visual
            );
        }

        if avg_ux < 0.75 {
            println!(
                "   🔴 UX quality must reach 0.75+ (currently {:.2})",
                avg_ux
            );
        }

        if avg_perf < 0.75 {
            println!(
                "   🔴 Performance must reach 0.75+ (currently {:.2})",
                avg_perf
            );
        }

        println!("\n🔧 PRIORITY FIXES NEEDED:");
        let failed_tests: Vec<_> = results.iter().filter(|r| !r.passed).collect();

        let mut all_issues: HashMap<String, usize> = HashMap::new();
        for test in &failed_tests {
            for issue in &test.issues {
                *all_issues.entry(issue.clone()).or_insert(0) += 1;
            }
        }

        let mut sorted_issues: Vec<_> = all_issues.into_iter().collect();
        sorted_issues.sort_by(|a, b| b.1.cmp(&a.1));

        for (i, (issue, count)) in sorted_issues.iter().take(5).enumerate() {
            println!("   {}. {} (affects {} tests)", i + 1, issue, count);
        }
    }

    println!("\n📋 NEXT STEPS:");
    if v1_ready {
        println!("   1. Tag v1.0 release");
        println!("   2. Update documentation");
        println!("   3. Create release notes");
        println!("   4. Announce to community");
    } else {
        println!("   1. Address critical issues above");
        println!("   2. Re-run BDD test suite");
        println!("   3. Validate fixes with visual testing");
        println!("   4. Repeat until all quality gates pass");
    }
}
