use libamdgpu_top::AMDGPU::MetricsInfo;
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
    let block = section_block(app, Section::Gpu, "GPU", SectionBox::Gpu);
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
            .constraints([Constraint::Length(1); 5])
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
                Span::styled(format!(" {pwr_s} "), Style::default().fg(app.theme.power())),
            ])),
            left[2],
        );
        // line 3: clocks + fan
        let (sclk_s, mclk_s, fan_s) = gpu_clocks_fan(a);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {sclk_s} "),
                    Style::default().fg(app.theme.clock()),
                ),
                Span::styled(
                    format!(" {mclk_s} "),
                    Style::default().fg(app.theme.clock()),
                ),
                Span::styled(fan_s, Style::default().fg(app.theme.fan())),
            ])),
            left[3],
        );
        // line 4: memory-controller utilization and/or SoC DRAM throughput
        let bandwidth = gpu_memory_bandwidth(a);
        if !bandwidth.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!(" {bandwidth}"),
                    Style::default().fg(app.theme.bandwidth()),
                ))),
                left[4],
            );
        }

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
            app.hist_gpu[i].braille_graph(gcols[0].width as usize, 3, app.theme.gpu()),
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
                Style::default().fg(app.theme.gpu().sample(0.6)),
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

fn gpu_memory_bandwidth(a: &libamdgpu_top::app::AppAmdgpuTop) -> String {
    let (dram_reads, dram_writes) = a.stat.metrics.as_ref().map_or((None, None), |metrics| {
        (
            metrics.get_average_dram_reads(),
            metrics.get_average_dram_writes(),
        )
    });

    memory_bandwidth_text(a.stat.activity.umc, dram_reads, dram_writes)
}

fn memory_bandwidth_text(
    umc_percent: Option<u16>,
    dram_reads_mb_s: Option<u16>,
    dram_writes_mb_s: Option<u16>,
) -> String {
    let utilization = supported_metric(umc_percent).map(|percent| format!("MBW {percent}%"));

    let reads = supported_metric(dram_reads_mb_s);
    let writes = supported_metric(dram_writes_mb_s);
    let throughput = (reads.is_some() || writes.is_some()).then(|| {
        let reads = reads.map_or("-".into(), fmt_bandwidth_rate);
        let writes = writes.map_or("-".into(), fmt_bandwidth_rate);
        format!("DRAM R{reads} W{writes}")
    });

    match (utilization, throughput) {
        (Some(utilization), Some(throughput)) => format!("{utilization} | {throughput}"),
        (Some(utilization), None) => utilization,
        (None, Some(throughput)) => throughput,
        (None, None) => String::new(),
    }
}

fn supported_metric(value: Option<u16>) -> Option<u16> {
    value.filter(|value| *value != u16::MAX)
}

fn fmt_bandwidth_rate(mb_per_second: u16) -> String {
    if mb_per_second >= 1_000 {
        format!("{:.1}G/s", f64::from(mb_per_second) / 1_000.0)
    } else {
        format!("{mb_per_second}M/s")
    }
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
    use super::{
        effective_power_cap, fmt_bandwidth_rate, fmt_bytes, memory_bandwidth_text, power_text,
        short_name,
    };

    #[test]
    fn byte_formatting_uses_binary_units() {
        assert_eq!(fmt_bytes(512 << 20), "512M");
        assert_eq!(fmt_bytes(3 << 30), "3.0G");
    }

    #[test]
    fn bandwidth_combines_memory_utilization_and_dram_throughput() {
        assert_eq!(
            memory_bandwidth_text(Some(42), Some(10_860), Some(5_257)),
            "MBW 42% | DRAM R10.9G/s W5.3G/s"
        );
        assert_eq!(memory_bandwidth_text(Some(42), None, None), "MBW 42%");
    }

    #[test]
    fn bandwidth_falls_back_to_dram_throughput() {
        assert_eq!(
            memory_bandwidth_text(None, Some(10_860), Some(5_257)),
            "DRAM R10.9G/s W5.3G/s"
        );
        assert_eq!(
            memory_bandwidth_text(None, Some(890), Some(0)),
            "DRAM R890M/s W0M/s"
        );
        assert_eq!(fmt_bandwidth_rate(999), "999M/s");
        assert_eq!(fmt_bandwidth_rate(1_000), "1.0G/s");
    }

    #[test]
    fn bandwidth_handles_unsupported_metrics() {
        assert_eq!(memory_bandwidth_text(None, None, None), "");
        assert_eq!(
            memory_bandwidth_text(None, Some(u16::MAX), Some(u16::MAX)),
            ""
        );
        assert_eq!(
            memory_bandwidth_text(None, Some(u16::MAX), Some(512)),
            "DRAM R- W512M/s"
        );
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
