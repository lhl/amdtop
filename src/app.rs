//! Application state: `libamdgpu_top` apps, samplers, history, and UI state.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
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

pub struct GpuDevice {
    pub device_path: DevicePath,
    pub app: Option<AppAmdgpuTop>,
    pub hist_gpu: History,
    pub hist_mem: History,
}

impl GpuDevice {
    fn new(device_path: DevicePath, app: Option<AppAmdgpuTop>) -> Self {
        Self {
            device_path,
            app,
            hist_gpu: History::new(HISTORY_CAPACITY),
            hist_mem: History::new(HISTORY_CAPACITY),
        }
    }

    pub fn is_sleeping(&self) -> bool {
        !self.device_path.check_if_device_is_active()
    }

    fn try_activate(&mut self) -> bool {
        let is_awake = self.device_path.check_if_device_is_active();
        let device_path = self.device_path.clone();
        activate_if_awake(&mut self.app, is_awake, || {
            let amdgpu_dev = device_path.init().ok()?;
            AppAmdgpuTop::new(amdgpu_dev, device_path, &AppOption::default())
        })
    }
}

pub struct App {
    pub gpus: Vec<GpuDevice>,
    pub cpu: CpuSampler,
    pub mem: SystemMem,
    process_rss_kb: HashMap<i32, u64>,
    pub collapse: CollapseState,
    pub section: Section,
    pub hist_cpu: History,
    pub hist_npu: History,
    pub hist_cores: Vec<History>, // per logical CPU
    pub npu_info: Option<NpuInfo>,
    pub has_npu: bool,
    pub theme: Theme,
    pub theme_name: String,
    pub themes: Vec<String>,
    pub block_style: usize,
    pub cpu_model: String,
    process_index_sender: Sender<Vec<DevicePath>>,
}

#[derive(Clone, Debug)]
pub struct NpuInfo {
    pub name: String,
    pub bdf: String,
    pub fw_version: Option<String>,
    pub fdinfo_supported: bool,
}

struct DiscoveredDevices {
    gpus: Vec<GpuDevice>,
    process_device_paths: Vec<DevicePath>,
    npu_info: Option<NpuInfo>,
}

impl App {
    pub fn init() -> Self {
        let DiscoveredDevices {
            gpus,
            process_device_paths,
            npu_info,
        } = discover_devices();
        let has_npu = npu_info.is_some();
        let process_index_sender = spawn_process_index_thread(process_device_paths);

        let collapse = CollapseState::load();
        let theme_name = if collapse.theme.is_empty() {
            DEFAULT_THEME.to_string()
        } else {
            collapse.theme.clone()
        };
        let theme = Theme::load(&theme_name);
        let block_style = collapse.block_style % crate::gauge::BLOCK_STYLES.len();

        Self {
            gpus,
            cpu: CpuSampler::default(),
            mem: SystemMem::default(),
            process_rss_kb: HashMap::new(),
            collapse,
            section: Section::Gpu,
            hist_cpu: History::new(HISTORY_CAPACITY),
            hist_npu: History::new(HISTORY_CAPACITY),
            hist_cores: Vec::new(),
            npu_info,
            has_npu,
            theme,
            theme_name,
            themes: Theme::list_available(),
            block_style,
            cpu_model: cpu_model(),
            process_index_sender,
        }
    }

