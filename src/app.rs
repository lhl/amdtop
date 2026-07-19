//! Application state: `libamdgpu_top` apps, samplers, history, and UI state.

use std::fs;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

use libamdgpu_top::app::{AppAmdgpuTop, AppOption};
use libamdgpu_top::{DevicePath, stat};

use crate::config::CollapseState;
use crate::cpu::{CpuSampler, SystemMem, cpu_model};
use crate::history::History;
use crate::theme::{DEFAULT_THEME, Theme};

const HISTORY_CAPACITY: usize = 80;
const PROCESS_INDEX_REFRESH_SECS: u64 = 5;

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum Section {
    Cpu,
    Gpu,
    Npu,
    Processes,
}

impl Section {
    pub const ALL: [Section; 4] = [Section::Cpu, Section::Gpu, Section::Npu, Section::Processes];
}

pub struct App {
    pub apps: Vec<AppAmdgpuTop>,
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
    pub fn init() -> Self {
        let mut dps = DevicePath::get_device_path_list();
        // libamdgpu_top discovers devices through read_dir(), whose order is
        // unspecified. PCI order is deterministic and matches the physical
        // ordering used by ROCm's system-management tools.
        dps.sort_by_key(|device| {
            let pci = device.pci;
            (pci.domain, pci.bus, pci.dev, pci.func)
        });
        for dp in &mut dps {
            dp.fill_amdgpu_device_name();
        }
        let (apps, _suspended) =
            AppAmdgpuTop::create_app_and_suspended_list(&dps, &AppOption::default());
        let n = apps.len();
        let npu_info = detect_npu(&apps);
        let has_npu = npu_info.is_some();

        // AppAmdgpuTop populates its shared process indexes once during
        // construction. The library's frontends start this worker separately;
        // without it, processes created or restarted after amdtop starts never
        // appear in the process table.
        let mut process_device_paths = dps;
        if let Some(xdna_device_path) = apps.iter().find_map(|app| app.xdna_device_path.as_ref()) {
            process_device_paths.push(xdna_device_path.clone());
        }
        stat::spawn_update_index_thread(process_device_paths, PROCESS_INDEX_REFRESH_SECS);

        let collapse = CollapseState::load();
        let theme_name = if collapse.theme.is_empty() {
            DEFAULT_THEME.to_string()
        } else {
            collapse.theme.clone()
        };
        let theme = Theme::load(&theme_name);
        let block_style = collapse.block_style % crate::gauge::BLOCK_STYLES.len();

        Self {
            apps,
            cpu: CpuSampler::default(),
            mem: SystemMem::default(),
            collapse,
            section: Section::Gpu,
            hist_cpu: History::new(HISTORY_CAPACITY),
            hist_gpu: (0..n).map(|_| History::new(HISTORY_CAPACITY)).collect(),
            hist_mem: (0..n).map(|_| History::new(HISTORY_CAPACITY)).collect(),
            hist_npu: History::new(HISTORY_CAPACITY),
            hist_cores: Vec::new(),
            npu_info,
            has_npu,
            theme,
            theme_name,
            themes: Theme::list_available(),
            block_style,
            cpu_model: cpu_model(),
        }
    }

    pub fn cycle_theme(&mut self, forward: bool) -> std::io::Result<()> {
        if self.themes.is_empty() {
            return Ok(());
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
        self.save_state()
    }

    pub fn cycle_block(&mut self, forward: bool) -> std::io::Result<()> {
        let len = crate::gauge::BLOCK_STYLES.len();
        self.block_style = if forward {
            (self.block_style + 1) % len
        } else {
            (self.block_style + len - 1) % len
        };
        self.collapse.block_style = self.block_style;
        self.save_state()
    }

    pub fn block_style_name(&self) -> &'static str {
        crate::gauge::block_style(self.block_style).name
    }

    pub fn sample(&mut self, interval: Duration) {
        for app in &mut self.apps {
            app.update(interval);
        }
        self.cpu.tick();
        self.mem.tick();

        // CPU history
        self.hist_cpu.push(self.cpu.cpu_percent.round() as u64);

        // per-core history
        if self.hist_cores.len() != self.cpu.per_core_percent.len() {
            self.hist_cores = (0..self.cpu.per_core_percent.len())
                .map(|_| History::new(HISTORY_CAPACITY))
                .collect();
        }
        for (i, p) in self.cpu.per_core_percent.iter().enumerate() {
            self.hist_cores[i].push(p.round() as u64);
        }

        // GPU / MEM history per device
        for (i, app) in self.apps.iter().enumerate() {
            let gfx = app.stat.activity.gfx.unwrap_or(0) as u64;
            self.hist_gpu[i].push(gfx);
            let memory = gpu_mem_info(app);
            self.hist_mem[i].push(memory.percent.round() as u64);
        }

        // NPU aggregate (sum of per-context npu%, clamped)
        let npu_sum = self
            .apps
            .iter()
            .flat_map(|app| &app.stat.xdna_fdinfo.proc_usage)
            .map(|process| process.usage.npu.max(0))
            .sum::<i64>();
        self.hist_npu.push(npu_sum.clamp(0, 100) as u64);
    }

    pub fn save_state(&self) -> std::io::Result<()> {
        self.collapse.save()
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

    pub fn toggle_collapse(&mut self) -> std::io::Result<()> {
        match self.section {
            Section::Cpu => self.collapse.cpu = !self.collapse.cpu,
            Section::Gpu => self.collapse.gpu = !self.collapse.gpu,
            Section::Npu => self.collapse.npu = !self.collapse.npu,
            Section::Processes => self.collapse.processes = !self.collapse.processes,
        }
        self.save_state()
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

/// APU-aware memory usage. APUs use the GTT (system RAM) pool; discrete GPUs
/// use VRAM.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemInfo {
    pub percent: f64,
    pub used_bytes: u64,
    pub total_bytes: u64,
}

pub fn gpu_mem_info(app: &AppAmdgpuTop) -> MemInfo {
    let usage = &app.stat.vram_usage.0;
    memory_info(
        app.device_info.is_apu,
        (usage.vram.heap_usage, usage.vram.usable_heap_size),
        (usage.gtt.heap_usage, usage.gtt.usable_heap_size),
    )
}

fn memory_info(is_apu: bool, vram: (u64, u64), gtt: (u64, u64)) -> MemInfo {
    let (used_bytes, total_bytes) = if is_apu { gtt } else { vram };
    let percent = if total_bytes == 0 {
        0.0
    } else {
        (used_bytes as f64 / total_bytes as f64 * 100.0).clamp(0.0, 100.0)
    };
    MemInfo {
        percent,
        used_bytes,
        total_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::memory_info;

    #[test]
    fn memory_info_selects_gtt_for_apus_and_vram_for_discrete_gpus() {
        let apu = memory_info(true, (10, 100), (20, 200));
        let discrete = memory_info(false, (10, 100), (20, 200));

        assert_eq!(
            (apu.used_bytes, apu.total_bytes, apu.percent),
            (20, 200, 10.0)
        );
        assert_eq!(
            (discrete.used_bytes, discrete.total_bytes, discrete.percent),
            (10, 100, 10.0)
        );
    }

    #[test]
    fn memory_info_handles_zero_and_overcommitted_pools() {
        assert_eq!(memory_info(false, (10, 0), (0, 0)).percent, 0.0);
        assert_eq!(memory_info(false, (200, 100), (0, 0)).percent, 100.0);
    }
}
