use std::sync::Arc;

use crossbeam_channel::Sender;
use rustfft::{FftPlanner, num_complex::Complex};

pub struct AudioProcessor {
    fft: Arc<dyn rustfft::Fft<f32>>,
    buffer: Vec<Complex<f32>>,
    window_size: usize,
    visualization_sender: Sender<Vec<u64>>,
}

impl AudioProcessor {
    pub fn new(window_size: usize, visualization_sender: Sender<Vec<u64>>) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(window_size);
        let buffer = vec![Complex::default(); window_size];

        Self {
            fft,
            buffer,
            window_size,
            visualization_sender,
        }
    }

    pub fn process_audio(&mut self, input: &[f32]) {
        //Apply Hanning window and copy to buffer
        for (i, &sample) in input.iter().take(self.window_size).enumerate() {
            let window = 0.5
                * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / self.window_size as f32).cos());
            self.buffer[i] = Complex::new(sample * window, 0.0);
        }

        // Perform FFT
        self.fft.process(&mut self.buffer);

        // Convert to magnitudes and scale for visualization
        let magnitudes: Vec<u64> = self
            .buffer
            .iter()
            .take(self.window_size / 2)
            .map(|c| (c.norm() * 100.0) as u64)
            .collect();

        // Send to visualization thread
        let _ = self.visualization_sender.send(magnitudes);
    }
}
