//! Reads the system through `sysinfo` and the platform module.

use std::collections::VecDeque;
use std::time::Duration;

use sysinfo::{Components, ProcessRefreshKind, ProcessesToUpdate, System};

use super::{platform, CpuStats, Hardware, MemoryStats, ProcessRow, Snapshot, HISTORY};

/// More rows than any window shows; the UI draws what fits.
const PROCESS_ROWS: usize = 24;
/// Reading sensors can be slow on Windows, and temperature changes slowly.
const TEMPERATURE_EVERY: u32 = 5;

pub struct Collector {
    system: Option<System>,
    components: Option<Components>,
    hardware: Hardware,
    history: Vec<VecDeque<f32>>,
    temperature: Option<f32>,
    samples: u32,
}

impl Collector {
    pub fn new() -> Self {
        let mut system = System::new().ok();
        if let Some(system) = &mut system {
            system.refresh_cpu_all();
            // Usage is measured between two reads, so the first needs a gap.
            std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(200)));
        }
        Self {
            system,
            components: Components::new_with_refreshed_list().ok(),
            hardware: platform::hardware(),
            history: Vec::new(),
            temperature: None,
            samples: 0,
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        if self.samples.is_multiple_of(TEMPERATURE_EVERY) {
            self.temperature = self.components.as_mut().and_then(read_temperature);
        }
        self.samples = self.samples.wrapping_add(1);

        let mut snapshot = Snapshot {
            uptime_seconds: System::uptime().ok(),
            power: platform::power(),
            hardware: self.hardware.clone(),
            ..Snapshot::default()
        };
        let Some(system) = &mut self.system else { return snapshot };

        system.refresh_cpu_all();
        system.refresh_memory();
        let kind = ProcessRefreshKind::nothing().with_cpu().with_memory().without_tasks();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);

        let usage: Vec<f32> = system.cpus().iter().map(sysinfo::Cpu::usage).collect();
        record_history(&mut self.history, &usage);

        let (frequency_mhz, max_frequency_mhz) = platform::frequencies(system);
        snapshot.cpu = CpuStats {
            model: system.cpus().first().and_then(|cpu| clean_cpu_name(cpu.brand())),
            history: self.history.iter().map(|core| core.iter().copied().collect()).collect(),
            temperature: self.temperature,
            frequency_mhz,
            max_frequency_mhz,
            tasks: Some(system.processes().len()),
        };
        snapshot.memory = (system.total_memory() > 0).then(|| MemoryStats {
            total: system.total_memory(),
            used: system.used_memory(),
            free: system.free_memory(),
            swap_total: system.total_swap(),
            swap_used: system.used_swap(),
        });
        snapshot.processes = top_processes(system, usage.len());
        snapshot
    }
}

/// Drop trademark marks and the rated speed, which only cost width in a narrow
/// panel. The speed has its own cell.
fn clean_cpu_name(brand: &str) -> Option<String> {
    let name = brand.split(" @ ").next().unwrap_or(brand);
    let text = name.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    let words: Vec<&str> = text.split_whitespace().collect();
    (!words.is_empty()).then(|| words.join(" "))
}

fn record_history(history: &mut Vec<VecDeque<f32>>, usage: &[f32]) {
    history.resize_with(usage.len(), VecDeque::new);
    for (core, value) in history.iter_mut().zip(usage) {
        if core.len() == HISTORY {
            core.pop_front();
        }
        core.push_back(value.clamp(0.0, 100.0));
    }
}

fn top_processes(system: &System, cores: usize) -> Vec<ProcessRow> {
    let cores = cores.max(1) as f32;
    let total = system.total_memory().max(1) as f32;
    let rows = system
        .processes()
        .values()
        // Pid 0 is the idle process on Windows, not real work.
        .filter(|process| process.pid().as_u32() != 0)
        .map(|process| ProcessRow {
            pid: process.pid().as_u32(),
            name: process.name().to_string_lossy().into_owned(),
            cpu: process.cpu_usage() / cores,
            memory: process.memory() as f32 / total * 100.0,
        })
        .collect();
    rank_processes(rows)
}

fn rank_processes(mut rows: Vec<ProcessRow>) -> Vec<ProcessRow> {
    rows.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.memory.total_cmp(&a.memory)).then(a.pid.cmp(&b.pid)));
    rows.truncate(PROCESS_ROWS);
    rows
}

