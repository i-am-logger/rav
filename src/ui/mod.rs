use crate::{audio::AudioData, config::Config, signal::SignalProcessor};

pub mod v1_interface;
pub use v1_interface::{SpeedyV1Interface, VisualizationMode};

#[cfg(test)]
mod tests;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use flume::Receiver;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{BarChart, Block, Borders, Chart, Dataset, Paragraph, Sparkline, Tabs},
    Frame, Terminal,
};
#[cfg(test)]
use ratatui::backend::TestBackend;
use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};
use tokio::time::interval;
use tracing::{info, warn};

// Use a backend type that adapts to tests vs runtime
#[cfg(test)]
type AppTerminal = Terminal<TestBackend>;
#[cfg(not(test))]
type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

pub struct App {
    config: Config,
    signal_processor: SignalProcessor,
    terminal: AppTerminal,
    current_magnitudes: Vec<f32>,
    normalized_magnitudes: Vec<f32>,
    should_quit: bool,
    last_update: Instant,
    fps_counter: u32,
    current_fps: f32,
    selected_tab: usize,
    // Demo mode fields
    demo_mode: bool,
    demo_pattern: usize,
    last_audio_received: Instant,
    // Professional UI interface
    use_professional_ui: bool,
    v1_interface: Option<SpeedyV1Interface>,
}

impl App {
    pub fn new(config: Config, signal_processor: SignalProcessor) -> Self {
        // Initialize terminal
        #[cfg(not(test))]
        let terminal: AppTerminal = {
            let backend = CrosstermBackend::new(io::stdout());
            Terminal::new(backend).expect("Failed to create terminal")
        };
        #[cfg(test)]
        let terminal: AppTerminal = {
            let backend = TestBackend::new(80, 24);
            Terminal::new(backend).expect("Failed to create test terminal")
        };

        // Create v1 interface immediately since we default to professional mode
        let v1_interface = match SpeedyV1Interface::new(
            config.clone(),
            SignalProcessor::new(
                config.audio.sample_rate,
                config.display.frequency_bands,
                config.display.frequency_range,
            ),
        ) {
            Ok(interface) => {
                info!("✅ Professional UI (v1.0 interface) initialized successfully");
                Some(interface)
            }
            Err(e) => {
                warn!(
                    "⚠️ Failed to create professional UI, falling back to simple interface: {}",
                    e
                );
                None
            }
        };

        Self {
            config,
            signal_processor,
            terminal,
            current_magnitudes: vec![0.0; 80],
            normalized_magnitudes: vec![0.0; 80],
            should_quit: false,
            last_update: Instant::now(),
            fps_counter: 0,
            current_fps: 0.0,
            selected_tab: 0,
            // Demo mode initialization
            demo_mode: false,
            demo_pattern: 0,
            last_audio_received: Instant::now(),
            // Professional UI initialization - default to advanced interface
            use_professional_ui: true, // Start with professional UI by default
            v1_interface,              // Created immediately
        }
    }

