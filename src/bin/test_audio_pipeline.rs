//! Diagnostic: capture real audio and report what the analyser sees.
//!
//! Runs the full pipeline without a TUI, so a silent display can be diagnosed as
//! either "no audio reaching us" or "audio arriving but the render is wrong".

use anyhow::Result;
use rav::{
    audio::AudioCapture,
    config::Config,
    signal::{
        mapping::{BarMap, DEFAULT_SCALE},
        spectrum::{MAX_HEIGHT, Spectrum},
    },
};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load(None).await?;
    let mut capture =
        AudioCapture::new(None, config.audio.sample_rate, config.audio.buffer_size).await?;
    let receiver = capture.start().await?;

    let sample_rate = capture.sample_rate();
    let channels = capture.channels().max(1) as usize;
    println!("🎵 Capturing at {sample_rate}Hz, {channels} channel(s)");
    println!("   Play something now - sampling for 5 seconds.\n");

    let mut spectrum = Spectrum::new(Spectrum::DEFAULT_SIZE)?;
    let mut window = vec![0.0f32; spectrum.size()];
    let map = BarMap::new(24, spectrum.bins(), DEFAULT_SCALE);
    let mut bars = Vec::new();

    let deadline = Instant::now() + Duration::from_secs(5);
    let (mut frames, mut loudest) = (0u32, 0.0f32);

    while Instant::now() < deadline {
        while let Ok(data) = receiver.try_recv() {
            frames += 1;
            let n = window.len();
            for frame in data.samples.chunks(channels) {
                let mono = frame.iter().sum::<f32>() / frame.len() as f32;
                window.copy_within(1..n, 0);
                window[n - 1] = mono;
            }
        }

        let magnitudes = spectrum.analyse(&window);
        map.sample(magnitudes, &mut bars);
        let peak = bars.iter().fold(0.0f32, |a, &b| a.max(b));
        loudest = loudest.max(peak);

        let row: String = bars
            .iter()
            .map(|&v| {
                let level = ((v / MAX_HEIGHT).clamp(0.0, 1.0) * 8.0) as usize;
                [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"][level.min(8)]
            })
            .collect();
        print!("\r[{row}] peak {peak:6.2}");
        use std::io::Write;
        std::io::stdout().flush().ok();

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    capture.stop();
    println!("\n");

    if frames == 0 {
        println!("❌ No audio buffers arrived. The device is not delivering samples.");
    } else if loudest <= 0.0 {
        println!("⚠️  {frames} buffers arrived but every sample was silent.");
        println!("   On macOS, check that Background Music is the output device.");
    } else {
        println!("✅ {frames} buffers, loudest band {loudest:.2} of {MAX_HEIGHT:.0} full scale.");
    }
    Ok(())
}
