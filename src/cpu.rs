//! CPU utilization (/proc/stat) + system memory/load (/proc/meminfo, /proc/loadavg).

use std::fs;
use std::time::Instant;

#[derive(Default)]
pub struct CpuSampler {
    prev_total: u64,
    prev_idle: u64,
    prev_per: Vec<(u64, u64)>,
    pub cpu_percent: f64,
    pub per_core_percent: Vec<f64>,
    last: Option<Instant>,
}

fn parse_cpu_line(line: &str) -> Option<(u64, u64)> {
    let mut count = 0;
    let mut total = 0_u64;
    let mut idle = 0_u64;
    let mut guest = 0_u64;
    let mut guest_nice = 0_u64;

    for (index, field) in line.split_whitespace().skip(1).enumerate() {
        let value = field.parse::<u64>().ok()?;
        count += 1;
        total = total.saturating_add(value);
        match index {
            3 | 4 => idle = idle.saturating_add(value),
            8 => guest = value,
            9 => guest_nice = value,
            _ => {}
        }
    }
    if count < 4 {
        return None;
    }

    // guest/guest_nice are already counted in user/nice; avoid double count.
    Some((total.saturating_sub(guest).saturating_sub(guest_nice), idle))
}

impl CpuSampler {
    pub fn tick(&mut self) {
        let now = Instant::now();
        if self.last.is_none() {
            self.prime();
            self.last = Some(now);
            return;
        }
        if let Some(prev) = self.last
            && now.duration_since(prev).as_millis() < 200
        {
            return;
        }
        self.prime();
        self.last = Some(now);
    }

    fn prime(&mut self) {
        let Ok(content) = fs::read_to_string("/proc/stat") else {
            return;
        };
        let mut lines = content.lines();
        if let Some(first) = lines.next()
            && let Some((total, idle)) = parse_cpu_line(first)
        {
            let dt = total.saturating_sub(self.prev_total);
            let di = idle.saturating_sub(self.prev_idle);
            if dt > 0 {
                self.cpu_percent = ((1.0 - di as f64 / dt as f64) * 100.0).clamp(0.0, 100.0);
            }
            self.prev_total = total;
            self.prev_idle = idle;
        }
        self.per_core_percent.clear();
        let mut index = 0;
        for line in lines.filter(|line| line.starts_with("cpu") && !line.starts_with("cpu ")) {
            let Some((total, idle)) = parse_cpu_line(line) else {
                continue;
            };
            let previous = self.prev_per.get(index).copied().unwrap_or((total, idle));
            let delta_total = total.saturating_sub(previous.0);
            let delta_idle = idle.saturating_sub(previous.1);
            let percent = if delta_total > 0 {
                ((1.0 - delta_idle as f64 / delta_total as f64) * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            };

            self.per_core_percent.push(percent);
            if let Some(previous) = self.prev_per.get_mut(index) {
                *previous = (total, idle);
            } else {
                self.prev_per.push((total, idle));
            }
            index += 1;
        }
        self.prev_per.truncate(index);
    }
}

/// Read the CPU model name from /proc/cpuinfo (first "model name" line).
pub fn cpu_model() -> String {
    if let Ok(s) = fs::read_to_string("/proc/cpuinfo") {
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("model name")
                && let Some(v) = rest.split(':').nth(1)
            {
                return v.trim().to_string();
            }
        }
    }
    "CPU".to_string()
}

pub struct SystemMem {
    pub mem_total_kb: u64,
    pub mem_avail_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
}

impl Default for SystemMem {
    fn default() -> Self {
        Self {
            mem_total_kb: 1,
            mem_avail_kb: 1,
            swap_total_kb: 0,
            swap_free_kb: 0,
            load1: 0.0,
            load5: 0.0,
            load15: 0.0,
        }
    }
}

impl SystemMem {
    pub fn tick(&mut self) {
        if let Ok(contents) = fs::read_to_string("/proc/meminfo") {
            self.update_meminfo(&contents);
        }
        if let Ok(contents) = fs::read_to_string("/proc/loadavg") {
            self.update_loadavg(&contents);
        }
    }

