// Non-TUI smoke test: sample telemetry once and print, to verify the data
// accessors work on real hardware without fighting the terminal.
use std::fs;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::Duration;

use libamdgpu_top::DevicePath;
use libamdgpu_top::app::{AppAmdgpuTop, AppOption};
use libamdgpu_top::xdna;

fn main() {
    let mut dps = DevicePath::get_device_path_list();
    for dp in dps.iter_mut() {
        dp.fill_amdgpu_device_name();
    }
    println!("found {} device paths", dps.len());
    let (mut apps, suspended) =
        AppAmdgpuTop::create_app_and_suspended_list(&dps, &AppOption::default());
    println!("{} active apps, {} suspended", apps.len(), suspended.len());

    if let Some(xdna_dp) = apps
        .iter()
        .find_map(|app| app.xdna_device_path.clone())
        .or_else(xdna::find_xdna_device)
    {
        println!(
            "xdna detected: {} | bdf {} | accel {} | fw {:?} | fdinfo drm-driver {}",
            xdna_dp.device_name,
            xdna_dp.pci,
            xdna_dp.accel.display(),
            xdna_dp.get_xdna_fw_version().ok(),
            fdinfo_has_drm_driver(&xdna_dp.accel),
        );
    } else {
        println!("xdna detected: no");
    }

    for app in apps.iter_mut() {
        app.update(Duration::from_millis(500));
        let st = &app.stat;
        let dp = &app.device_path;
        let kind = if dp.is_xdna() { "NPU" } else { "GPU" };
        let v = &st.vram_usage.0.vram;
        println!("\n[{kind}] {} | pci bus {}", dp.menu_entry(), dp.pci.bus);
        println!("  gfx%: {:?}", st.activity.gfx);
        println!("  amdgpu has_npu hint: {}", app.device_info.has_npu);
        println!(
            "  libamdgpu_top xdna path: {}",
            app.xdna_device_path.is_some()
        );
        println!(
            "  vram: {}M / {}M",
            v.heap_usage >> 20,
            v.usable_heap_size >> 20
        );
        if let Some(s) = st.sensors.as_ref() {
            let t = s.junction_temp.as_ref().or(s.edge_temp.as_ref());
            println!("  temp: {:?}", t.map(|t| t.current));
            println!(
                "  avg power: {:?} W",
                s.average_power.as_ref().map(|p| p.value)
            );
            println!("  power cap: {:?}", s.power_cap.as_ref().map(|c| c.current));
            println!("  sclk: {:?} mclk: {:?}", s.sclk, s.mclk);
            println!("  fan: {:?}", s.fan_rpm);
        } else {
            println!("  sensors: None");
        }
        println!("  fdinfo procs: {}", st.fdinfo.proc_usage.len());
        for pu in st.fdinfo.proc_usage.iter().take(5) {
            println!(
                "    pid={} name={} vram={}M gfx={} cpu={}",
                pu.pid,
                pu.name,
                pu.usage.vram_usage >> 10,
                pu.usage.gfx,
                pu.usage.cpu
            );
        }
        // NPU fdinfo
        println!("  xdna fdinfo procs: {}", st.xdna_fdinfo.proc_usage.len());
    }
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
