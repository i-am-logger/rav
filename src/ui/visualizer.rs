use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};
use std::time::{Duration, Instant};

pub struct Visualizer {
    data: Vec<u64>,
    max_height: u64,
    sensitivity: f32,
    overlay_timeout: Option<Instant>, // When to hide the overlay
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            max_height: 100,
            sensitivity: 1.0,
            overlay_timeout: None,
        }
    }

    pub fn increase_sensitivity(&mut self) {
        self.sensitivity *= 1.2;
        self.sensitivity = self.sensitivity.min(100.0);
        self.overlay_timeout = Some(Instant::now() + Duration::from_millis(500));
    }

    pub fn decrease_sensitivity(&mut self) {
        self.sensitivity /= 1.2;
        self.sensitivity = self.sensitivity.max(1.0);
        self.overlay_timeout = Some(Instant::now() + Duration::from_millis(500));
    }

    pub fn update(&mut self, new_data: Vec<u64>) {
        self.data = new_data;
        self.max_height = self.data.iter().max().copied().unwrap_or(100);

        // Check if we should hide the overlay
        if let Some(timeout) = self.overlay_timeout {
            if Instant::now() > timeout {
                self.overlay_timeout = None;
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::NONE);
        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let bar_width = 3u16;
        let gap = 1u16;
        let num_bars = (inner_area.width / (bar_width + gap)) as usize;
        let data = if self.data.len() > num_bars {
            let chunk_size = self.data.len() / num_bars;
            (0..num_bars)
                .map(|i| {
                    let start = i * chunk_size;
                    let end = (i + 1) * chunk_size;
                    let sum: u64 = self.data[start..end].iter().sum();
                    sum / chunk_size as u64
                })
                .collect::<Vec<_>>()
        } else {
            self.data.clone()
        };

        for (i, &value) in data.iter().enumerate() {
            let scaled_value = (value as f32 * self.sensitivity) as f64;
            let max_scaled = (self.max_height as f32 * self.sensitivity) as f64;

            let height = ((scaled_value / max_scaled) * inner_area.height as f64) as u16;
            let bar_area = Rect::new(
                inner_area.left() + (i as u16 * (bar_width + gap)),
                inner_area.bottom().saturating_sub(height),
                bar_width,
                height,
            );

            frame.render_widget(
                Block::default().style(Style::default().bg(Color::Cyan)),
                bar_area,
            );
        }

        // Draw sensitivity overlay if active
        if self.overlay_timeout.is_some() {
            let text = format!("Sensitivity: {:.1}x", self.sensitivity);
            let width = text.len() as u16 + 4; // Add padding
            let height = 3;

            // Center the overlay
            let overlay_area = Rect::new(
                inner_area.left() + (inner_area.width - width) / 2,
                inner_area.top() + (inner_area.height - height) / 2,
                width,
                height,
            );

            let overlay_block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default().bg(Color::Black));

            frame.render_widget(overlay_block, overlay_area);
            frame.render_widget(
                Paragraph::new(text)
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::White)),
                overlay_area.inner(Margin {
                    vertical: 1,
                    horizontal: 2,
                }),
            );
        }
    }
}
