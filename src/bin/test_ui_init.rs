// Simple test to verify Professional UI initialization works correctly
use speedy::{
    config::Config, 
    signal::SignalProcessor,
    ui::SpeedyV1Interface
};
use tracing::{info, warn, error};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize basic logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🧪 Testing Professional UI initialization...");

    // Create default config
    let config = Config::default();
    info!("✅ Config created successfully");

    // Create signal processor
    let signal_processor = SignalProcessor::new(
        config.audio.sample_rate,
        config.display.frequency_bands,
        config.display.frequency_range.clone(),
    );
    info!("✅ SignalProcessor created successfully");

    // Test Professional UI creation
    info!("🎨 Attempting to create Professional UI (v1 interface)...");
    match SpeedyV1Interface::new(config.clone(), signal_processor) {
        Ok(_interface) => {
            info!("✅ Professional UI (v1 interface) created successfully!");
            info!("🎯 Professional UI initialization test PASSED");
        }
        Err(e) => {
            error!("❌ Failed to create Professional UI: {}", e);
            error!("🎯 Professional UI initialization test FAILED");
            return Err(e);
        }
    }

    info!("🎉 UI initialization test completed successfully");
    Ok(())
}
