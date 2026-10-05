//! Read-only readiness facts. Unknown is not equivalent to false or zero.
pub use crate::model::{PowerReadiness, Probe, Readiness, VolumeReadiness};

#[cfg(windows)]
#[path = "readiness/windows.rs"]
mod windows;

/// Collect independent native facts without creating state or writing a journal.
/// Unsupported platforms return all Unknown. No paths or error details escape.
pub fn collect() -> Readiness {
    #[cfg(windows)]
    {
        windows::collect()
    }
    #[cfg(not(windows))]
    {
        Readiness::default()
    }
}

#[cfg(any(windows, test))]
fn probe<T>(value: Option<T>) -> Probe<T> {
    value.map_or(Probe::Unknown, Probe::Known)
}

#[cfg(any(windows, test))]
fn collect_with(
    system: impl FnOnce() -> Option<VolumeReadiness>,
    journal: impl FnOnce() -> Option<VolumeReadiness>,
    power: impl FnOnce() -> Option<PowerReadiness>,
    reboot: impl FnOnce() -> Option<bool>,
) -> Readiness {
    Readiness {
        system_volume: probe(system()),
        journal_volume: probe(journal()),
        power: probe(power()),
        windows_update_reboot: probe(reboot()),
    }
}

// The worker owns all native state and drops it before publishing its result.
// A timeout retains the receiver, so repeated requests cannot accumulate workers.
#[cfg(any(windows, test))]
fn bounded_probe(
    worker: &std::sync::Mutex<Option<std::sync::mpsc::Receiver<Option<bool>>>>,
    timeout: std::time::Duration,
    query: impl FnOnce() -> Option<bool> + Send + 'static,
) -> Option<bool> {
    use std::sync::mpsc;
    let mut slot = worker.try_lock().ok()?;
    if let Some(receiver) = slot.as_ref() {
        match receiver.try_recv() {
            Err(mpsc::TryRecvError::Empty) => return None,
            // Discard a stale result, including Known(false), then query anew.
            _ => *slot = None,
        }
    }
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("readiness-com".into())
        .spawn(move || {
            let value = query();
            let _ = sender.send(value);
        })
        .ok()?;
    *slot = Some(receiver);
    match slot.as_ref()?.recv_timeout(timeout) {
        Ok(value) => {
            *slot = None;
            value
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            *slot = None;
            None
        }
        Err(mpsc::RecvTimeoutError::Timeout) => None,
    }
}

#[cfg(any(windows, test))]
fn decode_power(ac: u8, flags: u8, percent: u8) -> PowerReadiness {
    // 255 must precede the 128 test: it is unknown, not no battery.
    // Reserved bits and contradictory absence/status bits are not evidence.
    let battery_present = match flags {
        255 => None,
        128 => Some(false),
        value if value & !0x0f == 0 => Some(true),
        _ => None,
    };
    PowerReadiness {
        ac_connected: match ac {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        },
        battery_percent: if battery_present == Some(true) && percent <= 100 {
            Some(percent)
        } else {
            None
        },
        battery_present,
    }
}

#[cfg(any(windows, test))]
fn power_fact(ac: u8, flags: u8, percent: u8) -> Option<PowerReadiness> {
    let value = decode_power(ac, flags, percent);
    (value.ac_connected.is_some() || value.battery_present.is_some()).then_some(value)
}

#[cfg(any(windows, test))]
fn local_path(path: &[u16]) -> bool {
    // Only absolute DOS drive paths, never UNC, device paths, relative paths,
    // alternate streams or wildcard names. Keep UTF-16 intact (no UTF-8 conversion).
    path.len() >= 4
        && matches!(path[0], 65..=90 | 97..=122)
        && path[1] == b':' as u16
        && path[2] == b'\\' as u16
        && path.last() == Some(&0)
        && path[3..path.len() - 1]
            .iter()
            .all(|c| *c >= 32 && !matches!(*c, 34 | 42 | 47 | 58 | 60 | 62 | 63 | 124))
        && !path[3..path.len() - 1].split(|c| *c == 92).any(|part| {
            part == [46]
                || part == [46, 46]
                || part.last() == Some(&32)
                || part.last() == Some(&46)
                || dos_device(part)
        })
}

