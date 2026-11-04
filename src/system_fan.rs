//! System fans that live outside the amdgpu driver's hwmon device.
//!
//! APUs have no fan controller wired to the display engine, so the driver
//! exposes neither `pwm1` nor `fan1_input` for them: on a Strix Halo board the
//! fan that cools the package is driven by the platform embedded controller,
//! which registers a hwmon device of its own (`cros_ec` on Framework hardware).

use std::fs;
use std::path::{Path, PathBuf};

const HWMON_ROOT: &str = "/sys/class/hwmon";
const AMDGPU_HWMON_NAME: &str = "amdgpu";

/// hwmon drivers known to expose the fan that cools the CPU/APU package.
///
/// The names are what each driver registers with the hwmon core, which is
/// what `/sys/class/hwmon/hwmonN/name` returns: the thinkpad-acpi platform
/// driver is called `thinkpad_hwmon`, but its sensor is called `thinkpad`.
///
/// This list only breaks ties. When exactly one non-GPU sensor has a
/// tachometer it is unambiguous, even if its driver is not listed here, so
/// unknown embedded controllers keep working without a code change.
const SYSTEM_FAN_DRIVERS: &[&str] = &[
    "cros_ec",  // Framework and other ChromiumOS-EC machines
    "asusec",   // ASUS laptops with an EC fan channel (asus-ec-sensors)
    "dell_smm", // Dell laptops via SMM (dell-smm-hwmon)
    "thinkpad", // Lenovo ThinkPads via thinkpad-acpi
];

/// The generic ACPI fan device (`drivers/acpi/fan_hwmon.c`), present on many
/// laptops and often reading 0 while the real fan spins on another sensor, so
/// it ranks behind every other candidate and is only trusted when it alone
/// reports a spinning tachometer.
const ACPI_FAN_DRIVER: &str = "acpi_fan";

/// A platform fan sensor, discovered from the hwmon devices outside amdgpu.
#[derive(Clone, Debug)]
pub struct SystemFan {
    hwmon_path: PathBuf,
}

impl SystemFan {
    /// Find the sensor that represents the system fan, if any.
    pub fn discover() -> Option<Self> {
        let sensor = select_fan_sensor(read_hwmon_sensors(Path::new(HWMON_ROOT)))?;
        Some(Self {
            hwmon_path: sensor.hwmon_path,
        })
    }

    pub fn read_rpm(&self) -> Option<u32> {
        fan_rpm(&read_fan_inputs(&self.hwmon_path))
    }
}

/// One `/sys/class/hwmon/hwmonN` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HwmonSensor {
    hwmon_path: PathBuf,
    driver: String,
    has_fan: bool,
    /// Measured speed from this device's tachometers, if any is readable.
    rpm: Option<u32>,
}

/// Decide which hwmon device is the system fan.
fn select_fan_sensor(sensors: Vec<HwmonSensor>) -> Option<HwmonSensor> {
    let candidates: Vec<HwmonSensor> = sensors
        .into_iter()
        .filter(|sensor| sensor.has_fan && sensor.driver != AMDGPU_HWMON_NAME)
        .collect();

    // `acpi_fan` shadows the real fan on many laptops and typically sits at
    // zero, so set it aside unless it is the machine's only tachometer.
    let (acpi_fan, others): (Vec<HwmonSensor>, Vec<HwmonSensor>) = candidates
        .into_iter()
        .partition(|sensor| sensor.driver == ACPI_FAN_DRIVER);

    if others.is_empty() {
        // Report `acpi_fan` alone only when its tachometer actually spins;
        // a parked or unpopulated one would claim a fan that is not there.
        return acpi_fan
            .into_iter()
            .find(|sensor| sensor.rpm.unwrap_or(0) > 0);
    }

    if let Some(sensor) = others
        .iter()
        .find(|sensor| SYSTEM_FAN_DRIVERS.contains(&sensor.driver.as_str()))
    {
        return Some(sensor.clone());
    }

    // Otherwise a lone tachometer can only be the platform fan. With two or
    // more, one of them may be a chassis or drive fan, so refuse to guess.
    let mut others = others.into_iter();
    match (others.next(), others.next()) {
        (Some(sensor), None) => Some(sensor),
        _ => None,
    }
}

