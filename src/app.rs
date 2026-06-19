//! Application state: libamdgpu_top apps, samplers, history, UI section state.

use std::fs;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

use libamdgpu_top::DevicePath;
use libamdgpu_top::app::{AppAmdgpuTop, AppOption};

use crate::config::CollapseState;
use crate::cpu::{CpuSampler, SystemMem, cpu_model};
use crate::history::History;
use crate::theme::{DEFAULT_THEME, Theme};

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum Section {
    Cpu,
    Gpu,
    Npu,
    Processes,
}

impl Section {
    pub const ALL: [Section; 4] = [Section::Cpu, Section::Gpu, Section::Npu, Section::Processes];
    pub fn label(self) -> &'static str {
        match self {
            Section::Cpu => "CPU",
            Section::Gpu => "GPU",
            Section::Npu => "NPU",
            Section::Processes => "PROCESSES",
        }
    }
}

pub struct App {
    pub apps: Vec<AppAmdgpuTop>,
    pub suspended: Vec<DevicePath>,
    pub cpu: CpuSampler,
    pub mem: SystemMem,
    pub collapse: CollapseState,
    pub section: Section,
    pub hist_cpu: History,
    pub hist_gpu: Vec<History>, // per app: gfx busy %
    pub hist_mem: Vec<History>, // per app: memory pool %
    pub hist_npu: History,
    pub hist_cores: Vec<History>, // per logical CPU
    pub npu_info: Option<NpuInfo>,
    pub has_npu: bool,
    pub theme: Theme,
    pub theme_name: String,
    pub themes: Vec<String>,
    pub block_style: usize,
    pub cpu_model: String,
}

#[derive(Clone, Debug)]
pub struct NpuInfo {
    pub name: String,
    pub bdf: String,
    pub fw_version: Option<String>,
    pub fdinfo_supported: bool,
}

impl App {
    pub fn init() -> std::io::Result<Self> {
        let mut dps = DevicePath::get_device_path_list();
        for dp in dps.iter_mut() {
            dp.fill_amdgpu_device_name();
        }
        let (apps, suspended) =
            AppAmdgpuTop::create_app_and_suspended_list(&dps, &AppOption::default());
        let n = apps.len();
        let npu_info = detect_npu(&apps);
        let has_npu = npu_info.is_some();

        let collapse = CollapseState::load();
        let theme_name = if collapse.theme.is_empty() {
            DEFAULT_THEME.to_string()
        } else {
            collapse.theme.clone()
        };
        let theme = Theme::load(&theme_name);
        let block_style = collapse.block_style as usize % crate::gauge::BLOCK_STYLES.len();

        Ok(Self {
            apps,
            suspended,
            cpu: CpuSampler::default(),
            mem: SystemMem::default(),
            collapse,
            section: Section::Gpu,
            hist_cpu: History::new(80),
            hist_gpu: (0..n).map(|_| History::new(80)).collect(),
            hist_mem: (0..n).map(|_| History::new(80)).collect(),
            hist_npu: History::new(80),
            hist_cores: Vec::new(),
            npu_info,
            has_npu,
            theme,
            theme_name,
            themes: Theme::list_available(),
            block_style,
            cpu_model: cpu_model(),
        })
    }

    pub fn cycle_theme(&mut self, forward: bool) {
        if self.themes.is_empty() {
            return;
        }
        let idx = self
            .themes
            .iter()
            .position(|t| t == &self.theme_name)
            .unwrap_or(0);
        let len = self.themes.len();
        let next = if forward {
            (idx + 1) % len
        } else {
            (idx + len - 1) % len
        };
        self.theme_name = self.themes[next].clone();
        self.theme = Theme::load(&self.theme_name);
        self.collapse.theme = self.theme_name.clone();
        self.save_state();
    }

    pub fn cycle_block(&mut self, forward: bool) {
        let len = crate::gauge::BLOCK_STYLES.len();
        self.block_style = if forward {
            (self.block_style + 1) % len
        } else {
            (self.block_style + len - 1) % len
        };
        self.collapse.block_style = self.block_style as u8;
        self.save_state();
    }

