use cpal::{BuildStreamError, DefaultStreamConfigError, PlayStreamError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RavError {
    #[error("Audio error:  {0}")]
    AudioError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Audio device error: {0}")]
    DeviceError(#[from] cpal::DevicesError),

    #[error("Audio stream error: {0}")]
    StreamError(#[from] cpal::StreamError),

    #[error("Build stream error: {0}")]
    BuildStreamError(#[from] BuildStreamError),

    #[error("Play stream error: {0}")]
    PlayStreamError(#[from] PlayStreamError),

    #[error("Default stream config error: {0}")]
    DefaultStreamConfigError(#[from] DefaultStreamConfigError),
}

pub type Result<T> = std::result::Result<T, RavError>;