/// The measured speed, from one hwmon device's `fanN_input` values. Labels,
/// faults, limits, targets, and everything else are ignored, whatever a caller
/// hands in.
fn fan_rpm(readings: &[(String, String)]) -> Option<u32> {
    // Controllers expose a fixed number of channels and leave the unwired ones
    // at zero, so the fastest spinning tachometer is the installed fan.
    readings
        .iter()
        .filter(|(name, _)| is_fan_input(name))
        .filter_map(|(_, raw)| raw.trim().parse().ok())
        .max()
}

/// Match `fanN_input` only, so `fanN_label`, `fanN_fault`, `fanN_max`, and
/// `pwmN` are not mistaken for a tachometer.
fn is_fan_input(name: &str) -> bool {
    let Some(number) = name
        .strip_suffix("_input")
        .and_then(|stem| stem.strip_prefix("fan"))
    else {
        return false;
    };
    !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
}

fn read_hwmon_sensors(root: &Path) -> Vec<HwmonSensor> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut sensors = entries
        .flatten()
        .filter_map(|entry| {
            let hwmon_path = entry.path();
            let driver = fs::read_to_string(hwmon_path.join("name"))
                .ok()?
                .trim()
                .to_string();
            let rpm = fan_rpm(&read_fan_inputs(&hwmon_path));
            Some(HwmonSensor {
                has_fan: hwmon_path.join("fan1_input").is_file(),
                hwmon_path,
                driver,
                rpm,
            })
        })
        .collect::<Vec<_>>();

    // Probe order, so the choice does not depend on directory enumeration.
    sensors.sort_by(|left, right| left.hwmon_path.cmp(&right.hwmon_path));
    sensors
}

