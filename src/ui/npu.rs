use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Row, Table};

use crate::app::{App, Section};
use crate::gauge::{self, Kind};
use crate::theme::{SectionBox, UtilKind};

use super::{render_bar, section_block};

pub(super) fn draw(f: &mut Frame, area: Rect, app: &App) {
    let block = section_block(app, Section::Npu, "NPU", SectionBox::Npu);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let npu = app.npu_info.as_ref();
    let fdinfo_supported = npu.is_some_and(|n| n.fdinfo_supported);

    if app.is_collapsed(Section::Npu) {
        let pct_span = if fdinfo_supported {
            let npu_pct = app.hist_npu.latest() as f64;
            Span::styled(
                format!(" {:>3}% ", npu_pct.round()),
                Style::default().fg(app.theme.util_color(npu_pct, UtilKind::Npu)),
            )
        } else {
            Span::styled(" N/A ", Style::default().fg(app.theme.inactive_fg()))
        };
        let status = if fdinfo_supported {
            format!("{} contexts  ", npu_ctx_count(app))
        } else {
            "telemetry unavailable  ".to_string()
        };
        let line = Line::from(vec![
            pct_span,
            Span::styled(status, Style::default().fg(app.theme.graph_text())),
        ]);
        f.render_widget(Paragraph::new(line), inner);
        return;
    }

    let top_table = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(inner);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(38), Constraint::Min(20)])
        .split(top_table[0]);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(cols[0]);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " NPU ",
                Style::default()
                    .fg(app.theme.hi_fg())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                npu.map_or("XDNA", |npu| npu.name.as_str()),
                Style::default().fg(app.theme.title()),
            ),
        ])),
        left[0],
    );
    let firmware = npu
        .and_then(|npu| npu.fw_version.as_deref())
        .unwrap_or("unknown");
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" fw ", Style::default().fg(app.theme.graph_text())),
            Span::styled(firmware, Style::default().fg(app.theme.proc_misc())),
        ])),
        left[1],
    );
    let bdf = npu.map_or("unknown", |npu| npu.bdf.as_str());
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" bdf ", Style::default().fg(app.theme.graph_text())),
            Span::styled(bdf, Style::default().fg(app.theme.proc_misc())),
        ])),
        left[2],
    );
    let telemetry = if fdinfo_supported {
        "fdinfo telemetry"
    } else {
        "detected; fdinfo unavailable"
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {telemetry}"),
            Style::default().fg(app.theme.graph_text()),
        ))),
        left[3],
    );

    let npu_pct = fdinfo_supported.then(|| app.hist_npu.latest() as f64);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(cols[1]);
    f.render_widget(
        Paragraph::new(render_bar(
            app,
            gauge::Bar::new("NPU", npu_pct, cols[1].width as usize, Kind::Npu),
        )),
        right[0],
    );
    if fdinfo_supported {
        let graph = app
            .hist_npu
            .braille_graph(cols[1].width as usize, 2, app.theme.npu());
        let grows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(right[1]);
        for (line, area) in graph.into_iter().zip(grows.iter().copied()) {
            f.render_widget(Paragraph::new(line), area);
        }
    } else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " telemetry N/A",
                Style::default().fg(app.theme.inactive_fg()),
            ))),
            right[1],
        );
    }

    let rows: Vec<Row> = app
        .active_apps()
        .flat_map(|a| a.stat.xdna_fdinfo.proc_usage.iter())
        .map(|pu| {
            Row::new(vec![
                pu.pid.to_string(),
                pu.name.chars().take(24).collect::<String>(),
                pu.ids_count.to_string(),
                format!("{}K", pu.usage.total_memory),
                pu.usage.npu.to_string(),
            ])
        })
        .collect();

    if rows.is_empty() {
        let msg = if fdinfo_supported {
            " no active NPU contexts "
        } else {
            " NPU present; amdxdna fdinfo telemetry unavailable "
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                msg,
                Style::default().fg(app.theme.graph_text()),
            ))),
            top_table[1],
        );
        return;
    }

    let header = Row::new(vec!["PID", "NAME", "CTX", "MEM", "NPU%"]).style(
        Style::default()
            .fg(app.theme.proc_misc())
            .add_modifier(Modifier::BOLD),
    );
    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(5),
            Constraint::Length(10),
            Constraint::Length(6),
        ],
    )
    .header(header);
    f.render_widget(table, top_table[1]);
}

fn npu_ctx_count(app: &App) -> usize {
    app.active_apps()
        .map(|a| a.stat.xdna_fdinfo.proc_usage.len())
        .sum()
}
