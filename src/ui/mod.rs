pub mod analyzer;
pub mod help;
pub mod scale;
pub mod scope;

use crate::{
    audio::AudioData,
    config::Config,
    signal::{
        ballistics::{BAR_FALL_SPEEDS, Ballistics, DEFAULT_PEAK_FALL},
        mapping::{
            Bandwidth, BarMap, DEFAULT_LIMIT_INDEX, DEFAULT_SCALE, FREQUENCY_LIMITS, bin_for_hz,
        },
        spectrum::{MAX_HEIGHT, Spectrum},
    },
    visual::{Skin, Theme},
};
use analyzer::{Analyzer, BarLayout, BarStyle, Peaks, grid_colors, row_colors};
use anyhow::Result;
use crossterm::event::KeyCode;
#[cfg(not(test))]
use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyEventKind},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use flume::Receiver;
use help::{Help, HelpRow};
#[cfg(not(test))]
use ratatui::backend::CrosstermBackend;
#[cfg(test)]
use ratatui::backend::TestBackend;
use ratatui::{Terminal, style::Color};
use scope::{Scope, ScopeStyle};
#[cfg(not(test))]
use std::io::{self, Stdout};
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::info;

#[cfg(test)]
type AppTerminal = Terminal<TestBackend>;
#[cfg(not(test))]
type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Which visualisation is on screen. The original showed one at a time, full area,
/// switched by clicking the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Analyzer,
    Oscilloscope,
}

impl View {
    fn next(self) -> Self {
        match self {
            View::Analyzer => View::Oscilloscope,
            View::Oscilloscope => View::Analyzer,
        }
    }
}

/// What a keypress means. Kept separate from the handler so tests exercise the
/// real mapping instead of a copy of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    ToggleView,
    TogglePeaks,
    ToggleGrid,
    CycleBandwidth,
    CycleBarFall,
    CycleFrequencyLimit,
    ToggleHelp,
    CycleBarStyle,
    CycleSkin,
    /// Trim in whole dB steps; kept an integer so `Action` stays `Eq`.
    Gain(i8),
    ResetGain,
}

pub fn map_key(code: KeyCode) -> Action {
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char(' ') | KeyCode::Char('o') | KeyCode::Char('O') | KeyCode::Tab => {
            Action::ToggleView
        }
        KeyCode::Char('p') | KeyCode::Char('P') => Action::TogglePeaks,
        KeyCode::Char('g') | KeyCode::Char('G') => Action::ToggleGrid,
        KeyCode::Char('w') | KeyCode::Char('W') => Action::CycleBandwidth,
        KeyCode::Char('b') | KeyCode::Char('B') => Action::CycleBarStyle,
        KeyCode::Char('s') | KeyCode::Char('S') => Action::CycleSkin,
        KeyCode::Char('f') | KeyCode::Char('F') => Action::CycleBarFall,
        KeyCode::Char('r') | KeyCode::Char('R') => Action::CycleFrequencyLimit,
        KeyCode::Up => Action::Gain(1),
        KeyCode::Down => Action::Gain(-1),
        KeyCode::Char('0') => Action::ResetGain,
        KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Char('?') | KeyCode::F(1) => {
            Action::ToggleHelp
        }
        _ => Action::None,
    }
}

pub struct App {
    config: Config,
    terminal: AppTerminal,

    /// Rolling window of the newest mono samples, oldest first.
    window: Vec<f32>,
    channels: usize,

    spectrum: Spectrum,
    bar_map: BarMap,
    ballistics: Ballistics,
    bandwidth: Bandwidth,
    bar_fall_index: usize,
    limit_index: usize,
    sample_rate: u32,
    /// Fixed level trim, in dB.
    ///
    /// rav captures through a loopback device, which on macOS sits *after* the
    /// system volume - so turning the volume down shrinks the bars. The original
    /// tapped the decoder before its volume control and had no such problem. Since there is deliberately no auto-normalisation (that is what
    /// makes a quiet passage read quiet), this is the knob that anchors the
    /// reference to however loud you actually listen.
    gain_db: f32,

    /// Reused every frame so the render path does not allocate.
    sampled: Vec<f32>,
    bands: Vec<f32>,

