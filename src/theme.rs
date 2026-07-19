//! btop theme loader. Parses standard `.theme` files (hex `#RRGGBB`,
//! 2-char grayscale `#BW`, or `R G B` decimal) and exposes resolved colors
//! + gradient samplers. Defaults to `everforest-dark-hard`.
//!
//! Search paths (first hit wins):
//!   `$XDG_CONFIG_HOME/btop/themes/` (`~/.config/btop/themes`)
//!   /usr/local/share/btop/themes/
//!   /usr/share/btop/themes/

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use ratatui::style::Color;

pub const DEFAULT_THEME: &str = "onedark";

#[derive(Clone, Copy)]
pub struct Gradient {
    pub start: Color,
    pub mid: Option<Color>,
    pub end: Option<Color>,
}

impl Gradient {
    /// Sample the gradient at `t` in [0,1].
    /// - start only -> flat
    /// - start+end -> linear lerp
    /// - start+mid+end -> start->mid (t<0.5), mid->end (t>=0.5)
    pub fn sample(self, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        match (self.mid, self.end) {
            (None, None) | (Some(_), None) => self.start,
            (None, Some(e)) => lerp(self.start, e, t),
            (Some(m), Some(e)) => {
                if t < 0.5 {
                    lerp(self.start, m, t * 2.0)
                } else {
                    lerp(m, e, (t - 0.5) * 2.0)
                }
            }
        }
    }
}

fn lerp(a: Color, b: Color, t: f64) -> Color {
    let (ar, ag, ab) = to_rgb(a);
    let (br, bg, bb) = to_rgb(b);
    Color::Rgb(
        (ar as f64 + (br as f64 - ar as f64) * t).round() as u8,
        (ag as f64 + (bg as f64 - ag as f64) * t).round() as u8,
        (ab as f64 + (bb as f64 - ab as f64) * t).round() as u8,
    )
}

fn to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Reset | Color::Black => (0, 0, 0),
        Color::Red => (128, 0, 0),
        Color::Green => (0, 128, 0),
        Color::Yellow => (128, 128, 0),
        Color::Blue => (0, 0, 128),
        Color::Magenta => (128, 0, 128),
        Color::Cyan => (0, 128, 128),
        Color::Gray => (192, 192, 192),
        Color::DarkGray => (128, 128, 128),
        Color::LightRed => (255, 0, 0),
        Color::LightGreen => (0, 255, 0),
        Color::LightYellow => (255, 255, 0),
        Color::LightBlue => (0, 0, 255),
        Color::LightMagenta => (255, 0, 255),
        Color::LightCyan => (0, 255, 255),
        Color::White => (255, 255, 255),
        Color::Rgb(red, green, blue) => (red, green, blue),
        Color::Indexed(index @ 0..=15) => ANSI_COLORS[index as usize],
        Color::Indexed(index @ 16..=231) => {
            let index = index - 16;
            let component = |value| if value == 0 { 0 } else { 55 + value * 40 };
            (
                component(index / 36),
                component((index / 6) % 6),
                component(index % 6),
            )
        }
        Color::Indexed(index) => {
            let value = 8 + (index - 232) * 10;
            (value, value, value)
        }
    }
}

