mod app;
mod audio;
mod error;
mod ui;

use anyhow::Result;
use crossbeam_channel::unbounded;
use crossterm::{
    ExecutableCommand,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::time::Duration;

use crate::{
    app::App,
    audio::capture::AudioCapture,
    ui::events::{AppEvent, EventHandler},
};

fn main() -> Result<()> {
    // Setup terminal
    terminal::enable_raw_mode()?;
    std::io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;

    // Create channels for audio data
    let (sender, receiver) = unbounded();

    // Initialize audio capture
    let _audio_capture = AudioCapture::new(sender)?;

    // Create app and event handler
    let mut app = App::new(receiver);
    let event_handler = EventHandler::new(Duration::from_millis(16));

    // Main loop
    while !app.should_quit {
        // Draw UI
        terminal.draw(|frame| {
            app.visualizer.render(frame, frame.area());
        })?;

        // Handle events
        match event_handler.next()? {
            AppEvent::Key(key) => app.handle_key(key),
            AppEvent::Tick => app.update(),
        }
    }

    // Cleanup
    terminal::disable_raw_mode()?;
    std::io::stdout().execute(LeaveAlternateScreen)?;

    Ok(())
}
