use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Everything lives in the library, so the binary and the tests compile one copy
// of the tree rather than two that can drift apart.
use rav::{audio::AudioCapture, config::Config, ui::App};

#[derive(Parser, Debug)]
#[command(name = "rav")]
#[command(about = "A high-performance terminal audio visualizer")]
#[command(version)]
struct Args {
    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<std::path::PathBuf>,

    /// Audio device to use
    #[arg(short = 'd', long)]
    device: Option<String>,

    /// Enable debug logging
    #[arg(long)]
    debug: bool,

    /// List available audio devices
    #[arg(long)]
    list_devices: bool,

    /// Use test audio generator instead of real audio input
    #[arg(long)]
    test_audio: bool,

    /// Log almost nothing. `RUST_LOG` still wins if it is set
    #[arg(long)]
    clean: bool,

    /// Theme: `rav`, `winamp`, `terminal`, `mono`, or a name/path of a theme file
    #[arg(long, value_name = "NAME|PATH")]
    theme: Option<String>,

    /// Which surface to draw on. `kitty` draws the bars as pixels in WezTerm,
    /// Ghostty or kitty, and drops back to block characters on a terminal
    /// that will not say how big it is in pixels; `auto` still chooses them
    /// however capable your terminal is, and `window` is not built yet. Press
    /// `h` for what rav settled on and why.
    #[arg(
        long,
        value_name = "auto|glyphs|kitty|window",
        default_value = "auto",
        value_parser = surface
    )]
    surface: rav::surface::Choice,
}

