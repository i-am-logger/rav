use crossterm::event::{self, Event, KeyCode};
use std::time::Duration;

pub enum AppEvent {
    Key(KeyCode),
    Tick,
}

pub struct EventHandler {
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        Self { tick_rate }
    }

    pub fn next(&self) -> anyhow::Result<AppEvent> {
        if event::poll(self.tick_rate)? {
            if let Event::Key(key) = event::read()? {
                return Ok(AppEvent::Key(key.code));
            }
        }
        Ok(AppEvent::Tick)
    }
}
