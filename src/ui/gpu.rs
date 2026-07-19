use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Section, gpu_mem_info};
use crate::gauge::{self, Kind};
use crate::theme::{SectionBox, UtilKind};

use super::{render_bar, section_block};

const GPU_VALUE_WIDTH: usize = 15;

pub(super) fn draw(f: &mut Frame, area: Rect, app: &App) {
    let block = section_block(app, Section::Gpu, "GPU", SectionBox::Mem);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.is_collapsed(Section::Gpu) {
        let mut spans: Vec<Span> = Vec::new();
        for (i, a) in app.apps.iter().enumerate() {
            let gfx = a.stat.activity.gfx.unwrap_or(0);
            let memory = gpu_mem_info(a);
            spans.push(Span::styled(
                format!(
                    " GPU{}  GPU {:>3}%  MEM {:>3}%  ",
                    i,
                    gfx,
                    memory.percent.round() as i64
                ),
                Style::default().fg(app.theme.util_color(f64::from(gfx), UtilKind::Gpu)),
            ));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), inner);
        return;
    }

    // each GPU = 6-row band
    let bands = Layout::default()
        .direction(Direction::Vertical)
        .constraints((0..app.apps.len()).map(|_| Constraint::Length(6)))
        .split(inner);

    for (i, a) in app.apps.iter().enumerate() {
        let band = bands[i];
        // left: identity+stats (38), right: gauges+history
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(38), Constraint::Min(20)])
            .split(band);

        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1); 4])
            .split(cols[0]);

        // line 0: GPU index + name
        let name = &a.device_path.device_name;
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" GPU{i} "),
                    Style::default()
                        .fg(app.theme.hi_fg())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(short_name(name), Style::default().fg(app.theme.title())),
            ])),
            left[0],
        );
        // line 1: bus-id + type
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", bus_id(a)),
                    Style::default().fg(app.theme.graph_text()),
                ),
                Span::styled(
                    if a.device_info.is_apu { "APU" } else { "dGPU" },
                    Style::default().fg(app.theme.proc_misc()),
                ),
            ])),
            left[1],
        );
        // line 2: temp + power
        let (temp_s, pwr_s) = gpu_temp_power(a);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {temp_s} "),
                    Style::default().fg(app.theme.temp().sample(0.5)),
                ),
                Span::styled(
                    format!(" {pwr_s} "),
                    Style::default().fg(app.theme.graph_text()),
                ),
            ])),
            left[2],
        );
        // line 3: clocks + fan
        let (sclk_s, mclk_s, fan_s) = gpu_clocks_fan(a);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {sclk_s} "),
                    Style::default().fg(app.theme.proc_misc()),
                ),
                Span::styled(
                    format!(" {mclk_s} "),
                    Style::default().fg(app.theme.proc_misc()),
                ),
                Span::styled(fan_s, Style::default().fg(app.theme.graph_text())),
            ])),
            left[3],
        );

        // right: GPU gauge, MEM gauge, then two side-by-side history graphs
        // (util | mem), double-height, each with a label below.
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // GPU bar
                Constraint::Length(1), // MEM bar
                Constraint::Length(3), // graphs (side by side)
                Constraint::Length(1), // labels
            ])
            .split(cols[1]);

        let gfx = f64::from(a.stat.activity.gfx.unwrap_or(0));
        let memory = gpu_mem_info(a);
        let rw = cols[1].width as usize;

        f.render_widget(
            Paragraph::new(render_bar(
                app,
                gauge::Bar::new("GPU", Some(gfx), rw, Kind::Gpu).with_value("", GPU_VALUE_WIDTH),
            )),
            right[0],
        );
        let mem_val = format!(
            "{} / {}",
            fmt_bytes(memory.used_bytes),
            fmt_bytes(memory.total_bytes)
        );
        f.render_widget(
            Paragraph::new(render_bar(
                app,
                gauge::Bar::new("MEM", Some(memory.percent), rw, Kind::Mem)
                    .with_value(&mem_val, GPU_VALUE_WIDTH),
            )),
            right[1],
        );

        // graphs side by side
        let gcols = Layout::default()
            .direction(Direction::Horizontal)
            .spacing(2)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(right[2]);
        render_graph(
            f,
            gcols[0],
            app.hist_gpu[i].braille_graph(gcols[0].width as usize, 3, app.theme.cpu()),
        );
        render_graph(
            f,
            gcols[1],
            app.hist_mem[i].braille_graph(gcols[1].width as usize, 3, app.theme.used()),
        );

        // labels below each graph
        let lcols = Layout::default()
            .direction(Direction::Horizontal)
            .spacing(2)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(right[3]);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "GPU util",
                Style::default().fg(app.theme.cpu().sample(0.6)),
            ))),
            lcols[0],
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "MEM",
                Style::default().fg(app.theme.used().sample(0.6)),
            ))),
            lcols[1],
        );
    }
}

