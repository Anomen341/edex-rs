//! Linux sources: the kernel's sysfs files. All are readable without root.

use std::fs;
use std::path::Path;

use sysinfo::System;

use super::{chassis_name, clean_field, short_vendor, Hardware, Power};

const DMI: &str = "/sys/class/dmi/id";
const POWER_SUPPLY: &str = "/sys/class/power_supply";
const CPU0_MAX_KHZ: &str = "/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq";

pub fn hardware() -> Hardware {
    read_hardware(Path::new(DMI))
}

fn read_hardware(dir: &Path) -> Hardware {
    let field = |name: &str| fs::read_to_string(dir.join(name)).ok().and_then(|text| clean_field(&text));
    Hardware {
        manufacturer: field("sys_vendor").map(short_vendor),
        model: field("product_name"),
        chassis: field("chassis_type").and_then(|text| text.parse().ok()).and_then(chassis_name),
    }
}

pub fn power() -> Option<Power> {
    read_power(Path::new(POWER_SUPPLY))
}

/// A machine with no system battery is on mains. A mouse's battery also
/// appears here, marked with the scope `Device`, and doesn't count.
fn read_power(dir: &Path) -> Option<Power> {
    let read = |path: &Path, name: &str| fs::read_to_string(path.join(name)).ok().map(|text| text.trim().to_owned());
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let is_battery = read(&path, "type").as_deref() == Some("Battery");
        let is_device = read(&path, "scope").as_deref() == Some("Device");
        if !is_battery || is_device {
            continue;
        }
        let status = read(&path, "status").unwrap_or_default();
        if status == "Full" || status == "Not charging" {
            return Some(Power::Ac);
        }
        return Some(Power::Battery {
            percent: read(&path, "capacity").and_then(|text| text.parse().ok()),
            charging: status == "Charging",
        });
    }
    Some(Power::Ac)
}

/// Current clock from the kernel, averaged over cores, and the rated maximum.
pub fn frequencies(system: &mut System) -> (Option<u64>, Option<u64>) {
    system.refresh_cpu_frequency();
    let cpus = system.cpus();
    let total: u64 = cpus.iter().map(sysinfo::Cpu::frequency).sum();
    let current = (total > 0).then(|| total / cpus.len() as u64);
    let max = fs::read_to_string(CPU0_MAX_KHZ)
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .map(|khz| khz / 1000);
    (current, max)
}

/// `sysinfo` reads swap accurately here.
pub fn swap_used(_total: u64) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("edex-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, path: &str, text: &str) {
            let file = self.0.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn hardware_reads_dmi_and_drops_filler() {
        let dir = TempDir::new("dmi");
        dir.write("sys_vendor", "LENOVO\n");
        dir.write("product_name", "To Be Filled By O.E.M.\n");
        dir.write("chassis_type", "10\n");
        let hardware = read_hardware(&dir.0);
        assert_eq!(hardware.manufacturer.as_deref(), Some("Lenovo"));
        assert_eq!(hardware.model, None);
        assert_eq!(hardware.chassis.as_deref(), Some("Notebook"));
    }

    #[test]
    fn missing_dmi_gives_empty_hardware() {
        assert_eq!(read_hardware(Path::new("/nonexistent")), Hardware::default());
    }

    #[test]
    fn no_battery_means_mains() {
        let dir = TempDir::new("nobat");
        dir.write("AC/type", "Mains\n");
        assert_eq!(read_power(&dir.0), Some(Power::Ac));
    }

    #[test]
    fn battery_states_are_read() {
        let dir = TempDir::new("bat");
        dir.write("BAT0/type", "Battery\n");
        dir.write("BAT0/capacity", "42\n");
        dir.write("BAT0/status", "Discharging\n");
        assert_eq!(read_power(&dir.0), Some(Power::Battery { percent: Some(42), charging: false }));
        dir.write("BAT0/status", "Charging\n");
        assert_eq!(read_power(&dir.0), Some(Power::Battery { percent: Some(42), charging: true }));
        dir.write("BAT0/status", "Full\n");
        assert_eq!(read_power(&dir.0), Some(Power::Ac));
    }

    #[test]
    fn a_wireless_mouse_is_not_the_system_battery() {
        let dir = TempDir::new("mouse");
        dir.write("hid-mouse/type", "Battery\n");
        dir.write("hid-mouse/scope", "Device\n");
        dir.write("hid-mouse/capacity", "5\n");
        assert_eq!(read_power(&dir.0), Some(Power::Ac));
    }

    #[test]
    fn unreadable_power_directory_is_unknown_not_a_crash() {
        assert_eq!(read_power(Path::new("/nonexistent")), None);
    }
}