fn read_fan_inputs(hwmon_path: &Path) -> Vec<(String, String)> {
    let Ok(entries) = fs::read_dir(hwmon_path) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let raw = is_fan_input(&name).then(|| fs::read_to_string(entry.path()).ok())??;
            Some((name, raw))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{HwmonSensor, SYSTEM_FAN_DRIVERS, fan_rpm, is_fan_input, select_fan_sensor};

    fn sensor(driver: &str, has_fan: bool) -> HwmonSensor {
        HwmonSensor {
            hwmon_path: format!("/sys/class/hwmon/{driver}").into(),
            driver: driver.to_string(),
            has_fan,
            rpm: None,
        }
    }

    fn acpi_fan(rpm: u32) -> HwmonSensor {
        HwmonSensor {
            rpm: Some(rpm),
            ..sensor("acpi_fan", true)
        }
    }

    fn acpi_fan_unreadable() -> HwmonSensor {
        sensor("acpi_fan", true)
    }

    fn driver_of(selected: Option<HwmonSensor>) -> Option<String> {
        selected.map(|sensor| sensor.driver)
    }

    fn reading(name: &str, value: &str) -> (String, String) {
        (name.to_string(), value.to_string())
    }

    #[test]
    fn amdgpu_tachometers_are_never_the_system_fan() {
        // A discrete card reports its own fan; nothing else is available.
        assert_eq!(
            driver_of(select_fan_sensor(vec![sensor("amdgpu", true)])),
            None
        );
        assert_eq!(
            driver_of(select_fan_sensor(vec![
                sensor("amdgpu", true),
                sensor("k10temp", false),
            ])),
            None
        );
    }

    #[test]
    fn sensors_without_a_tachometer_are_ignored() {
        assert_eq!(
            driver_of(select_fan_sensor(vec![
                sensor("k10temp", false),
                sensor("nvme", false),
            ])),
            None
        );
    }

    #[test]
    fn a_single_fan_sensor_is_used_even_when_the_driver_is_unknown() {
        // APUs such as Strix Halo reach this path: amdgpu has no fan, and the
        // platform controller is the only remaining tachometer.
        assert_eq!(
            driver_of(select_fan_sensor(vec![
                sensor("amdgpu", false),
                sensor("some_future_ec", true),
            ])),
            Some("some_future_ec".into())
        );
    }

    #[test]
    fn a_known_embedded_controller_wins_over_other_fan_sensors() {
        for driver in SYSTEM_FAN_DRIVERS {
            assert_eq!(
                driver_of(select_fan_sensor(vec![
                    sensor("amdgpu", false),
                    sensor("nct6775", true),
                    sensor(driver, true),
                ])),
                Some((*driver).into()),
                "{driver} should be preferred"
            );
        }
    }

    #[test]
    fn ambiguous_fan_sensors_are_not_guessed() {
        // Two unlisted fan sensors could be a chassis or drive fan, and either
        // one could be wrong, so report no system fan at all.
        assert_eq!(
            driver_of(select_fan_sensor(vec![
                sensor("nct6775", true),
                sensor("w83793", true),
            ])),
            None
        );
    }

    #[test]
    fn the_spinning_fan_is_reported_over_empty_ec_headers() {
        // Framework Desktop exposes three tachometers and populates one.
        let readings = [
            reading("fan1_input", "1410\n"),
            reading("fan2_input", "0\n"),
            reading("fan3_input", "0\n"),
        ];
        assert_eq!(fan_rpm(&readings), Some(1410));
    }

    #[test]
    fn a_parked_fan_reads_as_zero() {
        let readings = [reading("fan1_input", "0\n"), reading("fan2_input", "0")];
        assert_eq!(fan_rpm(&readings), Some(0));
    }

    #[test]
    fn unusable_fan_values_produce_no_reading() {
        assert_eq!(fan_rpm(&[]), None);
        let readings = [
            reading("fan1_input", "\n"),
            reading("fan2_input", "n/a\n"),
            // A target speed is not a measurement, and the controller's own
            // thermal policy is invisible to the host.
            reading("fan1_target", "926\n"),
        ];
        assert_eq!(fan_rpm(&readings), None);
    }

    #[test]
    fn driver_names_match_what_the_kernel_registers() {
        // The names are read from the hwmon device's `name` attribute, which
        // is what the driver passes to devm_hwmon_device_register_with_*();
        // the platform driver's own name (thinkpad_hwmon, dell_smm_hwmon) is
        // not it, and msi_ec registers no hwmon device at all.
        assert_eq!(
            SYSTEM_FAN_DRIVERS,
            &["cros_ec", "asusec", "dell_smm", "thinkpad"]
        );
    }

    #[test]
    fn a_real_thinkpad_fan_wins_over_the_generic_acpi_fan() {
        // Krackan Point ThinkPad: hwmon6 is acpi_fan reading 0 while the fan
        // spins at 3978 RPM on hwmon9 (thinkpad).
        assert_eq!(
            driver_of(select_fan_sensor(vec![
                sensor("amdgpu", false),
                acpi_fan(0),
                sensor("thinkpad", true),
            ])),
            Some("thinkpad".into())
        );
    }

    #[test]
    fn acpi_fan_ranks_last_behind_other_fan_sensors() {
        assert_eq!(
            driver_of(select_fan_sensor(vec![
                acpi_fan(1200),
                sensor("some_future_ec", true)
            ])),
            Some("some_future_ec".into())
        );
    }

    #[test]
    fn a_spinning_acpi_fan_is_used_when_it_is_the_only_candidate() {
        assert_eq!(
            driver_of(select_fan_sensor(vec![
                sensor("amdgpu", false),
                acpi_fan(2400)
            ])),
            Some("acpi_fan".into())
        );
    }

    #[test]
    fn a_stopped_acpi_fan_is_not_reported_when_alone() {
        // Many laptops expose acpi_fan with an unpopulated tachometer; showing
        // `fan 0RPM` next to a spinning (or parked-but-real) fan is a lie.
        assert_eq!(driver_of(select_fan_sensor(vec![acpi_fan(0)])), None);
    }

    #[test]
    fn an_unreadable_acpi_fan_is_not_reported_when_alone() {
        assert_eq!(
            driver_of(select_fan_sensor(vec![acpi_fan_unreadable()])),
            None
        );
    }

    #[test]
    fn only_tachometer_attributes_are_read() {
        assert!(is_fan_input("fan1_input"));
        assert!(is_fan_input("fan12_input"));
        assert!(!is_fan_input("fan1_label"));
        assert!(!is_fan_input("fan1_fault"));
        assert!(!is_fan_input("fan1_max"));
        assert!(!is_fan_input("fan1_target"));
        assert!(!is_fan_input("pwm1_input"));
        assert!(!is_fan_input("fan_input"));
        assert!(!is_fan_input("in1_input"));
    }
}
