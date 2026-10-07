//! Background file writes: history, the status file, the changed list and the saved check.
use crate::app;
use iced::futures;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

pub enum Cache {
    Keep,
    Save(String, u64, Arc<secblitz::engine::Report>),
    Forget,
}

pub struct Changed {
    pub changed: Vec<String>,
    pub armed: Vec<String>,
}

pub fn persist(
    dir: Option<PathBuf>,
    entry: Option<app::history::Entry>,
    status: secblitz::status::Status,
    changed: Option<Changed>,
    cache: Cache,
) {
    write_in_order(move || {
        if let (Some(dir), Some(entry)) = (&dir, &entry) {
            let _ = app::history::record(dir, entry);
        }
        // Switched back means working at the previous check and flagged now.
        let status = match &changed {
            Some(now) => {
                let mut before = secblitz::status::read_changed().unwrap_or_default();
                before.retain(|id| now.changed.contains(id));
                let _ = secblitz::status::write_changed(&now.armed);
                status.with_changed(&before)
            }
            None => status,
        };
        let _ = secblitz::status::write(&status);
        if let Some(dir) = &dir {
            match cache {
                Cache::Keep => {}
                Cache::Save(user, at, report) => {
                    let _ = app::last_check::save(dir, &user, at, &report);
                }
                Cache::Forget => app::last_check::forget(dir),
            }
        }
    });
}

pub fn forget_check(dir: Option<PathBuf>) {
    let Some(dir) = dir else { return };
    write_in_order(move || app::last_check::forget(&dir));
}

type Job = Box<dyn FnOnce() + Send>;

/// Writes queued but not yet finished.
struct Pending {
    count: Mutex<usize>,
    idle: Condvar,
}

static PENDING: Pending = Pending {
    count: Mutex::new(0),
    idle: Condvar::new(),
};

impl Pending {
    fn begin(&self) {
        *self.count.lock().unwrap_or_else(|e| e.into_inner()) += 1;
    }

    fn finish(&self) {
        let mut count = self.count.lock().unwrap_or_else(|e| e.into_inner());
        *count -= 1;
        if *count == 0 {
            self.idle.notify_all();
        }
    }

    fn wait_idle(&self) {
        let mut count = self.count.lock().unwrap_or_else(|e| e.into_inner());
        while *count > 0 {
            count = self.idle.wait(count).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// Run file writes one after another on a single background thread, in the
/// order they were queued: a later "forget" must never land before an
/// earlier save, and history read-modify-writes must not interleave.
fn write_in_order(job: impl FnOnce() + Send + 'static) {
    static QUEUE: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
    PENDING.begin();
    let queue = QUEUE.get_or_init(|| {
        let (tx, rx) = channel::<Job>();
        std::thread::spawn(move || {
            for job in rx {
                job();
                PENDING.finish();
            }
        });
        Mutex::new(tx)
    });
    let sent = queue
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .send(Box::new(job));
    if sent.is_err() {
        PENDING.finish();
    }
}

/// Saves the preferences after any earlier queued write; resolves to whether it worked.
/// The tray cannot read these preferences, so it gets a copy next to the status file, shared by everyone on this PC.
fn mirror_notify(prefs: &app::settings::Prefs) -> bool {
    secblitz::status::write_notify(&prefs.notify()).is_ok()
}

pub fn sync_notify(prefs: &app::settings::Prefs) {
    let prefs = prefs.clone();
    write_in_order(move || {
        mirror_notify(&prefs);
    });
}

pub fn save_prefs(prefs: app::settings::Prefs) -> impl std::future::Future<Output = bool> {
    let (tx, rx) = futures::channel::oneshot::channel();
    write_in_order(move || {
        let saved = app::settings::save(&prefs).is_ok();
        let _ = tx.send(saved && mirror_notify(&prefs));
    });
    async move { rx.await.unwrap_or(true) }
}

/// Block until every queued write has landed (used before reading it back).
pub fn wait_persisted() {
    PENDING.wait_idle();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_land_in_order_and_wait_persisted_waits_for_them() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        for n in 0..20 {
            let seen = seen.clone();
            write_in_order(move || {
                if n == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
                seen.lock().unwrap().push(n);
            });
        }
        wait_persisted();
        assert_eq!(*seen.lock().unwrap(), (0..20).collect::<Vec<_>>());
    }
}