fn read_temperature(components: &mut Components) -> Option<f32> {
    components.refresh(true);
    let readings = components.iter().map(|c| (c.label().to_owned(), c.temperature()));
    pick_cpu_temperature(readings)
}

/// The best CPU reading. Sensors from other parts, such as a drive, are never
/// used in its place: showing N/A beats showing the wrong component.
fn pick_cpu_temperature(readings: impl Iterator<Item = (String, Option<f32>)>) -> Option<f32> {
    const PREFERRED: [&str; 6] = ["package", "tctl", "tdie", "cpu", "coretemp", "k10temp"];
    let valid: Vec<(String, f32)> = readings
        .filter_map(|(label, value)| Some((label.to_ascii_lowercase(), value?)))
        .filter(|(_, value)| (1.0..=130.0).contains(value))
        .collect();
    PREFERRED
        .iter()
        .find_map(|word| valid.iter().find(|(label, _)| label.contains(word)))
        .map(|(_, value)| *value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(label: &str, value: f32) -> (String, Option<f32>) {
        (label.to_owned(), Some(value))
    }

    #[test]
    fn package_temperature_wins_over_single_cores() {
        let list = vec![reading("coretemp Core 0", 40.0), reading("coretemp Package id 0", 55.0)];
        assert_eq!(pick_cpu_temperature(list.into_iter()), Some(55.0));
    }

    #[test]
    fn other_sensors_never_stand_in_for_the_cpu() {
        let list = vec![reading("nvme Composite", 44.0), reading("acpitz temp1", 30.0)];
        assert_eq!(pick_cpu_temperature(list.into_iter()), None);
    }

    #[test]
    fn nonsense_and_missing_readings_are_ignored() {
        let list = vec![reading("CPU", 0.0), reading("CPU 2", 900.0), ("CPU 3".to_owned(), None)];
        assert_eq!(pick_cpu_temperature(list.into_iter()), None);
        assert_eq!(pick_cpu_temperature(std::iter::empty()), None);
    }

    #[test]
    fn cpu_names_lose_their_trademark_marks() {
        let name = clean_cpu_name("Intel(R) Celeron(R)  N5095A @ 2.00GHz ");
        assert_eq!(name.as_deref(), Some("Intel Celeron N5095A"));
        assert_eq!(clean_cpu_name("   "), None);
    }

    #[test]
    fn history_is_capped_and_clamped() {
        let mut history = Vec::new();
        for i in 0..HISTORY + 10 {
            record_history(&mut history, &[i as f32, 150.0]);
        }
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].len(), HISTORY);
        assert_eq!(history[0][0], 10.0);
        assert_eq!(history[1][0], 100.0);
    }

    #[test]
    fn history_follows_a_change_in_core_count() {
        let mut history = Vec::new();
        record_history(&mut history, &[1.0, 2.0, 3.0]);
        record_history(&mut history, &[4.0]);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn busiest_processes_come_first_and_the_list_is_capped() {
        let row = |pid, cpu, memory| ProcessRow { pid, name: String::new(), cpu, memory };
        let ranked = rank_processes(vec![row(1, 1.0, 9.0), row(2, 5.0, 1.0), row(3, 1.0, 20.0)]);
        assert_eq!(ranked.iter().map(|r| r.pid).collect::<Vec<_>>(), [2, 3, 1]);
        let many = (0..100).map(|i| row(i, 0.0, 0.0)).collect();
        assert_eq!(rank_processes(many).len(), PROCESS_ROWS);
    }

    #[test]
    fn nan_usage_cannot_panic_the_sort() {
        let row = |pid, cpu| ProcessRow { pid, name: String::new(), cpu, memory: 0.0 };
        assert_eq!(rank_processes(vec![row(1, f32::NAN), row(2, 1.0)]).len(), 2);
    }

    #[test]
    fn collector_reads_this_machine() {
        let mut collector = Collector::new();
        let snapshot = collector.sample();
        assert!(snapshot.uptime_seconds.is_some());
        assert!(snapshot.cpu.history.iter().flatten().all(|usage| (0.0..=100.0).contains(usage)));
        assert!(snapshot.cpu.tasks.is_some_and(|t| t > 0));
        assert!(!snapshot.cpu.history.is_empty());
        assert!(snapshot.processes.iter().all(|p| p.cpu >= 0.0 && p.memory >= 0.0));
    }
}
