//! Local calendar days for counters that reset at midnight.

pub fn local_day(t: u64) -> u64 {
    local_seconds(t) / 86_400
}

#[cfg(windows)]
pub fn local_seconds(t: u64) -> u64 {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{
        FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime,
    };
    const EPOCH_GAP_SECS: u64 = 11_644_473_600;
    const EMPTY_TIME: SYSTEMTIME = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    let ticks = (t + EPOCH_GAP_SECS) * 10_000_000;
    let utc_file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    // SAFETY: all pointers refer to live local values of the right type.
    unsafe {
        let mut utc = EMPTY_TIME;
        let mut local = EMPTY_TIME;
        let mut local_file = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        if FileTimeToSystemTime(&utc_file, &mut utc) == 0
            || SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0
            || SystemTimeToFileTime(&local, &mut local_file) == 0
        {
            return t;
        }
        let ticks =
            (u64::from(local_file.dwHighDateTime) << 32) | u64::from(local_file.dwLowDateTime);
        (ticks / 10_000_000).saturating_sub(EPOCH_GAP_SECS)
    }
}

#[cfg(not(windows))]
pub fn local_seconds(t: u64) -> u64 {
    t
}
