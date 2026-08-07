#![allow(dead_code)]

use crate::theme::Gradient;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Paragraph, Widget};

// Braille dot bit patterns:
// 0x01  0x08
// 0x02  0x10
// 0x04  0x20
// 0x40  0x80
const DOT_BITS: [[u32; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// A compact, btop-style history chart.
///
/// The surrounding block is supplied by the UI so titles, focus borders, and
/// padding stay consistent with the rest of the dashboard.
pub struct BrailleChart<'a> {
    data: &'a [(f64, f64)],
    max: f64,
    gradient: &'a Gradient,
    graph_text: Color,
    block: Option<Block<'a>>,
    fill: bool,
}

impl<'a> BrailleChart<'a> {
    pub fn new(
        data: &'a [(f64, f64)],
        max: f64,
        gradient: &'a Gradient,
        graph_text: Color,
    ) -> Self {
        Self {
            data,
            max,
            gradient,
            graph_text,
            block: None,
            fill: true,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn fill(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }
}

impl Widget for BrailleChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let inner = if let Some(block) = self.block {
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        if inner.width < 4 || inner.height == 0 {
            return;
        }

        // Keep the scale inside the panel. The previous chart put it outside
        // its border, which made adjacent boxes look visually disconnected.
        let label_width = if inner.width >= 22 { 6 } else { 0 };
        let graph_area = Rect {
            x: inner.x + label_width,
            y: inner.y,
            width: inner.width.saturating_sub(label_width),
            height: inner.height,
        };

        if graph_area.width < 2 || graph_area.height == 0 {
            return;
        }

        let max = self.max.max(1.0);
        draw_scale_and_guides(buf, inner, graph_area, label_width, max, self.graph_text);

        if self.data.is_empty() {
            Paragraph::new("collecting samples")
                .style(Style::default().fg(self.graph_text))
                .alignment(Alignment::Center)
                .render(graph_area, buf);
            return;
        }

        let grid_width = graph_area.width as usize * 2;
        let grid_height = graph_area.height as usize * 4;
        let mut grid = vec![vec![false; grid_width]; grid_height];

        // Use the most recent samples and right-align short histories. This
        // gives the graph a stable "now" edge while samples accumulate.
        let sample_count = self.data.len();
        if sample_count <= grid_width {
            let offset = grid_width - sample_count;
            let mut previous_y = None;
            for (index, (_, value)) in self.data.iter().enumerate() {
                let column = offset + index;
                let y = value_to_y(*value, max, grid_height);
                plot_column(&mut grid, column, y, previous_y, grid_height, self.fill);
                previous_y = Some(y);
            }
        } else {
            let step = sample_count as f64 / grid_width as f64;
            let mut previous_y = None;
            for column in 0..grid_width {
                let index = ((column as f64 * step) as usize).min(sample_count - 1);
                let y = value_to_y(self.data[index].1, max, grid_height);
                plot_column(&mut grid, column, y, previous_y, grid_height, self.fill);
                previous_y = Some(y);
            }
        }

        for cell_y in 0..graph_area.height as usize {
            for cell_x in 0..graph_area.width as usize {
                let mut braille = 0x2800;
                for (dot_y, row_bits) in DOT_BITS.iter().enumerate() {
                    for (dot_x, bit) in row_bits.iter().enumerate() {
                        let grid_y = cell_y * 4 + dot_y;
                        let grid_x = cell_x * 2 + dot_x;
                        if grid[grid_y][grid_x] {
                            braille |= bit;
                        }
                    }
                }

                if braille != 0x2800 {
                    let position = (graph_area.x + cell_x as u16, graph_area.y + cell_y as u16);
                    if let Some(cell) = buf.cell_mut(position) {
                        cell.set_char(char::from_u32(braille).unwrap_or(' '));
                        cell.set_style(
                            Style::default().fg(self
                                .gradient
                                .at(row_gradient_percent(cell_y, graph_area.height as usize))),
                        );
                    }
                }
            }
        }
    }
}

fn value_to_y(value: f64, max: f64, grid_height: usize) -> usize {
    let normalized = (value / max).clamp(0.0, 1.0);
    ((1.0 - normalized) * (grid_height.saturating_sub(1)) as f64) as usize
}

fn plot_column(
    grid: &mut [Vec<bool>],
    column: usize,
    y: usize,
    previous_y: Option<usize>,
    grid_height: usize,
    fill: bool,
) {
    if let Some(previous_y) = previous_y {
        let (start, end) = if y < previous_y {
            (y, previous_y)
        } else {
            (previous_y, y)
        };
        for row in grid.iter_mut().take(end + 1).skip(start) {
            row[column] = true;
        }
    } else {
        grid[y][column] = true;
    }

    if fill {
        for row in grid.iter_mut().take(grid_height).skip(y) {
            row[column] = true;
        }
    }
}

fn draw_scale_and_guides(
    buf: &mut Buffer,
    inner: Rect,
    graph_area: Rect,
    label_width: u16,
    max: f64,
    graph_text: Color,
) {
    let guides = if graph_area.height > 2 {
        vec![
            (0, max),
            (graph_area.height / 2, max / 2.0),
            (graph_area.height - 1, 0.0),
        ]
    } else {
        vec![(0, max)]
    };

    for (row, value) in guides {
        if label_width > 0 {
            let label = format_axis_value(value);
            let label_x = inner.x + label_width.saturating_sub(label.len() as u16 + 1);
            buf.set_string(
                label_x,
                inner.y + row,
                label,
                Style::default().fg(graph_text),
            );
        }

        for x in graph_area.x..graph_area.right() {
            if let Some(cell) = buf.cell_mut((x, graph_area.y + row)) {
                cell.set_char('·');
                cell.set_style(Style::default().fg(graph_text));
            }
        }
    }
}

fn row_gradient_percent(row: usize, height: usize) -> f64 {
    if height <= 1 {
        100.0
    } else {
        100.0 - row as f64 * 100.0 / (height - 1) as f64
    }
}

fn format_axis_value(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value >= 10_000.0 {
        format!("{:.0}K", value / 1_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else if value >= 10.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_values_stay_compact() {
        assert_eq!(format_axis_value(0.0), "0.0");
        assert_eq!(format_axis_value(42.0), "42");
        assert_eq!(format_axis_value(1_250.0), "1.2K");
        assert_eq!(format_axis_value(2_500_000.0), "2.5M");
    }

    #[test]
    fn values_are_clamped_to_the_chart() {
        assert_eq!(value_to_y(-10.0, 100.0, 20), 19);
        assert_eq!(value_to_y(100.0, 100.0, 20), 0);
        assert_eq!(value_to_y(200.0, 100.0, 20), 0);
    }

    #[test]
    fn graph_rows_span_the_full_theme_gradient() {
        assert_eq!(row_gradient_percent(0, 5), 100.0);
        assert_eq!(row_gradient_percent(2, 5), 50.0);
        assert_eq!(row_gradient_percent(4, 5), 0.0);
    }
}
