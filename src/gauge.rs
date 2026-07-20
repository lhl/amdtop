//! nvitop/btop-style block gauges with a FIXED-width track so bars align.
//! Layout:  `LABEL ███████░░░░  62%   used / total`
//!          [label][----- track -----][pct][--- value field ---]
//! The track width is `width - label - pct - value_field`, so as long as
//! callers in the same band pass the same `width` and `value_field`, every
//! bar's track is identical length and the percentages line up in a column.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::theme::{Theme, UtilKind};

const SMOOTH_RAMP: [&str; 9] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];

/// A user-cycleable gauge fill style. `full` is the glyph for a complete cell,
/// `empty` for the unfilled track. `ramp` Some(..) renders precise fractional
/// sub-cells (the leading edge); None rounds to the nearest whole cell.
pub struct BlockStyle {
    pub name: &'static str,
    pub full: &'static str,
    pub empty: &'static str,
    pub ramp: Option<[&'static str; 9]>,
}

pub const BLOCK_STYLES: &[BlockStyle] = &[
    // index 0 is the default: ¾ blocks give a lighter, "broken up" look
    BlockStyle {
        name: "3/4",
        full: "▊",
        empty: "░",
        ramp: None,
    },
    BlockStyle {
        name: "smooth",
        full: "█",
        empty: "░",
        ramp: Some(SMOOTH_RAMP),
    },
    // braille / LED cell (⠀–⣿, ⣿ is the full 2×4 cell)
    BlockStyle {
        name: "dotmatrix",
        full: "⣿",
        empty: "⠀",
        ramp: None,
    },
    BlockStyle {
        name: "lines",
        full: "━",
        empty: "─",
        ramp: None,
    },
    BlockStyle {
        name: "squares",
        full: "■",
        empty: "□",
        ramp: None,
    },
    BlockStyle {
        name: "rects",
        full: "▮",
        empty: "▯",
        ramp: None,
    },
    BlockStyle {
        name: "pills",
        full: "▰",
        empty: "▱",
        ramp: None,
    },
];

pub fn block_style(i: usize) -> &'static BlockStyle {
    &BLOCK_STYLES[i % BLOCK_STYLES.len()]
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Cpu,
    Gpu,
    Mem,
    Npu,
}

impl Kind {
    fn util_kind(self) -> UtilKind {
        match self {
            Kind::Cpu => UtilKind::Cpu,
            Kind::Gpu => UtilKind::Gpu,
            Kind::Mem => UtilKind::Mem,
            Kind::Npu => UtilKind::Npu,
        }
    }
}

/// Inputs for a fixed-width gauge bar.
#[derive(Clone, Copy)]
pub struct Bar<'a> {
    pub label: &'a str,
    pub pct: Option<f64>,
    pub value: &'a str,
    pub width: usize,
    pub value_field: usize,
    pub kind: Kind,
}

impl<'a> Bar<'a> {
    pub fn new(label: &'a str, pct: Option<f64>, width: usize, kind: Kind) -> Self {
        Self {
            label,
            pct,
            value: "",
            width,
            value_field: 0,
            kind,
        }
    }

    pub fn with_value(mut self, value: &'a str, value_field: usize) -> Self {
        self.value = value;
        self.value_field = value_field;
        self
    }
}

/// Render a gauge with an optional right-aligned absolute value. An empty
/// `value` still reserves `value_field` columns so sibling bars remain aligned.
pub fn bar(input: Bar<'_>, theme: &Theme, style: &BlockStyle) -> Line<'static> {
    let Bar {
        label,
        pct,
        value,
        width,
        value_field,
        kind,
    } = input;
    let label_part = format!("{label} ");
    let pct_str = match pct {
        Some(p) => format!("{:>3.0}%", p.round().clamp(0.0, 100.0)),
        None => " N/A".to_string(),
    };
    // reserved = label + " " + pct(4) + (value_field + 2 separators)
    let reserved = label_part.chars().count()
        + 1
        + pct_str.chars().count()
        + if value_field > 0 { 2 + value_field } else { 0 };
    let track = width.saturating_sub(reserved);

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(5);
    spans.push(Span::styled(
        label_part,
        Style::default().fg(theme.graph_text()),
    ));

    let fill_color = pct.map_or(theme.inactive_fg(), |percent| {
        theme.util_color(percent, kind.util_kind())
    });

    match pct {
        Some(p) => {
            let clamped = (p / 100.0).clamp(0.0, 1.0);
            let n = (track as f64 * clamped * 8.0).round() as usize;
            let (mut q, r) = (n / 8, n % 8);
            let partial = match style.ramp {
                Some(ramp) if r > 0 => ramp[r].to_string(),
                Some(_) => String::new(),
                None => {
                    if r >= 4 {
                        q += 1; // round to nearest whole cell
                    }
                    String::new()
                }
            };
            let q = q.min(track);
            let filled = style.full.repeat(q);
            let used = q + partial.chars().count();
            let empty = style.empty.repeat(track.saturating_sub(used));
            spans.push(Span::styled(
                format!("{filled}{partial}"),
                Style::default().fg(fill_color),
            ));
            spans.push(Span::styled(
                empty,
                Style::default().fg(theme.inactive_fg()),
            ));
        }
        None => spans.push(Span::styled(
            style.empty.repeat(track),
            Style::default().fg(theme.inactive_fg()),
        )),
    }

    spans.push(Span::styled(
        format!(" {pct_str}"),
        Style::default().fg(fill_color),
    ));
    if value_field > 0 {
        spans.push(Span::styled(
            format!("  {value:>value_field$}"),
            Style::default().fg(theme.main_fg()),
        ));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::{BLOCK_STYLES, Bar, Kind, bar, block_style};
    use crate::theme::Theme;

    fn text(line: &ratatui::text::Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn block_style_index_wraps() {
        assert_eq!(block_style(0).name, "3/4");
        assert_eq!(block_style(BLOCK_STYLES.len()).name, "3/4");
    }

    #[test]
    fn gauge_reserves_the_requested_width() {
        let theme = Theme::load("__amdtop_test_missing_theme__");
        let line = bar(
            Bar::new("GPU", Some(50.0), 32, Kind::Gpu).with_value("1G / 2G", 8),
            &theme,
            block_style(1),
        );

        assert_eq!(line.width(), 32);
        assert!(text(&line).contains(" 50%"));
        assert!(text(&line).ends_with("   1G / 2G"));
    }

    #[test]
    fn gauge_clamps_percentages_and_displays_missing_values() {
        let theme = Theme::load("__amdtop_test_missing_theme__");
        let over = bar(
            Bar::new("GPU", Some(150.0), 16, Kind::Gpu),
            &theme,
            block_style(0),
        );
        let missing = bar(Bar::new("NPU", None, 16, Kind::Npu), &theme, block_style(0));

        assert!(text(&over).contains("100%"));
        assert!(text(&missing).contains("N/A"));
        assert_eq!(over.width(), 16);
        assert_eq!(missing.width(), 16);
    }
}
