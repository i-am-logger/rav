use anyhow::Result;
use clap::Parser;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod audio;
mod config;
mod quality;
mod signal;
mod ui;
mod visual;

use crate::{audio::AudioCapture, config::Config, signal::SignalProcessor, ui::App};

#[derive(Parser, Debug)]
#[command(name = "speedy")]
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
        .open("speedy.log")
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
                    .unwrap_or_else(|_| format!("speedy={log_level},info").into()),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(log_file)
            )
            .init();
    }

    info!("🚀 Speedy Audio Visualizer starting up...");

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

    // Initialize signal processor
    let signal_processor = SignalProcessor::new(
        config.audio.sample_rate,
        config.display.frequency_bands,
        config.display.frequency_range.clone(),
    );

    // Start audio capture
    let audio_receiver = audio_capture.start().await?;
    info!("🎵 Audio capture started");

    // Initialize and run UI with proper signal handling
    let mut app = App::new(config, signal_processor);

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

    info!("🚦 Speedy shutting down gracefully");

    result
}