const ANSI_COLORS: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (128, 0, 0),
    (0, 128, 0),
    (128, 128, 0),
    (0, 0, 128),
    (128, 0, 128),
    (0, 128, 128),
    (192, 192, 192),
    (128, 128, 128),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (0, 0, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

/// Parse a btop color value. Formats: `#RRGGBB`, `#BW` (2-hex grayscale),
/// `R G B` (decimal). Empty string -> None (terminal default / transparent).
fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(h) = s.strip_prefix('#') {
        if h.len() == 6 {
            let r = u8::from_str_radix(&h[0..2], 16).ok()?;
            let g = u8::from_str_radix(&h[2..4], 16).ok()?;
            let b = u8::from_str_radix(&h[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
        if h.len() == 2 {
            // 2-hex grayscale: "#ff" -> 255
            let v = u8::from_str_radix(h, 16).ok()?;
            return Some(Color::Rgb(v, v, v));
        }
        return None;
    }
    // decimal "R G B"
    let mut fields = s.split_whitespace();
    let red = fields.next()?.parse().ok()?;
    let green = fields.next()?.parse().ok()?;
    let blue = fields.next()?.parse().ok()?;
    fields
        .next()
        .is_none()
        .then_some(Color::Rgb(red, green, blue))
}

fn color(values: &HashMap<String, String>, key: &str) -> Option<Color> {
    values.get(key).and_then(|value| parse_color(value))
}

fn color_or(values: &HashMap<String, String>, key: &str, default: Color) -> Color {
    color(values, key).unwrap_or(default)
}

fn gradient(values: &HashMap<String, String>, name: &str) -> Gradient {
    Gradient {
        start: color(values, &format!("{name}_start")).unwrap_or(Color::Rgb(0xa7, 0xc0, 0x80)),
        mid: color(values, &format!("{name}_mid")),
        end: color(values, &format!("{name}_end")),
    }
}

pub struct Theme {
    main_bg: Option<Color>,
    main_fg: Color,
    title: Color,
    hi_fg: Color,
    selected_bg: Color,
    selected_fg: Color,
    inactive_fg: Color,
    graph_text: Color,
    proc_misc: Color,
    cpu_box: Color,
    mem_box: Color,
    npu_box: Color,
    proc_box: Color,
    temp: Gradient,
    cpu: Gradient,
    used: Gradient,
    process: Gradient,
}

impl Theme {
    /// Load a named btop theme from the standard search paths.
    /// Falls back to the built-in everforest-dark-hard palette if not found.
    pub fn load(name: &str) -> Self {
        for directory in search_dirs() {
            let path = directory.join(format!("{name}.theme"));
            if let Ok(text) = fs::read_to_string(path) {
                return Self::parse(&text);
            }
        }
        Self::parse(EVERFOREST_FALLBACK)
    }

    /// List all available theme names found in the search paths (sorted, unique).
    pub fn list_available() -> Vec<String> {
        let mut set = std::collections::BTreeSet::new();
        for dir in search_dirs() {
            if let Ok(rd) = fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|x| x.to_str()) == Some("theme")
                        && let Some(stem) = p.file_stem().and_then(|x| x.to_str())
                    {
                        set.insert(stem.to_string());
                    }
                }
            }
        }
        set.insert(DEFAULT_THEME.to_string());
        set.into_iter().collect()
    }

    fn parse(text: &str) -> Self {
        let mut raw = HashMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // theme[key]="value"
            if let Some(rest) = line.strip_prefix("theme[")
                && let Some(close) = rest.find(']')
            {
                let key = rest[..close].to_string();
                let val_part = rest[close + 1..].trim_start();
                let val = val_part.strip_prefix('=').unwrap_or(val_part).trim();
                // strip quotes
                let val = val.trim_matches('"');
                raw.insert(key, val.to_string());
            }
        }
        let main_fg = color_or(&raw, "main_fg", Color::Rgb(0xd3, 0xc6, 0xaa));
        let div_line = color_or(&raw, "div_line", Color::Rgb(0x37, 0x41, 0x45));
        Self {
            main_bg: color(&raw, "main_bg"),
            main_fg,
            title: color_or(&raw, "title", main_fg),
            hi_fg: color_or(&raw, "hi_fg", Color::Rgb(0xe6, 0x7e, 0x80)),
            selected_bg: color_or(&raw, "selected_bg", Color::Rgb(0x37, 0x41, 0x45)),
            selected_fg: color_or(&raw, "selected_fg", Color::Rgb(0xdb, 0xbc, 0x7f)),
            inactive_fg: color_or(&raw, "inactive_fg", Color::Rgb(0x50, 0x49, 0x45)),
            graph_text: color_or(&raw, "graph_text", main_fg),
            proc_misc: color_or(&raw, "proc_misc", Color::Rgb(0xa7, 0xc0, 0x80)),
            cpu_box: color_or(&raw, "cpu_box", div_line),
            mem_box: color_or(&raw, "mem_box", div_line),
            npu_box: color_or(&raw, "net_box", div_line),
            proc_box: color_or(&raw, "proc_box", div_line),
            temp: gradient(&raw, "temp"),
            cpu: gradient(&raw, "cpu"),
            used: gradient(&raw, "used"),
            process: gradient(&raw, "process"),
        }
    }

    /// Main background. None means to use the terminal default.
    pub fn main_bg(&self) -> Option<Color> {
        self.main_bg
    }
    pub fn main_fg(&self) -> Color {
        self.main_fg
    }
    pub fn title(&self) -> Color {
        self.title
    }
    pub fn hi_fg(&self) -> Color {
        self.hi_fg
    }
    pub fn selected_bg(&self) -> Color {
        self.selected_bg
    }
    pub fn selected_fg(&self) -> Color {
        self.selected_fg
    }
    pub fn inactive_fg(&self) -> Color {
        self.inactive_fg
    }
    pub fn graph_text(&self) -> Color {
        self.graph_text
    }
    pub fn proc_misc(&self) -> Color {
        self.proc_misc
    }
    pub fn box_color(&self, kind: SectionBox) -> Color {
        match kind {
            SectionBox::Cpu => self.cpu_box,
            SectionBox::Mem => self.mem_box,
            SectionBox::Npu => self.npu_box,
            SectionBox::Proc => self.proc_box,
        }
    }

    pub fn temp(&self) -> Gradient {
        self.temp
    }
    pub fn cpu(&self) -> Gradient {
        self.cpu
    }
    pub fn used(&self) -> Gradient {
        self.used
    }
    pub fn process(&self) -> Gradient {
        self.process
    }

    /// Sample the appropriate theme gradient for a utilization percentage.
    pub fn util_color(&self, pct: f64, kind: UtilKind) -> Color {
        // map nvitop thresholds onto gradient positions:
        // light (<10%) -> start, moderate (10-75/80%) -> mid-ish, heavy -> end
        let g = match kind {
            UtilKind::Gpu => self.cpu(),
            UtilKind::Mem => self.used(),
            UtilKind::Npu => self.process(),
        };
        let t = (pct / 100.0).clamp(0.0, 1.0);
        g.sample(t)
    }
}