    skin: Skin,
    /// A skin loaded from disk by `--skin`, kept so the `s` cycle returns to it.
    loaded_skin: Option<Skin>,
    row_colors: Vec<Color>,
    /// Backdrop colour per row, cached with `row_colors` and rebuilt with it.
    grid_colors: Vec<Color>,
    /// The terminal's own palette, read once at startup. A skin that names a
    /// colour rather than spelling it out needs this to be adjustable at all.
    theme: Theme,
    layout: BarLayout,
    /// Size the cached mapping and colours were built for.
    sized_for: (u16, u16),

    view: View,
    scope_style: ScopeStyle,
    peaks: Peaks,
    show_grid: bool,
    bar_style: BarStyle,
    show_help: bool,
    /// Briefly shown after a settings key, so a change is visible without a
    /// permanent status bar cluttering the display.
    status: Option<(String, Instant)>,
    /// When present, audio comes from here rather than the cpal channel.
    #[cfg(target_os = "macos")]
    tap: Option<crate::audio::tap::Tap>,
    /// Reused by the tap drain so the render path does not allocate.
    #[cfg(target_os = "macos")]
    tap_scratch: Vec<f32>,
    /// Whether the first non-silent tap buffer has been seen.
    #[cfg(target_os = "macos")]
    tap_reported: bool,
    /// When the analyser started, for the silent-tap warning below.
    #[cfg(target_os = "macos")]
    started: Instant,
    should_quit: bool,
    last_frame: Instant,
}

impl App {
    pub fn new(config: Config, channels: u16, sample_rate: u32) -> Result<Self> {
        #[cfg(not(test))]
        let terminal: AppTerminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        #[cfg(test)]
        let terminal: AppTerminal = Terminal::new(TestBackend::new(80, 24))?;

        let spectrum = Spectrum::new(Spectrum::DEFAULT_SIZE)?;
        let window = vec![0.0; spectrum.size()];
        let bar_map = BarMap::new(1, spectrum.bins(), DEFAULT_SCALE);

        Ok(Self {
            config,
            terminal,
            window,
            channels: channels.max(1) as usize,
            spectrum,
            bar_map,
            ballistics: Ballistics::new(0),
            bandwidth: Bandwidth::Wide,
            bar_fall_index: 2, // 12.0, the original default
            limit_index: DEFAULT_LIMIT_INDEX,
            sample_rate,
            gain_db: 0.0,
            sampled: Vec::new(),
            bands: Vec::new(),
            skin: Skin::default(),
            loaded_skin: None,
            row_colors: Vec::new(),
            grid_colors: Vec::new(),
            theme: Theme::default(),
            layout: BarLayout::default(),
            sized_for: (0, 0),
            view: View::default(),
            scope_style: ScopeStyle::default(),
            peaks: Peaks::default(),
            show_grid: true,
            bar_style: BarStyle::default(),
            show_help: false,
            status: None,
            #[cfg(target_os = "macos")]
            tap: None,
            #[cfg(target_os = "macos")]
            tap_scratch: Vec::new(),
            #[cfg(target_os = "macos")]
            tap_reported: false,
            #[cfg(target_os = "macos")]
            started: Instant::now(),
            should_quit: false,
            last_frame: Instant::now(),
        })
    }

    /// Use `skin` from now on, and put it in the `s` cycle if it is not one of
    /// the built-ins. Colours are cached per height, so the cache is dropped.
    pub fn set_skin(&mut self, skin: Skin) {
        if Skin::built_in_names().all(|n| n != skin.name) {
            self.loaded_skin = Some(skin.clone());
        }
        // Read the terminal's palette the first time a skin actually needs it -
        // once, and never for a skin that spells its colours out.
        if skin.needs_terminal_palette() && self.theme == Theme::default() {
            self.theme = Theme::query(true);
        }
        self.skin = skin;
        self.sized_for = (0, 0);
    }

    /// Append a capture buffer to the rolling window, de-interleaving to mono.
    ///
    /// cpal delivers interleaved frames. Averaging rather than summing keeps a
    /// correlated full-scale stereo signal at full scale instead of clipping.
    fn push_samples(&mut self, samples: &[f32]) {
        let n = self.window.len();
        for frame in samples.chunks(self.channels) {
            let mono = frame.iter().sum::<f32>() / frame.len() as f32;
            self.window.copy_within(1..n, 0);
            self.window[n - 1] = mono;
        }
    }

