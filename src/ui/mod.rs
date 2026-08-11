pub mod analyzer;
pub mod help;
pub mod scale;
pub mod scope;
pub mod status;

use crate::{
    audio::AudioData,
    config::Config,
    render::Ink,
    signal::{
        ballistics::{BAR_FALL_SPEEDS, Ballistics, DEFAULT_PEAK_FALL},
        mapping::{
            Bandwidth, BarMap, DEFAULT_LIMIT_INDEX, DEFAULT_SCALE, FREQUENCY_LIMITS, bin_for_hz,
        },
        spectrum::{MAX_HEIGHT, Spectrum},
    },
    visual::{Palette, Theme},
};
use analyzer::{Analyzer, BarLayout, BarStyle, Peaks, grid_colors, row_colors};
use anyhow::Result;
// The event types are named by the render loop whichever backend it is driving,
// so only the parts that touch a real terminal are gated.
use crossterm::event::{Event, KeyCode, KeyEventKind};
#[cfg(not(test))]
use crossterm::{
    ExecutableCommand, event,
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
use status::Status;
#[cfg(not(test))]
use std::io::{self, Stdout};
use std::time::{Duration, Instant};
use tokio::time::{MissedTickBehavior, interval};
use tracing::{debug, info};

#[cfg(test)]
type AppTerminal = Terminal<TestBackend>;
#[cfg(not(test))]
type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

/// The only place a theme's colour meets the terminal's idea of one.
///
/// A named colour stays named all the way here and is handed over as a *slot*,
/// so the terminal paints it from whatever palette the user runs - which is the
/// whole reason the `terminal` and `mono` themes exist. Resolving it to RGB
/// earlier would replace their colours with ones rav chose.
///
/// ratatui spells slot 7 `Gray` and slot 15 `White`; a theme calls them `white`
/// and `bright-white`, which is what the author reads off their configuration.
/// The disagreement is about naming, not about which colour, and it is confined
/// to this table.
pub fn to_color(ink: Ink) -> Color {
    match ink {
        Ink::Rgb(r, g, b) => Color::Rgb(r, g, b),
        Ink::Ansi(slot) => match slot & 0x0f {
            0 => Color::Black,
            1 => Color::Red,
            2 => Color::Green,
            3 => Color::Yellow,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::Gray,
            8 => Color::DarkGray,
            9 => Color::LightRed,
            10 => Color::LightGreen,
            11 => Color::LightYellow,
            12 => Color::LightBlue,
            13 => Color::LightMagenta,
            14 => Color::LightCyan,
            _ => Color::White,
        },
    }
}

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
    CycleTheme,
    /// Trim in whole dB steps; kept an integer so `Action` stays `Eq`.
    Gain(i8),
    ResetGain,
    /// Widen or narrow the bars. Integral for the same reason as `Gain`.
    BarSize(i8),
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
        KeyCode::Char('t') | KeyCode::Char('T') => Action::CycleTheme,
        KeyCode::Char('f') | KeyCode::Char('F') => Action::CycleBarFall,
        KeyCode::Char('r') | KeyCode::Char('R') => Action::CycleFrequencyLimit,
        KeyCode::Up => Action::Gain(1),
        KeyCode::Down => Action::Gain(-1),
        KeyCode::Char('0') => Action::ResetGain,
        // `=` as well as `+`: `+` needs Shift on most layouts, and every browser
        // and terminal already takes `=` for zoom-in.
        KeyCode::Char('+') | KeyCode::Char('=') => Action::BarSize(1),
        KeyCode::Char('-') => Action::BarSize(-1),
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

    theme: Theme,
    /// A theme loaded from disk by `--theme`, kept so the `t` cycle returns to it.
    loaded_theme: Option<Theme>,
    row_colors: Vec<Ink>,
    /// Backdrop colour per row, cached with `row_colors` and rebuilt with it.
    grid_colors: Vec<Ink>,
    /// The terminal's own palette, read once at startup. A theme that names a
    /// colour rather than spelling it out needs this to be adjustable at all.
    palette: Palette,
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
            theme: Theme::default(),
            loaded_theme: None,
            row_colors: Vec::new(),
            grid_colors: Vec::new(),
            palette: Palette::default(),
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

    /// Use `theme` from now on, and put it in the `s` cycle if it is not one of
    /// the built-ins. Colours are cached per height, so the cache is dropped.
    pub fn set_theme(&mut self, theme: Theme) {
        if Theme::built_in_names().all(|n| n != theme.name) {
            self.loaded_theme = Some(theme.clone());
        }
        // Read the terminal's palette the first time a theme actually needs it -
        // once, and never for a theme that spells its colours out.
        if theme.needs_terminal_palette() && self.palette == Palette::default() {
            self.palette = Palette::query(true);
        }
        self.theme = theme;
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
        self.row_colors = row_colors(ramp_height, &self.theme);
        self.grid_colors = grid_colors(ramp_height, &self.theme, &self.palette);
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
            Action::CycleTheme => {
                let next = self.next_theme();
                self.set_theme(next);
                self.note(format!("theme {}", self.theme.name));
            }
            Action::Gain(delta) => {
                self.gain_db = (self.gain_db + delta as f32).clamp(-40.0, 40.0);
                self.note(format!("gain {:+.0} dB", self.gain_db));
            }
            Action::ResetGain => {
                self.gain_db = 0.0;
                self.note("gain +0 dB".to_string());
            }
            Action::BarSize(delta) => {
                self.layout.resize(delta);
                // Bar width decides how many bands fit, so the mapping, the
                // ballistics and both colour ramps are all stale now. Zeroing
                // this is what forces the next frame through `resize`.
                self.sized_for = (0, 0);
                self.note(format!("bar size {}", self.layout.bar_width));
            }
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::None => {}
        }
    }

    /// The next theme in the `s` rotation.
    ///
    /// The built-ins in file order, with a `--theme` one inserted after the
    /// default so cycling always comes back to it rather than dropping it after
    /// the first press.
    fn next_theme(&self) -> Theme {
        let mut order: Vec<Theme> = Theme::built_in_names()
            .filter_map(|n| Theme::built_in(n).and_then(|r| r.ok()))
            .collect();
        if let Some(loaded) = &self.loaded_theme {
            order.insert(1, loaded.clone());
        }
        let at = order
            .iter()
            .position(|s| s.name == self.theme.name)
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
                key: "t",
                description: "theme",
                value: Some(self.theme.name.clone()),
            },
            HelpRow {
                key: "+ / -",
                description: "bar size",
                value: Some(self.layout.bar_width.to_string()),
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

        // The audio device is the clock. Every wake below is something actually
        // happening - a buffer arriving, a key, or the watchdog - so the loop
        // never renders a frame nobody asked for, and a buffer is drawn as it
        // lands rather than waiting out the remainder of a tick.
        let keys = key_reader();

        // A ceiling, not a cadence. A device with very small buffers could
        // deliver faster than the terminal can usefully repaint; audio is still
        // ingested on every wake, only the drawing is coalesced.
        //
        // Kept as a moving deadline rather than "has `min_frame` passed since
        // the last frame", because that form quantises to the wake interval and
        // loses whatever does not divide it: buffers arriving every 5.2ms take
        // four to clear a 16.7ms gap, which turns a 60fps ceiling into 48. The
        // deadline carries the remainder instead, so the average comes out at
        // the rate that was asked for.
        let fps = self.config.display.refresh_rate.clamp(1, 240) as f64;
        let min_frame = Duration::from_secs_f64(1.0 / fps);
        let mut next_frame = Instant::now();

        // Only for a device that stops delivering *entirely*. A running device
        // sends silent buffers while nothing is playing, so the ballistics keep
        // falling on their own; this is what stops the display freezing
        // mid-decay if the stream dies, and it is deliberately slow because in
        // every healthy case it never fires.
        let mut watchdog = interval(WATCHDOG);
        watchdog.set_missed_tick_behavior(MissedTickBehavior::Delay);
        watchdog.tick().await; // the first tick is immediate

        #[cfg(target_os = "macos")]
        let tapped = self.tap.is_some();
        #[cfg(not(target_os = "macos"))]
        let tapped = false;

        // Wakes and frames a second, at debug level. They differ on purpose - a
        // wake landing inside the frame ceiling is coalesced - and the gap
        // between them is the only way to see the ceiling doing its job. The
        // frame count is also the audio device's buffer rate, so this answers
        // "why is it running at that speed" without a profiler.
        let mut wakes = 0u64;
        let mut frames = 0u64;
        let mut counted_from = Instant::now();

        loop {
            if self.should_quit {
                break;
            }

            if counted_from.elapsed() >= Duration::from_secs(1) {
                debug!("{frames} frames from {wakes} wakes");
                wakes = 0;
                frames = 0;
                counted_from = Instant::now();
            }
            wakes += 1;

            // Wait for something to happen. No sleep, no polling: each arm is a
            // source that wakes the loop when it has something, and the OS does
            // the waiting.
            #[cfg(target_os = "macos")]
            let tap_ready = async {
                match &self.tap {
                    Some(tap) => tap.ready().await,
                    // A pending future, so this arm simply never fires when
                    // there is no tap rather than spinning on a ready `()`.
                    None => std::future::pending().await,
                }
            };
            #[cfg(not(target_os = "macos"))]
            let tap_ready = std::future::pending::<()>();

            tokio::select! {
                // Biased so a quit key is not left waiting behind a backlog of
                // audio on a loaded machine.
                biased;
                Ok(event) = keys.recv_async() => {
                    if let Event::Key(key) = event
                        && key.kind == KeyEventKind::Press
                    {
                        self.apply(map_key(key.code));
                    }
                }
                () = tap_ready => {}
                Ok(data) = audio_receiver.recv_async(), if !tapped => {
                    self.push_samples(&data.samples);
                }
                _ = watchdog.tick() => {}
            }

            // Drain the rest of the burst. A held key repeats faster than the
            // display needs to change, and taking one per pass would spend a
            // frame on each instead of settling and drawing the result once.
            while let Ok(event) = keys.try_recv() {
                if let Event::Key(key) = event
                    && key.kind == KeyEventKind::Press
                {
                    self.apply(map_key(key.code));
                }
            }

            // Drain whatever else is queued, so a backlog is absorbed into the
            // window rather than rendered one stale frame at a time.
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

            if self.should_quit {
                break;
            }

            let area = self.terminal.size()?;
            if (area.width, area.height) != self.sized_for {
                self.resize(area.width, area.height);
            }

            // Measure on the audio's schedule, not the display's.
            //
            // This used to sit below the frame ceiling, so a coalesced wake
            // skipped the analysis with it. That is harmless while the only
            // consumer is an animation, but it makes the *interval between
            // spectra* a property of the display - frame rate, a terminal
            // stall, whether two buffers happened to arrive inside one frame.
            // Anything that measures across time rather than within one frame,
            // an onset detector above all, would inherit that jitter into its
            // measurement rather than into how it looks.
            //
            // The ballistics step comes with it, for the same reason and a
            // plainer one: it is framerate-independent because it integrates
            // dt, and stepping it on the display's cadence rather than the
            // signal's threw away the accuracy that buys.
            let magnitudes = self.spectrum.analyse(&self.window);
            self.bar_map.sample(magnitudes, &mut self.sampled);
            self.bandwidth.group(&self.sampled, &mut self.bands);
            // Gain is applied before the clip, so full scale always means
            // exactly full scale whatever the trim.
            let gain = 10f32.powf(self.gain_db / 20.0);
            for v in self.bands.iter_mut() {
                *v = (*v * gain / MAX_HEIGHT).min(1.0);
            }

            let measured_at = Instant::now();
            let dt = measured_at.duration_since(self.last_frame).as_secs_f32();
            self.last_frame = measured_at;
            self.ballistics.step(&self.bands, dt);

            // Only the drawing is capped. Everything above ran on the signal.
            if measured_at < next_frame {
                continue;
            }
            next_frame += min_frame;
            // A device slower than the ceiling, or a stall, would otherwise
            // leave the deadline in the past and owing frames it would then take
            // back to back. There is no debt worth paying here: the next frame
            // shows the current state either way.
            if next_frame < measured_at {
                next_frame = measured_at + min_frame;
            }
            frames += 1;
            let scope_gain = gain;

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
                theme,
                layout,
                window,
                view,
                scope_style,
                peaks,
                show_grid,
                bar_style,
                ..
            } = self;
            let cap_color = theme.peak;
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
                            theme,
                            style: *scope_style,
                            gain: scope_gain,
                        },
                        f.area(),
                    ),
                }
                // The transient note still fires on every settings key; the
                // overlay is the full picture when you want it.
                if let Some(text) = &status {
                    f.render_widget(
                        Status {
                            text,
                            foreground: Color::Rgb(222, 222, 222),
                            // The floor of the *active* theme, so the note sits
                            // on the backdrop the user is looking at.
                            background: to_color(theme.grid[0]),
                        },
                        f.area(),
                    );
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
}

