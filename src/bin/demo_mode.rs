use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use flume::unbounded;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    Terminal,
};
use speedy::{
    audio::AudioData,
    config::Config,
    signal::SignalProcessor,
    testing::audio_generator::AudioGenerator,
    ui::{draw_main_content, draw_status_bar, draw_tabs, App},
};
use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};
use tokio::time::interval;

struct DemoApp {
    config: Config,
    signal_processor: SignalProcessor,
    terminal: Terminal<CrosstermBackend<Stdout>>,
    current_magnitudes: Vec<f32>,
    normalized_magnitudes: Vec<f32>,
    should_quit: bool,
    selected_tab: usize,
    fps_counter: u32,
    current_fps: f32,
    last_fps_update: Instant,
}

impl DemoApp {
    fn new() -> Self {
        let config = Config::default();
        let signal_processor = SignalProcessor::new(
            config.audio.sample_rate,
            config.display.frequency_bands,
            config.display.frequency_range,
        );

        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend).expect("Failed to create terminal");

        Self {
            config,
            signal_processor,
            terminal,
            current_magnitudes: vec![0.0; 80],
            normalized_magnitudes: vec![0.0; 80],
            should_quit: false,
            selected_tab: 0,
            fps_counter: 0,
            current_fps: 0.0,
            last_fps_update: Instant::now(),
        }
    }

    async fn run(
        &mut self,
        audio_receiver: flume::Receiver<AudioData>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Setup terminal
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        self.terminal.clear()?;

        println!(
            "🚀 SPEEDY DEMO MODE - Press 'q' to quit, Tab to switch views, ↑/↓ for sensitivity"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;

        let target_fps = self.config.display.refresh_rate;
        let frame_duration = Duration::from_millis(1000 / target_fps as u64);
        let mut frame_interval = interval(frame_duration);

        loop {
            // Handle events
            self.handle_events().await?;

            if self.should_quit {
                break;
            }

            // Process demo audio data
            while let Ok(audio_data) = audio_receiver.try_recv() {
                match self.signal_processor.process(&audio_data) {
                    Ok(magnitudes) => {
                        self.current_magnitudes = magnitudes;
                        self.normalized_magnitudes = self.signal_processor.normalize_magnitudes(
                            &self.current_magnitudes,
                            self.config.display.sensitivity,
                        );
                    }
                    Err(_) => {
                        // Use demo data if processing fails
                        self.normalized_magnitudes =
                            AudioGenerator::dynamic_spectrum_magnitudes(80);
                    }
                }
            }

            // Wait for next frame
            frame_interval.tick().await;

            // Update FPS counter
            self.fps_counter += 1;
            if self.last_fps_update.elapsed() >= Duration::from_secs(1) {
                self.current_fps = self.fps_counter as f32;
                self.fps_counter = 0;
                self.last_fps_update = Instant::now();
            }

            // Render frame
            let normalized_magnitudes = &self.normalized_magnitudes;
            let current_fps = self.current_fps;
            let selected_tab = self.selected_tab;
            let sensitivity = self.config.display.sensitivity;

            self.terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(0),
                        Constraint::Length(3),
                    ])
                    .split(f.size());

                draw_tabs(f, chunks[0], selected_tab);
                draw_main_content(f, chunks[1], selected_tab, normalized_magnitudes);
                draw_status_bar(
                    f,
                    chunks[2],
                    current_fps,
                    sensitivity,
                    normalized_magnitudes.len(),
                );
            })?;
        }

        // Cleanup terminal
        disable_raw_mode()?;
        io::stdout().execute(LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;

        println!("👋 Demo mode completed");
        Ok(())
    }

    async fn handle_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            self.should_quit = true;
                        }
                        KeyCode::Tab => {
                            self.selected_tab = (self.selected_tab + 1) % 4;
                        }
                        KeyCode::Char('1') => self.selected_tab = 0,
                        KeyCode::Char('2') => self.selected_tab = 1,
                        KeyCode::Char('3') => self.selected_tab = 2,
                        KeyCode::Char('4') => self.selected_tab = 3,
                        KeyCode::Up => {
                            self.config.display.sensitivity =
                                (self.config.display.sensitivity + 0.1).min(5.0);
                        }
                        KeyCode::Down => {
                            self.config.display.sensitivity =
                                (self.config.display.sensitivity - 0.1).max(0.1);
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut demo_app = DemoApp::new();
    let (audio_tx, audio_rx) = unbounded();

    // Generate continuous demo audio data
    tokio::spawn(async move {
        let mut audio_gen = AudioGenerator::new(44100.0);
        let mut pattern = 0;

        loop {
            let samples = match pattern % 4 {
                0 => {
                    // Rock-style spectrum
                    let magnitudes = AudioGenerator::test_spectrum_magnitudes(80);
                    audio_gen.generate_music_spectrum(1024)
                }
                1 => {
                    // Electronic/synthesizer sounds
                    audio_gen.multi_tone(&[220.0, 440.0, 880.0], &[0.5, 0.3, 0.2], 1024)
                }
                2 => {
                    // Bass-heavy patterns
                    audio_gen.multi_tone(&[60.0, 80.0, 120.0], &[0.8, 0.6, 0.4], 1024)
                }
                3 => {
                    // High-frequency content
                    audio_gen.frequency_sweep(1000.0, 4000.0, 0.6, 1024)
                }
                _ => audio_gen.generate_white_noise(1024),
            };

            let audio_data = AudioData {
                samples,
                sample_rate: 44100,
                timestamp: std::time::Instant::now(),
            };

            if audio_tx.send(audio_data).is_err() {
                break;
            }

            pattern += 1;
            tokio::time::sleep(Duration::from_millis(200)).await; // Change pattern every 200ms
        }
    });

    demo_app.run(audio_rx).await?;
    Ok(())
}
