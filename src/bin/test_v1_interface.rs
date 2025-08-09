// Test program for Speedy v1.0 Professional Interface
// Demonstrates the new immersive, visualization-focused UI

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use speedy::{
    config::Config,
    signal::SignalProcessor,
    testing::audio_generator::AudioGenerator,
    ui::{SpeedyV1Interface, VisualizationMode},
};
use std::io;
use tokio::time::{interval, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Testing Speedy v1.0 Professional Interface");
    println!("==============================================");
    println!("Press CTRL+C or Q to quit");
    println!("Starting interface...\n");

    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Create configuration and signal processor
    let config = Config::default();
    let signal_processor = SignalProcessor::new(
        config.audio.sample_rate,
        64, // More frequency bands for v1.0
        config.display.frequency_range.clone(),
    );

    // Create V1 interface
    let mut interface = SpeedyV1Interface::new(config, signal_processor)?;

    // Audio generator for testing
    let mut audio_gen = AudioGenerator::new(44100.0);
    let mut scenario_index = 0;

    // Test scenarios that cycle automatically
    let test_scenarios = [
        ("Rock Concert", generate_rock_spectrum_v1()),
        ("Electronic Dance", generate_edm_spectrum_v1()),
        ("Classical Orchestra", generate_classical_spectrum_v1()),
        ("Jazz Club", generate_jazz_spectrum_v1()),
        ("Ambient", generate_ambient_spectrum_v1()),
    ];

    let mut frame_interval = interval(Duration::from_millis(16)); // ~60 FPS
    let mut scenario_change_time = std::time::Instant::now();

    loop {
        // Handle keyboard input
        if event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('h') | KeyCode::Char('?') => interface.toggle_help(),
                        KeyCode::Char(' ') | KeyCode::Tab => interface.cycle_mode(),
                        KeyCode::Char('t') => interface.cycle_theme(),
                        KeyCode::Char('1') => interface.current_mode = VisualizationMode::Bars,
                        KeyCode::Char('2') => interface.current_mode = VisualizationMode::Wave,
                        KeyCode::Char('3') => interface.current_mode = VisualizationMode::Spectrum,
                        KeyCode::Char('4') => interface.current_mode = VisualizationMode::Circle,
                        KeyCode::Char('5') => interface.current_mode = VisualizationMode::Particles,
                        KeyCode::Up => interface.adjust_sensitivity(0.1),
                        KeyCode::Down => interface.adjust_sensitivity(-0.1),
                        _ => {}
                    }
                }
            }
        }

        // Check if we should change scenarios
        if scenario_change_time.elapsed() >= Duration::from_secs(5) {
            scenario_index = (scenario_index + 1) % test_scenarios.len();
            println!("\r🎵 Now playing: {}", test_scenarios[scenario_index].0);
            scenario_change_time = std::time::Instant::now();
        }

        // Generate test audio data for current scenario
        let magnitudes = test_scenarios[scenario_index].1.clone();
        interface.update_magnitudes(magnitudes);

        // Wait for next frame
        frame_interval.tick().await;

        // Render frame
        terminal.draw(|frame| {
            interface.draw(frame);
        })?;
    }

    // Cleanup
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    println!("✨ Speedy v1.0 interface test completed!");
    Ok(())
}

fn generate_rock_spectrum_v1() -> Vec<f32> {
    let bands = 64;
    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;
            let time_factor = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f32()
                * 2.0)
                .sin()
                * 0.3
                + 0.7;

            // Rock: Strong bass, present mids, moderate highs with dynamics
            if freq_ratio < 0.15 {
                0.85 * time_factor + 0.15 * (freq_ratio * 15.0 + time_factor * 3.0).sin()
            } else if freq_ratio < 0.4 {
                0.7 * time_factor + 0.2 * ((freq_ratio - 0.2) * 8.0 + time_factor * 2.0).sin()
            } else if freq_ratio < 0.7 {
                0.5 * time_factor + 0.3 * ((freq_ratio - 0.5) * 6.0 + time_factor).sin()
            } else {
                0.3 * time_factor + 0.2 * (freq_ratio * 10.0 + time_factor * 4.0).sin()
            }
        })
        .collect()
}

fn generate_edm_spectrum_v1() -> Vec<f32> {
    let bands = 64;
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;

            // EDM: Massive sub-bass, scooped mids, bright highs with drops
            let drop_phase = (time * 0.25).sin();
            let intensity = (drop_phase * 0.5 + 0.5).max(0.2);

            if freq_ratio < 0.08 {
                1.0 * intensity // Massive sub-bass
            } else if freq_ratio < 0.25 {
                (0.8 + 0.2 * (time * 3.0 + freq_ratio * 10.0).sin()) * intensity
            } else if freq_ratio < 0.6 {
                0.3 * intensity + 0.1 * (freq_ratio * 5.0 + time * 2.0).sin()
            } else {
                (0.7 + 0.3 * (time * 4.0 + freq_ratio * 8.0).sin()) * intensity
            }
        })
        .collect()
}

fn generate_classical_spectrum_v1() -> Vec<f32> {
    let bands = 64;
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;

            // Classical: Natural frequency distribution with rich harmonics
            let fundamental = 0.4 + 0.3 * (freq_ratio * 4.0 + time * 0.5).sin();
            let harmonics = 0.1 * (freq_ratio * 8.0 + time * 0.7).sin()
                + 0.05 * (freq_ratio * 16.0 + time * 1.2).sin()
                + 0.03 * (freq_ratio * 32.0 + time * 0.3).sin();

            (fundamental + harmonics).clamp(0.0, 1.0)
        })
        .collect()
}

fn generate_jazz_spectrum_v1() -> Vec<f32> {
    let bands = 64;
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;

            // Jazz: Warm bass, rich mids, sparkly highs with swing feel
            let swing_factor = (time * 1.5).sin() * 0.3 + 0.7;

            if freq_ratio < 0.2 {
                // Warm upright bass
                0.6 * swing_factor + 0.2 * (freq_ratio * 10.0 + time * 0.8).sin()
            } else if freq_ratio < 0.6 {
                // Rich piano/horn midrange
                0.7 * swing_factor + 0.3 * ((freq_ratio - 0.3) * 6.0 + time * 1.2).sin()
            } else {
                // Sparkling cymbals and harmonics
                0.4 * swing_factor + 0.4 * (freq_ratio * 12.0 + time * 2.0).sin()
            }
        })
        .collect()
}

fn generate_ambient_spectrum_v1() -> Vec<f32> {
    let bands = 64;
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f32();

    (0..bands)
        .map(|i| {
            let freq_ratio = i as f32 / bands as f32;

            // Ambient: Gentle, evolving soundscape
            let evolution1 = (time * 0.3 + freq_ratio * 2.0).sin();
            let evolution2 = (time * 0.5 + freq_ratio * 1.5).cos();
            let evolution3 = (time * 0.2 + freq_ratio * 3.0).sin();

            let base_amplitude = 0.3 + 0.2 * evolution1;
            let texture = 0.1 * evolution2 + 0.05 * evolution3;

            (base_amplitude + texture).clamp(0.0, 0.8)
        })
        .collect()
}
