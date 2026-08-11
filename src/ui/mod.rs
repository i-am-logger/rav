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
    surface::Chosen,
    units::{Curve, Level},
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
pub enum Visualisation {
    #[default]
    Analyzer,
    Oscilloscope,
}

impl Visualisation {
    fn next(self) -> Self {
        match self {
            Visualisation::Analyzer => Visualisation::Oscilloscope,
            Visualisation::Oscilloscope => Visualisation::Analyzer,
        }
    }
}

/// What a keypress means. Kept separate from the handler so tests exercise the
/// real mapping instead of a copy of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    CycleVisualisation,
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
            Action::CycleVisualisation
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

    /// How a measured amplitude becomes a level to draw, from the preset.
    ///
    /// Linear on every screen rav ships for. It is a field rather than a
    /// constant because a four-light strip cannot afford linear - three of its
    /// four rungs would sit in the top 12 dB - and that is a property of the
    /// hardware a preset is written for, not of the renderer.
    curve: Curve,

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

    /// Which surface rav would draw on, and why.
    ///
    /// Reported, not obeyed - the glyph renderer draws every frame whatever this
    /// says. It is here so the choice is visible in the help overlay for a beta
    /// before anything depends on it being right.
    surface: Chosen,

    visualisation: Visualisation,
    scope_style: ScopeStyle,
    peaks: Peaks,
    show_grid: bool,
    bar_style: BarStyle,
    show_help: bool,
    /// Briefly shown after a settings key, so a change is visible without a
    /// permanent status bar cluttering the display.
    status: Option<(String, Instant)>,
    /// What rav is listening to, for the help overlay to name.
    ///
    /// "Why are the bars showing the wrong thing" has one answer and it is this,
    /// and until now it was only in `rav.log`. A microphone hears the room, so a
    /// display fed by one moves convincingly while showing nothing of what is
    /// playing - which looks like rav working rather than rav on the wrong
    /// source.
    source: Option<String>,
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

        // Everything about how rav looks and feels on startup comes from one
        // place. Four constants scattered through this constructor is how they
        // drift from the preset that claims to describe them - and how a
        // "default" ends up meaning something different in two files.
        let preset = rav_appearance::preset::RAV;
        // The dial starts where the preset sits, so cycling `f` moves from what
        // rav actually opened with rather than from a hardcoded index that
        // happens to agree today.
        let bar_fall_index = BAR_FALL_SPEEDS
            .iter()
            .position(|&speed| speed == preset.ballistics.bar_fall)
            .unwrap_or(0);

        Ok(Self {
            config,
            terminal,
            window,
            channels: channels.max(1) as usize,
            spectrum,
            bar_map,
            ballistics: Ballistics::new(0)
                .with_speeds(preset.ballistics.bar_fall, preset.ballistics.peak_fall)
                .with_reference_fps(preset.ballistics.reference_fps),
            bandwidth: Bandwidth::Wide,
            bar_fall_index,
            limit_index: DEFAULT_LIMIT_INDEX,
            sample_rate,
            gain_db: 0.0,
            curve: preset.curve,
            sampled: Vec::new(),
            bands: Vec::new(),
            theme: Theme::from(preset.theme),
            loaded_theme: None,
            row_colors: Vec::new(),
            grid_colors: Vec::new(),
            palette: Palette::default(),
            layout: BarLayout::default(),
            sized_for: (0, 0),
            surface: Chosen::UNASKED,
            visualisation: Visualisation::default(),
            scope_style: ScopeStyle::default(),
            peaks: Peaks::default(),
            show_grid: true,
            bar_style: BarStyle::default(),
            show_help: false,
            status: None,
            source: None,
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
            self.palette = crate::visual::palette::query(true);
        }
        self.theme = theme;
        self.sized_for = (0, 0);
    }

    /// Record which surface was chosen, for the help overlay to report.
    ///
    /// Nothing downstream reads it yet. Deciding is separate from drawing so the
    /// probe that makes the decision can be in users' hands - and shown to be
    /// harmless - a beta before the renderer that would act on it.
    pub fn set_surface(&mut self, chosen: Chosen) {
        self.surface = chosen;
    }

    /// Turn the rolling window into a level per band, and report the gain.
    ///
    /// Everything between the samples and the bars, and nothing else: the
    /// ballistics are stepped by the caller, because they need a `dt` and a
    /// clock is the one thing that cannot be handed to a test.
    ///
    /// Extracted from the render loop unchanged rather than as a redesign. It
    /// was the only part of the chain with no way in from a test - which is why
    /// nothing checks that a note lights its bar within a frame of sounding,
    /// only that it lights the right one eventually.
    fn measure(&mut self) -> f32 {
        let magnitudes = self.spectrum.analyse(&self.window);
        self.bar_map.sample(magnitudes, &mut self.sampled);
        self.bandwidth.group(&self.sampled, &mut self.bands);
        // Gain is applied before the clip, so full scale always means
        // exactly full scale whatever the trim.
        let gain = 10f32.powf(self.gain_db / 20.0);
        for v in self.bands.iter_mut() {
            // The preset's response curve, applied where the measured
            // amplitude becomes a level to draw. Linear for every preset
            // rav ships on a screen, which is rav's documented choice and
            // what makes a quiet passage read quiet; a short LED bar needs
            // otherwise, and says so in its own preset rather than here.
            *v = self
                .curve
                .apply(Level::new(*v * gain / MAX_HEIGHT))
                .fraction();
        }
        gain
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
            Action::CycleVisualisation => {
                self.visualisation = self.visualisation.next();
                self.note(match self.visualisation {
                    Visualisation::Analyzer => "analyser".to_string(),
                    Visualisation::Oscilloscope => "oscilloscope".to_string(),
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
            .filter_map(Theme::built_in)
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
        let showing = match self.visualisation {
            Visualisation::Analyzer => "analyser",
            Visualisation::Oscilloscope => "oscilloscope",
        };
        let bandwidth = match self.bandwidth {
            Bandwidth::Wide => "wide",
            Bandwidth::Thin => "thin",
        };
        let mut rows = vec![
            HelpRow {
                key: "space",
                description: "switch visualisation",
                value: Some(showing.to_string()),
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
            // No key, because there is none: `--surface` is a flag. "Would"
            // rather than "does" is the literal truth for this beta - the glyph
            // renderer draws the frame you are looking at whatever this says.
            HelpRow {
                key: "",
                description: "would draw on",
                value: Some(format!(
                    "{} ({})",
                    self.surface.surface.label(),
                    self.surface.because
                )),
            },
        ];
        // Last, and with no key, because it is the one line here that reports
        // rather than offers - and it is what the rest of the panel is worth
        // nothing without. Beneath "would draw on", so the two readouts read in
        // the order the sound travels: what rav hears, then what it draws with.
        if let Some(source) = &self.source {
            rows.push(HelpRow {
                key: "",
                description: "listening to",
                value: Some(source.clone()),
            });
        }
        rows
    }

    /// Name the source the display is being fed from.
    ///
    /// Set once the source is settled, never guessed: on macOS the tap is tried
    /// *after* the capture device is opened, so anything announced earlier can
    /// be contradicted a moment later.
    pub fn listening_to(&mut self, source: impl Into<String>) {
        self.source = Some(source.into());
    }

    /// Feed the rolling window from a process tap instead of a cpal stream.
    #[cfg(target_os = "macos")]
    pub fn use_tap(&mut self, tap: crate::audio::tap::Tap) {
        self.channels = tap.channels().max(1) as usize;
        self.sample_rate = tap.sample_rate();
        self.sized_for = (0, 0); // the mapping depends on the sample rate
        self.tap = Some(tap);
        self.listening_to("system audio");
    }

    pub async fn run(&mut self, audio_receiver: Receiver<AudioData>) -> Result<()> {
        #[cfg(not(test))]
        let _terminal = TerminalGuard::take_the_terminal()?;
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
            let gain = self.measure();

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
                visualisation,
                scope_style,
                peaks,
                show_grid,
                bar_style,
                ..
            } = self;
            let cap_color = theme.peak;
            let grid = show_grid.then_some(grid_colors.as_slice());
            terminal.draw(|f| {
                match visualisation {
                    Visualisation::Analyzer => f.render_widget(
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
                    Visualisation::Oscilloscope => f.render_widget(
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

/// Put the terminal back the way it was found.
///
/// Raw mode off, off the alternate screen, cursor visible. The one place that
/// knows what setting rav up did, so the way out cannot drift from the way in.
#[cfg(not(test))]
fn hand_the_terminal_back() {
    let _ = disable_raw_mode();
    let _ = io::stdout().execute(LeaveAlternateScreen);
    let _ = io::stdout().execute(crossterm::cursor::Show);
}

#[cfg(not(test))]
type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

/// Holds the terminal for as long as rav is drawing on it.
///
/// Every way out has to hand it back: returning normally, returning an error
/// through `?`, and panicking. Drop covers the first two. The panic hook covers
/// the third, because `panic = "abort"` in the release build never unwinds and
/// so never drops anything - and without it a panic leaves the user on the
/// alternate screen in raw mode, with no echo, no line editing, and a backtrace
/// they cannot see.
///
/// The hook rav replaces still runs, so a panic still reports itself, into
/// `rav.log` where `main` points stderr before anything touches the terminal.
#[cfg(not(test))]
struct TerminalGuard {
    replaced_hook: Option<std::sync::Arc<PanicHook>>,
}

#[cfg(not(test))]
impl TerminalGuard {
    fn take_the_terminal() -> Result<Self> {
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;

        let replaced = std::sync::Arc::new(std::panic::take_hook());
        let on_panic = std::sync::Arc::clone(&replaced);
        std::panic::set_hook(Box::new(move |panicked| {
            hand_the_terminal_back();
            (*on_panic)(panicked);
        }));
        Ok(Self {
            replaced_hook: Some(replaced),
        })
    }
}

#[cfg(not(test))]
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        hand_the_terminal_back();

        // `take_hook` and `set_hook` panic when called from a panicking thread,
        // and a second panic during an unwind aborts. The hook has already done
        // this work by the time an unwind reaches here, and the process is on
        // its way out regardless.
        if std::thread::panicking() {
            return;
        }

        // The hook is process-wide, so leaving rav's in place would have a
        // later panic restore a terminal nobody is holding - and a second run
        // would stack another on top.
        let _ = std::panic::take_hook();
        if let Some(replaced) = self.replaced_hook.take()
            && let Ok(hook) = std::sync::Arc::try_unwrap(replaced)
        {
            std::panic::set_hook(hook);
        }
    }
}

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

    /// One channel, so a generated signal reaches the window unaltered.
    ///
    /// `push_samples` averages across channels, so handing a two-channel app a
    /// mono tone averages each pair of neighbouring samples - which is a
    /// low-pass filter, and quietly moves the thing under test.
    fn listening_app() -> App {
        let mut a = App::new(Config::default(), 1, 48_000).expect("app should build");
        a.resize(80, 24);
        a
    }

    /// Samples in one frame at the 60fps ceiling.
    const FRAME_SAMPLES: usize = 48_000 / 60;

    #[test]
    fn a_note_lights_the_display_in_the_first_frame_that_carries_it() {
        // The timing half of "does the picture match the music", and until the
        // analysis had a seam there was no way to ask it - `tests/music.rs` can
        // only show that a note lights the *right* bar, never that it does so
        // while the note is still sounding.
        //
        // No threshold: silence analyses to exactly zero, so "lit" is "above
        // zero" and the assertion is about which frame rather than how much.
        let mut a = listening_app();
        a.push_samples(&vec![0.0; Spectrum::DEFAULT_SIZE]);
        a.measure();
        assert!(
            a.bands.iter().all(|&level| level == 0.0),
            "silence lit something: {:?}",
            a.bands,
        );

        // One frame's worth of a note and not a sample more.
        let note = crate::testing::Instrument::pure(48_000.0).play(
            &[crate::testing::Note::MIDDLE_C.octave_up(2)],
            FRAME_SAMPLES,
        );
        a.push_samples(&note);
        a.measure();
        assert!(
            a.bands.iter().any(|&level| level > 0.0),
            "the frame the note started in showed nothing",
        );
    }

    #[test]
    fn a_note_that_stops_leaves_its_cap_hanging_above_a_falling_bar() {
        // The whole point of the effect, end to end through `App` for the first
        // time rather than against the ballistics alone: the bar drops away and
        // the cap stays up to mark where it had been.
        let mut a = listening_app();
        let note = crate::testing::Instrument::pure(48_000.0)
            .play(&[crate::testing::Note::MIDDLE_C.octave_up(2)], 4096);
        a.push_samples(&note);
        a.measure();
        a.ballistics.step(&a.bands, 1.0 / 60.0);

        let loudest = (0..a.ballistics.len())
            .max_by(|&x, &y| {
                let (x, y) = (a.ballistics.bars()[x], a.ballistics.bars()[y]);
                x.partial_cmp(&y).expect("no NaN in a bar")
            })
            .expect("bars to compare");
        let struck = a.ballistics.bars()[loudest];
        assert!(struck > 0.0, "the note did not register at all");

        // Silence for a third of a second, which is a full-scale bar's fall.
        for _ in 0..20 {
            a.push_samples(&vec![0.0; FRAME_SAMPLES]);
            a.measure();
            a.ballistics.step(&a.bands, 1.0 / 60.0);
        }

        let bar = a.ballistics.bars()[loudest];
        let cap = a.ballistics.peaks()[loudest];
        assert!(bar < struck, "the bar did not fall: {struck} -> {bar}");
        assert!(cap > bar, "the cap came down with the bar: {cap} vs {bar}");
    }

    #[test]
    fn everything_rav_opens_with_comes_from_the_preset() {
        // Four constants scattered through the constructor is how a "default"
        // ends up meaning one thing in the preset and another in the app. Each
        // of these is asserted against the preset rather than a literal, so
        // editing the preset moves rav and editing rav alone cannot.
        let a = app();
        let preset = rav_appearance::preset::RAV;
        assert_eq!(a.theme.name, preset.theme.name, "colours");
        assert_eq!(a.curve, preset.curve, "response");
        assert_eq!(
            BAR_FALL_SPEEDS[a.bar_fall_index], preset.ballistics.bar_fall,
            "the fall dial starts where the preset sits"
        );
    }

    #[test]
    fn the_response_curve_comes_from_the_preset_and_is_linear() {
        // A field nothing reads is not a feature, so this pins both halves: the
        // app takes its curve from the preset rather than hardcoding one, and
        // the preset rav ships is linear - which is its documented choice and
        // what makes a quiet passage read quiet.
        assert_eq!(app().curve, rav_appearance::preset::RAV.curve);
        assert_eq!(app().curve, Curve::Linear);
    }

    #[test]
    fn a_curve_changes_what_a_quiet_band_draws() {
        // The mechanism the field exists for, exercised end to end rather than
        // asserted on the type: the same measured amplitude reaches the display
        // as a different level once the preset asks for a window.
        let quiet = Level::new(0.1); // -20 dBFS
        let linear = Curve::Linear.apply(quiet);
        let windowed = Curve::Decibel { floor: -48.0 }.apply(quiet);
        assert!(
            windowed > linear,
            "a window should lift a quiet band, got {windowed:?} from {linear:?}"
        );
        // And full scale is still full scale either way, so the top of the
        // display means the same thing whatever the curve.
        assert_eq!(Curve::Linear.apply(Level::FULL), Level::FULL);
        assert_eq!(
            Curve::Decibel { floor: -48.0 }.apply(Level::FULL),
            Level::FULL
        );
    }

    #[test]
    fn every_key_the_readme_documents_is_bound() {
        // The README is where a user learns the keys, and `map_key`'s catch-all
        // `_ => Action::None` means an unbound key is a keypress that does
        // nothing rather than a build failure. So a binding can be renamed or
        // dropped and the only symptom is a documented key that does not work.
        let readme = std::fs::read_to_string("README.md").expect("beside the source");
        let table = readme
            .split("| Key | |")
            .nth(1)
            .expect("the key table is still in the README");

        let code_for = |spelling: &str| match spelling {
            "Esc" => Some(KeyCode::Esc),
            "Space" => Some(KeyCode::Char(' ')),
            "Tab" => Some(KeyCode::Tab),
            "↑" => Some(KeyCode::Up),
            "↓" => Some(KeyCode::Down),
            other => other
                .chars()
                .next()
                .filter(|_| other.chars().count() == 1)
                .map(KeyCode::Char),
        };

        let mut checked = 0;
        // `skip(1)` steps over what is left of the header line itself, which is
        // empty and would end the run before it began. The count at the bottom
        // is there because a table this fails to parse looks exactly like a
        // table with nothing wrong in it.
        for row in table
            .lines()
            .skip(1)
            .take_while(|line| line.starts_with('|'))
        {
            // Only the first column: later columns name values, not keys.
            let keys = row.split('|').nth(1).unwrap_or_default();
            for spelling in keys.split('`').skip(1).step_by(2) {
                let code = code_for(spelling)
                    .unwrap_or_else(|| panic!("the test cannot spell {spelling:?}"));
                assert_ne!(
                    map_key(code),
                    Action::None,
                    "the README documents {spelling:?} and nothing is bound to it",
                );
                checked += 1;
            }
        }
        assert!(checked >= 15, "only {checked} keys found - the table moved");
    }

    #[test]
    fn the_readme_lists_the_values_the_code_offers() {
        // These are what drift: adding a fall speed or a theme is a one-line
        // change in one file, and the README is in another. Each list here is
        // built from the code, so the assertion is that the document agrees with
        // it rather than that both agree with something written twice.
        let readme = std::fs::read_to_string("README.md").expect("beside the source");

        let speeds = BAR_FALL_SPEEDS
            .iter()
            .map(|speed| format!("{speed}"))
            .collect::<Vec<_>>()
            .join(", ");
        assert!(readme.contains(&speeds), "fall speeds: expected {speeds:?}");

        let themes = Theme::built_in_names().collect::<Vec<_>>().join(", ");
        assert!(readme.contains(&themes), "themes: expected {themes:?}");

        // Bar styles in the order `b` actually walks them, from the default.
        let mut style = BarStyle::default();
        let mut styles = Vec::new();
        for _ in 0..6 {
            styles.push(style.label());
            style = style.next();
        }
        assert_eq!(style, BarStyle::default(), "the cycle did not come home");
        let styles = styles.join(", ");
        assert!(readme.contains(&styles), "bar styles: expected {styles:?}");
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
        assert_eq!(
            a.visualisation,
            Visualisation::Analyzer,
            "analyser is the default"
        );
        a.apply(Action::CycleVisualisation);
        assert_eq!(a.visualisation, Visualisation::Oscilloscope);
        a.apply(Action::CycleVisualisation);
        assert_eq!(
            a.visualisation,
            Visualisation::Analyzer,
            "toggling twice returns"
        );
    }

    #[test]
    fn tab_and_o_both_switch_view() {
        assert_eq!(map_key(KeyCode::Tab), Action::CycleVisualisation);
        assert_eq!(map_key(KeyCode::Char('o')), Action::CycleVisualisation);
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
        a.apply(Action::CycleVisualisation);
        assert_eq!(value_for(&a, "space"), Some("oscilloscope".to_string()));

        a.apply(Action::ToggleHelp);
        assert!(!a.show_help);
    }

    #[test]
    fn the_overlay_reports_the_surface_and_why() {
        // The one place a user can see what the startup probe concluded. Without
        // the reason it is unarguable: "glyphs" alone does not say whether the
        // terminal was asked and declined or never asked.
        let mut a = app();
        a.set_surface(crate::surface::choose(
            crate::surface::Choice::Auto,
            true,
            || true,
        ));
        let readout = a
            .help_rows()
            .into_iter()
            .find(|r| r.description == "would draw on")
            .and_then(|r| r.value);
        assert_eq!(
            readout,
            Some("glyphs (tmux or screen is in the way)".to_string())
        );
    }

    #[test]
    fn r_and_w_walk_their_whole_list_and_come_home() {
        // The other cycles are covered one by one; these two were not. A cycle
        // that skips an entry hides a setting the help panel still offers, and
        // one that does not come home leaves a key you can press for ever
        // without getting back to where you started.
        let mut a = app();
        // Read from `limit_index` and the table it indexes, not from the note:
        // the note times out after a couple of seconds, so a loaded machine
        // would collapse two presses into one empty string and the dedupe
        // below would fail for a reason that has nothing to do with the cycle.
        let ranges: Vec<Option<u32>> = (0..FREQUENCY_LIMITS.len())
            .map(|_| {
                a.apply(Action::CycleFrequencyLimit);
                FREQUENCY_LIMITS[a.limit_index]
            })
            .collect();
        assert_eq!(
            ranges
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            FREQUENCY_LIMITS.len(),
            "the range cycle repeats itself: {ranges:?}",
        );
        let home = a.limit_index;
        a.apply(Action::CycleFrequencyLimit);
        assert_ne!(a.limit_index, home, "and it still moves after a full turn");

        // Bandwidth is two, so one press each way must return.
        let start = a.bandwidth;
        a.apply(Action::CycleBandwidth);
        assert_ne!(a.bandwidth, start, "w changed nothing");
        a.apply(Action::CycleBandwidth);
        assert_eq!(a.bandwidth, start, "w does not come home");
    }

    #[test]
    fn every_key_that_should_change_the_picture_changes_it() {
        // A key wired to a field nothing reads is a key that does nothing, and
        // the help overlay would still cheerfully report its new value. So each
        // of these is pressed and the frame compared, rather than the setting
        // being read back - which is the test agreeing with itself.
        let frame = |a: &mut App| {
            let mut terminal = Terminal::new(TestBackend::new(60, 16)).unwrap();
            // Gated exactly as the render loop gates it. Resizing unconditionally
            // rebuilds the caches whatever the key did, so a key that forgot to
            // invalidate `sized_for` - which in a real run leaves the picture
            // stale - would still change the frame here and pass.
            if (60, 16) != a.sized_for {
                a.resize(60, 16);
            }
            let analyzer = Analyzer {
                bars: a.ballistics.bars(),
                peaks: a.ballistics.peaks(),
                row_colors: &a.row_colors,
                cap_color: a.theme.peak,
                grid: a.show_grid.then_some(a.grid_colors.as_slice()),
                bar_style: a.bar_style,
                layout: a.layout,
                peaks_style: a.peaks,
            };
            terminal
                .draw(|f| f.render_widget(analyzer, f.area()))
                .unwrap();
            terminal.backend().buffer().clone()
        };

        // A signal, so there is something on screen to change.
        let mut a = app();
        a.resize(60, 16);
        let bands: Vec<f32> = (0..a.bands.len())
            .map(|i| 0.3 + (i % 5) as f32 / 8.0)
            .collect();
        a.ballistics.step(&bands, 1.0 / 60.0);

        for key in [
            Action::CycleTheme,
            Action::CycleBarStyle,
            Action::TogglePeaks,
            Action::ToggleGrid,
            Action::BarSize(1),
        ] {
            let before = frame(&mut a);
            a.apply(key);
            let after = frame(&mut a);
            assert_ne!(before, after, "{key:?} changed nothing on screen");
        }

        // Bandwidth and the frequency range change how many bins are sampled
        // and which, which shows in the mapping rather than in one frame of a
        // fixed signal. `bands` is filled by the analysis, so it is empty here.
        // Gated for the same reason as `frame`: forcing a rebuild here would
        // hide a key that never asked for one.
        let settle = |a: &mut App| {
            if (60, 16) != a.sized_for {
                a.resize(60, 16);
            }
        };
        let sampled_before = a.bar_map.len();
        a.apply(Action::CycleBandwidth);
        settle(&mut a);
        assert_ne!(sampled_before, a.bar_map.len(), "bandwidth changed nothing");

        let top_before = a.bar_map.positions().last().copied();
        a.apply(Action::CycleFrequencyLimit);
        settle(&mut a);
        assert_ne!(
            top_before,
            a.bar_map.positions().last().copied(),
            "the frequency range changed nothing",
        );
    }

    #[test]
    fn the_overlay_says_what_rav_is_listening_to() {
        // "Why are the bars showing the wrong thing" has one answer and this is
        // where it is now. A microphone hears the room, so a display fed by one
        // moves convincingly while showing nothing of what is playing - which
        // reads as rav working, not as rav on the wrong source.
        let mut a = app();
        assert!(
            a.help_rows()
                .iter()
                .all(|r| r.description != "listening to"),
            "a source nobody has named is not worth a row",
        );

        a.listening_to("MacBook Pro Microphone");
        let source = a
            .help_rows()
            .into_iter()
            .find(|r| r.description == "listening to")
            .and_then(|r| r.value);
        assert_eq!(source, Some("MacBook Pro Microphone".to_string()));

        // And it is the panel's last word, after everything you can press.
        let rows = a.help_rows();
        assert_eq!(rows.last().unwrap().description, "listening to");
        assert_eq!(rows.last().unwrap().key, "", "it is not a key you press");
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
        let theme = crate::visual::theme::load("terminal").expect("built in");
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
    fn cycling_themes_walks_the_bundled_order_and_returns() {
        // The path a user without --theme presses constantly, and the one that
        // exercises the generated consts end to end: the themes are static
        // data rather than something parsed at startup, and this is where a
        // wrong order or a missing entry would actually be seen.
        //
        // Four presses must return to where they started, or the cycle has
        // dropped an entry or gained one.
        let mut a = app();
        assert_eq!(a.theme.name, "rav", "rav is the default");

        let seen: Vec<String> = (0..4)
            .map(|_| {
                let label = a.theme.name.clone();
                a.apply(Action::CycleTheme);
                label
            })
            .collect();
        assert_eq!(seen, vec!["rav", "winamp", "terminal", "mono"]);
        assert_eq!(a.theme.name, "rav", "the fourth press comes back round");
    }

    #[test]
    fn a_cycled_theme_is_the_same_data_the_parser_would_have_produced() {
        // The consts replaced a parse at startup, so this checks the swap did
        // not change what reaches the display - not that the two agree in
        // isolation, which theme.rs already covers, but that the app hands on
        // exactly what a user loading the same file by path would get.
        let mut a = app();
        a.apply(Action::CycleTheme);
        let cycled = a.theme.clone();
        let from_disk = crate::visual::theme::load(&format!(
            "crates/rav-appearance/themes/{}.toml",
            cycled.name
        ))
        .expect("the bundled file is beside the crate");
        assert_eq!(cycled, from_disk);
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
