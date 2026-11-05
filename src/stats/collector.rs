//! Reads the system through `sysinfo` and the platform module.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use sysinfo::{Components, ProcessRefreshKind, ProcessesToUpdate, System};

use super::{platform, Hardware, MemoryStats, ProcessRow, Snapshot, HISTORY};

/// More rows than any window shows; the UI draws what fits.
const PROCESS_ROWS: usize = 24;
/// CPU use is read every tick; everything else changes slowly enough for this.
const SLOW_EVERY: u32 = 4;
/// Reading sensors can be slow on Windows, and temperature changes slowly.
const TEMPERATURE_EVERY: u32 = 20;

pub struct Collector {
    system: Option<System>,
    components: Option<Components>,
    hardware: Hardware,
    history: Vec<VecDeque<f32>>,
    temperature: Option<f32>,
    /// The highest clock seen, because Windows only reports the base clock as
    /// the maximum and a boosting CPU goes past it.
    peak_mhz: u64,
    ticks: u32,
    latest: Snapshot,
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
            peak_mhz: 0,
            ticks: 0,
            latest: Snapshot::default(),
        }
    }

    /// One tick: new CPU use always, the slower values every few ticks.
    pub fn sample(&mut self) -> Snapshot {
        if self.ticks.is_multiple_of(TEMPERATURE_EVERY) {
            self.temperature = self.components.as_mut().and_then(read_temperature);
        }
        if self.ticks.is_multiple_of(SLOW_EVERY) {
            self.read_slow_values();
        }
        self.ticks = self.ticks.wrapping_add(1);

        if let Some(system) = &mut self.system {
            system.refresh_cpu_usage();
            let usage: Vec<f32> = system.cpus().iter().map(sysinfo::Cpu::usage).collect();
            record_history(&mut self.history, &usage);
            self.latest.cpu.history = self.history.iter().map(|core| core.iter().copied().collect()).collect();
            self.latest.sampled_at = Some(Instant::now());
        }
        self.latest.clone()
    }

    fn read_slow_values(&mut self) {
        let latest = &mut self.latest;
        latest.uptime_seconds = System::uptime().ok();
        latest.power = platform::power();
        latest.hardware = self.hardware.clone();
        latest.cpu.temperature = self.temperature;
        let Some(system) = &mut self.system else { return };

        system.refresh_memory();
        let kind = ProcessRefreshKind::nothing().with_cpu().with_memory().without_tasks();
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);

        let (frequency_mhz, rated_mhz) = platform::frequencies(system);
        self.peak_mhz = self.peak_mhz.max(frequency_mhz.unwrap_or(0));
        latest.cpu.frequency_mhz = frequency_mhz;
        latest.cpu.max_frequency_mhz = rated_mhz.map(|rated| rated.max(self.peak_mhz));
        latest.cpu.model = system.cpus().first().and_then(|cpu| clean_cpu_name(cpu.brand()));
        latest.cpu.tasks = Some(system.processes().len());

        let swap_total = system.total_swap();
        latest.memory = (system.total_memory() > 0).then(|| MemoryStats {
            total: system.total_memory(),
            used: system.used_memory(),
            free: system.free_memory(),
            swap_total,
            swap_used: platform::swap_used(swap_total).unwrap_or_else(|| system.used_swap()),
        });
        latest.processes = top_processes(system, system.cpus().len());
    }
}

/// Drop trademark marks, rated speed and core count, which only cost width in
/// a narrow panel. Speed and cores have their own cells.
fn clean_cpu_name(brand: &str) -> Option<String> {
    let name = brand.split(" @ ").next().unwrap_or(brand);
    let text = name.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    let words: Vec<&str> = text
        .split_whitespace()
        .filter(|word| *word != "Processor" && *word != "CPU" && !word.ends_with("-Core"))
        .collect();
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
        let name = clean_cpu_name("AMD Ryzen 9 5900X 12-Core Processor");
        assert_eq!(name.as_deref(), Some("AMD Ryzen 9 5900X"));
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
