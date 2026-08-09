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

    /// Enable clean mode (minimal logging for better TUI)
    #[arg(long)]
    clean: bool,

    /// Skin: `rav`, `winamp`, `terminal`, `mono`, or a name/path of a skin file
    #[arg(long, value_name = "NAME|PATH")]
    skin: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
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

    // Initialize audio capture
    let mut audio_capture = AudioCapture::new(
        args.device.as_deref(),
        config.audio.sample_rate,
        config.audio.buffer_size,
    )
    .await?;

    // Start audio capture
    let audio_receiver = audio_capture.start().await?;
    info!("🎵 Audio capture started");

    // Both are only final after start(), which may renegotiate. The channel
    // count matters: cpal delivers interleaved frames, and ignoring it silently
    // mixes L and R into the time domain.
    info!(
        "Capturing at {}Hz, {} channel(s)",
        audio_capture.sample_rate(),
        audio_capture.channels()
    );

    let mut app = App::new(
        config,
        audio_capture.channels(),
        audio_capture.sample_rate(),
    )?;

    // A named skin that cannot be read is an error rather than a silent fall
    // back to the default: the display would look right and be the wrong skin.
    if let Some(skin) = args.skin.as_deref() {
        app.set_skin(rav::visual::Skin::load(skin)?);
        info!("Using skin: {skin}");
    }

    // Prefer a CoreAudio process tap. It captures every process' output ahead of
    // the system volume, so playback level does not drive the display, and it
    // needs no virtual device installed. Falls back to the capture device when
    // unavailable - macOS below 14.2, or the recording permission refused.
    #[cfg(target_os = "macos")]
    match rav::audio::tap::Tap::start() {
        Ok(tap) => {
            info!(
                "Using CoreAudio process tap: {}Hz, {} channel(s)",
                tap.sample_rate(),
                tap.channels()
            );
            // The cpal stream is already running, and its channel is unbounded.
            // Leaving it feeding a receiver the app will never drain again grows
            // the queue by ~350 KB/s until the process is killed.
            audio_capture.stop();
            app.use_tap(tap);
        }
        Err(e) => {
            info!("Process tap unavailable ({e}); using the capture device instead");
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

    // Explicitly stop audio capture before exit
    info!("Stopping audio capture...");
    audio_capture.stop();

    // Small delay to allow cleanup
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    info!("🚦 RAV shutting down gracefully");

    result
}