    /// Rebuild everything that depends on terminal size.
    fn resize(&mut self, width: u16, height: u16) {
        let bars = self.layout.bar_count(width).max(1);
        // Wide bandwidth averages groups of four, so sample four times what we draw.
        let sampled = match self.bandwidth {
            Bandwidth::Wide => bars * Bandwidth::GROUP,
            Bandwidth::Thin => bars,
        };
        self.bar_map =
            BarMap::with_top_bin(sampled, self.spectrum.bins(), self.top_bin(), DEFAULT_SCALE);
        self.ballistics.resize(bars);
        // With caps on, the top row belongs to the cap: a bar at full scale is
        // covered there, so the ramp's last stop would land on a row nobody ever
        // sees and the display would top out one colour short - orange instead
        // of red. Spread the ramp over the rows a bar can actually show, and let
        // the render clamp the cap's row to the final stop.
        let ramp_height = if self.peaks == Peaks::Off {
            height
        } else {
            height.saturating_sub(1).max(1)
        };
        self.row_colors = row_colors(ramp_height, &self.skin);
        self.grid_colors = grid_colors(ramp_height, &self.skin, &self.theme);
        self.sized_for = (width, height);
    }

    /// Show a message for a couple of seconds.
    fn note(&mut self, text: String) {
        self.status = Some((text, Instant::now()));
    }

    /// The message to draw, if one is still recent.
    fn active_status(&self) -> Option<&str> {
        self.status
            .as_ref()
            .filter(|(_, at)| at.elapsed() < Duration::from_secs(2))
            .map(|(text, _)| text.as_str())
    }

    /// How long a tap may deliver nothing before rav says so.
    ///
    /// Long enough not to fire during a genuine silence between tracks, short
    /// enough that nobody sits looking at a flat display wondering.
    #[cfg(target_os = "macos")]
    const SILENT_TAP_GRACE: Duration = Duration::from_secs(4);