    fn update_meminfo(&mut self, contents: &str) {
        for line in contents.lines() {
            let mut fields = line.split_whitespace();
            let Some(key) = fields.next() else { continue };
            let Some(value) = fields.next().and_then(|value| value.parse::<u64>().ok()) else {
                continue;
            };
            match key {
                "MemTotal:" => self.mem_total_kb = value,
                "MemAvailable:" => self.mem_avail_kb = value,
                "SwapTotal:" => self.swap_total_kb = value,
                "SwapFree:" => self.swap_free_kb = value,
                _ => {}
            }
        }
    }

    fn update_loadavg(&mut self, contents: &str) {
        let mut fields = contents.split_whitespace();
        let values = (
            fields.next().and_then(|value| value.parse().ok()),
            fields.next().and_then(|value| value.parse().ok()),
            fields.next().and_then(|value| value.parse().ok()),
        );
        if let (Some(load1), Some(load5), Some(load15)) = values {
            (self.load1, self.load5, self.load15) = (load1, load5, load15);
        }
    }

    pub fn mem_used_pct(&self) -> f64 {
        let total = self.mem_total_kb.max(1);
        ((1.0 - self.mem_avail_kb as f64 / total as f64) * 100.0).clamp(0.0, 100.0)
    }
    pub fn swap_used_pct(&self) -> f64 {
        if self.swap_total_kb == 0 {
            0.0
        } else {
            ((1.0 - self.swap_free_kb as f64 / self.swap_total_kb as f64) * 100.0).clamp(0.0, 100.0)
        }
    }
    pub fn mem_used_gb(&self) -> f64 {
        (self.mem_total_kb.saturating_sub(self.mem_avail_kb)) as f64 / 1_048_576.0
    }
    pub fn mem_total_gb(&self) -> f64 {
        self.mem_total_kb as f64 / 1_048_576.0
    }
    pub fn swap_used_gb(&self) -> f64 {
        (self.swap_total_kb.saturating_sub(self.swap_free_kb)) as f64 / 1_048_576.0
    }
    pub fn swap_total_gb(&self) -> f64 {
        self.swap_total_kb as f64 / 1_048_576.0
    }
}

#[cfg(test)]
mod tests {
    use super::{SystemMem, parse_cpu_line};

    #[test]
    fn parse_cpu_line_counts_idle_and_excludes_guest_times() {
        let (total, idle) =
            parse_cpu_line("cpu 100 20 30 400 50 6 7 8 9 10").expect("valid aggregate CPU line");

        assert_eq!(total, 621);
        assert_eq!(idle, 450);
    }

    #[test]
    fn parse_cpu_line_rejects_incomplete_or_invalid_data() {
        assert_eq!(parse_cpu_line("cpu 1 2 3"), None);
        assert_eq!(parse_cpu_line("cpu 1 2 invalid 4"), None);
    }

    #[test]
    fn system_memory_parsers_update_known_fields() {
        let mut memory = SystemMem::default();
        memory.update_meminfo(
            "MemTotal: 1048576 kB\nMemAvailable: 262144 kB\n\
             SwapTotal: 524288 kB\nSwapFree: 131072 kB\nIgnored: 99 kB\n",
        );
        memory.update_loadavg("1.25 2.50 3.75 1/100 123\n");

        assert_eq!(memory.mem_total_kb, 1_048_576);
        assert_eq!(memory.mem_avail_kb, 262_144);
        assert_eq!(memory.swap_total_kb, 524_288);
        assert_eq!(memory.swap_free_kb, 131_072);
        assert_eq!(
            (memory.load1, memory.load5, memory.load15),
            (1.25, 2.5, 3.75)
        );
        assert_eq!(memory.mem_used_pct(), 75.0);
        assert_eq!(memory.swap_used_pct(), 75.0);
    }

    #[test]
    fn malformed_load_average_preserves_previous_values() {
        let mut memory = SystemMem {
            load1: 1.0,
            load5: 2.0,
            load15: 3.0,
            ..SystemMem::default()
        };
        memory.update_loadavg("invalid 4.0 5.0");
        assert_eq!((memory.load1, memory.load5, memory.load15), (1.0, 2.0, 3.0));
    }

    #[test]
    fn memory_percentages_are_bounded() {
        let memory = SystemMem {
            mem_total_kb: 10,
            mem_avail_kb: 20,
            swap_total_kb: 10,
            swap_free_kb: 20,
            ..SystemMem::default()
        };
        assert_eq!(memory.mem_used_pct(), 0.0);
        assert_eq!(memory.swap_used_pct(), 0.0);
    }
}