    pub async fn run(&mut self, audio_receiver: Receiver<AudioData>) -> Result<()> {
        // Setup terminal (skip raw/alt screen in tests)
        #[cfg(not(test))]
        {
            enable_raw_mode()?;
            io::stdout().execute(EnterAlternateScreen)?;
        }
        self.terminal.clear()?;

        info!("🎨 Starting UI main loop");

        let mut last_fps_update = Instant::now();
        let target_fps = self.config.display.refresh_rate;
        let frame_duration = Duration::from_millis(1000 / target_fps as u64);
        let mut frame_interval = interval(frame_duration);

        loop {
            // Handle events
            self.handle_events().await?;

            if self.should_quit {
                break;
            }

            // Process audio data if available
            let mut received_audio = false;
            while let Ok(audio_data) = audio_receiver.try_recv() {
                received_audio = true;
                self.last_audio_received = Instant::now();
                self.demo_mode = false; // Disable demo mode when real audio is available

                match self.signal_processor.process(&audio_data) {
                    Ok(magnitudes) => {
                        self.current_magnitudes = magnitudes;
                        self.normalized_magnitudes = self.signal_processor.normalize_magnitudes(
                            &self.current_magnitudes,
                            self.config.display.sensitivity,
                        );
                    }
                    Err(e) => {
                        warn!("Failed to process audio data: {}", e);
                    }
                }
            }

            // Auto-activate demo mode if no audio received for 2 seconds
            if !received_audio && self.last_audio_received.elapsed() > Duration::from_secs(2) {
                if !self.demo_mode {
                    self.demo_mode = true;
                    info!("🎭 Activating demo mode - generating dynamic visuals");
                }

                // Generate demo audio data
                if self.use_professional_ui {
                    self.generate_demo_visuals_for_v1();
                } else {
                    self.generate_demo_visuals();
                }
            }

            // Update professional interface with current magnitude data
            if self.use_professional_ui && received_audio {
                if let Some(ref mut v1_interface) = self.v1_interface {
                    v1_interface.update_magnitudes(self.normalized_magnitudes.clone());
                    // Update FPS in professional interface
                    v1_interface.fps = self.current_fps;
                }
            }

            // Wait for next frame
            frame_interval.tick().await;

            // Update FPS counter
            self.fps_counter += 1;
            if last_fps_update.elapsed() >= Duration::from_secs(1) {
                self.current_fps = self.fps_counter as f32;
                self.fps_counter = 0;
                last_fps_update = Instant::now();
            }

            // Render frame
            let config = &self.config;
            let normalized_magnitudes = &self.normalized_magnitudes;
            let current_fps = self.current_fps;
            let selected_tab = self.selected_tab;

            self.terminal.draw(|f| {
                if self.use_professional_ui {
                    // Use professional v1 interface
                    if let Some(ref mut v1_interface) = self.v1_interface {
                        v1_interface.draw(f);
                    } else {
                        // Fallback to simple interface if professional UI failed to initialize
                        warn!("⚠️ Professional UI unavailable, falling back to simple interface");
                        self.use_professional_ui = false; // Disable professional UI mode
                        let chunks = Layout::default()
                            .direction(Direction::Vertical)
                            .margin(1)
                            .constraints([
                                Constraint::Length(3),
                                Constraint::Min(0),
                                Constraint::Length(3),
                            ])
                            .split(f.area());

                        draw_tabs(f, chunks[0], selected_tab);
                        draw_main_content(f, chunks[1], selected_tab, normalized_magnitudes);
                        draw_status_bar(
                            f,
                            chunks[2],
                            current_fps,
                            config.display.sensitivity,
                            normalized_magnitudes.len(),
                        );
                    }
                } else {
                    // Use simple tabbed interface
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints([
                            Constraint::Length(3),
                            Constraint::Min(0),
                            Constraint::Length(3),
                        ])
                        .split(f.area());

                    draw_tabs(f, chunks[0], selected_tab);
                    draw_main_content(f, chunks[1], selected_tab, normalized_magnitudes);
                    draw_status_bar(
                        f,
                        chunks[2],
                        current_fps,
                        config.display.sensitivity,
                        normalized_magnitudes.len(),
                    );
                }
            })?;
            self.last_update = Instant::now();
        }

        // Cleanup terminal
        #[cfg(not(test))]
        {
            disable_raw_mode()?;
            io::stdout().execute(LeaveAlternateScreen)?;
            self.terminal.show_cursor()?;
        }

        info!("👋 UI shut down complete");
        Ok(())
    }

