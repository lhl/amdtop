use std::cmp::Ordering;

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Row, Table};

use crate::app::{App, PROCESS_COLUMNS, Section};
use crate::theme::SectionBox;

use super::section_block;

pub(super) fn draw(f: &mut Frame, area: Rect, app: &App) {
    let block = section_block(app, Section::Processes, "PROCESSES", SectionBox::Proc);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.is_collapsed(Section::Processes) {
        let n: usize = app
            .active_apps()
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

    let mut rows = collect_process_rows(app);
    sort_process_rows(&mut rows, app.process_sort_key, app.process_sort_desc);

    let header = Row::new(header_cells(app.process_sort_key, app.process_sort_desc)).style(
        Style::default()
            .fg(app.theme.proc_misc())
            .add_modifier(Modifier::BOLD),
    );
    let table_rows: Vec<Row> = rows
        .iter()
        .map(|row| {
            let gpu_activity = row.gfx.max(row.compute).max(row.dma).clamp(0, 100) as f64;
            Row::new(vec![
                row.gpu.to_string(),
                row.pid.to_string(),
                row.name.chars().take(24).collect::<String>(),
                fmt_kib(row.rss_kb),
                fmt_kib(Some(row.vram_kib)),
                fmt_kib(Some(row.gtt_kib)),
                row.gfx.to_string(),
                row.compute.to_string(),
                row.dma.to_string(),
                row.cpu.to_string(),
            ])
            .style(Style::default().fg(app.theme.process().sample(gpu_activity / 100.0)))
        })
        .collect();
    let table = Table::new(
        table_rows,
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

/// One flattened process-table row. Collecting into plain structs lets the
/// table sort across GPUs without touching the live backend state.
struct ProcessRow {
    gpu: usize,
    pid: i32,
    name: String,
    rss_kb: Option<u64>,
    vram_kib: u64,
    gtt_kib: u64,
    gfx: i64,
    compute: i64,
    dma: i64,
    cpu: i64,
}

fn collect_process_rows(app: &App) -> Vec<ProcessRow> {
    app.gpus
        .iter()
        .enumerate()
        .filter_map(|(gi, gpu)| gpu.app.as_ref().map(|gpu_app| (gi, gpu_app)))
        .flat_map(|(gi, a)| {
            a.stat.fdinfo.proc_usage.iter().map(move |pu| ProcessRow {
                gpu: gi,
                pid: pu.pid,
                name: pu.name.clone(),
                rss_kb: app.process_rss_kb(pu.pid),
                vram_kib: pu.usage.vram_usage,
                gtt_kib: pu.usage.gtt_usage,
                gfx: pu.usage.gfx,
                compute: pu.usage.compute,
                dma: pu.usage.dma,
                cpu: pu.usage.cpu,
            })
        })
        .collect()
}

/// Sort rows by sort key 1..=9. Key 0 (native GPU order) and unknown keys
/// keep the existing order. Ties always fall back to ascending PID, so the
/// table does not jitter every tick when equal values reshuffle.
fn sort_process_rows(rows: &mut [ProcessRow], key: usize, desc: bool) {
    if key == 0 {
        return;
    }
    rows.sort_by(|a, b| {
        let primary = match (sort_value(a, key), sort_value(b, key)) {
            (Some(av), Some(bv)) => av.cmp(&bv),
            _ => Ordering::Equal,
        };
        let primary = if desc { primary.reverse() } else { primary };
        primary.then(a.pid.cmp(&b.pid))
    });
}

/// The column value selected by `key`, or `None` for key 0.
fn sort_value(row: &ProcessRow, key: usize) -> Option<SortValue> {
    match key {
        1 => Some(SortValue::Num(row.pid as i128)),
        2 => Some(SortValue::Text(row.name.clone())),
        3 => Some(SortValue::Num(row.rss_kb.unwrap_or(0) as i128)),
        4 => Some(SortValue::Num(row.vram_kib as i128)),
        5 => Some(SortValue::Num(row.gtt_kib as i128)),
        6 => Some(SortValue::Num(row.gfx as i128)),
        7 => Some(SortValue::Num(row.compute as i128)),
        8 => Some(SortValue::Num(row.dma as i128)),
        9 => Some(SortValue::Num(row.cpu as i128)),
        _ => None,
    }
}

/// The header labels, with a direction arrow appended to the active sort
/// column. Key 0 (native order) marks no column.
fn header_cells(sort_key: usize, desc: bool) -> Vec<String> {
    PROCESS_COLUMNS
        .iter()
        .enumerate()
        .map(|(col, label)| {
            if col == sort_key && sort_key >= 1 {
                format!("{label}{}", if desc { "▼" } else { "▲" })
            } else {
                (*label).to_string()
            }
        })
        .collect()
}

/// A comparable sort value. Each column is homogeneous (all numeric or all
/// text), but `Ord` must be total.
#[derive(PartialEq, Eq)]
enum SortValue {
    Num(i128),
    Text(String),
}

impl PartialOrd for SortValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SortValue {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Num(a), Self::Num(b)) => a.cmp(b),
            (Self::Text(a), Self::Text(b)) => a.cmp(b),
            (Self::Num(_), Self::Text(_)) => Ordering::Less,
            (Self::Text(_), Self::Num(_)) => Ordering::Greater,
        }
    }
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
    use super::{ProcessRow, fmt_kib, header_cells, sort_process_rows};

    fn test_row(pid: i32, vram_kib: u64, name: &str, rss_kb: Option<u64>) -> ProcessRow {
        ProcessRow {
            gpu: 0,
            pid,
            name: name.to_string(),
            rss_kb,
            vram_kib,
            gtt_kib: 0,
            gfx: 0,
            compute: 0,
            dma: 0,
            cpu: 0,
        }
    }

    fn pids(rows: &[ProcessRow]) -> Vec<i32> {
        rows.iter().map(|row| row.pid).collect()
    }

    #[test]
    fn memory_columns_use_compact_binary_units() {
        assert_eq!(fmt_kib(Some(512 << 10)), "512M");
        assert_eq!(fmt_kib(Some(3 << 20)), "3.0G");
        assert_eq!(fmt_kib(None), "-");
    }

    #[test]
    fn vram_descending_orders_highest_first_with_pid_tiebreak() {
        let mut rows = vec![
            test_row(300, 512, "c", None),
            test_row(100, 1024, "a", None),
            test_row(200, 512, "b", None),
        ];

        sort_process_rows(&mut rows, 4, true);

        assert_eq!(pids(&rows), vec![100, 200, 300]);
    }

    #[test]
    fn name_sorts_lexicographically_in_both_directions() {
        let mut rows = vec![
            test_row(5, 0, "bravo", None),
            test_row(20, 0, "alpha", None),
            test_row(30, 0, "charlie", None),
        ];

        sort_process_rows(&mut rows, 2, false);
        assert_eq!(pids(&rows), vec![20, 5, 30]);

        sort_process_rows(&mut rows, 2, true);
        assert_eq!(pids(&rows), vec![30, 5, 20]);
    }

    #[test]
    fn native_key_keeps_existing_order() {
        let mut rows = vec![
            test_row(3, 300, "c", None),
            test_row(1, 100, "a", None),
            test_row(2, 200, "b", None),
        ];

        sort_process_rows(&mut rows, 0, true);
        assert_eq!(pids(&rows), vec![3, 1, 2]);

        sort_process_rows(&mut rows, 0, false);
        assert_eq!(pids(&rows), vec![3, 1, 2]);
    }

    #[test]
    fn missing_rss_sorts_as_zero() {
        let mut rows = vec![
            test_row(1, 0, "a", None),
            test_row(2, 0, "b", Some(512)),
            test_row(3, 0, "c", Some(0)),
        ];

        sort_process_rows(&mut rows, 3, true);
        assert_eq!(pids(&rows), vec![2, 1, 3]);

        sort_process_rows(&mut rows, 3, false);
        assert_eq!(pids(&rows), vec![1, 3, 2]);
    }

    #[test]
    fn header_marks_only_the_active_sort_column() {
        let cells = header_cells(4, true);
        assert_eq!(cells[4], "VRAM▼");
        assert_eq!(cells[0], "GPU");
        assert_eq!(cells[1], "PID");

        let native = header_cells(0, false);
        assert_eq!(
            native,
            vec![
                "GPU", "PID", "NAME", "MEM", "VRAM", "GTT", "GFX%", "COMP%", "DMA%", "CPU%"
            ]
        );
    }
}