fn gpu_temp_power(a: &libamdgpu_top::app::AppAmdgpuTop) -> (String, String) {
    let st = &a.stat;
    let s = st.sensors.as_ref();
    let temp = s.and_then(|x| x.junction_temp.as_ref().or(x.edge_temp.as_ref()));
    let temp_s = temp.map_or("  -".into(), |t| format!("{:>3}°C", t.current));
    let pwr_s = s.map_or_else(
        || "  -".into(),
        |sensors| {
            power_text(
                sensors.average_power.as_ref().map(|power| power.value),
                sensors
                    .power_cap
                    .as_ref()
                    .and_then(|cap| effective_power_cap(cap.current, cap.default)),
            )
        },
    );
    (temp_s, pwr_s)
}

fn effective_power_cap(current_watts: u32, default_watts: u32) -> Option<u32> {
    [current_watts, default_watts]
        .into_iter()
        .find(|watts| *watts > 0)
}

fn power_text(average_watts: Option<u32>, cap_watts: Option<u32>) -> String {
    match (average_watts, cap_watts) {
        (Some(average), Some(cap)) => format!("{average}/{cap}W"),
        (Some(average), None) => format!("{average}W"),
        _ => "  -".into(),
    }
}

fn gpu_clocks_fan(a: &libamdgpu_top::app::AppAmdgpuTop) -> (String, String, String) {
    let Some(s) = a.stat.sensors.as_ref() else {
        return ("sclk -".into(), "mclk -".into(), "fan -".into());
    };
    let sclk = s
        .sclk
        .map_or("sclk -".into(), |value| format!("sclk {value}M"));
    let mclk = s
        .mclk
        .map_or("mclk -".into(), |value| format!("mclk {value}M"));
    let fan = s
        .fan_rpm
        .map_or("fan -".into(), |value| format!("fan {value}r"));
    (sclk, mclk, fan)
}

fn bus_id(app: &libamdgpu_top::app::AppAmdgpuTop) -> String {
    app.device_path.pci.to_string()
}

fn render_graph(f: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    for (line, row) in lines.into_iter().zip(area.rows()) {
        f.render_widget(Paragraph::new(line), row);
    }
}

fn fmt_bytes(bytes: u64) -> String {
    const GIB: u64 = 1 << 30;
    const MIB: u64 = 1 << 20;
    if bytes >= GIB {
        format!("{:.1}G", bytes as f64 / GIB as f64)
    } else {
        format!("{}M", bytes / MIB)
    }
}

fn short_name(s: &str) -> String {
    s.replace("AMD Radeon Graphics", "Radeon")
        .replace("AMD Radeon", "Radeon")
        .chars()
        .take(26)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{effective_power_cap, fmt_bytes, power_text, short_name};

    #[test]
    fn byte_formatting_uses_binary_units() {
        assert_eq!(fmt_bytes(512 << 20), "512M");
        assert_eq!(fmt_bytes(3 << 30), "3.0G");
    }

    #[test]
    fn zero_power_caps_fall_back_to_the_default_limit() {
        assert_eq!(effective_power_cap(0, 303), Some(303));
        assert_eq!(effective_power_cap(280, 303), Some(280));
        assert_eq!(effective_power_cap(0, 0), None);
        assert_eq!(power_text(Some(9), Some(303)), "9/303W");
        assert_eq!(power_text(None, Some(303)), "  -");
    }

    #[test]
    fn gpu_names_are_shortened_for_the_layout() {
        assert_eq!(short_name("AMD Radeon Graphics"), "Radeon");
        assert!(short_name(&"x".repeat(40)).chars().count() <= 26);
    }
}
