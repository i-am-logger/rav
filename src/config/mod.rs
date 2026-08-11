//! On-disk settings.
//!
//! Deliberately small: only what rav reads at startup and cannot ask the
//! terminal or the audio device for. Everything about the *look* - bar style,
//! peaks, frequency range, gain - is a runtime key, and the colours come from a
//! theme file, so none of it belongs here.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub audio: AudioConfig,
    pub display: DisplayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Requested capture rate. The device may renegotiate it; whatever it lands
    /// on is what the analysis chain uses.
    pub sample_rate: u32,
    /// Samples per block handed to the analyser.
    pub buffer_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    /// Redraw rate, clamped to 1..=240 at use.
    pub refresh_rate: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            audio: AudioConfig {
                sample_rate: 44100,
                buffer_size: 1024,
            },
            display: DisplayConfig { refresh_rate: 60 },
        }
    }
}

impl Config {
    pub async fn load(config_path: Option<&Path>) -> Result<Self> {
        if let Some(path) = config_path {
            // Load from specified path
            let content = tokio::fs::read_to_string(path).await?;
            return Ok(toml::from_str(&content)?);
        }

        let Some(config_dir) = dirs::config_dir() else {
            return Ok(Self::default());
        };

        let default_path = config_dir.join("rav").join("config.toml");
        if default_path.exists() {
            let content = tokio::fs::read_to_string(&default_path).await?;
            Ok(toml::from_str(&content)?)
        } else {
            // Write the defaults out, so the file exists to be edited.
            let config = Self::default();
            config.save_to_default_location().await?;
            Ok(config)
        }
    }

    pub async fn save_to_default_location(&self) -> Result<()> {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("rav");
            tokio::fs::create_dir_all(&config_path).await?;

            let config_file = config_path.join("config.toml");
            let content = toml::to_string_pretty(self)?;
            tokio::fs::write(config_file, content).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_with_extra_keys_still_loads() {
        // Older files carry settings that moved to runtime keys; they must not
        // stop rav from starting.
        let text = r#"
            [audio]
            sample_rate = 48000
            buffer_size = 2048
            device = "some-capture-device"

            [display]
            refresh_rate = 30
            sensitivity = 1.0
        "#;
        let config: Config = toml::from_str(text).expect("should parse");
        assert_eq!(config.audio.sample_rate, 48_000);
        assert_eq!(config.display.refresh_rate, 30);
    }

    #[test]
    fn the_defaults_round_trip() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.audio.buffer_size, Config::default().audio.buffer_size);
    }
}