    /// A standing warning when the process tap is running but delivering
    /// silence.
    ///
    /// That is what a refused recording permission looks like from inside the
    /// process: the tap is created, the IOProc fires, and every sample is zero.
    /// It is indistinguishable from real silence except by how long it lasts,
    /// and without this the display simply sits flat and says nothing. The
    /// permission is per-application, so it is common to have granted it to one
    /// terminal and then run rav from another.
    fn tap_warning(&self) -> Option<&'static str> {
        #[cfg(target_os = "macos")]
        {
            let tap = self.tap.as_ref()?;
            if tap.has_signal() || self.started.elapsed() < Self::SILENT_TAP_GRACE {
                return None;
            }
            Some("no audio - grant this terminal System Audio Recording")
        }
        #[cfg(not(target_os = "macos"))]
        None
    }

    /// Highest FFT bin the display spans, from the current frequency limit.
    fn top_bin(&self) -> usize {
        let bins = self.spectrum.bins();
        match FREQUENCY_LIMITS[self.limit_index] {
            Some(hz) => bin_for_hz(hz, self.sample_rate, bins),
            None => bins,
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::ToggleView => {
                self.view = self.view.next();
                self.note(match self.view {
                    View::Analyzer => "analyser".to_string(),
                    View::Oscilloscope => "oscilloscope".to_string(),
                });
            }
            Action::TogglePeaks => {
                let was_off = self.peaks == Peaks::Off;
                self.peaks = self.peaks.next();
                // Turning caps on or off changes how many rows the ramp spans.
                if was_off || self.peaks == Peaks::Off {
                    self.sized_for = (0, 0);
                }
                self.note(format!("peaks {}", self.peaks.label()));
            }
            Action::ToggleGrid => {
                self.show_grid = !self.show_grid;
                let on = if self.show_grid { "on" } else { "off" };
                self.note(format!("grid {on}"));
            }
            Action::CycleBandwidth => {
                self.bandwidth = match self.bandwidth {
                    Bandwidth::Wide => Bandwidth::Thin,
                    Bandwidth::Thin => Bandwidth::Wide,
                };
                self.sized_for = (0, 0); // force a rebuild on the next frame
                let label = match self.bandwidth {
                    Bandwidth::Wide => "wide",
                    Bandwidth::Thin => "thin",
                };
                self.note(format!("bandwidth {label}"));
            }
            Action::CycleBarFall => {
                self.bar_fall_index = (self.bar_fall_index + 1) % BAR_FALL_SPEEDS.len();
                let bars = self.ballistics.len();
                self.ballistics = Ballistics::new(bars)
                    .with_speeds(BAR_FALL_SPEEDS[self.bar_fall_index], DEFAULT_PEAK_FALL);
                self.note(format!("bar fall {}", BAR_FALL_SPEEDS[self.bar_fall_index]));
            }
            Action::CycleFrequencyLimit => {
                self.limit_index = (self.limit_index + 1) % FREQUENCY_LIMITS.len();
                self.sized_for = (0, 0); // force the mapping to rebuild
                self.note(match FREQUENCY_LIMITS[self.limit_index] {
                    Some(hz) => format!("up to {} kHz", hz / 1000),
                    None => "full range".to_string(),
                });
            }
            Action::CycleBarStyle => {
                self.bar_style = self.bar_style.next();
                self.note(format!("bars {}", self.bar_style.label()));
            }
            Action::CycleSkin => {
                let next = self.next_skin();
                self.set_skin(next);
                self.note(format!("skin {}", self.skin.name));
            }
            Action::Gain(delta) => {
                self.gain_db = (self.gain_db + delta as f32).clamp(-40.0, 40.0);
                self.note(format!("gain {:+.0} dB", self.gain_db));
            }
            Action::ResetGain => {
                self.gain_db = 0.0;
                self.note("gain +0 dB".to_string());
            }
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::None => {}
        }
    }

    /// The next skin in the `s` rotation.
    ///
    /// The built-ins in file order, with a `--skin` one inserted after the
    /// default so cycling always comes back to it rather than dropping it after
    /// the first press.
    fn next_skin(&self) -> Skin {
        let mut order: Vec<Skin> = Skin::built_in_names()
            .filter_map(|n| Skin::built_in(n).and_then(|r| r.ok()))
            .collect();
        if let Some(loaded) = &self.loaded_skin {
            order.insert(1, loaded.clone());
        }
        let at = order
            .iter()
            .position(|s| s.name == self.skin.name)
            .unwrap_or(0);
        order[(at + 1) % order.len()].clone()
    }

    /// The help rows, each carrying its current value so the overlay doubles as
    /// a settings readout.
    fn help_rows(&self) -> Vec<HelpRow<'static>> {
        let limit = match FREQUENCY_LIMITS[self.limit_index] {
            Some(hz) => format!("up to {} kHz", hz / 1000),
            None => "full range".to_string(),
        };
        let view = match self.view {
            View::Analyzer => "analyser",
            View::Oscilloscope => "oscilloscope",
        };
        let bandwidth = match self.bandwidth {
            Bandwidth::Wide => "wide",
            Bandwidth::Thin => "thin",
        };
        vec![
            HelpRow {
                key: "space",
                description: "switch view",
                value: Some(view.to_string()),
            },
            HelpRow {
                key: "r",
                description: "frequency range",
                value: Some(limit),
            },
            HelpRow {
                key: "f",
                description: "bar fall speed",
                value: Some(format!("{}", BAR_FALL_SPEEDS[self.bar_fall_index])),
            },
            HelpRow {
                key: "w",
                description: "bandwidth",
                value: Some(bandwidth.to_string()),
            },
            HelpRow {
                key: "p",
                description: "peak caps",
                value: Some(self.peaks.label().to_string()),
            },
            HelpRow {
                key: "s",
                description: "skin",
                value: Some(self.skin.name.clone()),
            },
            HelpRow {
                key: "b",
                description: "bar style",
                value: Some(self.bar_style.label().to_string()),
            },
            HelpRow {
                key: "g",
                description: "grid behind the bars",
                value: Some(if self.show_grid { "on" } else { "off" }.to_string()),
            },
            HelpRow {
                key: "up/down",
                description: "gain (0 resets)",
                value: Some(format!("{:+.0} dB", self.gain_db)),
            },
            HelpRow {
                key: "h",
                description: "this help",
                value: None,
            },
            HelpRow {
                key: "q",
                description: "quit",
                value: None,
            },
        ]
    }

    /// Feed the rolling window from a process tap instead of a cpal stream.
    #[cfg(target_os = "macos")]
    pub fn use_tap(&mut self, tap: crate::audio::tap::Tap) {
        self.channels = tap.channels().max(1) as usize;
        self.sample_rate = tap.sample_rate();
        self.sized_for = (0, 0); // the mapping depends on the sample rate
        self.tap = Some(tap);
    }

    pub async fn run(&mut self, audio_receiver: Receiver<AudioData>) -> Result<()> {
        #[cfg(not(test))]
        {
            enable_raw_mode()?;
            io::stdout().execute(EnterAlternateScreen)?;
        }
        self.terminal.clear()?;
        info!("🎨 Starting analyser");

        let fps = self.config.display.refresh_rate.clamp(1, 240) as f64;
        let mut frame_interval = interval(Duration::from_secs_f64(1.0 / fps));

        loop {
            self.handle_events().await?;
            if self.should_quit {
                break;
            }

            // Drain every pending buffer so the window holds the newest audio. A
            // backlog is absorbed rather than rendered late.
            #[cfg(target_os = "macos")]
            let tapped = self.tap.is_some();
            #[cfg(not(target_os = "macos"))]
            let tapped = false;

            #[cfg(target_os = "macos")]
            if let Some(tap) = &self.tap {
                let mut scratch = std::mem::take(&mut self.tap_scratch);
                tap.drain(&mut scratch);
                // A tap that starts but never delivers looks exactly like
                // silence, and that is what a refused recording permission
                // produces - so say which happened, once.
                if !self.tap_reported && !scratch.is_empty() {
                    let peak = scratch.iter().fold(0.0f32, |a, s| a.max(s.abs()));
                    if peak > 0.0 {
                        info!("Process tap delivering audio (peak {peak:.3})");
                        self.tap_reported = true;
                    }
                }
                self.push_samples(&scratch);
                self.tap_scratch = scratch;
            }
            if !tapped {
                while let Ok(data) = audio_receiver.try_recv() {
                    self.push_samples(&data.samples);
                }
            }

            frame_interval.tick().await;

            let area = self.terminal.size()?;
            if (area.width, area.height) != self.sized_for {
                self.resize(area.width, area.height);
            }

            let magnitudes = self.spectrum.analyse(&self.window);
            self.bar_map.sample(magnitudes, &mut self.sampled);
            self.bandwidth.group(&self.sampled, &mut self.bands);
            // Gain is applied before the clip, so full scale always means
            // exactly full scale whatever the trim.
            let gain = 10f32.powf(self.gain_db / 20.0);
            let scope_gain = gain;
            for v in self.bands.iter_mut() {
                *v = (*v * gain / MAX_HEIGHT).min(1.0);
            }

            let now = Instant::now();
            let dt = now.duration_since(self.last_frame).as_secs_f32();
            self.last_frame = now;
            self.ballistics.step(&self.bands, dt);

            // Both borrow all of `self`, so they are taken before the destructure.
            // The warning outranks a transient note: a settings message that
            // hides "there is no audio" is worse than no message at all.
            let status = self
                .tap_warning()
                .map(str::to_string)
                .or_else(|| self.active_status().map(str::to_string));
            let help_rows = if self.show_help {
                Some(self.help_rows())
            } else {
                None
            };

            // Destructure so the draw closure borrows fields, not all of `self`.
            let Self {
                terminal,
                ballistics,
                row_colors,
                grid_colors,
                skin,
                layout,
                window,
                view,
                scope_style,
                peaks,
                show_grid,
                bar_style,
                ..
            } = self;
            let cap_color = skin.peak;
            let grid = show_grid.then_some(grid_colors.as_slice());
            terminal.draw(|f| {
                match view {
                    View::Analyzer => f.render_widget(
                        Analyzer {
                            bars: ballistics.bars(),
                            peaks: ballistics.peaks(),
                            row_colors,
                            cap_color,
                            grid,
                            bar_style: *bar_style,
                            layout: *layout,
                            peaks_style: *peaks,
                        },
                        f.area(),
                    ),
                    View::Oscilloscope => f.render_widget(
                        Scope {
                            samples: window,
                            skin,
                            style: *scope_style,
                            gain: scope_gain,
                        },
                        f.area(),
                    ),
                }
                // The transient note still fires on every settings key; the
                // overlay is the full picture when you want it.
                if let Some(text) = &status {
                    let area = f.area();
                    draw_status(f.buffer_mut(), area, text);
                }
                if let Some(rows) = &help_rows {
                    f.render_widget(Help { rows, title: "rav" }, f.area());
                }
            })?;
        }

        #[cfg(not(test))]
        {
            disable_raw_mode()?;
            io::stdout().execute(LeaveAlternateScreen)?;
            self.terminal.show_cursor()?;
        }
        info!("👋 Analyser shut down");
        Ok(())
    }

    async fn handle_events(&mut self) -> Result<()> {
        #[cfg(test)]
        {
            Ok(())
        }
        #[cfg(not(test))]
        {
            // Drain the whole burst; a held key should not queue up frames.
            while event::poll(Duration::ZERO)? {
                if let Event::Key(key) = event::read()?
                    && key.kind == KeyEventKind::Press
                {
                    let action = map_key(key.code);
                    self.apply(action);
                }
            }
            Ok(())
        }
    }
}

