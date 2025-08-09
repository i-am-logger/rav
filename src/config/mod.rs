use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub audio: AudioConfig,
    pub display: DisplayConfig,
    pub colors: ColorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub buffer_size: usize,
    pub device: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub refresh_rate: u32,
    pub frequency_bands: usize,
    pub frequency_range: (f32, f32),
    pub smoothing_factor: f32,
    pub sensitivity: f32,
    pub mode: VisualizationMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VisualizationMode {
    Bars,
    Wave,
    Spectrum,
    Dots,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConfig {
    pub gradient: Vec<String>,
    pub background: String,
    pub border: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            audio: AudioConfig {
                sample_rate: 44100,
                buffer_size: 1024,
                device: None,
            },
            display: DisplayConfig {
                refresh_rate: 60,
                frequency_bands: 80,
                frequency_range: (20.0, 20000.0),
                smoothing_factor: 0.8,
                sensitivity: 1.0,
                mode: VisualizationMode::Bars,
            },
            colors: ColorConfig {
                gradient: vec![
                    "#FF0000".to_string(), // Red
                    "#FF8000".to_string(), // Orange
                    "#FFFF00".to_string(), // Yellow
                    "#80FF00".to_string(), // Green-Yellow
                    "#00FF00".to_string(), // Green
                    "#00FF80".to_string(), // Green-Cyan
                    "#00FFFF".to_string(), // Cyan
                    "#0080FF".to_string(), // Blue-Cyan
                    "#0000FF".to_string(), // Blue
                    "#8000FF".to_string(), // Purple
                    "#FF00FF".to_string(), // Magenta
                ],
                background: "#000000".to_string(),
                border: "#FFFFFF".to_string(),
            },
        }
    }
}

impl Config {
    pub async fn load(config_path: Option<&Path>) -> Result<Self> {
        if let Some(path) = config_path {
            // Load from specified path
            let content = tokio::fs::read_to_string(path).await?;
            Ok(toml::from_str(&content)?)
        } else {
            // Try to load from default location
            if let Some(config_dir) = dirs::config_dir() {
                let default_path = config_dir.join("speedy").join("config.toml");
                if default_path.exists() {
                    let content = tokio::fs::read_to_string(&default_path).await?;
                    Ok(toml::from_str(&content)?)
                } else {
                    // Create default config and save it
                    let config = Self::default();
                    config.save_to_default_location().await?;
                    Ok(config)
                }
            } else {
                Ok(Self::default())
            }
        }
    }

    pub async fn save_to_default_location(&self) -> Result<()> {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("speedy");
            tokio::fs::create_dir_all(&config_path).await?;

            let config_file = config_path.join("config.toml");
            let content = toml::to_string_pretty(self)?;
            tokio::fs::write(config_file, content).await?;
        }
        Ok(())
    }
}
