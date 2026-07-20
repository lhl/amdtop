//! Ring-buffer history + braille graph rendering (nvitop-faithful algorithm).
//!
//! Each terminal cell holds a 2×4 braille grid = 2 horizontal data samples ×
//! 4 vertical dots. With 5 levels per column (0..=4 dots), each cell encodes
//! a (`left_level`, `right_level`) pair. A graph `H` rows tall has `4*H` vertical
//! sub-pixels. Colored per-cell via the theme gradient.

use std::collections::VecDeque;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::theme::Gradient;

pub struct History {
    buf: VecDeque<u64>,
    capacity: usize,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, value: u64) {
        if self.capacity == 0 {
            return;
        }
        if self.buf.len() >= self.capacity {
            self.buf.pop_front();
        }
        self.buf.push_back(value.min(100));
    }

    pub fn latest(&self) -> u64 {
        self.buf.back().copied().unwrap_or(0)
    }

    /// Render a multi-row braille area graph of the most recent `2*width`
    /// samples. `height` = terminal rows. Values 0..100. Each cell colored by
    /// the theme gradient sampled at the cell's peak value.
    pub fn braille_graph(
        &self,
        width: usize,
        height: usize,
        grad: &Gradient,
    ) -> Vec<Line<'static>> {
        if height == 0 || width == 0 {
            return Vec::new();
        }
        // We need 2*width samples, right-aligned so the newest is at the right
        // edge. Pad the LEFT with baseline zeros when the buffer isn't full.
        let need = 2 * width;
        let n = self.buf.len();
        let take = n.min(need);
        let pad = need - take;
        let mut samples =
            std::iter::repeat_n(0, pad).chain(self.buf.iter().skip(n - take).copied());

        // Build bottom-first rows. A small minimum height keeps an idle
        // baseline visible, matching nvitop's graph style.
        let mut rows: Vec<Vec<(char, Option<Color>)>> =
            (0..height).map(|_| Vec::with_capacity(width)).collect();

        for _ in 0..width {
            let v1 = samples.next().unwrap_or(0) as f64;
            let v2 = samples.next().unwrap_or(0) as f64;
            let scaled1 = (height as f64 * (v1 / 100.0)).max(0.2);
            let scaled2 = (height as f64 * (v2 / 100.0)).max(0.2);
            let color = grad.sample(v1.max(v2) / 100.0);

            for (height_index, row) in rows.iter_mut().enumerate() {
                let level1 = (5.0 * (scaled1 - height_index as f64))
                    .round()
                    .clamp(0.0, 4.0) as u8;
                let level2 = (5.0 * (scaled2 - height_index as f64))
                    .round()
                    .clamp(0.0, 4.0) as u8;
                let color = (level1 != 0 || level2 != 0).then_some(color);
                row.push((braille_char(level1, level2), color));
            }
        }

        // nvitop reverses the bottom-first data for top-first terminal output.
        rows.into_iter()
            .rev()
            .map(|row| {
                let spans = row
                    .into_iter()
                    .map(|(cell, color)| match color {
                        Some(color) => Span::styled(cell.to_string(), Style::default().fg(color)),
                        None => Span::raw(cell.to_string()),
                    })
                    .collect::<Vec<_>>();
                Line::from(spans)
            })
            .collect()
    }
}

/// Build a braille char from left/right column fill levels (0..=4 each).
/// Left column dots (bottom-up): 7,3,2,1. Right column: 8,6,5,4.
fn braille_char(l1: u8, l2: u8) -> char {
    let mut bits: u32 = 0;
    if l1 >= 1 {
        bits |= 0x40;
    } // dot 7
    if l1 >= 2 {
        bits |= 0x04;
    } // dot 3
    if l1 >= 3 {
        bits |= 0x02;
    } // dot 2
    if l1 >= 4 {
        bits |= 0x01;
    } // dot 1
    if l2 >= 1 {
        bits |= 0x80;
    } // dot 8
    if l2 >= 2 {
        bits |= 0x20;
    } // dot 6
    if l2 >= 3 {
        bits |= 0x10;
    } // dot 5
    if l2 >= 4 {
        bits |= 0x08;
    } // dot 4
    char::from_u32(0x2800 + bits).unwrap_or(' ')
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::{History, braille_char};
    use crate::theme::Gradient;

    #[test]
    fn history_keeps_the_most_recent_clamped_values() {
        let mut history = History::new(3);
        for value in [10, 20, 30, 140] {
            history.push(value);
        }

        assert_eq!(
            history.buf.iter().copied().collect::<Vec<_>>(),
            vec![20, 30, 100]
        );
        assert_eq!(history.latest(), 100);
    }

    #[test]
    fn zero_capacity_history_ignores_samples() {
        let mut history = History::new(0);
        history.push(50);
        assert!(history.buf.is_empty());
    }

    #[test]
    fn braille_columns_map_to_expected_cells() {
        assert_eq!(braille_char(0, 0), '⠀');
        assert_eq!(braille_char(4, 4), '⣿');
    }

    #[test]
    fn braille_graph_has_requested_dimensions() {
        let mut history = History::new(8);
        for value in [0, 25, 50, 75, 100] {
            history.push(value);
        }
        let gradient = Gradient::three(Color::Red, Color::Red, Color::Red);

        let graph = history.braille_graph(4, 3, &gradient);
        assert_eq!(graph.len(), 3);
        assert!(graph.iter().all(|line| line.width() == 4));
    }

    #[test]
    fn braille_graph_handles_empty_dimensions() {
        let history = History::new(8);
        let gradient = Gradient::three(Color::Red, Color::Red, Color::Red);

        assert!(history.braille_graph(0, 1, &gradient).is_empty());
        assert!(history.braille_graph(1, 0, &gradient).is_empty());
    }
}