#[cfg(any(windows, test))]
fn dos_device(part: &[u16]) -> bool {
    let mut stem: Vec<u16> = part
        .iter()
        .copied()
        .take_while(|c| *c != 46)
        .map(|c| if (97..=122).contains(&c) { c - 32 } else { c })
        .collect();
    while stem.last() == Some(&32) {
        stem.pop();
    }
    ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
        .iter()
        .any(|name| stem.iter().copied().eq(name.encode_utf16()))
        || (stem.len() == 4
            && (stem[..3] == [67, 79, 77] || stem[..3] == [76, 80, 84])
            && matches!(stem[3], 49..=57 | 0xb9 | 0xb2 | 0xb3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_worker_retains_one_outstanding_and_cleans_up() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{mpsc, Arc, Mutex};
        use std::time::Duration;
        let worker = Mutex::new(None);
        let (finish, wait) = mpsc::channel();
        let (started, start) = mpsc::channel();
        let cleaned = Arc::new(AtomicBool::new(false));
        let flag = cleaned.clone();
        struct Cleanup(Arc<AtomicBool>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        assert_eq!(
            bounded_probe(&worker, Duration::ZERO, move || {
                let _cleanup = Cleanup(flag);
                started.send(()).unwrap();
                wait.recv().unwrap();
                Some(true)
            }),
            None
        );
        start.recv_timeout(Duration::from_secs(2)).unwrap();
        for _ in 0..100 {
            assert_eq!(
                bounded_probe(&worker, Duration::ZERO, || panic!("overlapping worker")),
                None
            );
        }
        assert!(!cleaned.load(Ordering::SeqCst));
        finish.send(()).unwrap();
        // Observe completion deterministically, then put a stale result back to
        // exercise discard/restart without scheduler-dependent sleeps.
        worker
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(cleaned.load(Ordering::SeqCst));
        let (send, receive) = mpsc::channel();
        send.send(Some(true)).unwrap();
        *worker.lock().unwrap() = Some(receive);
        assert_eq!(
            bounded_probe(&worker, Duration::from_secs(2), || Some(false)),
            Some(false)
        );
        assert!(worker.lock().unwrap().is_none());
        let lock = worker.lock().unwrap();
        assert_eq!(
            bounded_probe(&worker, Duration::ZERO, || panic!("lock contention")),
            None
        );
        drop(lock);
    }

    #[test]
    fn lost_receiver_does_not_skip_worker_cleanup() {
        use std::sync::{mpsc, Mutex};
        use std::time::Duration;
        let worker = Mutex::new(None);
        let (finish, wait) = mpsc::channel();
        let (cleaned, cleanup) = mpsc::channel();
        struct Cleanup(mpsc::Sender<()>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        bounded_probe(&worker, Duration::ZERO, move || {
            let _cleanup = Cleanup(cleaned);
            wait.recv().unwrap();
            Some(false)
        });
        drop(worker);
        finish.send(()).unwrap();
        cleanup.recv_timeout(Duration::from_secs(2)).unwrap();
    }

    #[test]
    fn independent_failures_and_known_false() {
        for failed in 0..4 {
            let result = collect_with(
                || {
                    (failed != 0).then_some(VolumeReadiness {
                        available_bytes: 7,
                        read_only: false,
                    })
                },
                || {
                    (failed != 1).then_some(VolumeReadiness {
                        available_bytes: 9,
                        read_only: true,
                    })
                },
                || (failed != 2).then(|| decode_power(1, 128, 255)),
                || (failed != 3).then_some(false),
            );
            assert_eq!(matches!(result.system_volume, Probe::Unknown), failed == 0);
            assert_eq!(matches!(result.journal_volume, Probe::Unknown), failed == 1);
            assert_eq!(matches!(result.power, Probe::Unknown), failed == 2);
            assert_eq!(
                result.windows_update_reboot,
                if failed == 3 {
                    Probe::Unknown
                } else {
                    Probe::Known(false)
                }
            );
        }
    }

    #[test]
    fn serde_preserves_large_counts_and_unknown() {
        let value = Probe::Known(VolumeReadiness {
            available_bytes: (1_u64 << 40) + 123,
            read_only: true,
        });
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["status"], "known");
        assert_eq!(
            json["value"]["available_bytes"].as_u64(),
            Some((1_u64 << 40) + 123)
        );
        assert_eq!(
            serde_json::from_value::<Probe<VolumeReadiness>>(json).unwrap(),
            value
        );
        assert_eq!(
            serde_json::to_value(Probe::<bool>::default()).unwrap(),
            serde_json::json!({"status": "unknown"})
        );
        assert_eq!(
            serde_json::from_str::<Readiness>(
                &serde_json::to_string(&Readiness::default()).unwrap()
            )
            .unwrap(),
            Readiness::default()
        );
    }

    #[test]
    fn power_sentinels_and_invalid_flags() {
        assert_eq!(power_fact(255, 255, 50), None);
        assert_eq!(power_fact(255, 129, 30), None);
        assert!(power_fact(1, 255, 255).is_some());
        assert!(power_fact(255, 128, 255).is_some());
        assert_eq!(
            decode_power(255, 255, 50),
            PowerReadiness {
                ac_connected: None,
                battery_percent: None,
                battery_present: None
            }
        );
        assert_eq!(
            decode_power(1, 128, 100),
            PowerReadiness {
                ac_connected: Some(true),
                battery_percent: None,
                battery_present: Some(false)
            }
        );
        for flags in [16, 32, 64, 129, 254] {
            assert_eq!(decode_power(2, flags, 30).battery_present, None);
        }
        for percent in [101, 254, 255] {
            assert_eq!(decode_power(0, 1, percent).battery_percent, None);
        }
        for percent in [0, 50, 100] {
            assert_eq!(decode_power(0, 9, percent).battery_percent, Some(percent));
        }
        assert_eq!(decode_power(0, 0, 255).ac_connected, Some(false));
    }

    #[test]
    fn paths_are_absolute_local_and_nul_terminated() {
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        for path in ["C:\\", "D:\\ProgramData\\Secblitz", "E:\\資料"] {
            assert!(local_path(&wide(path)));
        }
        for path in [
            "",
            "C:",
            "C:relative",
            "\\\\host\\share",
            "\\\\?\\C:\\",
            "C:\\..\\x",
            "C:\\a:stream",
            "C:\\x\0hidden",
            "C:\\x.",
            "C:\\NUL.txt",
            "C:\\NUL .txt",
            "C:\\com1",
            "C:\\LPT².log",
        ] {
            assert!(!local_path(&wide(path)));
        }
        assert!(!local_path(&[67, 58, 92]));
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_is_unknown() {
        assert_eq!(collect(), Readiness::default());
    }
}

#[cfg(any(windows, test))]
trait VolumeApi {
    fn fixed(&self, path: &[u16]) -> bool;
    fn root(&self, path: &[u16]) -> Option<Vec<u16>>;
    fn available(&self, root: &[u16]) -> Option<u64>;
    fn flags(&self, root: &[u16]) -> Option<u32>;
}

#[cfg(any(windows, test))]
fn volume_with(api: &impl VolumeApi, path: &[u16]) -> Option<VolumeReadiness> {
    if !local_path(path) {
        return None;
    }
    // Reject mapped drives before resolving the full path, then check the
    // resolved volume too: a local mount point can lead elsewhere.
    let drive = [path[0], path[1], path[2], 0];
    if !api.fixed(&drive) {
        return None;
    }
    let root = api.root(path)?;
    if !local_path(&root) || !api.fixed(&root) {
        return None;
    }
    Some(VolumeReadiness {
        available_bytes: api.available(&root)?,
        // Win32 FILE_READ_ONLY_VOLUME.
        read_only: api.flags(&root)? & 0x0008_0000 != 0,
    })
}

#[cfg(test)]
mod volume_tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        fail: u8,
        root: Vec<u16>,
        flags: u32,
        available: u64,
        calls: RefCell<Vec<&'static str>>,
    }
    impl VolumeApi for Fake {
        fn fixed(&self, _: &[u16]) -> bool {
            self.calls.borrow_mut().push("fixed");
            self.fail != 4 && !(self.fail == 5 && self.calls.borrow().len() == 3)
        }
        fn root(&self, _: &[u16]) -> Option<Vec<u16>> {
            self.calls.borrow_mut().push("root");
            (self.fail != 1).then(|| self.root.clone())
        }
        fn available(&self, _: &[u16]) -> Option<u64> {
            self.calls.borrow_mut().push("available");
            (self.fail != 2).then_some(self.available)
        }
        fn flags(&self, _: &[u16]) -> Option<u32> {
            self.calls.borrow_mut().push("flags");
            (self.fail != 3).then_some(self.flags)
        }
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    fn fake(fail: u8) -> Fake {
        Fake {
            fail,
            root: wide("D:\\"),
            flags: 0x0008_0000,
            available: 1_u64 << 42,
            calls: RefCell::new(Vec::new()),
        }
    }
    #[test]
    fn failures_and_large_quota() {
        let path = wide("D:\\ProgramData\\Secblitz");
        for fail in 1..=5 {
            assert_eq!(volume_with(&fake(fail), &path), None);
        }
        assert_eq!(
            volume_with(&fake(0), &path),
            Some(VolumeReadiness {
                available_bytes: 1_u64 << 42,
                read_only: true
            })
        );
        let mut api = fake(0);
        api.flags = 0;
        assert!(!volume_with(&api, &path).unwrap().read_only);
        for available in [0, 1, 4096, (1_u64 << 32) + 1] {
            api.available = available;
            assert_eq!(volume_with(&api, &path).unwrap().available_bytes, available);
        }
    }
    #[test]
    fn rejects_bad_paths_and_remote_roots_before_disk_queries() {
        let api = fake(0);
        assert_eq!(volume_with(&api, &wide("\\\\server\\share")), None);
        assert!(api.calls.borrow().is_empty());
        let api = fake(4);
        assert_eq!(volume_with(&api, &wide("D:\\data")), None);
        assert_eq!(*api.calls.borrow(), ["fixed"]);
        for root in ["", "\\\\server\\share", "D:relative"] {
            let mut api = fake(0);
            api.root = wide(root);
            assert_eq!(volume_with(&api, &wide("D:\\data")), None);
            assert_eq!(*api.calls.borrow(), ["fixed", "root"]);
        }
    }
}
