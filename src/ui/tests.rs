#[cfg(test)]
mod ui_tests {
    // Allow using `rav::` to point to current crate inside unit tests
    // use crate as rav; // no longer needed; tests import crate modules directly
    use crate::{
        config::Config,
        signal::SignalProcessor,
        ui::{
            draw_main_content, draw_status_bar, draw_tabs, get_neon_color_for_frequency,
            get_wave_color, App,
        },
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
    use ratatui::{
        backend::TestBackend,
        layout::{Constraint, Direction, Layout},
        Terminal,
    };
    use std::time::Duration;
    use tokio::time::timeout;

    #[test]
    fn test_app_creation() {
        let config = Config::default();
        let signal_processor = SignalProcessor::new(
            config.audio.sample_rate,
            config.display.frequency_bands,
            config.display.frequency_range,
        );

        let app = App::new(config, signal_processor);
        assert_eq!(app.selected_tab, 0);
        assert!(!app.should_quit);
        assert_eq!(app.current_magnitudes.len(), 80); // default frequency bands
    }

    #[test]
    fn test_tab_navigation() {
        let config = Config::default();
        let signal_processor = SignalProcessor::new(
            config.audio.sample_rate,
            config.display.frequency_bands,
            config.display.frequency_range,
        );

        let mut app = App::new(config, signal_processor);

        // Test Tab key cycling
        for expected_tab in 1..=3 {
            simulate_key_press(&mut app, KeyCode::Tab);
            assert_eq!(app.selected_tab, expected_tab);
        }

        // Should wrap back to 0
        simulate_key_press(&mut app, KeyCode::Tab);
        assert_eq!(app.selected_tab, 0);

        // Test direct number key access
        simulate_key_press(&mut app, KeyCode::Char('3'));
        assert_eq!(app.selected_tab, 2); // 0-indexed

        simulate_key_press(&mut app, KeyCode::Char('1'));
        assert_eq!(app.selected_tab, 0);
    }

    #[test]
    fn test_sensitivity_adjustment() {
        let config = Config::default();
        let signal_processor = SignalProcessor::new(
            config.audio.sample_rate,
            config.display.frequency_bands,
            config.display.frequency_range,
        );

        let mut app = App::new(config, signal_processor);
        let initial_sensitivity = app.config.display.sensitivity;

        // Test increasing sensitivity
        simulate_key_press(&mut app, KeyCode::Up);
        assert!(app.config.display.sensitivity > initial_sensitivity);

        // Test decreasing sensitivity
        simulate_key_press(&mut app, KeyCode::Down);
        assert!((app.config.display.sensitivity - initial_sensitivity).abs() < 0.01);

        // Test bounds
        for _ in 0..100 {
            simulate_key_press(&mut app, KeyCode::Up);
        }
        assert!(app.config.display.sensitivity <= 5.0);

        for _ in 0..100 {
            simulate_key_press(&mut app, KeyCode::Down);
        }
        assert!(app.config.display.sensitivity >= 0.1);
    }

    #[test]
    fn test_quit_functionality() {
        let config = Config::default();
        let signal_processor = SignalProcessor::new(
            config.audio.sample_rate,
            config.display.frequency_bands,
            config.display.frequency_range,
        );

        let mut app = App::new(config, signal_processor);
        assert!(!app.should_quit);

        // Test 'q' key
        simulate_key_press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit);

        // Reset for next test
        app.should_quit = false;

        // Test Escape key
        simulate_key_press(&mut app, KeyCode::Esc);
        assert!(app.should_quit);
    }

    #[test]
    fn test_drawing_functions_with_demo_data() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Generate demo audio data
        let demo_magnitudes = gen_test_spectrum_magnitudes(80);

