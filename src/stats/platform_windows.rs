//! Windows sources. Each is a plain API call that needs no admin rights.
//! WMI is avoided: it can stall for seconds, and `wmic` is gone from Windows 11.

use std::cell::RefCell;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use sysinfo::System;
use windows_sys::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterValue, PdhOpenQueryW,
    PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};
use windows_sys::Win32::System::Power::{
    CallNtPowerInformation, GetSystemPowerStatus, ProcessorInformation, PROCESSOR_POWER_INFORMATION,
    SYSTEM_POWER_STATUS,
};
use windows_sys::Win32::System::SystemInformation::GetSystemFirmwareTable;

use super::{smbios, Hardware, Power};

/// Lets a counter read above 100. From `pdh.h`; missing from the bindings.
const PDH_FMT_NOCAP100: u32 = 0x0000_8000;
/// The firmware table provider for raw SMBIOS data.
const RSMB: u32 = u32::from_be_bytes(*b"RSMB");
/// `RawSMBIOSData` has 8 bytes of header before the table.
const SMBIOS_HEADER: usize = 8;

const BATTERY_FLAG_NONE: u8 = 128;
const BATTERY_FLAG_CHARGING: u8 = 8;
const BATTERY_FLAG_UNKNOWN: u8 = 255;
const PERCENT_UNKNOWN: u8 = 255;
const AC_ONLINE: u8 = 1;

pub fn hardware() -> Hardware {
    // SAFETY: a null buffer with size 0 only asks for the required size.
    let size = unsafe { GetSystemFirmwareTable(RSMB, 0, null_mut(), 0) };
    if size as usize <= SMBIOS_HEADER {
        return Hardware::default();
    }
    let mut buffer = vec![0u8; size as usize];
    // SAFETY: the buffer is `size` bytes long, as passed.
    let written = unsafe { GetSystemFirmwareTable(RSMB, 0, buffer.as_mut_ptr(), size) };
    if written == 0 || written > size {
        return Hardware::default();
    }
    smbios::parse(&buffer[SMBIOS_HEADER..written as usize])
}

pub fn power() -> Option<Power> {
    // SAFETY: the struct is plain data, and the call fills it in.
    let mut status: SYSTEM_POWER_STATUS = unsafe { zeroed() };
    // SAFETY: `status` is a valid, writable struct of the expected type.
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return None;
    }
    if status.BatteryFlag == BATTERY_FLAG_UNKNOWN {
        return None;
    }
    if status.BatteryFlag & BATTERY_FLAG_NONE != 0 {
        return Some(Power::Ac);
    }
    let charging = status.BatteryFlag & BATTERY_FLAG_CHARGING != 0;
    if status.ACLineStatus == AC_ONLINE && !charging {
        return Some(Power::Ac);
    }
    let percent = (status.BatteryLifePercent != PERCENT_UNKNOWN).then_some(status.BatteryLifePercent);
    Some(Power::Battery { percent, charging })
}

/// Live readings from Windows performance counters, the source Task Manager
/// uses. These need two collections before they report anything.
struct Counters {
    query: PDH_HQUERY,
    performance: PDH_HCOUNTER,
    paging: PDH_HCOUNTER,
}

impl Counters {
    fn open() -> Option<Self> {
        let mut query = null_mut();
        // SAFETY: `query` is a valid out pointer, and a null source means live data.
        if unsafe { PdhOpenQueryW(null(), 0, &mut query) } != 0 {
            return None;
        }
        let add = |path: &str| {
            let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
            let mut counter = null_mut();
            // SAFETY: `wide` is NUL-terminated and outlives the call.
            let status = unsafe { PdhAddEnglishCounterW(query, wide.as_ptr(), 0, &mut counter) };
            (status == 0).then_some(counter).unwrap_or(null_mut())
        };
        Some(Self {
            query,
            performance: add("\\Processor Information(_Total)\\% Processor Performance"),
            paging: add("\\Paging File(_Total)\\% Usage"),
        })
    }

    fn read(&self, counter: PDH_HCOUNTER) -> Option<f64> {
        if counter.is_null() {
            return None;
        }
        let mut value = PDH_FMT_COUNTERVALUE::default();
        // Boost runs past 100%, so the default cap must be turned off.
        let format = PDH_FMT_DOUBLE | PDH_FMT_NOCAP100;
        // SAFETY: `counter` came from this query and `value` is a valid out pointer.
        let status = unsafe { PdhGetFormattedCounterValue(counter, format, null_mut(), &mut value) };
        // SAFETY: PDH_FMT_DOUBLE selects the union's double member.
        (status == 0).then(|| unsafe { value.Anonymous.doubleValue })
    }
}

thread_local! {
    /// PDH handles are not `Send`, and only the stats thread uses them.
    static COUNTERS: RefCell<Option<Counters>> = RefCell::new(Counters::open());
}

/// Current and rated clock. The power manager gives the rated clock, but on
/// many CPUs its current value sticks at the base clock, so the live figure is
/// the rated clock scaled by the performance counter. If that counter is
/// unavailable the power manager's own value is used.
pub fn frequencies(system: &mut System) -> (Option<u64>, Option<u64>) {
    let (reported, rated) = power_manager_clock(system);
    let performance = COUNTERS.with_borrow(|counters| {
        let counters = counters.as_ref()?;
        // SAFETY: the query handle is valid for the life of `counters`.
        if unsafe { PdhCollectQueryData(counters.query) } != 0 {
            return None;
        }
        counters.read(counters.performance)
    });
    let live = rated.zip(performance).map(|(mhz, percent)| (mhz as f64 * percent / 100.0) as u64);
    (live.filter(|mhz| *mhz > 0).or(reported), rated)
}

/// Page file use. `sysinfo` estimates this from committed memory, which reads
/// zero whenever commit fits in RAM, even with a page file in use.
pub fn swap_used(total: u64) -> Option<u64> {
    let percent = COUNTERS.with_borrow(|counters| counters.as_ref()?.read(counters.as_ref()?.paging))?;
    Some((total as f64 * percent / 100.0) as u64)
}

fn power_manager_clock(system: &System) -> (Option<u64>, Option<u64>) {
    let cores = system.cpus().len();
    if cores == 0 {
        return (None, None);
    }
    // SAFETY: the struct is plain data, and the call fills it in.
    let mut info: Vec<PROCESSOR_POWER_INFORMATION> = (0..cores).map(|_| unsafe { zeroed() }).collect();
    let bytes = (cores * size_of::<PROCESSOR_POWER_INFORMATION>()) as u32;
    // SAFETY: the buffer holds `bytes` bytes, as passed.
    let status = unsafe { CallNtPowerInformation(ProcessorInformation, null(), 0, info.as_mut_ptr().cast(), bytes) };
    if status != 0 {
        return (None, None);
    }
    let mean = |pick: fn(&PROCESSOR_POWER_INFORMATION) -> u32| {
        let total: u64 = info.iter().map(|core| u64::from(pick(core))).sum();
        (total > 0).then(|| total / cores as u64)
    };
    (mean(|core| core.CurrentMhz), mean(|core| core.MaxMhz))
}
