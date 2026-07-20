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
        "GPU", "PID", "NAME", "MEM", "VRAM", "GTT", "GFX%", "COMP%", "DMA%", "CPU%",
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
                let gpu_activity = [pu.usage.gfx, pu.usage.compute, pu.usage.dma]
                    .into_iter()
                    .max()
                    .unwrap_or(0)
                    .clamp(0, 100) as f64;
                Row::new(vec![
                    gi.to_string(),
                    pu.pid.to_string(),
                    pu.name.chars().take(24).collect::<String>(),
                    fmt_kib(app.process_rss_kb(pu.pid)),
                    fmt_kib(Some(pu.usage.vram_usage)),
                    fmt_kib(Some(pu.usage.gtt_usage)),
                    pu.usage.gfx.to_string(),
                    pu.usage.compute.to_string(),
                    pu.usage.dma.to_string(),
                    pu.usage.cpu.to_string(),
                ])
                .style(Style::default().fg(app.theme.process().sample(gpu_activity / 100.0)))
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

fn fmt_kib(kib: Option<u64>) -> String {
    const KIB_PER_MIB: u64 = 1 << 10;
    const KIB_PER_GIB: u64 = 1 << 20;

    let Some(kib) = kib else {
        return "-".to_string();
    };
    if kib >= KIB_PER_GIB {
        format!("{:.1}G", kib as f64 / KIB_PER_GIB as f64)
    } else {
        format!("{}M", kib / KIB_PER_MIB)
    }
}

#[cfg(test)]
mod tests {
    use super::fmt_kib;

    #[test]
    fn memory_columns_use_compact_binary_units() {
        assert_eq!(fmt_kib(Some(512 << 10)), "512M");
        assert_eq!(fmt_kib(Some(3 << 20)), "3.0G");
        assert_eq!(fmt_kib(None), "-");
    }
}
