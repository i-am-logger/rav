use ratatui::{
    prelude::*,
    widgets::{Block, Borders},
};

pub struct Visualizer {
    data: Vec<u64>,
    max_height: u64,
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            max_height: 100,
        }
    }

    pub fn update(&mut self, new_data: Vec<u64>) {
        self.data = new_data;
        self.max_height = self.data.iter().max().copied().unwrap_or(100);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::NONE);
        //     .title("Audio Visualization");

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        let bar_width = 3u16;
        let gap = 1u16;
        let num_bars = (inner_area.width / (bar_width + gap)) as usize;
        let data = if self.data.len() > num_bars {
            // Average the data to fit the available space
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
            let height =
                ((value as f64 / self.max_height as f64) * inner_area.height as f64) as u16;
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
    }
}