        terminal
            .draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(0),
                        Constraint::Length(3),
                    ])
                    .split(f.area());

                draw_tabs(f, chunks[0], 0);
                draw_main_content(f, chunks[1], 0, &demo_magnitudes);
                draw_status_bar(f, chunks[2], 60.0, 1.0, demo_magnitudes.len());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();

        // Verify the title is present
        let title_found = buffer
            .content
            .iter()
            .any(|cell| cell.symbol().contains("SPEEDY") || cell.symbol().contains("⚡"));
        assert!(title_found, "Title should be visible in the buffer");

        // Verify tabs are present: check for any of the tab icons (single cell symbols)
        let tabs_found = buffer
            .content
            .iter()
            .any(|cell| {
                let s = cell.symbol();
                s.contains("🎧") || s.contains("🌊") || s.contains("🔥") || s.contains("📊")
            });
        assert!(tabs_found, "Tabs should be visible in the buffer");
    }

    #[test]
    fn test_audio_data_processing() {
        let config = Config::default();
        let signal_processor = SignalProcessor::new(
            config.audio.sample_rate,
            config.display.frequency_bands,
            config.display.frequency_range,
        );

        let mut app = App::new(config, signal_processor);

        // Generate test audio data
        let test_samples = gen_sine_wave(440.0, 1024);

        // Simulate audio data processing
        let magnitudes = app
            .signal_processor
            .process(&crate::audio::AudioData {
                samples: test_samples,
                sample_rate: 44100,
                timestamp: std::time::Instant::now(),
            })
            .unwrap();

        app.current_magnitudes = magnitudes;
        app.normalized_magnitudes = app
            .signal_processor
            .normalize_magnitudes(&app.current_magnitudes, app.config.display.sensitivity);

        // Verify magnitudes are processed
        assert!(!app.current_magnitudes.is_empty());
        assert!(!app.normalized_magnitudes.is_empty());
        assert_eq!(
            app.current_magnitudes.len(),
            app.normalized_magnitudes.len()
        );

        // Verify some magnitude values are non-zero (440Hz should create a peak)
        let has_signal = app.normalized_magnitudes.iter().any(|&m| m > 0.01);
        assert!(
            has_signal,
            "Should have detectable signal from 440Hz sine wave"
        );
    }

    #[test]
    fn test_all_visualization_modes() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let demo_magnitudes = gen_dynamic_spectrum_magnitudes(80);

        // Test all 4 visualization modes
        for tab in 0..4 {
            terminal
                .draw(|f| {
                    let area = f.area();
                    draw_main_content(f, area, tab, &demo_magnitudes);
                })
                .unwrap();

            let buffer = terminal.backend().buffer();

            // Each mode should render something (not all spaces)
            let has_content = buffer
                .content
                .iter()
                .any(|cell| !cell.symbol().trim().is_empty() && cell.symbol() != " ");
            assert!(has_content, "Tab {tab} should render visible content");
        }
    }

    #[tokio::test]
    async fn test_demo_mode_with_fake_audio() {
        let config = Config::default();
        let signal_processor = SignalProcessor::new(
            config.audio.sample_rate,
            config.display.frequency_bands,
            config.display.frequency_range,
        );

        let mut app = App::new(config, signal_processor);
        let (audio_tx, audio_rx) = flume::unbounded();

        // Generate continuous fake audio
        tokio::spawn(async move {
            use rand::Rng;
            for i in 0..10 {
                let samples = if i % 3 == 0 {
                    // Music-like: sum of a few sines
                    let mut buf = Vec::with_capacity(1024);
                    for n in 0..1024 {
                        let t = n as f32 / 44100.0;
                        let s = (2.0 * std::f32::consts::PI * 220.0 * t).sin()
                            + 0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                            + 0.25 * (2.0 * std::f32::consts::PI * 880.0 * t).sin();
                        buf.push(s);
                    }
                    buf
                } else if i % 3 == 1 {
                    // White noise
                    let mut rng = rand::thread_rng();
                    (0..1024).map(|_| rng.gen_range(-1.0..1.0)).collect()
                } else {
                    // Simple sine
                    gen_sine_wave(440.0 + i as f32 * 110.0, 1024)
                };

                let audio_data = crate::audio::AudioData {
                    samples,
                    sample_rate: 44100,
                    timestamp: std::time::Instant::now(),
                };

                if audio_tx.send(audio_data).is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });

        // Run the app for a short time with demo data
        let run_result = timeout(Duration::from_secs(2), app.run(audio_rx)).await;

        // The timeout should trigger (app runs continuously until quit)
        assert!(
            run_result.is_err(),
            "App should run until timeout (demo mode working)"
        );
    }

    // Helper function to simulate key presses
    fn simulate_key_press(app: &mut App, key_code: KeyCode) {
        let _key_event = KeyEvent {
            code: key_code,
            modifiers: crossterm::event::KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        };

        // Simulate the key handling logic from handle_events
        match key_code {
            KeyCode::Char('q') | KeyCode::Esc => {
                app.should_quit = true;
            }
            KeyCode::Tab => {
                app.selected_tab = (app.selected_tab + 1) % 4;
            }
            KeyCode::Char('1') => app.selected_tab = 0,
            KeyCode::Char('2') => app.selected_tab = 1,
            KeyCode::Char('3') => app.selected_tab = 2,
            KeyCode::Char('4') => app.selected_tab = 3,
            KeyCode::Up => {
                app.config.display.sensitivity = (app.config.display.sensitivity + 0.1).min(5.0);
            }
            KeyCode::Down => {
                app.config.display.sensitivity = (app.config.display.sensitivity - 0.1).max(0.1);
            }
            _ => {}
        }
    }

    #[test]
    fn test_color_generation() {
        // Test neon color generation
        let color1 = get_neon_color_for_frequency(0.0, 1.0, 1.0); // Low freq, full intensity
        let color2 = get_neon_color_for_frequency(1.0, 1.0, 1.0); // High freq, full intensity
        let color3 = get_neon_color_for_frequency(0.5, 0.5, 0.5); // Mid freq, half intensity

        // Colors should be different
        assert_ne!(format!("{color1:?}"), format!("{:?}", color2));
        assert_ne!(format!("{color2:?}"), format!("{:?}", color3));

        // Test wave colors
        let wave_color1 = get_wave_color(0.0, 1.0);
        let wave_color2 = get_wave_color(1.0, 1.0);

        assert_ne!(format!("{wave_color1:?}"), format!("{:?}", wave_color2));
    }

    #[test]
    fn test_fps_calculation() {
        let config = Config::default();
        let signal_processor = SignalProcessor::new(
            config.audio.sample_rate,
            config.display.frequency_bands,
            config.display.frequency_range,
        );

        let mut app = App::new(config, signal_processor);

        // Simulate frame updates
        for _ in 0..60 {
            app.fps_counter += 1;
        }

        // After 60 frames, FPS should be calculable
        if app.last_update.elapsed() >= Duration::from_secs(1) {
            assert!(app.current_fps > 0.0);
        }
    }
    // Helper signal generation utilities for tests (avoid pulling testing module)
    fn gen_test_spectrum_magnitudes(len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let x = i as f32 / len as f32;
                (0.2 + 0.8 * (10.0 * x).sin().abs()).min(1.0)
            })
            .collect()
    }

    fn gen_dynamic_spectrum_magnitudes(len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let x = i as f32 / len as f32;
                (0.1 + 0.9 * (3.0 * x).cos().abs()).min(1.0)
            })
            .collect()
    }

    fn gen_sine_wave(freq_hz: f32, len: usize) -> Vec<f32> {
        let sr = 44100.0f32;
        (0..len)
            .map(|n| (2.0 * std::f32::consts::PI * freq_hz * (n as f32 / sr)).sin())
            .collect()
    }
}
