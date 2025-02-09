use super::processor::AudioProcessor;
use crate::error::{RavError, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Sender;
use std::time::Duration;

pub struct AudioCapture {
    _stream: cpal::Stream,
}

impl AudioCapture {
    pub fn new(sender: Sender<Vec<u64>>) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| RavError::AudioError("No input device found".into()))?;

        let config = device.default_input_config()?;
        let mut processor = AudioProcessor::new(1024, sender);

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                processor.process_audio(data);
            },
            move |err| eprintln!("An error occurred on stream: {}", err),
            Some(Duration::from_secs_f32(0.1)), // 100ms buffer size
        )?;

        stream.play()?;

        Ok(Self { _stream: stream })
    }
}
