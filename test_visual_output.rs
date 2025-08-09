use speedy::{
    config::Config,
    signal::SignalProcessor,
    ui::App,
    audio::AudioData,
};
use flume::unbounded;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create basic config
    let config = Config::default();
    let signal_processor = SignalProcessor::new(
        config.audio.sample_rate,
        config.display.frequency_bands,
        config.display.frequency_range,
        config.audio.buffer_size,
    );
    
    // Create the UI app
    let mut app = App::new(config, signal_processor);
    
    // Create a channel for fake audio data
    let (audio_tx, audio_rx) = unbounded();
    
    // Send some fake audio data to generate visualizations
    tokio::spawn(async move {
        for i in 0..100 {
            let fake_samples: Vec<f32> = (0..1024)
                .map(|j| ((i * j) as f32 * 0.01).sin() * 0.5)
                .collect();
            
            let audio_data = AudioData {
                samples: fake_samples,
                sample_rate: 44100,
                timestamp: std::time::SystemTime::now(),
            };
            
            let _ = audio_tx.send(audio_data);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    
    // Run for 5 seconds
    tokio::select! {
        _ = app.run(audio_rx) => {},
        _ = tokio::time::sleep(Duration::from_secs(5)) => {}
    }
    
    println!("Visual test completed");
    Ok(())
}
