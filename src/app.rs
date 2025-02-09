use crate::ui::visualizer::Visualizer;
use crossbeam_channel::Receiver;
use crossterm::event::KeyCode;

pub struct App {
    pub should_quit: bool,
    pub visualizer: Visualizer,
    audio_receiver: Receiver<Vec<u64>>,
}

impl App {
    pub fn new(audio_receiver: Receiver<Vec<u64>>) -> Self {
        Self {
            should_quit: false,
            visualizer: Visualizer::new(),
            audio_receiver,
        }
    }

    pub fn update(&mut self) {
        // Process all available audio data
        while let Ok(data) = self.audio_receiver.try_recv() {
            self.visualizer.update(data);
        }
    }

    pub fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up => self.visualizer.increase_sensitivity(),
            KeyCode::Down => self.visualizer.decrease_sensitivity(),
            _ => {}
        }
    }
}