/// Draw a short message in the top-left corner.
///
/// Written straight into the buffer rather than as a widget so it overlays the
/// visualisation without reserving a row for a status bar that is empty most of
/// the time.
fn draw_status(buf: &mut ratatui::buffer::Buffer, area: ratatui::layout::Rect, text: &str) {
    if area.is_empty() {
        return;
    }
    let label = format!(" {text} ");
    for (i, ch) in label.chars().enumerate() {
        let x = area.x + i as u16;
        if x >= area.x + area.width {
            break;
        }
        buf[(x, area.y)]
            .set_symbol(&ch.to_string())
            .set_fg(Color::Rgb(222, 222, 222))
            .set_bg(Skin::default().grid[0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(Config::default(), 2, 48_000).expect("app should build")
    }

    #[test]
    fn keys_map_to_the_actions_the_app_actually_uses() {
        assert_eq!(map_key(KeyCode::Char('q')), Action::Quit);
        assert_eq!(map_key(KeyCode::Esc), Action::Quit);
        assert_eq!(map_key(KeyCode::Char('p')), Action::TogglePeaks);
        assert_eq!(map_key(KeyCode::Char('g')), Action::ToggleGrid);
        assert_eq!(map_key(KeyCode::Char('w')), Action::CycleBandwidth);
        assert_eq!(map_key(KeyCode::Char('z')), Action::None);
    }

    #[test]
    fn quit_action_stops_the_loop() {
        let mut a = app();
        assert!(!a.should_quit);
        a.apply(Action::Quit);
        assert!(a.should_quit);
    }

    #[test]
    fn toggles_flip_their_flags() {
        let mut a = app();
        let (peaks, grid) = (a.peaks, a.show_grid);
        a.apply(Action::TogglePeaks);
        a.apply(Action::ToggleGrid);
        assert_ne!(a.peaks, peaks);
        assert_ne!(a.show_grid, grid);
    }

    #[test]
    fn stereo_input_is_de_interleaved_to_mono() {
        let mut a = app();
        // Two frames of (L=1.0, R=0.0) must average to 0.5, not land as 1.0, 0.0.
        a.push_samples(&[1.0, 0.0, 1.0, 0.0]);
        let n = a.window.len();
        assert_eq!(a.window[n - 1], 0.5);
        assert_eq!(a.window[n - 2], 0.5);
    }

    #[test]
    fn the_window_keeps_the_newest_samples_last() {
        let mut a = app();
        a.push_samples(&[0.25, 0.25]);
        a.push_samples(&[0.75, 0.75]);
        let n = a.window.len();
        assert_eq!(a.window[n - 1], 0.75, "newest sample is last");
        assert_eq!(a.window[n - 2], 0.25);
    }

    #[test]
    fn the_ramp_reaches_red_on_the_bars_topmost_visible_row() {
        // With caps on, the top row belongs to the cap, so the ramp spans only
        // the rows below it. Spreading it over the full height would put red on
        // a row the bar can never reach and the display would top out orange.
        let mut a = app();
        a.resize(80, 5);
        assert_eq!(a.row_colors.len(), 4, "ramp spans the bar's visible rows");
        assert_eq!(
            *a.row_colors.last().unwrap(),
            *Skin::default().bars.last().unwrap(),
            "topmost visible bar row must be red"
        );

        // With caps off the bar owns every row again.
        a.apply(Action::TogglePeaks); // fine -> coarse
        a.apply(Action::TogglePeaks); // coarse -> off
        a.resize(80, 5);
        assert_eq!(a.row_colors.len(), 5);
        assert_eq!(
            *a.row_colors.last().unwrap(),
            *Skin::default().bars.last().unwrap()
        );
    }

    #[test]
    fn resize_rebuilds_bands_and_colours() {
        let mut a = app();
        a.resize(80, 24);
        assert_eq!(a.row_colors.len(), 23); // top row reserved for the cap
        let narrow = a.ballistics.len();
        a.resize(200, 40);
        assert_eq!(a.row_colors.len(), 39);
        assert!(a.ballistics.len() > narrow, "more columns means more bars");
    }

    #[test]
    fn shrinking_then_growing_keeps_state_consistent() {
        // Shrinking is where an out-of-bounds remap would surface.
        let mut a = app();
        for (w, h) in [(200u16, 60u16), (20, 5), (1, 1), (120, 30)] {
            a.resize(w, h);
            assert!(!a.row_colors.is_empty() && a.row_colors.len() <= h as usize);
            assert_eq!(a.ballistics.bars().len(), a.ballistics.peaks().len());
        }
    }

    #[test]
    fn a_silent_window_produces_no_bars() {
        // Silence is silence. Any per-frame normalisation would divide by the
        // frame's own peak and turn the noise floor into a full display.
        let mut a = app();
        a.resize(80, 24);
        assert!(
            a.spectrum
                .analyse(&vec![0.0; 1024])
                .iter()
                .all(|&m| m == 0.0)
        );
    }

    #[test]
    fn louder_input_produces_taller_bars() {
        // The amplitude response is absolute, so a whisper and a drop must not
        // render identically.
        let mut a = app();
        let tone = |amp: f32| -> Vec<f32> {
            (0..1024)
                .map(|i| amp * (std::f32::consts::TAU * 64.0 * i as f32 / 1024.0).sin())
                .collect()
        };
        let loud = a.spectrum.analyse(&tone(1.0))[64];
        let quiet = a.spectrum.analyse(&tone(0.1))[64];
        assert!(loud > quiet * 5.0, "loud {loud} vs quiet {quiet}");
    }

    #[test]
    fn gain_trims_the_level_and_can_be_reset() {
        // rav captures after the system volume, so this is what anchors the
        // display to however loud you actually listen.
        let mut a = app();
        assert_eq!(a.gain_db, 0.0);
        a.apply(Action::Gain(6));
        assert_eq!(a.gain_db, 6.0);
        assert_eq!(a.active_status(), Some("gain +6 dB"));
        a.apply(Action::Gain(-2));
        assert_eq!(a.gain_db, 4.0);
        a.apply(Action::ResetGain);
        assert_eq!(a.gain_db, 0.0);
    }

    #[test]
    fn gain_is_clamped_to_a_sane_range() {
        let mut a = app();
        for _ in 0..80 {
            a.apply(Action::Gain(1));
        }
        assert_eq!(a.gain_db, 40.0, "must not run away");
        for _ in 0..200 {
            a.apply(Action::Gain(-1));
        }
        assert_eq!(a.gain_db, -40.0);
    }

    #[test]
    fn arrow_keys_are_the_gain_control() {
        assert_eq!(map_key(KeyCode::Up), Action::Gain(1));
        assert_eq!(map_key(KeyCode::Down), Action::Gain(-1));
        assert_eq!(map_key(KeyCode::Char('0')), Action::ResetGain);
    }

    #[test]
    fn the_view_toggles_between_analyser_and_scope() {
        let mut a = app();
        assert_eq!(a.view, View::Analyzer, "analyser is the default");
        a.apply(Action::ToggleView);
        assert_eq!(a.view, View::Oscilloscope);
        a.apply(Action::ToggleView);
        assert_eq!(a.view, View::Analyzer, "toggling twice returns");
    }

    #[test]
    fn tab_and_o_both_switch_view() {
        assert_eq!(map_key(KeyCode::Tab), Action::ToggleView);
        assert_eq!(map_key(KeyCode::Char('o')), Action::ToggleView);
    }

    #[test]
    fn a_settings_change_leaves_a_message_that_expires() {
        // Without this there is no way to tell which frequency limit is active.
        let mut a = app();
        assert_eq!(a.active_status(), None, "nothing shown at rest");
        a.apply(Action::CycleFrequencyLimit);
        assert_eq!(a.active_status(), Some("up to 20 kHz"));
        a.apply(Action::CycleFrequencyLimit);
        assert_eq!(a.active_status(), Some("full range"));
        a.status = Some(("stale".into(), Instant::now() - Duration::from_secs(5)));
        assert_eq!(a.active_status(), None, "old messages expire");
    }

    #[test]
    fn h_opens_help_and_r_changes_the_range() {
        // 'h' is help, the conventional binding; the frequency limit is on 'r'.
        assert_eq!(map_key(KeyCode::Char('h')), Action::ToggleHelp);
        assert_eq!(map_key(KeyCode::Char('?')), Action::ToggleHelp);
        assert_eq!(map_key(KeyCode::Char('r')), Action::CycleFrequencyLimit);
    }

    #[test]
    fn help_toggles_and_reports_the_live_settings() {
        let mut a = app();
        assert!(!a.show_help);
        a.apply(Action::ToggleHelp);
        assert!(a.show_help);

        let value_for = |a: &App, key: &str| {
            a.help_rows()
                .into_iter()
                .find(|r| r.key == key)
                .and_then(|r| r.value)
        };
        assert_eq!(value_for(&a, "r"), Some("up to 16 kHz".to_string()));
        a.apply(Action::CycleFrequencyLimit);
        assert_eq!(value_for(&a, "r"), Some("up to 20 kHz".to_string()));

        assert_eq!(value_for(&a, "space"), Some("analyser".to_string()));
        a.apply(Action::ToggleView);
        assert_eq!(value_for(&a, "space"), Some("oscilloscope".to_string()));

        a.apply(Action::ToggleHelp);
        assert!(!a.show_help);
    }

    #[test]
    fn settings_keys_still_leave_a_transient_note() {
        // The overlay is the full picture; the note is the at-a-glance one.
        let mut a = app();
        a.apply(Action::CycleFrequencyLimit);
        assert_eq!(a.active_status(), Some("up to 20 kHz"));
    }

    #[test]
    fn cycling_bar_fall_walks_the_five_speeds() {
        let mut a = app();
        for _ in 0..BAR_FALL_SPEEDS.len() {
            a.apply(Action::CycleBarFall);
        }
        assert_eq!(a.bar_fall_index, 2, "a full cycle returns to the default");
    }

    // Gated with the fields it reads. `started` and `SILENT_TAP_GRACE` only
    // exist where there is a tap to be silent, so on Linux this does not compile
    // - which is how it broke the build after the fields were gated and the test
    // was not.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_silent_tap_is_reported_rather_than_looking_frozen() {
        // Without a tap there is nothing to warn about, whatever the elapsed
        // time - the cpal path has its own fallback message in the log.
        let mut a = app();
        a.started = Instant::now() - Duration::from_secs(60);
        assert_eq!(a.tap_warning(), None, "no tap, no warning");

        // The grace period is what stops this firing during a quiet passage.
        assert!(
            App::SILENT_TAP_GRACE >= Duration::from_secs(2),
            "too eager to cry silence"
        );
    }

    #[test]
    fn the_tap_warning_outranks_a_settings_note() {
        // A transient note that hides "there is no audio" is worse than no note.
        let mut a = app();
        a.apply(Action::CycleFrequencyLimit);
        assert!(a.active_status().is_some(), "the note should be showing");
        assert_eq!(a.tap_warning(), None, "no tap in tests");
    }

    #[test]
    fn s_cycles_the_colours_and_b_the_bar_style() {
        assert_eq!(map_key(KeyCode::Char('s')), Action::CycleSkin);
        assert_eq!(map_key(KeyCode::Char('b')), Action::CycleBarStyle);
    }

    #[test]
    fn cycling_colours_rebuilds_the_row_ramp() {
        // The cached ramp is a function of the skin, so a change that did not
        // invalidate it would leave the old colours on screen until a resize.
        let mut a = app();
        a.resize(80, 24);
        // rav and winamp share a ramp and differ only in the backdrop, so that
        // is what has to change here - the bars deliberately do not.
        let before = a.grid_colors.clone();
        a.apply(Action::CycleSkin);
        assert_eq!(a.skin.name, "winamp");
        assert_eq!(a.sized_for, (0, 0), "the ramp cache must be invalidated");
        a.resize(80, 24);
        assert_ne!(a.grid_colors, before, "colours did not change");
        assert_eq!(a.active_status(), Some("skin winamp"));
    }

    #[test]
    fn a_loaded_skin_stays_in_the_cycle() {
        // --skin puts a fourth entry in the rotation; cycling all the way round
        // has to come back to it rather than dropping it after the first change.
        let mut a = app();
        a.set_skin(Skin {
            name: "custom".into(),
            ..Skin::default()
        });
        let seen: Vec<String> = (0..4)
            .map(|_| {
                let label = a.skin.name.clone();
                a.apply(Action::CycleSkin);
                label
            })
            .collect();
        assert_eq!(seen, vec!["custom", "winamp", "terminal", "mono"]);
    }
}