    async fn handle_events(&mut self) -> Result<()> {
        #[cfg(test)]
        {
            // In tests, avoid crossterm I/O entirely to keep the loop running
            return Ok(());
        }
        #[cfg(not(test))]
        {
            // Non-blocking event polling
            if event::poll(Duration::from_millis(0))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        // Handle UI mode switching first
                        if let KeyCode::Char('p') = key.code {
                            self.toggle_professional_ui()?;
                            return Ok(());
                        }

                        // Handle events based on current UI mode
                        if self.use_professional_ui {
                            self.handle_professional_ui_events(key.code).await?
                        } else {
                            self.handle_simple_ui_events(key.code).await?
                        }
                    }
                }
            }
            Ok(())
        }
    }

    /// Toggle between simple and professional UI modes
    fn toggle_professional_ui(&mut self) -> Result<()> {
        self.use_professional_ui = !self.use_professional_ui;

        if self.use_professional_ui && self.v1_interface.is_none() {
            // Create the professional interface on-demand
            let v1_interface = SpeedyV1Interface::new(
                self.config.clone(),
                SignalProcessor::new(
                    self.config.audio.sample_rate,
                    self.config.display.frequency_bands,
                    self.config.display.frequency_range,
                ),
            )
            .map_err(|e| anyhow::anyhow!("Failed to create professional interface: {}", e))?;
            self.v1_interface = Some(v1_interface);
        }

        if self.use_professional_ui {
            info!("🚀 Switched to Professional UI mode (v1.0 interface)");
        } else {
            info!("🏠 Switched to Simple UI mode (tabbed interface)");
        }

        Ok(())
    }

    /// Handle events for the professional UI interface
    async fn handle_professional_ui_events(&mut self, key_code: KeyCode) -> Result<()> {
        if let Some(ref mut v1_interface) = self.v1_interface {
            match key_code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.should_quit = true;
                }
                KeyCode::Char(' ') | KeyCode::Tab => {
                    // Cycle visualization modes
                    v1_interface.cycle_mode();
                    info!(
                        "🔄 Switched visualization mode to: {:?}",
                        v1_interface.current_mode
                    );
                }
                KeyCode::Char('1') => v1_interface.current_mode = VisualizationMode::Bars,
                KeyCode::Char('2') => v1_interface.current_mode = VisualizationMode::Wave,
                KeyCode::Char('3') => v1_interface.current_mode = VisualizationMode::Spectrum,
                KeyCode::Char('4') => v1_interface.current_mode = VisualizationMode::Circle,
                KeyCode::Char('5') => v1_interface.current_mode = VisualizationMode::Particles,
                KeyCode::Char('t') | KeyCode::Char('T') => {
                    // Cycle color themes
                    v1_interface.cycle_theme();
                    info!("🎨 Switched color theme");
                }
                KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Char('?') => {
                    // Toggle help overlay
                    v1_interface.toggle_help();
                }
                KeyCode::Up => {
                    // Increase sensitivity
                    v1_interface.adjust_sensitivity(0.1);
                    self.config.display.sensitivity = v1_interface.config.display.sensitivity;
                    // Sync back
                }
                KeyCode::Down => {
                    // Decrease sensitivity
                    v1_interface.adjust_sensitivity(-0.1);
                    self.config.display.sensitivity = v1_interface.config.display.sensitivity;
                    // Sync back
                }
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    // Reset settings to defaults
                    v1_interface.config.display.sensitivity = 1.0;
                    self.config.display.sensitivity = 1.0;
                    info!("♻️ Reset settings to defaults");
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Handle events for the simple UI interface
    async fn handle_simple_ui_events(&mut self, key_code: KeyCode) -> Result<()> {
        match key_code {
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
                self.config.display.sensitivity = (self.config.display.sensitivity + 0.1).min(5.0);
            }
            KeyCode::Down => {
                self.config.display.sensitivity = (self.config.display.sensitivity - 0.1).max(0.1);
            }
            _ => {}
        }
        Ok(())
    }

    #[allow(dead_code)] // Kept for potential future refactoring
    fn draw(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(f.area());

        self.draw_tabs(f, chunks[0]);
        self.draw_main_content(f, chunks[1]);
        self.draw_status_bar(f, chunks[2]);
    }

    #[allow(dead_code)] // Kept for potential future refactoring
    fn draw_tabs(&self, f: &mut Frame, area: Rect) {
        let titles: Vec<Line> = ["Bars", "Wave", "Spectrum", "Info"]
            .iter()
            .cloned()
            .map(Line::from)
            .collect();

        let tabs = Tabs::new(titles)
            .block(Block::default().borders(Borders::ALL).title("Visualizer"))
            .style(Style::default().fg(Color::White))
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .bg(Color::Blue)
                    .fg(Color::White),
            )
            .select(self.selected_tab);

        f.render_widget(tabs, area);
    }

    #[allow(dead_code)] // Kept for potential future refactoring
    fn draw_main_content(&self, f: &mut Frame, area: Rect) {
        match self.selected_tab {
            0 => self.draw_bars(f, area),
            1 => self.draw_wave(f, area),
            2 => self.draw_spectrum(f, area),
            3 => self.draw_info(f, area),
            _ => {}
        }
    }

    #[allow(dead_code)] // Kept for potential future refactoring
    fn draw_bars(&self, f: &mut Frame, area: Rect) {
        // Create data with &str for compatibility with BarChart
        let data: Vec<(&str, u64)> = self
            .normalized_magnitudes
            .iter()
            .enumerate()
            .take(20) // Limit to 20 bars to fit in the display
            .map(|(i, &magnitude)| {
                let height = (magnitude * 100.0) as u64;
                // Use static string slices for simplicity
                let label = match i {
                    0 => "0",
                    1 => "1",
                    2 => "2",
                    3 => "3",
                    4 => "4",
                    5 => "5",
                    6 => "6",
                    7 => "7",
                    8 => "8",
                    9 => "9",
                    10 => "10",
                    11 => "11",
                    12 => "12",
                    13 => "13",
                    14 => "14",
                    15 => "15",
                    16 => "16",
                    17 => "17",
                    18 => "18",
                    19 => "19",
                    _ => "N",
                };
                (label, height)
            })
            .collect();

        let bar_chart = BarChart::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Audio Frequency Bars"),
            )
            .data(&data)
            .bar_width(3)
            .bar_style(Style::default().fg(self.get_color_for_magnitude(0.8)))
            .value_style(
                Style::default()
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_widget(bar_chart, area);
    }

    #[allow(dead_code)] // Kept for potential future refactoring
    fn draw_wave(&self, f: &mut Frame, area: Rect) {
        let wave_data: Vec<u64> = self
            .normalized_magnitudes
            .iter()
            .map(|&x| (x * 100.0) as u64)
            .collect();

        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Audio Waveform"),
            )
            .data(&wave_data)
            .style(Style::default().fg(Color::Green));

        f.render_widget(sparkline, area);
    }

    #[allow(dead_code)] // Kept for potential future refactoring
    fn draw_spectrum(&self, f: &mut Frame, area: Rect) {
        let data: Vec<(f64, f64)> = self
            .normalized_magnitudes
            .iter()
            .enumerate()
            .map(|(i, &magnitude)| (i as f64, magnitude as f64))
            .collect();

        let dataset = Dataset::default()
            .name("Spectrum")
            .marker(ratatui::symbols::Marker::Braille)
            .style(Style::default().fg(Color::Cyan))
            .data(&data);

        let chart = Chart::new(vec![dataset])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Frequency Spectrum"),
            )
            .x_axis(
                ratatui::widgets::Axis::default()
                    .title("Frequency Bands")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, self.normalized_magnitudes.len() as f64]),
            )
            .y_axis(
                ratatui::widgets::Axis::default()
                    .title("Magnitude")
                    .style(Style::default().fg(Color::Gray))
                    .bounds([0.0, 1.0]),
            );

        f.render_widget(chart, area);
    }

    #[allow(dead_code)] // Kept for potential future refactoring
    fn draw_info(&self, f: &mut Frame, area: Rect) {
        let info_text = vec![
            Line::from(vec![Span::styled(
                "🚀 Speedy Audio Visualizer",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "📊 Stats:",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(format!("  FPS: {:.1}", self.current_fps)),
            Line::from(format!(
                "  Frequency Bands: {}",
                self.normalized_magnitudes.len()
            )),
            Line::from(format!(
                "  Sensitivity: {:.1}",
                self.config.display.sensitivity
            )),
            Line::from(format!(
                "  Sample Rate: {} Hz",
                self.config.audio.sample_rate
            )),
            Line::from(""),
            Line::from(vec![Span::styled(
                "🎛️ Controls:",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from("  q/Esc - Quit"),
            Line::from("  Tab - Switch tabs"),
            Line::from("  1-4 - Select tab directly"),
            Line::from("  ↑/↓ - Adjust sensitivity"),
        ];

        let paragraph = Paragraph::new(info_text)
            .block(Block::default().borders(Borders::ALL).title("Information"))
            .alignment(Alignment::Left);

        f.render_widget(paragraph, area);
    }

    #[allow(dead_code)] // Kept for potential future refactoring
    fn draw_status_bar(&self, f: &mut Frame, area: Rect) {
        let status_text = format!(
            " FPS: {:.1} | Sensitivity: {:.1} | Bands: {} | Press 'q' to quit ",
            self.current_fps,
            self.config.display.sensitivity,
            self.normalized_magnitudes.len()
        );

        let status = Paragraph::new(status_text)
            .style(Style::default().bg(Color::Blue).fg(Color::White))
            .alignment(Alignment::Center);

        f.render_widget(status, area);
    }

    #[allow(dead_code)] // Kept for potential future refactoring
    fn get_color_for_magnitude(&self, magnitude: f32) -> Color {
        // Simple color gradient based on magnitude
        if magnitude < 0.2 {
            Color::Blue
        } else if magnitude < 0.4 {
            Color::Cyan
        } else if magnitude < 0.6 {
            Color::Green
        } else if magnitude < 0.8 {
            Color::Yellow
        } else {
            Color::Red
        }
    }

    /// Generate dynamic demo visuals when no audio input is available
    fn generate_demo_visuals(&mut self) {
        use std::f32::consts::PI;

        // Change pattern every 3 seconds for variety
        let _pattern_duration = Duration::from_secs(3);
        let elapsed = self.last_update.elapsed().as_secs_f32();
        self.demo_pattern = ((elapsed / 3.0) as usize) % 6;

        let time = elapsed * 2.0; // Animation speed factor
        let band_count = self.normalized_magnitudes.len();

        // Generate different patterns based on demo_pattern
        for i in 0..band_count {
            let freq_ratio = i as f32 / band_count as f32;
            let magnitude = match self.demo_pattern {
                0 => {
                    // "Music spectrum" - strong bass, moderate mids, lower highs
                    let bass_boost = if freq_ratio < 0.2 { 0.8 } else { 0.4 };
                    let wave = (time * 0.5 + freq_ratio * PI * 4.0).sin().abs();
                    (bass_boost * wave * (1.0 - freq_ratio * 0.5)).max(0.1)
                }
                1 => {
                    // "Electronic beats" - pulsing patterns
                    let pulse = (time * 3.0).sin().abs();
                    let harmonic = (freq_ratio * PI * 8.0 + time).sin().abs();
                    (pulse * harmonic * 0.8).max(0.1)
                }
                2 => {
                    // "Ambient waves" - slow, flowing patterns
                    let wave1 = (time * 0.8 + freq_ratio * PI * 2.0).sin();
                    let wave2 = (time * 1.2 + freq_ratio * PI * 3.0).cos();
                    ((wave1 * wave2).abs() * 0.7).max(0.1)
                }
                3 => {
                    // "Rock energy" - dynamic with emphasis on mid frequencies
                    let mid_boost = if freq_ratio > 0.3 && freq_ratio < 0.7 {
                        1.2
                    } else {
                        0.6
                    };
                    let energy = (time * 2.0 + freq_ratio * PI * 6.0).sin().abs();
                    let variation = (time * 0.3 + i as f32 * 0.1).cos().abs();
                    (mid_boost * energy * variation).max(0.1)
                }
                4 => {
                    // "Frequency sweep" - moving peaks
                    let sweep_pos = (time * 0.5).sin() * 0.5 + 0.5; // 0 to 1
                    let distance = (freq_ratio - sweep_pos).abs();
                    let peak = (-distance * 20.0).exp();
                    (peak * 0.9 + 0.1).min(1.0)
                }
                _ => {
                    // "Noise burst" - random-looking but deterministic
                    let seed = (freq_ratio * 1000.0 + time * 10.0) as u32;
                    let pseudo_random = ((seed.wrapping_mul(1103515245).wrapping_add(12345)) >> 16)
                        as f32
                        / 65536.0;
                    (pseudo_random * 0.8 + 0.2).min(1.0)
                }
            };

            self.normalized_magnitudes[i] = magnitude.clamp(0.0, 1.0);
        }
    }

    /// Generate demo visuals specifically for the professional v1 interface
    fn generate_demo_visuals_for_v1(&mut self) {
        // Generate similar patterns but update both simple and professional interfaces
        self.generate_demo_visuals();

        // Also update the v1 interface with the generated data
        if let Some(ref mut v1_interface) = self.v1_interface {
            v1_interface.update_magnitudes(self.normalized_magnitudes.clone());
            v1_interface.fps = self.current_fps;
        }
    }
}

// Standalone drawing functions to avoid borrowing issues
pub fn draw_tabs(f: &mut Frame, area: Rect, selected_tab: usize) {
    let tab_titles = [
        ("🎧 NEON BARS", Color::Rgb(255, 20, 147)),
        ("🌊 FLUID WAVE", Color::Rgb(0, 191, 255)),
        ("🔥 SPECTRUM", Color::Rgb(255, 100, 0)),
        ("📊 SYSTEM INFO", Color::Rgb(0, 255, 0)),
    ];

    let titles: Vec<Line> = tab_titles
        .iter()
        .enumerate()
        .map(|(i, (title, color))| {
            if i == selected_tab {
                Line::from(Span::styled(
                    format!(" {title} "),
                    Style::default()
                        .fg(*color)
                        .bg(Color::Rgb(20, 20, 40))
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(
                    format!(" {title} "),
                    Style::default().fg(Color::Rgb(100, 100, 120)),
                ))
            }
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(
                    "⚡ SPEEDY AUDIO VISUALIZER v1.0 ⚡",
                    Style::default()
                        .fg(Color::Rgb(0, 255, 255))
                        .add_modifier(Modifier::BOLD),
                ))
                .border_style(Style::default().fg(Color::Rgb(50, 150, 255))),
        )
        .style(Style::default().bg(Color::Rgb(10, 10, 25)))
        .select(selected_tab);

    f.render_widget(tabs, area);
}

pub fn draw_main_content(
    f: &mut Frame,
    area: Rect,
    selected_tab: usize,
    normalized_magnitudes: &[f32],
) {
    match selected_tab {
        0 => draw_bars_standalone(f, area, normalized_magnitudes),
        1 => draw_wave_standalone(f, area, normalized_magnitudes),
        2 => draw_spectrum_standalone(f, area, normalized_magnitudes),
        3 => draw_info_standalone(f, area, normalized_magnitudes),
        _ => {}
    }
}

fn draw_bars_standalone(f: &mut Frame, area: Rect, normalized_magnitudes: &[f32]) {
    // Create a sophisticated neon-style bar visualization
    let block = Block::default()
        .borders(Borders::ALL)
        .title("🎵 FREQUENCY SPECTRUM")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .border_style(Style::default().fg(Color::Rgb(0, 150, 150)));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // Draw custom neon bars using text characters
    let bar_count = (inner_area.width as usize)
        .min(normalized_magnitudes.len())
        .min(80);
    let bar_width = if bar_count > 0 {
        inner_area.width as usize / bar_count
    } else {
        1
    };
    let max_height = inner_area.height as f32;

    for row in 0..inner_area.height {
        let y_pos = 1.0 - (row as f32 / max_height);
        let mut line_spans = Vec::new();

        for i in 0..bar_count {
            let magnitude = normalized_magnitudes
                .get(i * normalized_magnitudes.len() / bar_count)
                .unwrap_or(&0.0);
            let bar_height = *magnitude;

            // Create neon effect with multiple intensity levels
            let char_and_color = if y_pos <= bar_height {
                let intensity_ratio = (bar_height - y_pos) / bar_height;
                let color = get_neon_color_for_frequency(
                    i as f32 / bar_count as f32,
                    *magnitude,
                    intensity_ratio,
                );

                if y_pos > bar_height - 0.1 {
                    ("▄", color) // Top glow
                } else if y_pos > bar_height - 0.2 {
                    ("▀", color) // Upper glow
                } else {
                    ("█", color) // Full intensity
                }
            } else {
                (" ", Color::Reset)
            };

            let bar_segment = char_and_color.0.repeat(bar_width.max(1));
            line_spans.push(Span::styled(
                bar_segment,
                Style::default().fg(char_and_color.1),
            ));
        }

        let line = Line::from(line_spans);
        let y = inner_area.y + row;
        if y < inner_area.y + inner_area.height {
            f.render_widget(
                Paragraph::new(line),
                Rect {
                    x: inner_area.x,
                    y,
                    width: inner_area.width,
                    height: 1,
                },
            );
        }
    }
}

/// Generate neon colors based on frequency position and intensity
fn get_neon_color_for_frequency(freq_ratio: f32, magnitude: f32, intensity: f32) -> Color {
    let base_colors = [
        Color::Rgb(255, 20, 147),  // Deep pink (low freq)
        Color::Rgb(255, 100, 0),   // Orange
        Color::Rgb(255, 255, 0),   // Yellow
        Color::Rgb(0, 255, 0),     // Lime (mid freq)
        Color::Rgb(0, 255, 255),   // Cyan
        Color::Rgb(100, 149, 237), // Blue
        Color::Rgb(138, 43, 226),  // Purple (high freq)
    ];

    let color_index = (freq_ratio * (base_colors.len() - 1) as f32) as usize;
    let base_color = base_colors[color_index.min(base_colors.len() - 1)];

    // Apply intensity and magnitude
    match base_color {
        Color::Rgb(r, g, b) => {
            let intensity_factor = (magnitude * intensity * 0.8 + 0.2).clamp(0.0, 1.0);
            Color::Rgb(
                (r as f32 * intensity_factor) as u8,
                (g as f32 * intensity_factor) as u8,
                (b as f32 * intensity_factor) as u8,
            )
        }
        _ => base_color,
    }
}

fn draw_wave_standalone(f: &mut Frame, area: Rect, normalized_magnitudes: &[f32]) {
    // Create a fluid wave visualization with dynamic colors
    let block = Block::default()
        .borders(Borders::ALL)
        .title("🌊 FLUID WAVEFORM")
        .title_style(
            Style::default()
                .fg(Color::Rgb(0, 191, 255))
                .add_modifier(Modifier::BOLD),
        )
        .border_style(Style::default().fg(Color::Rgb(30, 144, 255)));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let width = inner_area.width as usize;
    let height = inner_area.height as usize;
    let _center_y = height / 2;

    // Create smooth wave interpolation
    let wave_points: Vec<f32> = (0..width)
        .map(|x| {
            let sample_index = (x * normalized_magnitudes.len()) / width;
            let sample = normalized_magnitudes.get(sample_index).unwrap_or(&0.0);

            // Add some wave motion for visual appeal
            let wave_offset = ((x as f32 * 0.1).sin() * 0.1).abs();
            sample + wave_offset
        })
        .collect();

    // Draw the wave using various characters for different intensities
    for y in 0..height {
        let mut line_spans = Vec::new();

        for (x, wave_height) in wave_points.iter().enumerate().take(width) {
            let wave_height = *wave_height;
            let normalized_y = (y as f32 / height as f32) - 0.5; // -0.5 to 0.5
            let wave_y = wave_height * 0.5; // Scale wave

            let char_and_color = if (normalized_y - wave_y).abs() <= 0.05 {
                // Core wave line
                let intensity = wave_height;
                let color = get_wave_color(x as f32 / width as f32, intensity);
                ("█", color)
            } else if (normalized_y - wave_y).abs() <= 0.1 {
                // Wave glow
                let intensity = wave_height * 0.6;
                let color = get_wave_color(x as f32 / width as f32, intensity);
                ("░", color)
            } else if (normalized_y - wave_y).abs() <= 0.15 {
                // Faint glow
                let intensity = wave_height * 0.3;
                let color = get_wave_color(x as f32 / width as f32, intensity);
                ("▒", color)
            } else {
                (" ", Color::Reset)
            };

            line_spans.push(Span::styled(
                char_and_color.0.to_string(),
                Style::default().fg(char_and_color.1),
            ));
        }

        let line = Line::from(line_spans);
        let render_y = inner_area.y + y as u16;
        f.render_widget(
            Paragraph::new(line),
            Rect {
                x: inner_area.x,
                y: render_y,
                width: inner_area.width,
                height: 1,
            },
        );
    }
}

/// Generate wave colors with oceanic theme
fn get_wave_color(position: f32, intensity: f32) -> Color {
    let base_colors = [
        Color::Rgb(0, 100, 200),   // Deep ocean blue
        Color::Rgb(0, 150, 255),   // Ocean blue
        Color::Rgb(64, 224, 208),  // Turquoise
        Color::Rgb(0, 255, 255),   // Cyan
        Color::Rgb(173, 216, 230), // Light blue
        Color::Rgb(255, 255, 255), // White foam
    ];

    let color_index = (intensity * (base_colors.len() - 1) as f32) as usize;
    let base_color = base_colors[color_index.min(base_colors.len() - 1)];

    // Apply position-based tinting
    match base_color {
        Color::Rgb(r, g, b) => {
            let pos_factor = (position * 0.2 + 0.8).clamp(0.5, 1.0);
            let intensity_factor = (intensity * 0.8 + 0.2).clamp(0.2, 1.0);
            Color::Rgb(
                (r as f32 * pos_factor * intensity_factor) as u8,
                (g as f32 * pos_factor * intensity_factor) as u8,
                (b as f32 * intensity_factor) as u8,
            )
        }
        _ => base_color,
    }
}

fn draw_spectrum_standalone(f: &mut Frame, area: Rect, normalized_magnitudes: &[f32]) {
    let data: Vec<(f64, f64)> = normalized_magnitudes
        .iter()
        .enumerate()
        .map(|(i, &magnitude)| (i as f64, magnitude as f64))
        .collect();

    let dataset = Dataset::default()
        .name("Spectrum")
        .marker(ratatui::symbols::Marker::Braille)
        .style(Style::default().fg(Color::Cyan))
        .data(&data);

    let chart = Chart::new(vec![dataset])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Frequency Spectrum"),
        )
        .x_axis(
            ratatui::widgets::Axis::default()
                .title("Frequency Bands")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, normalized_magnitudes.len() as f64]),
        )
        .y_axis(
            ratatui::widgets::Axis::default()
                .title("Magnitude")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, 1.0]),
        );

    f.render_widget(chart, area);
}

fn draw_info_standalone(f: &mut Frame, area: Rect, normalized_magnitudes: &[f32]) {
    let info_text = vec![
        Line::from(vec![Span::styled(
            "🚀 Speedy Audio Visualizer",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "📊 Stats:",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  Audio Processing: Active"),
        Line::from(format!(
            "  Frequency Bands: {}",
            normalized_magnitudes.len()
        )),
        Line::from(""),
        Line::from(vec![Span::styled(
            "🎛️ Controls:",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  q/Esc - Quit"),
        Line::from("  Tab - Switch tabs"),
        Line::from("  1-4 - Select tab directly"),
        Line::from("  ↑/↓ - Adjust sensitivity"),
    ];

    let paragraph = Paragraph::new(info_text)
        .block(Block::default().borders(Borders::ALL).title("Information"))
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

pub fn draw_status_bar(
    f: &mut Frame,
    area: Rect,
    current_fps: f32,
    sensitivity: f32,
    band_count: usize,
) {
    let status_text = format!(
        " FPS: {current_fps:.1} | Sensitivity: {sensitivity:.1} | Bands: {band_count} | Press 'q' to quit "
    );

    let status = Paragraph::new(status_text)
        .style(Style::default().bg(Color::Blue).fg(Color::White))
        .alignment(Alignment::Center);

    f.render_widget(status, area);
}
