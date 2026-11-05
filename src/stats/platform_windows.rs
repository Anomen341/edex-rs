//! Windows sources. Each is a plain API call that needs no admin rights.
//! WMI is avoided: it can stall for seconds, and `wmic` is gone from Windows 11.

use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use sysinfo::System;
use windows_sys::Win32::System::Power::{
    CallNtPowerInformation, GetSystemPowerStatus, ProcessorInformation, PROCESSOR_POWER_INFORMATION,
    SYSTEM_POWER_STATUS,
};
use windows_sys::Win32::System::SystemInformation::GetSystemFirmwareTable;

use super::{smbios, Hardware, Power};

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

/// Current and maximum clock from the power manager, averaged over cores.
pub fn frequencies(system: &mut System) -> (Option<u64>, Option<u64>) {
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