    pub fn refresh_devices(&mut self) {
        let DiscoveredDevices {
            gpus,
            process_device_paths,
            npu_info,
        } = discover_devices();
        let previous_gpus = std::mem::take(&mut self.gpus);

        self.gpus = pair_refreshed_devices(
            gpus,
            previous_gpus,
            |gpu| {
                let device = &gpu.device_path;
                (device.pci, device.device_id, device.revision_id)
            },
            |gpu| {
                let device = &gpu.device_path;
                (device.pci, device.device_id, device.revision_id)
            },
        )
        .into_iter()
        .map(|(mut refreshed, previous)| {
            if let Some(previous) = previous {
                refreshed.hist_gpu = previous.hist_gpu;
                refreshed.hist_mem = previous.hist_mem;
            }
            refreshed
        })
        .collect();

        if !same_npu_device(self.npu_info.as_ref(), npu_info.as_ref()) {
            self.hist_npu = History::new(HISTORY_CAPACITY);
        }
        self.npu_info = npu_info;
        self.has_npu = self.npu_info.is_some();
        let _ = self.process_index_sender.send(process_device_paths);
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

    pub fn active_apps(&self) -> impl Iterator<Item = &AppAmdgpuTop> {
        self.gpus.iter().filter_map(|gpu| gpu.app.as_ref())
    }

    pub fn sample(&mut self, interval: Duration) {
        for gpu in &mut self.gpus {
            gpu.try_activate();
            if let Some(app) = gpu.app.as_mut() {
                app.update(interval);
            }
        }
        self.sample_process_memory();
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

        // GPU / MEM history per device. Leave sleeping-device histories
        // untouched so sleep is not misrepresented as measured zero activity.
        for gpu in &mut self.gpus {
            if gpu.is_sleeping() {
                continue;
            }
            if let Some(app) = gpu.app.as_ref() {
                let gfx = app.stat.activity.gfx.unwrap_or(0) as u64;
                gpu.hist_gpu.push(gfx);
                let memory = gpu_mem_info(app);
                gpu.hist_mem.push(memory.percent.round() as u64);
            }
        }

        // NPU aggregate (sum of per-context npu%, clamped)
        let npu_sum = self
            .active_apps()
            .flat_map(|app| &app.stat.xdna_fdinfo.proc_usage)
            .map(|process| process.usage.npu.max(0))
            .sum::<i64>();
        self.hist_npu.push(npu_sum.clamp(0, 100) as u64);
    }

    fn sample_process_memory(&mut self) {
        let pids: HashSet<i32> = self
            .active_apps()
            .flat_map(|app| &app.stat.fdinfo.proc_usage)
            .map(|process| process.pid)
            .collect();

        self.process_rss_kb = pids
            .into_iter()
            .filter_map(|pid| read_process_rss_kb(pid).map(|rss_kb| (pid, rss_kb)))
            .collect();
    }

    pub fn process_rss_kb(&self, pid: i32) -> Option<u64> {
        self.process_rss_kb.get(&pid).copied()
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

fn discover_devices() -> DiscoveredDevices {
    let mut device_paths = DevicePath::get_device_path_list();
    // libamdgpu_top discovers devices through read_dir(), whose order is
    // unspecified. PCI order is deterministic and matches the physical
    // ordering used by ROCm's system-management tools.
    device_paths.sort_by_key(|device| {
        let pci = device.pci;
        (pci.domain, pci.bus, pci.dev, pci.func)
    });
    for device_path in &mut device_paths {
        device_path.fill_amdgpu_device_name();
    }

    // Initialize only devices that are already awake. The backend's
    // suspended-list helper wakes one device when every GPU is asleep,
    // which is undesirable for a system monitor.
    let active_apps: Vec<AppAmdgpuTop> = device_paths
        .iter()
        .filter(|device_path| device_path.check_if_device_is_active())
        .filter_map(|device_path| {
            let amdgpu_device = device_path.init().ok()?;
            AppAmdgpuTop::new(amdgpu_device, device_path.clone(), &AppOption::default())
        })
        .collect();
    let npu_info = detect_npu(&active_apps);
    let gpus = merge_devices(
        device_paths.clone(),
        active_apps,
        |device_path| device_path.pci,
        |app| app.device_path.pci,
    )
    .into_iter()
    .map(|(device_path, app)| GpuDevice::new(device_path, app))
    .collect::<Vec<_>>();

    let mut process_device_paths = device_paths;
    if let Some(xdna_device_path) = gpus
        .iter()
        .filter_map(|gpu| gpu.app.as_ref())
        .find_map(|app| app.xdna_device_path.as_ref())
    {
        process_device_paths.push(xdna_device_path.clone());
    }

    DiscoveredDevices {
        gpus,
        process_device_paths,
        npu_info,
    }
}

// Keep one process-index worker for the application's lifetime, but let a
// manual device refresh replace its targets. This avoids leaking a permanent
// backend worker each time the hidden refresh action is used.
fn spawn_process_index_thread(initial_device_paths: Vec<DevicePath>) -> Sender<Vec<DevicePath>> {
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        let mut device_paths = initial_device_paths;
        let mut index = Vec::new();
        let interval = Duration::from_secs(PROCESS_INDEX_REFRESH_SECS);

        loop {
            if !device_paths.is_empty() {
                let all_processes = stat::get_process_list();

                for device_path in &device_paths {
                    let paths: &[&PathBuf] = if device_path.is_amdgpu() {
                        &[&device_path.render, &device_path.card]
                    } else {
                        &[&device_path.accel]
                    };
                    stat::update_index_by_all_proc(&mut index, paths, &all_processes);

                    if let Ok(mut current_index) = device_path.arc_proc_index.lock() {
                        current_index.clone_from(&index);
                    }
                }
            }

            match receiver.recv_timeout(interval) {
                Ok(refreshed_device_paths) => {
                    device_paths = refreshed_device_paths;
                    while let Ok(newer_device_paths) = receiver.try_recv() {
                        device_paths = newer_device_paths;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });

    sender
}

fn merge_devices<D, A, K: PartialEq>(
    devices: Vec<D>,
    mut active_apps: Vec<A>,
    device_key: impl Fn(&D) -> K,
    app_key: impl Fn(&A) -> K,
) -> Vec<(D, Option<A>)> {
    devices
        .into_iter()
        .map(|device| {
            let key = device_key(&device);
            let app = active_apps
                .iter()
                .position(|app| app_key(app) == key)
                .map(|position| active_apps.remove(position));
            (device, app)
        })
        .collect()
}

fn pair_refreshed_devices<D, P, K: PartialEq>(
    discovered: Vec<D>,
    previous: Vec<P>,
    discovered_key: impl Fn(&D) -> K,
    previous_key: impl Fn(&P) -> K,
) -> Vec<(D, Option<P>)> {
    merge_devices(discovered, previous, discovered_key, previous_key)
}

fn same_npu_device(previous: Option<&NpuInfo>, refreshed: Option<&NpuInfo>) -> bool {
    match (previous, refreshed) {
        (Some(previous), Some(refreshed)) => {
            previous.bdf == refreshed.bdf && previous.name == refreshed.name
        }
        (None, None) => true,
        _ => false,
    }
}

fn activate_if_awake<T>(
    app: &mut Option<T>,
    is_awake: bool,
    initialize: impl FnOnce() -> Option<T>,
) -> bool {
    if app.is_some() || !is_awake {
        return false;
    }

    *app = initialize();
    app.is_some()
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

fn read_process_rss_kb(pid: i32) -> Option<u64> {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    parse_process_rss_kb(&status)
}

fn parse_process_rss_kb(status: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        if fields.next() != Some("VmRSS:") {
            return None;
        }
        let rss_kb = fields.next()?.parse().ok()?;
        (fields.next() == Some("kB")).then_some(rss_kb)
    })
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
    use super::{
        NpuInfo, activate_if_awake, memory_info, merge_devices, pair_refreshed_devices,
        parse_process_rss_kb, same_npu_device,
    };

    #[test]
    fn device_merge_retains_sleeping_gpus_in_discovery_order() {
        let devices = vec![1, 2, 3];
        let active = vec![(3, "gpu3"), (1, "gpu1")];

        let merged = merge_devices(devices, active, |device| *device, |app| app.0);

        assert_eq!(
            merged,
            vec![(1, Some((1, "gpu1"))), (2, None), (3, Some((3, "gpu3"))),]
        );
    }

    #[test]
    fn device_refresh_preserves_state_only_for_the_same_hardware() {
        let discovered = vec![(1, 10, 1), (2, 20, 1), (3, 30, 1), (4, 40, 1)];
        let previous = vec![
            (1, 10, 1, "gpu1-history"),
            (2, 99, 1, "different-device"),
            (3, 30, 2, "different-revision"),
            (5, 50, 1, "removed"),
        ];

        let paired = pair_refreshed_devices(
            discovered,
            previous,
            |device| (device.0, device.1, device.2),
            |device| (device.0, device.1, device.2),
        );

        assert_eq!(
            paired,
            vec![
                ((1, 10, 1), Some((1, 10, 1, "gpu1-history"))),
                ((2, 20, 1), None),
                ((3, 30, 1), None),
                ((4, 40, 1), None),
            ]
        );
    }

    #[test]
    fn npu_refresh_preserves_state_only_for_the_same_hardware() {
        let previous = NpuInfo {
            name: "Ryzen AI".into(),
            bdf: "0000:c4:00.1".into(),
            fw_version: Some("1".into()),
            fdinfo_supported: false,
        };
        let same_npu_new_firmware = NpuInfo {
            fw_version: Some("2".into()),
            fdinfo_supported: true,
            ..previous.clone()
        };
        let replacement = NpuInfo {
            name: "Different NPU".into(),
            ..previous.clone()
        };

        assert!(same_npu_device(
            Some(&previous),
            Some(&same_npu_new_firmware)
        ));
        assert!(!same_npu_device(Some(&previous), Some(&replacement)));
        assert!(!same_npu_device(Some(&previous), None));
        assert!(same_npu_device(None, None));
    }

    #[test]
    fn sleeping_gpu_is_initialized_only_after_it_wakes() {
        let mut app = None;
        let mut initialization_count = 0;

        assert!(!activate_if_awake(&mut app, false, || {
            initialization_count += 1;
            Some("active")
        }));
        assert_eq!(app, None);
        assert_eq!(initialization_count, 0);

        assert!(activate_if_awake(&mut app, true, || {
            initialization_count += 1;
            Some("active")
        }));
        assert_eq!(app, Some("active"));
        assert_eq!(initialization_count, 1);

        assert!(!activate_if_awake(&mut app, true, || {
            initialization_count += 1;
            Some("replacement")
        }));
        assert_eq!(app, Some("active"));
        assert_eq!(initialization_count, 1);
    }

    #[test]
    fn awake_gpu_initialization_can_be_retried_after_a_failure() {
        let mut app = None;

        assert!(!activate_if_awake(&mut app, true, || None::<&str>));
        assert_eq!(app, None);
        assert!(activate_if_awake(&mut app, true, || Some("active")));
        assert_eq!(app, Some("active"));
    }

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

    #[test]
    fn process_rss_parser_reads_resident_system_memory() {
        let status = "Name:\ttest\nVmSize:\t4096 kB\nVmRSS:\t1536 kB\nThreads:\t2\n";

        assert_eq!(parse_process_rss_kb(status), Some(1536));
        assert_eq!(parse_process_rss_kb("VmRSS:\tinvalid kB\n"), None);
        assert_eq!(parse_process_rss_kb("VmRSS:\t1536 MB\n"), None);
        assert_eq!(parse_process_rss_kb("Name:\ttest\n"), None);
    }
}
