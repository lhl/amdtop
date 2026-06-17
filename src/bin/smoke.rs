// Non-TUI smoke test: sample telemetry once and print, to verify the data
// accessors work on real hardware without fighting the terminal.
use std::time::Duration;
use libamdgpu_top::app::{AppAmdgpuTop, AppOption};
use libamdgpu_top::DevicePath;

fn main() {
    let mut dps = DevicePath::get_device_path_list();
    for dp in dps.iter_mut() { dp.fill_amdgpu_device_name(); }
    println!("found {} device paths", dps.len());
    let (mut apps, suspended) = AppAmdgpuTop::create_app_and_suspended_list(&dps, &AppOption::default());
    println!("{} active apps, {} suspended", apps.len(), suspended.len());

    for app in apps.iter_mut() {
        app.update(Duration::from_millis(500));
        let st = &app.stat;
        let dp = &app.device_path;
        let kind = if dp.is_xdna() { "NPU" } else { "GPU" };
        let v = &st.vram_usage.0.vram;
        println!("\n[{kind}] {} | pci bus {}", dp.menu_entry(), dp.pci.bus);
        println!("  gfx%: {:?}", st.activity.gfx);
        println!("  vram: {}M / {}M", v.heap_usage >> 20, v.usable_heap_size >> 20);
        if let Some(s) = st.sensors.as_ref() {
            let t = s.junction_temp.as_ref().or(s.edge_temp.as_ref());
            println!("  temp: {:?}", t.map(|t| t.current));
            println!("  avg power: {:?} W", s.average_power.as_ref().map(|p| p.value));
            println!("  power cap: {:?}", s.power_cap.as_ref().map(|c| c.current));
            println!("  sclk: {:?} mclk: {:?}", s.sclk, s.mclk);
            println!("  fan: {:?}", s.fan_rpm);
        } else {
            println!("  sensors: None");
        }
        println!("  fdinfo procs: {}", st.fdinfo.proc_usage.len());
        for pu in st.fdinfo.proc_usage.iter().take(5) {
            println!("    pid={} name={} vram={}M gfx={} cpu={}",
                pu.pid, pu.name, pu.usage.vram_usage >> 10, pu.usage.gfx, pu.usage.cpu);
        }
        // NPU fdinfo
        println!("  xdna fdinfo procs: {}", st.xdna_fdinfo.proc_usage.len());
    }
}