#[derive(Clone, Copy)]
pub enum SectionBox {
    Cpu,
    Mem,
    Npu,
    Proc,
}

#[derive(Clone, Copy)]
pub enum UtilKind {
    Gpu,
    Mem,
    Npu,
}

fn search_dirs() -> Vec<PathBuf> {
    let xdg_config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    search_dirs_from(xdg_config.as_deref(), home.as_deref())
}

fn search_dirs_from(
    xdg_config: Option<&std::path::Path>,
    home: Option<&std::path::Path>,
) -> Vec<PathBuf> {
    let mut directories = Vec::with_capacity(4);
    if let Some(xdg_config) = xdg_config.filter(|path| path.is_absolute()) {
        directories.push(xdg_config.join("btop/themes"));
    }
    if let Some(home) = home {
        directories.push(home.join(".config/btop/themes"));
    }
    directories.push(PathBuf::from("/usr/local/share/btop/themes"));
    directories.push(PathBuf::from("/usr/share/btop/themes"));
    directories
}

/// Bundled fallback in case no theme files are installed on the system.
/// Minimal everforest-dark-hard palette.
const EVERFOREST_FALLBACK: &str = r##"
theme[main_bg]="#272e33"
theme[main_fg]="#d3c6aa"
theme[title]="#d3c6aa"
theme[hi_fg]="#e67e80"
theme[selected_bg]="#374145"
theme[selected_fg]="#dbbc7f"
theme[inactive_fg]="#272e33"
theme[graph_text]="#d3c6aa"
theme[proc_misc]="#a7c080"
theme[cpu_box]="#374145"
theme[mem_box]="#374145"
theme[net_box]="#374145"
theme[proc_box]="#374145"
theme[div_line]="#374145"
theme[temp_start]="#a7c080"
theme[temp_mid]="#dbbc7f"
theme[temp_end]="#f85552"
theme[cpu_start]="#a7c080"
theme[cpu_mid]="#dbbc7f"
theme[cpu_end]="#f85552"
theme[used_start]="#a7c080"
theme[used_mid]="#dbbc7f"
theme[used_end]="#f85552"
theme[free_start]="#f85552"
theme[free_mid]="#dbbc7f"
theme[free_end]="#a7c080"
theme[process_start]="#a7c080"
theme[process_mid]="#f85552"
theme[process_end]="#CC241D"
"##;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ratatui::style::Color;

    use super::{Gradient, Theme, parse_color, search_dirs_from, to_rgb};

    #[test]
    fn parse_color_supports_btop_formats() {
        assert_eq!(parse_color("#12aBcF"), Some(Color::Rgb(0x12, 0xab, 0xcf)));
        assert_eq!(parse_color("#80"), Some(Color::Rgb(0x80, 0x80, 0x80)));
        assert_eq!(parse_color("1 2 3"), Some(Color::Rgb(1, 2, 3)));
        assert_eq!(parse_color("1 nope 2 3"), None);
        assert_eq!(parse_color("1 2 3 4"), None);
        assert_eq!(parse_color("not-a-color"), None);
    }

    #[test]
    fn theme_parser_resolves_named_colors() {
        let theme = Theme::parse("theme[main_fg]=\"#010203\"");
        assert_eq!(theme.main_fg(), Color::Rgb(1, 2, 3));
    }

    #[test]
    fn gradient_interpolates_through_midpoint() {
        let gradient = Gradient {
            start: Color::Rgb(0, 0, 0),
            mid: Some(Color::Rgb(100, 100, 100)),
            end: Some(Color::Rgb(200, 200, 200)),
        };

        assert_eq!(gradient.sample(0.5), Color::Rgb(100, 100, 100));
        assert_eq!(gradient.sample(1.0), Color::Rgb(200, 200, 200));
    }

    #[test]
    fn indexed_colors_follow_the_xterm_palette() {
        assert_eq!(to_rgb(Color::Indexed(16)), (0, 0, 0));
        assert_eq!(to_rgb(Color::Indexed(21)), (0, 0, 255));
        assert_eq!(to_rgb(Color::Indexed(231)), (255, 255, 255));
        assert_eq!(to_rgb(Color::Indexed(232)), (8, 8, 8));
    }

    #[test]
    fn theme_search_paths_follow_xdg_conventions() {
        let paths = search_dirs_from(Some(Path::new("/tmp/xdg")), Some(Path::new("/home/test")));
        assert_eq!(paths[0], Path::new("/tmp/xdg/btop/themes"));
        assert_eq!(paths[1], Path::new("/home/test/.config/btop/themes"));

        let relative = search_dirs_from(Some(Path::new("relative")), Some(Path::new("/home/test")));
        assert_eq!(relative[0], Path::new("/home/test/.config/btop/themes"));
    }
}