/// Reject an unknown surface rather than falling back to `auto`.
///
/// A typo that silently means `auto` is a bug report about rav ignoring a flag.
fn surface(text: &str) -> Result<rav::surface::Choice, String> {
    rav::surface::Choice::parse(text)
        .ok_or_else(|| format!("unknown surface `{text}` - try auto, glyphs, kitty or window"))
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match rav_main().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(problem) => {
            // To stdout, deliberately. Everything rav can fail at happens after
            // stderr has been pointed at the log file, so reporting the usual
            // way means a mistyped `--theme` looks like rav dying in silence:
            // exit 1, a blank terminal, and the reason in a file the user has
            // no reason to look in.
            println!("rav: {problem:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn rav_main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging with clean mode support
    let log_level = if args.debug {
        "debug"
    } else if args.clean {
        "error"
    } else {
        "info"
    };

    // Always write logs to file to avoid TUI interference
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("rav.log")
        .expect("Failed to create log file");

    // Not everything that writes to this terminal goes through `tracing`.
    // libasound reports from C straight to stderr, and merely enumerating the
    // ALSA PCMs produces a dozen `pcm_route.c`/`pcm_dmix.c` lines that land on
    // top of the alternate screen - it has no error handler to install through
    // cpal, so the only place to intercept it is the file descriptor. Point
    // fd 2 at the log before anything touches cpal.
    //
    // Panic output follows it there, which is where it is legible anyway: a
    // backtrace painted over a raw-mode TUI is not.
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        // SAFETY: `log_file` is open for writing and outlives the call, and
        // STDERR_FILENO is a valid descriptor to replace. A failure here only
        // means stderr stays where it was, which is why the result is ignored.
        let _ = unsafe { libc::dup2(log_file.as_raw_fd(), libc::STDERR_FILENO) };
    }

    if args.clean {
        // Clean mode: minimal logging to file only
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "error".into()),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .compact()
                    .with_target(false)
                    .with_thread_ids(false)
                    .with_file(false)
                    .with_line_number(false)
                    .with_writer(log_file),
            )
            .init();
    } else {
        // Development mode: full logging to file
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| format!("rav={log_level},info").into()),
            )
            .with(tracing_subscriber::fmt::layer().with_writer(log_file))
            .init();
    }

    info!("🚀 RAV Audio Visualizer starting up...");

    // Load configuration
    let config = Config::load(args.config.as_deref()).await?;
    info!("Configuration loaded: {:?}", config);

    // Handle device listing
    if args.list_devices {
        AudioCapture::list_devices().await?;
        return Ok(());
    }

    // `--test-audio` opens no device at all, which is the whole of its use: a
    // machine with no loopback, no recording permission or no sound card still
    // shows a working display.
    let mut audio_capture = if args.test_audio {
        None
    } else {
        Some(
            AudioCapture::new(
                args.device.as_deref(),
                config.audio.sample_rate,
                config.audio.buffer_size,
            )
            .await?,
        )
    };

    let (audio_receiver, channels, sample_rate, source) = match &mut audio_capture {
        Some(capture) => {
            let receiver = capture.start().await?;
            info!("🎵 Audio capture started");
            // Both are only final after start(), which may renegotiate. The
            // channel count matters: cpal delivers interleaved frames, and
            // ignoring it silently mixes L and R into the time domain.
            info!(
                "Capturing at {}Hz, {} channel(s)",
                capture.sample_rate(),
                capture.channels()
            );
            (
                receiver,
                capture.channels(),
                capture.sample_rate(),
                capture.device_name(),
            )
        }
        None => {
            let rate = config.audio.sample_rate;
            (
                rav::audio::synthetic::start(rate),
                1,
                rate,
                "the built-in test signal".to_string(),
            )
        }
    };

    let mut app = App::new(config, channels, sample_rate)?;
    // The device for now; the tap below replaces it if it starts. Said here and
    // corrected there rather than announced once in the middle, because the
    // answer is not settled until the tap has had its turn.
    app.listening_to(source);

    // A named theme that cannot be read is an error rather than a silent fall
    // back to the default: the display would look right and be the wrong theme.
    if let Some(theme) = args.theme.as_deref() {
        app.set_theme(rav::visual::theme::load(theme)?);
        info!("Using theme: {theme}");
    }

    // Here, and not inside `App`, because asking the terminal has to happen
    // before the alternate screen is entered and while nothing else is reading
    // stdin - the same constraint the palette query above runs under.
    let chosen = rav::surface::choose(
        args.surface,
        rav::surface::multiplexed(),
        rav::surface::probe,
    );
    info!(
        "Would draw on {} ({})",
        chosen.surface.label(),
        chosen.because
    );
    app.set_surface(chosen);

    // Prefer a CoreAudio process tap. It captures every process' output ahead of
    // the system volume, so playback level does not drive the display, and it
    // needs no virtual device installed. Falls back to the capture device when
    // unavailable - macOS below 14.2, or the recording permission refused.
    //
    // Skipped entirely under `--test-audio`: taking the system's own output
    // would be capturing a device, which is the one thing that flag exists to
    // avoid.
    #[cfg(target_os = "macos")]
    if let Some(capture) = &mut audio_capture {
        match rav::audio::tap::Tap::start() {
            Ok(tap) => {
                info!(
                    "Using CoreAudio process tap: {}Hz, {} channel(s)",
                    tap.sample_rate(),
                    tap.channels()
                );
                // Nothing drains the cpal stream once the tap takes over. Its
                // queue is bounded and drops its oldest block, so this frees a
                // capture device and a callback rather than memory.
                capture.stop();
                app.use_tap(tap);
            }
            Err(e) => {
                info!("Process tap unavailable ({e}); using the capture device instead");
            }
        }
    }

    // Setup signal handling for graceful shutdown
    let result = tokio::select! {
        app_result = app.run(audio_receiver) => {
            app_result
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
            Ok(())
        }
    };

    // Explicitly stop audio capture before exit. The test signal needs no such
    // thing: its thread ends when the channel it feeds is dropped.
    if let Some(capture) = &mut audio_capture {
        info!("Stopping audio capture...");
        capture.stop();
    }

    // Small delay to allow cleanup
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    info!("🚦 RAV shutting down gracefully");

    result
}
