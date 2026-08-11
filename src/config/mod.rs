//! On-disk settings.
//!
//! Deliberately small: only what rav reads at startup and cannot ask the
//! terminal or the audio device for. Everything about the *look* - bar style,
//! peaks, frequency range, gain - is a runtime key, and the colours come from a
//! theme file, so none of it belongs here.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Everything here has a default, so a file may say only what it wants to
/// change. Requiring the whole shape means someone who writes three lines to
/// slow the redraw is told their file is missing a section they had no reason
/// to know about, and rav does not start.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub audio: AudioConfig,
    pub display: DisplayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// Requested capture rate. The device may renegotiate it; whatever it lands
    /// on is what the analysis chain uses.
    pub sample_rate: u32,
    /// Samples per block handed to the analyser.
    pub buffer_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// Redraw rate, clamped to 1..=240 at use.
    pub refresh_rate: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            buffer_size: 1024,
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self { refresh_rate: 60 }
    }
}

impl Config {
    pub async fn load(config_path: Option<&Path>) -> Result<Self> {
        if let Some(path) = config_path {
            return Self::read(path).await;
        }

        let Some(config_dir) = dirs::config_dir() else {
            return Ok(Self::default());
        };

        let default_path = config_dir.join("rav").join("config.toml");
        if default_path.exists() {
            Self::read(&default_path).await
        } else {
            // Write the defaults out, so the file exists to be edited.
            let config = Self::default();
            config.save_to_default_location().await?;
            Ok(config)
        }
    }

    /// Read one config file, naming it in whatever goes wrong. "No such file or
    /// directory" on its own leaves the user to guess which path rav tried.
    async fn read(path: &Path) -> Result<Self> {
        let content = tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("cannot read {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("{} is not a valid config", path.display()))
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
    fn a_file_may_say_only_what_it_wants_to_change() {
        // Three lines to slow the redraw is a reasonable thing to write. Being
        // told the file is missing a section you had no reason to know about,
        // and rav not starting, is not a reasonable thing to be told.
        let only_display: Config = toml::from_str("[display]\nrefresh_rate = 30\n")
            .expect("a file that sets one thing must load");
        assert_eq!(only_display.display.refresh_rate, 30);
        assert_eq!(
            only_display.audio.sample_rate,
            Config::default().audio.sample_rate,
            "the section it did not mention lost its default",
        );

        let only_rate: Config =
            toml::from_str("[audio]\nsample_rate = 48000\n").expect("half a section must load");
        assert_eq!(only_rate.audio.sample_rate, 48_000);
        assert_eq!(
            only_rate.audio.buffer_size,
            Config::default().audio.buffer_size,
        );

        assert!(
            toml::from_str::<Config>("").is_ok(),
            "an empty file is a file that changes nothing",
        );
    }

    #[test]
    fn a_setting_of_the_wrong_shape_is_still_refused() {
        // Defaults fill in what is absent, not what is wrong. A number written
        // as a word is a mistake worth being told about.
        assert!(toml::from_str::<Config>("[audio]\nsample_rate = \"loud\"\n").is_err());
    }

    #[test]
    fn the_defaults_round_trip() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.audio.buffer_size, Config::default().audio.buffer_size);
    }
}
