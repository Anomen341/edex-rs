//! System statistics, gathered on a background thread and sent to the UI.
//!
//! Every value that can be missing is an `Option`, and the UI shows N/A for
//! `None`. Nothing here may panic on missing data, and the UI never waits on it.

mod collector;
pub mod format;
#[cfg(target_os = "linux")]
#[path = "platform_linux.rs"]
mod platform;
#[cfg(windows)]
#[path = "platform_windows.rs"]
mod platform;
#[cfg(any(windows, test))]
mod smbios;

use std::sync::mpsc::{sync_channel, Receiver, TrySendError};
use std::thread;
use std::time::{Duration, Instant};

/// How often CPU use is sampled. Fast enough that the graphs feel live, and
/// above the 200 ms sysinfo needs between reads.
pub const CPU_INTERVAL: Duration = Duration::from_millis(250);
/// Samples kept per core: one minute.
pub const HISTORY: usize = 240;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    /// On mains, or a machine with no battery.
    Ac,
    Battery { percent: Option<u8>, charging: bool },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hardware {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub chassis: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CpuStats {
    pub model: Option<String>,
    /// Usage from 0 to 100 for each core, oldest sample first.
    pub history: Vec<Vec<f32>>,
    pub temperature: Option<f32>,
    pub frequency_mhz: Option<u64>,
    pub max_frequency_mhz: Option<u64>,
    pub tasks: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryStats {
    pub total: u64,
    pub used: u64,
    /// Memory that is neither in use nor cache.
    pub free: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    /// Share of the whole machine, so it can never pass 100.
    pub cpu: f32,
    pub memory: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub uptime_seconds: Option<u64>,
    pub power: Option<Power>,
    pub hardware: Hardware,
    pub cpu: CpuStats,
    pub memory: Option<MemoryStats>,
    pub processes: Vec<ProcessRow>,
    /// When the newest CPU sample was taken, for smooth graph scrolling.
    pub sampled_at: Option<Instant>,
}

/// Start collecting. The channel holds one snapshot, so a UI that stops
/// drawing costs nothing: samples are dropped rather than queued.
pub fn spawn() -> Receiver<Snapshot> {
    let (sender, receiver) = sync_channel(1);
    let spawned = thread::Builder::new().name("stats".to_owned()).spawn(move || {
        let mut collector = collector::Collector::new();
        loop {
            let started = Instant::now();
            let snapshot = collector.sample();
            if let Err(TrySendError::Disconnected(_)) = sender.try_send(snapshot) {
                return;
            }
            thread::sleep(CPU_INTERVAL.saturating_sub(started.elapsed()));
        }
    });
    // With no thread the sender is dropped, and the UI shows N/A everywhere.
    drop(spawned);
    receiver
}

/// `None` for the filler text firmware vendors leave in these fields.
fn clean_field(text: &str) -> Option<String> {
    const FILLER: [&str; 15] = [
        "",
        "0",
        "default string",
        "invalid",
        "n/a",
        "none",
        "not applicable",
        "not available",
        "not specified",
        "o.e.m.",
        "oem",
        "system manufacturer",
        "system product name",
        "to be filled by o.e.m.",
        "unknown",
    ];
    let text = text.trim();
    let filler = FILLER.contains(&text.to_ascii_lowercase().as_str());
    (!filler).then(|| text.to_owned())
}

/// The everyday name for vendors whose legal names are too long for the panel.
fn short_vendor(name: String) -> String {
    const KNOWN: [(&str, &str); 8] = [
        ("micro-star", "MSI"),
        ("asustek", "ASUS"),
        ("gigabyte", "Gigabyte"),
        ("hewlett-packard", "HP"),
        ("dell", "Dell"),
        ("lenovo", "Lenovo"),
        ("asrock", "ASRock"),
        ("acer", "Acer"),
    ];
    let lower = name.to_ascii_lowercase();
    KNOWN
        .iter()
        .find(|(prefix, _)| lower.starts_with(prefix))
        .map_or(name, |(_, short)| (*short).to_owned())
}

/// The name for an SMBIOS chassis type number.
fn chassis_name(code: u8) -> Option<String> {
    let name = match code & 0x7F {
        3 => "Desktop",
        4 => "Low Profile",
        5 => "Pizza Box",
        6 => "Mini Tower",
        7 => "Tower",
        8 => "Portable",
        9 => "Laptop",
        10 => "Notebook",
        11 => "Handheld",
        12 => "Docking Station",
        13 => "All in One",
        14 => "Sub Notebook",
        15 => "Space-saving",
        16 => "Lunch Box",
        17 => "Server",
        23 => "Rack Mount",
        24 => "Sealed PC",
        28 => "Blade",
        30 => "Tablet",
        31 => "Convertible",
        32 => "Detachable",
        33 => "IoT Gateway",
        34 => "Embedded PC",
        35 => "Mini PC",
        36 => "Stick PC",
        _ => return None,
    };
    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_filler_counts_as_missing() {
        assert_eq!(clean_field("  To Be Filled By O.E.M. "), None);
        assert_eq!(clean_field("Default string"), None);
        assert_eq!(clean_field(""), None);
        assert_eq!(clean_field(" ASUSTeK "), Some("ASUSTeK".to_owned()));
    }

    #[test]
    fn long_vendor_names_are_shortened() {
        assert_eq!(short_vendor("Micro-Star International Co., Ltd.".to_owned()), "MSI");
        assert_eq!(short_vendor("ASUSTeK COMPUTER INC.".to_owned()), "ASUS");
        assert_eq!(short_vendor("LENOVO".to_owned()), "Lenovo");
        assert_eq!(short_vendor("IP3 Tech".to_owned()), "IP3 Tech");
    }

    #[test]
    fn chassis_codes_map_to_names() {
        assert_eq!(chassis_name(3).as_deref(), Some("Desktop"));
        assert_eq!(chassis_name(10 | 0x80).as_deref(), Some("Notebook"));
        assert_eq!(chassis_name(2), None);
        assert_eq!(chassis_name(200), None);
    }

    #[test]
    fn a_real_snapshot_arrives_without_blocking_the_receiver() {
        let receiver = spawn();
        let start = Instant::now();
        assert!(receiver.try_recv().is_err() || start.elapsed() < Duration::from_millis(50));
        let snapshot = receiver.recv_timeout(Duration::from_secs(5)).expect("first sample");
        assert!(!snapshot.cpu.history.is_empty());
        assert!(snapshot.memory.is_some_and(|m| m.total > 0));
        assert!(!snapshot.processes.is_empty());
    }
}
