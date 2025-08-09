use anyhow::Result;
use rav::{audio::AudioCapture, config::Config, signal::SignalProcessor};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🧪 AUDIO PIPELINE VALIDATION TEST");
    info!("=================================");

    // Load default config
    let config = Config::load(None).await?;
    info!("Configuration loaded successfully");

    // Initialize audio capture with default device
    let mut audio_capture = AudioCapture::new(
        None, // Use default device
        config.audio.sample_rate,
        config.audio.buffer_size,
    )
    .await?;

    info!("✅ Audio capture initialized");

    // Initialize signal processor
    let mut signal_processor = SignalProcessor::new(
        config.audio.sample_rate,
        config.display.frequency_bands,
        config.display.frequency_range,
    );

    info!("✅ Signal processor initialized");

    // Start audio capture
    let audio_receiver = audio_capture.start().await?;
    info!("🎵 Audio capture started - listening for audio input...");

    let test_start = Instant::now();
    let test_duration = Duration::from_secs(10); // Test for 10 seconds
    let mut audio_received_count = 0;
    let mut last_magnitude_check = Instant::now();
    let mut max_magnitude = 0.0f32;
    let mut total_magnitude = 0.0f32;
    let mut magnitude_samples = 0;

    info!("🔊 PLEASE PLAY SOME AUDIO NOW to test the pipeline...");
    info!("Test duration: {} seconds", test_duration.as_secs());

    while test_start.elapsed() < test_duration {
        // Check for audio data
        match audio_receiver.try_recv() {
            Ok(audio_data) => {
                audio_received_count += 1;

                // Process the audio data
                match signal_processor.process(&audio_data) {
                    Ok(magnitudes) => {
                        let normalized_magnitudes = signal_processor
                            .normalize_magnitudes(&magnitudes, config.display.sensitivity);

                        // Calculate magnitude statistics
                        let current_magnitude: f32 = normalized_magnitudes.iter().sum::<f32>()
                            / normalized_magnitudes.len() as f32;
                        max_magnitude = max_magnitude.max(current_magnitude);
                        total_magnitude += current_magnitude;
                        magnitude_samples += 1;

                        // Log magnitude data every second
                        if last_magnitude_check.elapsed() >= Duration::from_secs(1) {
                            let avg_magnitude = total_magnitude / magnitude_samples as f32;
                            info!(
                                "📊 Audio Stats: Current={:.3}, Max={:.3}, Avg={:.3}, Samples={}",
                                current_magnitude,
                                max_magnitude,
                                avg_magnitude,
                                audio_received_count
                            );

                            // Check if we're getting significant audio data
                            if current_magnitude > 0.01 {
                                info!("✅ GOOD: Audio pipeline is receiving and processing audio data!");
                            } else {
                                warn!(
                                    "⚠️  Low audio levels - ensure audio is playing and not muted"
                                );
                            }

                            last_magnitude_check = Instant::now();
                        }
                    }
                    Err(e) => {
                        error!("❌ Signal processing failed: {}", e);
                    }
                }
            }
            Err(flume::TryRecvError::Empty) => {
                // No audio data available, continue
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(flume::TryRecvError::Disconnected) => {
                error!("❌ Audio receiver disconnected");
                break;
            }
        }
    }

    // Stop audio capture
    audio_capture.stop();

    // Test results
    info!("");
    info!("🏁 AUDIO PIPELINE TEST RESULTS");
    info!("==============================");
    info!("Duration: {:.1}s", test_start.elapsed().as_secs_f32());
    info!("Audio packets received: {}", audio_received_count);
    info!("Max magnitude detected: {:.3}", max_magnitude);

    if audio_received_count > 0 {
        let avg_magnitude = total_magnitude / magnitude_samples as f32;
        info!("Average magnitude: {:.3}", avg_magnitude);

        // Determine test result
        if audio_received_count > 50 && max_magnitude > 0.05 {
            info!("✅ PASS: Audio pipeline is working correctly!");
            info!("   - Audio data is being captured successfully");
            info!("   - Signal processing is working");
            info!("   - Magnitudes show audio activity");
        } else if audio_received_count > 10 {
            warn!("⚠️  PARTIAL: Audio capture working but low audio levels");
            warn!("   - Try playing louder audio or check audio settings");
            warn!("   - The pipeline itself appears functional");
        } else {
            error!("❌ FAIL: Audio pipeline not receiving sufficient data");
            error!("   - Check audio device permissions");
            error!("   - Ensure audio is playing");
            error!("   - Try different audio device");
        }
    } else {
        error!("❌ FAIL: No audio data received at all");
        error!("   - Audio capture may not be working");
        error!("   - Check device permissions and configuration");
    }

    info!("");
    info!("💡 RECOMMENDATIONS:");
    if max_magnitude < 0.01 {
        info!("   - Increase system volume or play louder audio");
        info!("   - Check microphone/line-in permissions");
        info!("   - Try running with --device to specify audio source");
    }

    if audio_received_count < 10 {
        info!("   - Check PulseAudio/PipeWire configuration");
        info!("   - Ensure selected audio device is not in use");
        info!("   - Try running: pactl list sources (PulseAudio)");
    }

    Ok(())
}