/// How long the display may go without an audio buffer before it redraws anyway.
///
/// Long on purpose. A running device delivers silent buffers when nothing is
/// playing, so this never fires in a healthy session - it exists so a stream
/// that dies outright leaves the bars resting on the floor rather than frozen
/// part-way down.
const WATCHDOG: Duration = Duration::from_millis(250);

/// Terminal events, on a channel the render loop can wait on.
///
/// A thread rather than a poll: `event::read` blocks until there is genuinely a
/// key, so the OS does the waiting and no cadence has to be guessed. crossterm's
/// own `EventStream` is this same thread with a `Stream` around it, and would
/// cost a futures dependency to await.
///
/// Never joined. It is parked in a read on a terminal that outlives the loop,
/// and there is nothing to clean up: the channel closes when the receiver drops
/// and the thread ends at the next keystroke, or with the process.
fn key_reader() -> flume::Receiver<Event> {
    let (tx, rx) = flume::unbounded();
    #[cfg(not(test))]
    std::thread::spawn(move || {
        while let Ok(event) = event::read() {
            if tx.send(event).is_err() {
                break;
            }
        }
    });
    #[cfg(test)]
    drop(tx);
    rx
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
            *Theme::default().bars.last().unwrap(),
            "topmost visible bar row must be red"
        );

        // With caps off the bar owns every row again.
        a.apply(Action::TogglePeaks); // fine -> coarse
        a.apply(Action::TogglePeaks); // coarse -> off
        a.resize(80, 5);
        assert_eq!(a.row_colors.len(), 5);
        assert_eq!(
            *a.row_colors.last().unwrap(),
            *Theme::default().bars.last().unwrap()
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
        assert_eq!(map_key(KeyCode::Char('t')), Action::CycleTheme);
        assert_eq!(map_key(KeyCode::Char('b')), Action::CycleBarStyle);
    }

    #[test]
    fn cycling_colours_rebuilds_the_row_ramp() {
        // The cached ramp is a function of the theme, so a change that did not
        // invalidate it would leave the old colours on screen until a resize.
        let mut a = app();
        a.resize(80, 24);
        // rav and winamp share a ramp and differ only in the backdrop, so that
        // is what has to change here - the bars deliberately do not.
        let before = a.grid_colors.clone();
        a.apply(Action::CycleTheme);
        assert_eq!(a.theme.name, "winamp");
        assert_eq!(a.sized_for, (0, 0), "the ramp cache must be invalidated");
        a.resize(80, 24);
        assert_ne!(a.grid_colors, before, "colours did not change");
        assert_eq!(a.active_status(), Some("theme winamp"));
    }

    #[test]
    fn a_named_colour_reaches_the_terminal_still_named() {
        // The whole reason the `terminal` and `mono` themes work. A name is a
        // deferral - "whatever green is here" - so it has to arrive at ratatui
        // as a slot the terminal paints, not as an RGB value rav chose. Resolve
        // it anywhere earlier and those themes quietly stop following the
        // user's colours while still looking plausible.
        assert_eq!(to_color(Ink::from_name("green").unwrap()), Color::Green);
        assert_eq!(
            to_color(Ink::from_name("bright-white").unwrap()),
            Color::White
        );
        // ratatui calls slot 7 `Gray`; a theme calls it `white`. Same slot.
        assert_eq!(to_color(Ink::from_name("white").unwrap()), Color::Gray);
        assert_eq!(to_color(Ink::Rgb(1, 2, 3)), Color::Rgb(1, 2, 3));
    }

    #[test]
    fn an_unanswered_palette_leaves_a_named_backdrop_named() {
        // `Palette::default()` is a terminal that said nothing, which is the
        // common case. Darkening has no number to work with, so the colour must
        // pass through untouched rather than being replaced by a guess.
        let theme = Theme::load("terminal").expect("built in");
        assert!(theme.needs_terminal_palette());
        let grid = grid_colors(16, &theme, &Palette::default());
        assert!(
            grid.iter().all(|c| !c.is_exact()),
            "an unanswered palette must not turn names into RGB"
        );
    }

    #[test]
    fn bar_size_keys_widen_and_narrow() {
        assert_eq!(map_key(KeyCode::Char('+')), Action::BarSize(1));
        // `=` shares the key with `+` on most layouts, so it means the same.
        assert_eq!(map_key(KeyCode::Char('=')), Action::BarSize(1));
        assert_eq!(map_key(KeyCode::Char('-')), Action::BarSize(-1));

        let mut a = app();
        let started = a.layout.bar_width;
        a.apply(Action::BarSize(1));
        assert_eq!(a.layout.bar_width, started + 1);
        a.apply(Action::BarSize(-1));
        assert_eq!(a.layout.bar_width, started);
    }

    #[test]
    fn bar_size_stops_at_both_ends() {
        // Narrower than one column is not a bar, and past the maximum a normal
        // terminal holds so few bands it stops being a spectrum.
        let mut a = app();
        for _ in 0..64 {
            a.apply(Action::BarSize(-1));
        }
        assert_eq!(a.layout.bar_width, analyzer::MIN_BAR_WIDTH);
        for _ in 0..64 {
            a.apply(Action::BarSize(1));
        }
        assert_eq!(a.layout.bar_width, analyzer::MAX_BAR_WIDTH);
    }

    #[test]
    fn resizing_the_bars_invalidates_the_cached_layout() {
        // Bar width decides the band count, so the mapping, the ballistics and
        // both colour ramps are stale afterwards. Forgetting this leaves the
        // old ramp on screen until the terminal itself is resized, which is the
        // kind of bug that only shows up on someone else's machine.
        let mut a = app();
        a.resize(80, 24);
        assert_eq!(a.sized_for, (80, 24));
        a.apply(Action::BarSize(1));
        assert_eq!(a.sized_for, (0, 0), "the next frame must rebuild");
    }

    #[test]
    fn a_wider_bar_means_fewer_of_them() {
        // Against the layout rather than `bands`, which the frame loop fills.
        let mut a = app();
        let narrow = a.layout.bar_count(80);
        a.apply(Action::BarSize(4));
        let wide = a.layout.bar_count(80);
        assert!(
            wide < narrow,
            "80 columns held {narrow} bars, then {wide} after widening"
        );
    }

    #[test]
    fn a_loaded_theme_stays_in_the_cycle() {
        // --theme puts a fourth entry in the rotation; cycling all the way round
        // has to come back to it rather than dropping it after the first change.
        let mut a = app();
        a.set_theme(Theme {
            name: "custom".into(),
            ..Theme::default()
        });
        let seen: Vec<String> = (0..4)
            .map(|_| {
                let label = a.theme.name.clone();
                a.apply(Action::CycleTheme);
                label
            })
            .collect();
        assert_eq!(seen, vec!["custom", "winamp", "terminal", "mono"]);
    }
}
