//! Native amdtop theme loading, semantic color roles, and gradient sampling.
//!
//! Themes use amdtop's versioned TOML format and are loaded from native
//! amdtop directories. A bundled registry guarantees that the default theme
//! and the shipped theme collection are available without btop or loose data
//! files installed on the system.

mod builtin;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use ratatui::style::Color;
use serde::Deserialize;

pub const DEFAULT_THEME: &str = "tokyo-night";
const THEME_SCHEMA: u32 = 1;

#[derive(Clone, Copy, Debug)]
struct GradientStop {
    at: f64,
    color: Color,
}

#[derive(Clone, Debug)]
pub struct Gradient {
    stops: Vec<GradientStop>,
}

impl Gradient {
    pub fn three(start: Color, mid: Color, end: Color) -> Self {
        Self {
            stops: vec![
                GradientStop {
                    at: 0.0,
                    color: start,
                },
                GradientStop {
                    at: 0.5,
                    color: mid,
                },
                GradientStop {
                    at: 1.0,
                    color: end,
                },
            ],
        }
    }

    /// Sample the gradient at `t` in [0,1], interpolating between the two
    /// surrounding positioned stops.
    pub fn sample(&self, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        let first = self.stops[0];
        if t <= first.at || self.stops.len() == 1 {
            return first.color;
        }

        for pair in self.stops.windows(2) {
            let [left, right] = [pair[0], pair[1]];
            if t <= right.at {
                let local = (t - left.at) / (right.at - left.at);
                return lerp(left.color, right.color, local);
            }
        }

        self.stops.last().map_or(first.color, |stop| stop.color)
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

/// Parse a native color literal. Formats are `#RRGGBB`, `#BW` (two-digit
/// grayscale), and `R G B` decimal. Empty and `default` represent the terminal
/// default and therefore return `None`.
fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("default") {
        return None;
    }
    if let Some(hex) = value.strip_prefix('#') {
        return match hex.len() {
            6 => Some(Color::Rgb(
                u8::from_str_radix(&hex[0..2], 16).ok()?,
                u8::from_str_radix(&hex[2..4], 16).ok()?,
                u8::from_str_radix(&hex[4..6], 16).ok()?,
            )),
            2 => {
                let gray = u8::from_str_radix(hex, 16).ok()?;
                Some(Color::Rgb(gray, gray, gray))
            }
            _ => None,
        };
    }

    let mut fields = value.split_whitespace();
    let red = fields.next()?.parse().ok()?;
    let green = fields.next()?.parse().ok()?;
    let blue = fields.next()?.parse().ok()?;
    fields
        .next()
        .is_none()
        .then_some(Color::Rgb(red, green, blue))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeTheme {
    schema: u32,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    palette: HashMap<String, String>,
    #[serde(default)]
    ui: NativeUi,
    #[serde(default)]
    stats: NativeStats,
    #[serde(default)]
    borders: NativeBorders,
    #[serde(default)]
    gradients: NativeGradients,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NativeUi {
    background: Option<String>,
    foreground: Option<String>,
    title: Option<String>,
    highlight: Option<String>,
    selected_background: Option<String>,
    selected_foreground: Option<String>,
    inactive: Option<String>,
    graph_text: Option<String>,
    misc: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NativeStats {
    clock: Option<String>,
    power: Option<String>,
    fan: Option<String>,
    bandwidth: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NativeBorders {
    cpu: Option<String>,
    gpu: Option<String>,
    memory: Option<String>,
    npu: Option<String>,
    processes: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NativeGradients {
    temperature: Option<NativeGradient>,
    cpu: Option<NativeGradient>,
    gpu: Option<NativeGradient>,
    memory: Option<NativeGradient>,
    npu: Option<NativeGradient>,
    process: Option<NativeGradient>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeGradient {
    stops: Vec<NativeGradientStop>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeGradientStop {
    at: f64,
    color: String,
}

struct ColorResolver {
    palette: HashMap<String, Color>,
}

impl ColorResolver {
    fn new(raw: HashMap<String, String>) -> Result<Self, String> {
        let mut palette = HashMap::with_capacity(raw.len());
        for (name, value) in raw {
            let color = parse_color(&value)
                .ok_or_else(|| format!("palette.{name} has invalid color {value:?}"))?;
            palette.insert(name, color);
        }
        Ok(Self { palette })
    }

    fn resolve(&self, value: &str, field: &str) -> Result<Color, String> {
        let value = value.trim();
        if let Some(name) = value.strip_prefix('$') {
            return self
                .palette
                .get(name)
                .copied()
                .ok_or_else(|| format!("{field} references unknown palette color {name:?}"));
        }
        if value.eq_ignore_ascii_case("default") || value.is_empty() {
            return Ok(Color::Reset);
        }
        parse_color(value).ok_or_else(|| format!("{field} has invalid color {value:?}"))
    }

    fn role(&self, value: Option<&String>, fallback: Color, field: &str) -> Result<Color, String> {
        value.map_or(Ok(fallback), |value| self.resolve(value, field))
    }

    fn background(
        &self,
        value: Option<&String>,
        fallback: Option<Color>,
    ) -> Result<Option<Color>, String> {
        let Some(value) = value else {
            return Ok(fallback);
        };
        if value.trim().is_empty() || value.eq_ignore_ascii_case("default") {
            return Ok(None);
        }
        self.resolve(value, "ui.background").map(Some)
    }

    fn gradient(
        &self,
        value: Option<&NativeGradient>,
        fallback: &Gradient,
        field: &str,
    ) -> Result<Gradient, String> {
        let Some(value) = value else {
            return Ok(fallback.clone());
        };
        if value.stops.is_empty() {
            return Err(format!("{field}.stops must contain at least one stop"));
        }

        let mut stops = Vec::with_capacity(value.stops.len());
        let mut previous = None;
        for (index, stop) in value.stops.iter().enumerate() {
            if !(0.0..=1.0).contains(&stop.at) {
                return Err(format!("{field}.stops[{index}].at must be between 0 and 1"));
            }
            if previous.is_some_and(|previous| stop.at <= previous) {
                return Err(format!(
                    "{field}.stops positions must be strictly increasing"
                ));
            }
            stops.push(GradientStop {
                at: stop.at,
                color: self.resolve(&stop.color, &format!("{field}.stops[{index}].color"))?,
            });
            previous = Some(stop.at);
        }

        Ok(Gradient { stops })
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
    clock: Color,
    power: Color,
    fan: Color,
    bandwidth: Color,
    cpu_box: Color,
    gpu_box: Color,
    npu_box: Color,
    proc_box: Color,
    temp: Gradient,
    cpu: Gradient,
    gpu: Gradient,
    memory: Gradient,
    npu: Gradient,
    process: Gradient,
}

impl Theme {
    pub fn load(name: &str) -> Self {
        Self::load_from(name, &theme_dirs())
    }

    fn load_from(name: &str, directories: &[PathBuf]) -> Self {
        for directory in directories {
            let path = directory.join(format!("{name}.toml"));
            if let Ok(text) = fs::read_to_string(path)
                && let Ok(theme) = Self::parse_native(&text)
            {
                return theme;
            }
        }

        builtin::get(name)
            .and_then(|text| Self::parse_native(text).ok())
            .or_else(|| builtin::get(DEFAULT_THEME).and_then(|text| Self::parse_native(text).ok()))
            .unwrap_or_else(Self::default_tokyo_night)
    }

    pub fn list_available() -> Vec<String> {
        Self::list_available_from(&theme_dirs())
    }

    fn list_available_from(directories: &[PathBuf]) -> Vec<String> {
        let mut names: BTreeSet<String> = builtin::names().map(str::to_string).collect();
        for directory in directories {
            if let Ok(entries) = fs::read_dir(directory) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|extension| extension.to_str()) == Some("toml")
                        && let Some(name) = path.file_stem().and_then(|name| name.to_str())
                    {
                        names.insert(name.to_string());
                    }
                }
            }
        }
        names.into_iter().collect()
    }

    #[cfg(test)]
    fn builtin_names() -> Vec<&'static str> {
        builtin::names().collect()
    }

    fn parse_native(text: &str) -> Result<Self, String> {
        let native: NativeTheme = toml::from_str(text).map_err(|error| error.to_string())?;
        if native.schema != THEME_SCHEMA {
            return Err(format!(
                "unsupported theme schema {}; expected {THEME_SCHEMA}",
                native.schema
            ));
        }
        let _display_name = native.name;
        let resolver = ColorResolver::new(native.palette)?;
        let defaults = Self::default_tokyo_night();

        Ok(Self {
            main_bg: resolver.background(native.ui.background.as_ref(), defaults.main_bg)?,
            main_fg: resolver.role(
                native.ui.foreground.as_ref(),
                defaults.main_fg,
                "ui.foreground",
            )?,
            title: resolver.role(native.ui.title.as_ref(), defaults.title, "ui.title")?,
            hi_fg: resolver.role(native.ui.highlight.as_ref(), defaults.hi_fg, "ui.highlight")?,
            selected_bg: resolver.role(
                native.ui.selected_background.as_ref(),
                defaults.selected_bg,
                "ui.selected_background",
            )?,
            selected_fg: resolver.role(
                native.ui.selected_foreground.as_ref(),
                defaults.selected_fg,
                "ui.selected_foreground",
            )?,
            inactive_fg: resolver.role(
                native.ui.inactive.as_ref(),
                defaults.inactive_fg,
                "ui.inactive",
            )?,
            graph_text: resolver.role(
                native.ui.graph_text.as_ref(),
                defaults.graph_text,
                "ui.graph_text",
            )?,
            proc_misc: resolver.role(native.ui.misc.as_ref(), defaults.proc_misc, "ui.misc")?,
            clock: resolver.role(native.stats.clock.as_ref(), defaults.clock, "stats.clock")?,
            power: resolver.role(native.stats.power.as_ref(), defaults.power, "stats.power")?,
            fan: resolver.role(native.stats.fan.as_ref(), defaults.fan, "stats.fan")?,
            bandwidth: resolver.role(
                native.stats.bandwidth.as_ref(),
                defaults.bandwidth,
                "stats.bandwidth",
            )?,
            cpu_box: resolver.role(native.borders.cpu.as_ref(), defaults.cpu_box, "borders.cpu")?,
            gpu_box: resolver.role(
                native
                    .borders
                    .gpu
                    .as_ref()
                    .or(native.borders.memory.as_ref()),
                defaults.gpu_box,
                "borders.gpu",
            )?,
            npu_box: resolver.role(native.borders.npu.as_ref(), defaults.npu_box, "borders.npu")?,
            proc_box: resolver.role(
                native.borders.processes.as_ref(),
                defaults.proc_box,
                "borders.processes",
            )?,
            temp: resolver.gradient(
                native.gradients.temperature.as_ref(),
                &defaults.temp,
                "gradients.temperature",
            )?,
            cpu: resolver.gradient(
                native.gradients.cpu.as_ref(),
                &defaults.cpu,
                "gradients.cpu",
            )?,
            gpu: resolver.gradient(
                native.gradients.gpu.as_ref(),
                &defaults.gpu,
                "gradients.gpu",
            )?,
            memory: resolver.gradient(
                native.gradients.memory.as_ref(),
                &defaults.memory,
                "gradients.memory",
            )?,
            npu: resolver.gradient(
                native.gradients.npu.as_ref(),
                &defaults.npu,
                "gradients.npu",
            )?,
            process: resolver.gradient(
                native.gradients.process.as_ref(),
                &defaults.process,
                "gradients.process",
            )?,
        })
    }

    fn default_tokyo_night() -> Self {
        let green = Color::Rgb(0x9e, 0xce, 0x6a);
        let yellow = Color::Rgb(0xe0, 0xaf, 0x68);
        let red = Color::Rgb(0xf7, 0x76, 0x8e);
        let utilization = Gradient::three(green, yellow, red);
        Self {
            main_bg: Some(Color::Rgb(0x1a, 0x1b, 0x26)),
            main_fg: Color::Rgb(0xcf, 0xc9, 0xc2),
            title: Color::Rgb(0xcf, 0xc9, 0xc2),
            hi_fg: Color::Rgb(0x7d, 0xcf, 0xff),
            selected_bg: Color::Rgb(0x41, 0x48, 0x68),
            selected_fg: Color::Rgb(0xcf, 0xc9, 0xc2),
            inactive_fg: Color::Rgb(0x56, 0x5f, 0x89),
            graph_text: Color::Rgb(0xcf, 0xc9, 0xc2),
            proc_misc: Color::Rgb(0x7d, 0xcf, 0xff),
            clock: Color::Rgb(0x7d, 0xcf, 0xff),
            power: Color::Rgb(0xcf, 0xc9, 0xc2),
            fan: Color::Rgb(0xcf, 0xc9, 0xc2),
            bandwidth: Color::Rgb(0xcf, 0xc9, 0xc2),
            cpu_box: Color::Rgb(0x56, 0x5f, 0x89),
            gpu_box: Color::Rgb(0x56, 0x5f, 0x89),
            npu_box: Color::Rgb(0x56, 0x5f, 0x89),
            proc_box: Color::Rgb(0x56, 0x5f, 0x89),
            temp: utilization.clone(),
            cpu: utilization.clone(),
            gpu: utilization.clone(),
            memory: utilization.clone(),
            npu: utilization.clone(),
            process: utilization,
        }
    }

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
    pub fn clock(&self) -> Color {
        self.clock
    }
    pub fn power(&self) -> Color {
        self.power
    }
    pub fn fan(&self) -> Color {
        self.fan
    }
    pub fn bandwidth(&self) -> Color {
        self.bandwidth
    }
    pub fn box_color(&self, kind: SectionBox) -> Color {
        match kind {
            SectionBox::Cpu => self.cpu_box,
            SectionBox::Gpu => self.gpu_box,
            SectionBox::Npu => self.npu_box,
            SectionBox::Proc => self.proc_box,
        }
    }
    pub fn temp(&self) -> &Gradient {
        &self.temp
    }
    pub fn cpu(&self) -> &Gradient {
        &self.cpu
    }
    pub fn gpu(&self) -> &Gradient {
        &self.gpu
    }
    pub fn used(&self) -> &Gradient {
        &self.memory
    }
    pub fn npu(&self) -> &Gradient {
        &self.npu
    }
    pub fn process(&self) -> &Gradient {
        &self.process
    }

    pub fn util_color(&self, pct: f64, kind: UtilKind) -> Color {
        let gradient = match kind {
            UtilKind::Cpu => self.cpu(),
            UtilKind::Gpu => self.gpu(),
            UtilKind::Mem => self.used(),
            UtilKind::Npu => self.npu(),
        };
        gradient.sample((pct / 100.0).clamp(0.0, 1.0))
    }
}

#[derive(Clone, Copy)]
pub enum SectionBox {
    Cpu,
    Gpu,
    Npu,
    Proc,
}

#[derive(Clone, Copy)]
pub enum UtilKind {
    Cpu,
    Gpu,
    Mem,
    Npu,
}

fn theme_dirs() -> Vec<PathBuf> {
    let xdg_config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    theme_dirs_from(xdg_config.as_deref(), home.as_deref())
}

fn theme_dirs_from(xdg_config: Option<&Path>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut directories = Vec::with_capacity(4);
    if let Some(xdg_config) = xdg_config.filter(|path| path.is_absolute()) {
        directories.push(xdg_config.join("amdtop/themes"));
    }
    if let Some(home) = home {
        directories.push(home.join(".config/amdtop/themes"));
    }
    directories.push(PathBuf::from("/usr/local/share/amdtop/themes"));
    directories.push(PathBuf::from("/usr/share/amdtop/themes"));
    directories
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use ratatui::style::Color;

    use super::{DEFAULT_THEME, SectionBox, Theme, builtin, parse_color, theme_dirs_from, to_rgb};

    const NATIVE_THEME: &str = r##"
schema = 1
name = "test-native"

[palette]
bg = "#010203"
fg = "#aabbcc"
accent = "#112233"

[ui]
background = "$bg"
foreground = "$fg"
title = "$fg"
highlight = "$accent"

[stats]
clock = "$accent"
power = "#445566"
fan = "#778899"
bandwidth = "#abcdef"

[borders]
cpu = "#100000"
gpu = "#200000"
memory = "#300000"
npu = "#400000"
processes = "#500000"

[gradients.gpu]
stops = [
  { at = 0.0, color = "#000000" },
  { at = 0.25, color = "#646464" },
  { at = 1.0, color = "#c8c8c8" },
]
"##;

    static TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    fn temp_theme_dir() -> PathBuf {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("amdtop-theme-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn parse_color_supports_native_formats() {
        assert_eq!(parse_color("#12aBcF"), Some(Color::Rgb(0x12, 0xab, 0xcf)));
        assert_eq!(parse_color("#80"), Some(Color::Rgb(0x80, 0x80, 0x80)));
        assert_eq!(parse_color("1 2 3"), Some(Color::Rgb(1, 2, 3)));
        assert_eq!(parse_color("1 nope 2 3"), None);
        assert_eq!(parse_color("1 2 3 4"), None);
        assert_eq!(parse_color("not-a-color"), None);
    }

    #[test]
    fn native_theme_resolves_palette_and_semantic_roles() {
        let theme = Theme::parse_native(NATIVE_THEME).unwrap();

        assert_eq!(theme.main_bg(), Some(Color::Rgb(1, 2, 3)));
        assert_eq!(theme.main_fg(), Color::Rgb(0xaa, 0xbb, 0xcc));
        assert_eq!(theme.hi_fg(), Color::Rgb(0x11, 0x22, 0x33));
        assert_eq!(theme.clock(), Color::Rgb(0x11, 0x22, 0x33));
        assert_eq!(theme.power(), Color::Rgb(0x44, 0x55, 0x66));
        assert_eq!(theme.fan(), Color::Rgb(0x77, 0x88, 0x99));
        assert_eq!(theme.bandwidth(), Color::Rgb(0xab, 0xcd, 0xef));
        assert_eq!(theme.box_color(SectionBox::Gpu), Color::Rgb(0x20, 0, 0));
    }

    #[test]
    fn native_gradient_supports_positioned_stops() {
        let theme = Theme::parse_native(NATIVE_THEME).unwrap();

        assert_eq!(theme.gpu().sample(0.25), Color::Rgb(100, 100, 100));
        assert_eq!(theme.gpu().sample(0.625), Color::Rgb(150, 150, 150));
        assert_eq!(theme.gpu().sample(1.0), Color::Rgb(200, 200, 200));
    }

    #[test]
    fn native_theme_rejects_unknown_schema_and_palette_references() {
        assert!(Theme::parse_native("schema = 2").is_err());
        assert!(Theme::parse_native("schema = 1\n[ui]\nforeground = \"$missing\"\n").is_err());
    }

    #[test]
    fn builtin_themes_include_the_default_without_external_files() {
        let names = Theme::builtin_names();
        assert_eq!(DEFAULT_THEME, "tokyo-night");
        assert!(names.contains(&DEFAULT_THEME));

        let theme = Theme::load_from(DEFAULT_THEME, &[]);
        assert_eq!(theme.main_bg(), Some(Color::Rgb(0x1a, 0x1b, 0x26)));
        assert_eq!(theme.main_fg(), Color::Rgb(0xcf, 0xc9, 0xc2));
    }

    #[test]
    fn partial_native_themes_inherit_the_default_theme() {
        let theme = Theme::parse_native("schema = 1").unwrap();
        assert_eq!(theme.main_bg(), Some(Color::Rgb(0x1a, 0x1b, 0x26)));
        assert_eq!(theme.hi_fg(), Color::Rgb(0x7d, 0xcf, 0xff));
    }

    #[test]
    fn complete_builtin_collection_is_embedded_and_valid() {
        let names = Theme::builtin_names();
        assert_eq!(names.len(), 41);
        for expected in [
            "dracula",
            "everforest-dark-hard",
            "gruvbox_dark",
            "nord",
            "onedark",
            "solarized_dark",
            "tokyo-night",
        ] {
            assert!(
                names.contains(&expected),
                "missing builtin theme {expected}"
            );
        }
        for (name, text) in builtin::BUILTIN_THEMES {
            Theme::parse_native(text)
                .unwrap_or_else(|error| panic!("builtin theme {name} is invalid: {error}"));
        }
    }

    #[test]
    fn native_user_theme_overrides_a_builtin_with_the_same_name() {
        let directory = temp_theme_dir();
        fs::write(
            directory.join("onedark.toml"),
            "schema = 1\n[ui]\nforeground = \"#010203\"\n",
        )
        .unwrap();

        let theme = Theme::load_from("onedark", std::slice::from_ref(&directory));
        assert_eq!(theme.main_fg(), Color::Rgb(1, 2, 3));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_theme_falls_back_to_embedded_default() {
        let theme = Theme::load_from("__amdtop_missing_theme__", &[]);
        assert_eq!(theme.main_fg(), Color::Rgb(0xcf, 0xc9, 0xc2));
    }

    #[test]
    fn available_themes_merge_native_files_and_builtins() {
        let directory = temp_theme_dir();
        fs::write(directory.join("custom.toml"), "schema = 1\n").unwrap();
        fs::write(directory.join("ignored.theme"), "not native").unwrap();

        let themes = Theme::list_available_from(std::slice::from_ref(&directory));
        assert!(themes.iter().any(|name| name == "custom"));
        assert!(themes.iter().any(|name| name == "onedark"));
        assert!(!themes.iter().any(|name| name == "ignored"));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn indexed_colors_follow_the_xterm_palette() {
        assert_eq!(to_rgb(Color::Indexed(16)), (0, 0, 0));
        assert_eq!(to_rgb(Color::Indexed(21)), (0, 0, 255));
        assert_eq!(to_rgb(Color::Indexed(231)), (255, 255, 255));
        assert_eq!(to_rgb(Color::Indexed(232)), (8, 8, 8));
    }

    #[test]
    fn native_theme_search_paths_follow_xdg_conventions() {
        let paths = theme_dirs_from(Some(Path::new("/tmp/xdg")), Some(Path::new("/home/test")));
        assert_eq!(paths[0], Path::new("/tmp/xdg/amdtop/themes"));
        assert_eq!(paths[1], Path::new("/home/test/.config/amdtop/themes"));
        assert_eq!(paths[2], Path::new("/usr/local/share/amdtop/themes"));
        assert_eq!(paths[3], Path::new("/usr/share/amdtop/themes"));

        let relative = theme_dirs_from(Some(Path::new("relative")), Some(Path::new("/home/test")));
        assert_eq!(relative[0], Path::new("/home/test/.config/amdtop/themes"));
    }
}
