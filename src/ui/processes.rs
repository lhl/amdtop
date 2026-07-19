use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Row, Table};

use crate::app::{App, Section};
use crate::theme::SectionBox;

use super::section_block;

pub(super) fn draw(f: &mut Frame, area: Rect, app: &App) {
    let block = section_block(app, Section::Processes, "PROCESSES", SectionBox::Proc);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.is_collapsed(Section::Processes) {
        let n: usize = app
            .apps
            .iter()
            .map(|a| a.stat.fdinfo.proc_usage.len())
            .sum();
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {n} processes "),
                Style::default().fg(app.theme.graph_text()),
            ))),
            inner,
        );
        return;
    }

    let header = Row::new(vec![
        "GPU", "PID", "NAME", "VRAM", "GTT", "GFX%", "COMP%", "DMA%", "CPU%",
    ])
    .style(
        Style::default()
            .fg(app.theme.proc_misc())
            .add_modifier(Modifier::BOLD),
    );
    let rows: Vec<Row> = app
        .apps
        .iter()
        .enumerate()
        .flat_map(|(gi, a)| {
            a.stat.fdinfo.proc_usage.iter().map(move |pu| {
                Row::new(vec![
                    gi.to_string(),
                    pu.pid.to_string(),
                    pu.name.chars().take(24).collect::<String>(),
                    format!("{}M", pu.usage.vram_usage >> 10),
                    format!("{}M", pu.usage.gtt_usage >> 10),
                    pu.usage.gfx.to_string(),
                    pu.usage.compute.to_string(),
                    pu.usage.dma.to_string(),
                    pu.usage.cpu.to_string(),
                ])
            })
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Length(7),
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Length(6),
        ],
    )
    .header(header);
    f.render_widget(table, inner);
}
