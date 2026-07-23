//! Modern TUI rendering: rounded borders, theme-defined gradients, braille
//! history graphs, collapsible CPU/GPU/NPU/Processes sections.
//!
//! GPU layout (nvitop/nvtop-inspired): one horizontal band per GPU.
//!   left column  -> identity + stats (index, name, bus-id, temp/pwr/clk/fan)
//!   right column -> GFX gauge, MEM/GTT gauge, braille history
//! Multi-GPU friendly: bands stack vertically.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Section};
use crate::gauge;
use crate::theme::SectionBox;

mod cpu;
mod gpu;
mod npu;
mod processes;

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    if let Some(bg) = app.theme.main_bg() {
        f.render_widget(Block::default().style(Style::default().bg(bg)), area);
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(build_constraints(app))
        .split(area);

    let mut idx = 1;
    draw_header(f, chunks[0], app);
    cpu::draw(f, chunks[idx], app);
    idx += 1;
    gpu::draw(f, chunks[idx], app);
    idx += 1;
    if app.has_npu {
        npu::draw(f, chunks[idx], app);
        idx += 1;
    }
    processes::draw(f, chunks[idx], app);
    draw_footer(f, chunks[chunks.len() - 1], app);
}

fn build_constraints(app: &App) -> Vec<Constraint> {
    let mut c = vec![Constraint::Length(1)];
    c.push(section_height(app, Section::Cpu));
    c.push(section_height(app, Section::Gpu));
    if app.has_npu {
        c.push(section_height(app, Section::Npu));
    }
    c.push(section_height(app, Section::Processes));
    c.push(Constraint::Length(1));
    c
}

fn section_height(app: &App, s: Section) -> Constraint {
    if app.is_collapsed(s) {
        return Constraint::Length(3);
    }
    let inner = match s {
        Section::Cpu => {
            let (_, rows) = cpu::grid_dimensions(app.cpu.per_core_percent.len().max(1));
            4 + rows as u16
        }
        Section::Gpu => (6 * app.gpus.len() as u16).max(1),
        Section::Npu => {
            let ctx = app
                .active_apps()
                .map(|a| a.stat.xdna_fdinfo.proc_usage.len())
                .sum::<usize>();
            5 + ctx.min(6) as u16
        }
        Section::Processes => return Constraint::Min(13),
    };
    Constraint::Length(inner + 2)
}

// ---------- shared ----------

fn section_block(app: &App, s: Section, title: &str, box_kind: SectionBox) -> Block<'static> {
    let focused = app.section == s;
    let indicator = collapse_indicator(app.is_collapsed(s));
    let title_span = Span::styled(
        format!(" {indicator} {title} "),
        Style::default()
            .fg(if focused {
                app.theme.hi_fg()
            } else {
                app.theme.title()
            })
            .add_modifier(Modifier::BOLD),
    );
    let border_color = if focused {
        app.theme.hi_fg()
    } else {
        app.theme.box_color(box_kind)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(border_color))
        .title(title_span)
}

fn collapse_indicator(collapsed: bool) -> &'static str {
    if collapsed { "▸" } else { "▾" }
}

fn render_bar(app: &App, bar: gauge::Bar<'_>) -> Line<'static> {
    gauge::bar(bar, &app.theme, gauge::block_style(app.block_style))
}

// ---------- header ----------

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let now = clock_string();
    let line = Line::from(vec![
        Span::styled(
            " amdtop ",
            Style::default()
                .fg(app.theme.hi_fg())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " {} device{} ",
                app.gpus.len(),
                if app.gpus.len() == 1 { "" } else { "s" }
            ),
            Style::default().fg(app.theme.graph_text()),
        ),
        Span::styled(
            format!(" {now} "),
            Style::default().fg(app.theme.graph_text()),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn clock_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let (m, d, hh, mm, ss) = civil(secs);
    format!("{m:02} {d:02} {hh:02}:{mm:02}:{ss:02} UTC")
}

fn civil(secs: u64) -> (u64, u64, u64, u64, u64) {
    let days = secs / 86400;
    let rem = secs % 86400;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (m, d, hh, mm, ss)
}

// ---------- footer ----------

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let bg = app.theme.selected_bg();
    let key = Style::default()
        .fg(app.theme.hi_fg())
        .add_modifier(Modifier::BOLD)
        .bg(bg);
    let lbl = Style::default().fg(app.theme.main_fg()).bg(bg);
    let spans = vec![
        Span::styled(" q/ctrl-c ", key),
        Span::styled("quit  ", lbl),
        Span::styled("tab ", key),
        Span::styled("section  ", lbl),
        Span::styled("space ", key),
        Span::styled("collapse  ", lbl),
        Span::styled("t/T ", key),
        Span::styled("theme ", lbl),
        Span::styled(
            format!("{} ", app.theme_name),
            Style::default()
                .fg(app.theme.selected_fg())
                .add_modifier(Modifier::BOLD)
                .bg(bg),
        ),
        Span::styled(" b ", key),
        Span::styled("blocks ", lbl),
        Span::styled(
            format!("{} ", app.block_style_name()),
            Style::default()
                .fg(app.theme.selected_fg())
                .add_modifier(Modifier::BOLD)
                .bg(bg),
        ),
    ];
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::{civil, collapse_indicator};

    #[test]
    fn civil_converts_known_utc_timestamps() {
        assert_eq!(civil(0), (1, 1, 0, 0, 0));
        assert_eq!(civil(951_782_400), (2, 29, 0, 0, 0));
    }

    #[test]
    fn collapse_indicator_matches_disclosure_conventions() {
        assert_eq!(collapse_indicator(true), "▸");
        assert_eq!(collapse_indicator(false), "▾");
    }
}