    pub fn block_style_name(&self) -> &'static str {
        crate::gauge::block_style(self.block_style).name
    }

    pub fn sample(&mut self) {
        for app in self.apps.iter_mut() {
            app.update(Duration::from_millis(1000));
        }
        self.cpu.tick();
        self.mem.tick();

        // CPU history
        self.hist_cpu.push(self.cpu.cpu_percent.round() as u64);

        // per-core history
        if self.hist_cores.len() != self.cpu.per_core_percent.len() {
            self.hist_cores = (0..self.cpu.per_core_percent.len())
                .map(|_| History::new(80))
                .collect();
        }
        for (i, p) in self.cpu.per_core_percent.iter().enumerate() {
            self.hist_cores[i].push(p.round() as u64);
        }

        // GPU / MEM history per device
        for (i, app) in self.apps.iter().enumerate() {
            let gfx = app.stat.activity.gfx.unwrap_or(0) as u64;
            self.hist_gpu[i].push(gfx);
            let (_, mem_pct, _) = gpu_mem_info(app);
            self.hist_mem[i].push(mem_pct.round() as u64);
        }

        // NPU aggregate (sum of per-context npu%, clamped)
        let mut npu_sum: i64 = 0;
        for app in &self.apps {
            for pu in &app.stat.xdna_fdinfo.proc_usage {
                npu_sum += pu.usage.npu.max(0);
            }
        }
        self.hist_npu.push(npu_sum.clamp(0, 100) as u64);
    }

    pub fn save_state(&self) {
        self.collapse.save();
    }

    pub fn next_section(&mut self) {
        let order: &[Section] = if self.has_npu {
            &Section::ALL
        } else {
            &[Section::Cpu, Section::Gpu, Section::Processes]
        };
        let idx = order.iter().position(|&s| s == self.section).unwrap_or(0);
        self.section = order[(idx + 1) % order.len()];
    }

    pub fn prev_section(&mut self) {
        let order: &[Section] = if self.has_npu {
            &Section::ALL
        } else {
            &[Section::Cpu, Section::Gpu, Section::Processes]
        };
        let idx = order.iter().position(|&s| s == self.section).unwrap_or(0);
        self.section = order[(idx + order.len() - 1) % order.len()];
    }

    pub fn toggle_collapse(&mut self) {
        match self.section {
            Section::Cpu => self.collapse.cpu = !self.collapse.cpu,
            Section::Gpu => self.collapse.gpu = !self.collapse.gpu,
            Section::Npu => self.collapse.npu = !self.collapse.npu,
            Section::Processes => self.collapse.processes = !self.collapse.processes,
        }
        self.save_state();
    }

    pub fn is_collapsed(&self, s: Section) -> bool {
        match s {
            Section::Cpu => self.collapse.cpu,
            Section::Gpu => self.collapse.gpu,
            Section::Npu => self.collapse.npu,
            Section::Processes => self.collapse.processes,
        }
    }
}

fn detect_npu(apps: &[AppAmdgpuTop]) -> Option<NpuInfo> {
    if let Some(info) = apps.iter().find_map(|app| {
        app.xdna_device_path.as_ref().map(|device_path| {
            NpuInfo::from_device_path(device_path, app.xdna_fw_version.clone(), true)
        })
    }) {
        return Some(info);
    }

    detect_npu_from_sysfs()
}

impl NpuInfo {
    fn from_device_path(
        device_path: &DevicePath,
        fw_version: Option<String>,
        fdinfo_supported: bool,
    ) -> Self {
        let name = if device_path.device_name.trim().is_empty() {
            read_trim(device_path.sysfs_path.join("vbnv")).unwrap_or_else(|| "XDNA NPU".to_string())
        } else {
            device_path.device_name.clone()
        };
        let fw_version = fw_version.or_else(|| device_path.get_xdna_fw_version().ok());

        Self {
            name,
            bdf: device_path.pci.to_string(),
            fw_version,
            fdinfo_supported,
        }
    }
}

fn detect_npu_from_sysfs() -> Option<NpuInfo> {
    let entries = fs::read_dir("/sys/class/accel").ok()?;

    for entry in entries.flatten() {
        let accel_name = entry.file_name();
        let accel_path = PathBuf::from("/dev/accel").join(&accel_name);
        let Ok(sysfs_path) = fs::canonicalize(entry.path().join("device")) else {
            continue;
        };

        // AMD XDNA accel devices expose these attributes via amdxdna. This is a
        // presence check only; fdinfo telemetry is optional and may be missing on
        // newer kernels even when the NPU itself is usable.
        if !(sysfs_path.join("device_type").exists() && sysfs_path.join("vbnv").exists()) {
            continue;
        }

        let name = read_trim(sysfs_path.join("vbnv")).unwrap_or_else(|| "XDNA NPU".to_string());
        let bdf = sysfs_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
        let fw_version = read_trim(sysfs_path.join("fw_version"));
        let fdinfo_supported = fdinfo_has_drm_driver(&accel_path);

        return Some(NpuInfo {
            name,
            bdf,
            fw_version,
            fdinfo_supported,
        });
    }

    None
}

fn read_trim(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn fdinfo_has_drm_driver(accel_path: &Path) -> bool {
    let Ok(file) = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(accel_path)
    else {
        return false;
    };
    let fd = file.as_raw_fd();

    fs::read_to_string(format!("/proc/self/fdinfo/{fd}"))
        .is_ok_and(|s| s.lines().any(|line| line.starts_with("drm-driver")))
}

/// APU-aware memory info: on APUs the real allocatable pool is GTT (system RAM
/// via GART), not the small VRAM carveout. Returns (label, pct, used_bytes,
/// total_bytes, pool_kind).
pub struct MemInfo {
    pub label: &'static str,
    pub pct: f64,
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub is_apu: bool,
}

pub fn gpu_mem_info(app: &AppAmdgpuTop) -> (String, f64, MemInfo) {
    let is_apu = app.device_info.is_apu;
    let v = &app.stat.vram_usage.0;
    let vram_used = v.vram.heap_usage;
    let vram_total = v.vram.usable_heap_size.max(1);
    let gtt_used = v.gtt.heap_usage;
    let gtt_total = v.gtt.usable_heap_size.max(1);

    // Always label "MEM" for clarity. On APUs the real pool is GTT (unified
    // system RAM); on dGPUs it's VRAM. The pool selection differs; the label
    // does not.
    let (used, total) = if is_apu {
        (gtt_used, gtt_total)
    } else {
        (vram_used, vram_total)
    };
    let label = "MEM";
    let pct = (used as f64 / total as f64) * 100.0;
    let display_label = "MEM".to_string();
    (
        display_label,
        pct,
        MemInfo {
            label,
            pct,
            used_bytes: used,
            total_bytes: total,
            is_apu,
        },
    )
}
