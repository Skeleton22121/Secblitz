//! The `SecblitzFilter` service: loopback listener, list refresh, status for the app.
//! `serve` is portable and tested on any host; service control lives in `scm.rs`.

use anyhow::{Context, Result};
use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::adapters;
use super::config::{self, ErrorCode, State, Status};
use super::fetch;
use super::lists::{self, SOURCES};
use super::matcher::Filter;
use super::server::{self, upstream_addrs, BindError, Shared};

const TICK: Duration = Duration::from_millis(200);
const CONFIG_EVERY: Duration = Duration::from_secs(2);
const UPSTREAM_EVERY: Duration = Duration::from_secs(30);
const UPSTREAM_RETRY_MIN: Duration = Duration::from_secs(5);
const BIND_RETRY: Duration = Duration::from_secs(30);
const REFRESH_EVERY: Duration = Duration::from_secs(60 * 60);
const STATUS_EVERY: Duration = Duration::from_secs(10);

pub struct Paths {
    pub config: PathBuf,
    pub status: PathBuf,
    pub lists: PathBuf,
}

impl Paths {
    pub fn production() -> Result<Self> {
        Ok(Paths {
            config: config::config_path()?,
            status: config::status_path()?,
            lists: config::lists_dir()?,
        })
    }
}

/// The only addresses the filter ever listens on.
pub fn listen_addresses() -> Vec<SocketAddr> {
    vec![
        SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 53),
        SocketAddr::new(Ipv6Addr::LOCALHOST.into(), 53),
    ]
}

#[derive(Default)]
struct Meta {
    state: State,
    domains: [u64; 3],
    lists_updated: Option<u64>,
    refresh_error: Option<ErrorCode>,
}

type SharedMeta = Arc<Mutex<Meta>>;

fn lock(meta: &SharedMeta) -> MutexGuard<'_, Meta> {
    meta.lock().unwrap_or_else(PoisonError::into_inner)
}

fn rebuild_from_disk(paths: &Paths, shared: &Shared, meta: &SharedMeta) {
    let lists = fetch::load_all(&paths.lists);
    let candidate = fetch::rebuild(&lists);
    drop(lists);
    let newest = SOURCES
        .iter()
        .filter_map(|s| fetch::stored_at(&paths.lists, s.id))
        .max();
    let mut m = lock(meta);
    m.lists_updated = newest;
    match candidate {
        None => {
            if m.state != State::Ready {
                m.state = State::NoLists;
            }
        }
        Some(candidate) => {
            let previous =
                Arc::clone(&shared.filter.read().unwrap_or_else(PoisonError::into_inner));
            match fetch::accept(candidate, &previous) {
                Ok(filter) => {
                    let counts = lists::counts(&filter);
                    m.domains = counts.map(|n| n as u64);
                    *shared
                        .filter
                        .write()
                        .unwrap_or_else(PoisonError::into_inner) = Arc::new(filter);
                    m.state = State::Ready;
                    if m.refresh_error == Some(ErrorCode::ListInvalid) {
                        m.refresh_error = None;
                    }
                }
                Err(_) => m.refresh_error = Some(ErrorCode::ListInvalid),
            }
        }
    }
}

fn refresh(paths: &Paths, shared: &Shared, meta: &SharedMeta) {
    let Ok(client) = fetch::client() else {
        lock(meta).refresh_error = Some(ErrorCode::DownloadFailed);
        return;
    };
    let mut error = None;
    let mut stored = false;
    for source in &SOURCES {
        let last = fetch::stored_at(&paths.lists, source.id);
        if !fetch::due(source, last, server::unix_now()) {
            continue;
        }
        let text = match fetch::download(&client, source) {
            Ok(text) => text,
            Err(_) => {
                error = Some(ErrorCode::DownloadFailed);
                continue;
            }
        };
        if fetch::usable_entries(source, &text) == 0
            || fetch::store(&paths.lists, source.id, &text).is_err()
        {
            error = Some(ErrorCode::ListInvalid);
            continue;
        }
        drop(text);
        stored = true;
        // The first lists to arrive start protecting at once.
        if lock(meta).state != State::Ready {
            rebuild_from_disk(paths, shared, meta);
        }
    }
    if stored {
        rebuild_from_disk(paths, shared, meta);
    }
    let mut m = lock(meta);
    // A broken stored set (ListInvalid from the rebuild) stays reported.
    if error.is_some() || m.refresh_error != Some(ErrorCode::ListInvalid) {
        m.refresh_error = error;
    }
}

struct Background {
    busy: Arc<AtomicBool>,
    paths: Arc<Paths>,
    shared: Arc<Shared>,
    meta: SharedMeta,
}

impl Background {
    fn start(&self, download: bool, load_first: bool) {
        if self.busy.swap(true, Ordering::AcqRel) {
            return;
        }
        let (busy, paths, shared, meta) = (
            Arc::clone(&self.busy),
            Arc::clone(&self.paths),
            Arc::clone(&self.shared),
            Arc::clone(&self.meta),
        );
        let spawned = thread::Builder::new()
            .name("secblitz-filter-lists".into())
            .spawn(move || {
                if load_first {
                    rebuild_from_disk(&paths, &shared, &meta);
                }
                // No downloads while Windows says the connection is metered.
                if download && !adapters::metered() {
                    refresh(&paths, &shared, &meta);
                }
                busy.store(false, Ordering::Release);
            });
        if spawned.is_err() {
            self.busy.store(false, Ordering::Release);
        }
    }
}

struct Every {
    period: Duration,
    next: Instant,
}

impl Every {
    fn new(period: Duration) -> Self {
        Every {
            period,
            next: Instant::now(),
        }
    }

    fn due(&mut self) -> bool {
        let now = Instant::now();
        if now < self.next {
            return false;
        }
        self.next = now + self.period;
        true
    }

    fn reset(&mut self) {
        self.next = Instant::now() + self.period;
    }
}

/// What changes the app should hear about at once (counts only every 10 s).
fn same_apart_from_counts(a: &Status, b: &Status) -> bool {
    let strip = |s: &Status| Status {
        written_at: 0,
        blocked: [0; 3],
        day: 0,
        ..s.clone()
    };
    strip(a) == strip(b)
}

struct Listeners {
    threads: Vec<JoinHandle<()>>,
}

fn listen(
    addrs: &[SocketAddr],
    shared: &Arc<Shared>,
    stop: &Arc<AtomicBool>,
) -> Result<Listeners, BindError> {
    let (udp, tcp) = server::bind(addrs)?;
    let mut threads = Vec::new();
    for socket in udp {
        let (shared, stop) = (Arc::clone(shared), Arc::clone(stop));
        threads.push(thread::spawn(move || {
            server::serve_udp(socket, shared, stop)
        }));
    }
    for listener in tcp {
        let (shared, stop) = (Arc::clone(shared), Arc::clone(stop));
        threads.push(thread::spawn(move || {
            server::serve_tcp(listener, shared, stop)
        }));
    }
    Ok(Listeners { threads })
}

/// The service's main loop. Returns after `stop` is set (within about a
/// second). `download` is false in tests.
pub fn serve(
    paths: Paths,
    addrs: &[SocketAddr],
    download: bool,
    stop: &Arc<AtomicBool>,
) -> Result<()> {
    // Best effort: the installer normally created it already.
    let _ = fs::create_dir_all(&paths.lists);
    let paths = Arc::new(paths);
    let mut network = adapters::upstream_servers();
    let shared = Arc::new(Shared::new(
        Filter::empty(),
        config::load_config(&paths.config),
        upstream_addrs(&network),
    ));
    if let Some(previous) = config::load_status(&paths.status) {
        shared
            .stats
            .resume(previous.day, previous.blocked, server::unix_now());
    }
    let meta: SharedMeta = Arc::new(Mutex::new(Meta::default()));
    let background = Background {
        busy: Arc::new(AtomicBool::new(false)),
        paths: Arc::clone(&paths),
        shared: Arc::clone(&shared),
        meta: Arc::clone(&meta),
    };
    background.start(download, true);

    let mut listeners: Option<Listeners> = None;
    let mut port_in_use = false;
    let mut bind_timer = Every::new(BIND_RETRY);
    let mut config_timer = Every::new(CONFIG_EVERY);
    let mut upstream_timer = Every::new(UPSTREAM_EVERY);
    let mut refresh_timer = Every::new(REFRESH_EVERY);
    let mut status_timer = Every::new(STATUS_EVERY);
    refresh_timer.reset();
    let mut last_status: Option<Status> = None;
    let mut upstream_checked = Instant::now();

    while !stop.load(Ordering::Acquire) {
        if listeners.is_none() && bind_timer.due() {
            match listen(addrs, &shared, stop) {
                Ok(l) => {
                    listeners = Some(l);
                    port_in_use = false;
                }
                Err(e) => port_in_use = e == BindError::PortInUse,
            }
        }
        if config_timer.due() {
            let fresh = config::load_config(&paths.config);
            *shared
                .config
                .write()
                .unwrap_or_else(PoisonError::into_inner) = fresh;
        }
        let failed = shared.upstream_failed.load(Ordering::Acquire)
            && upstream_checked.elapsed() >= UPSTREAM_RETRY_MIN;
        if upstream_timer.due() || failed {
            shared.upstream_failed.store(false, Ordering::Release);
            network = adapters::upstream_servers();
            *shared
                .upstream
                .write()
                .unwrap_or_else(PoisonError::into_inner) = upstream_addrs(&network);
            upstream_checked = Instant::now();
            upstream_timer.reset();
        }
        if download && refresh_timer.due() {
            background.start(true, false);
        }

        let now = server::unix_now();
        let (day, blocked) = shared.stats.snapshot(now);
        let status = {
            let m = lock(&meta);
            let last_error = if port_in_use {
                Some(ErrorCode::PortInUse)
            } else if m.refresh_error.is_some() {
                m.refresh_error
            } else if network.is_empty() {
                Some(ErrorCode::NoUpstream)
            } else {
                None
            };
            Status {
                listening: listeners.is_some(),
                state: m.state,
                lists_updated: m.lists_updated,
                day,
                blocked,
                domains: m.domains,
                last_error,
                written_at: now,
            }
        };
        let changed = last_status
            .as_ref()
            .is_none_or(|last| !same_apart_from_counts(last, &status));
        if changed || status_timer.due() {
            if config::save_status(&paths.status, &status).is_ok() {
                status_timer.reset();
            }
            last_status = Some(status);
        }
        thread::sleep(TICK);
    }

    // Tell the app right away that nothing is listening any more.
    let mut status = last_status.unwrap_or_default();
    status.listening = false;
    status.written_at = server::unix_now();
    let _ = config::save_status(&paths.status, &status);
    if let Some(l) = listeners {
        for t in l.threads {
            let _ = t.join();
        }
    }
    Ok(())
}

pub fn run() -> Result<()> {
    #[cfg(windows)]
    {
        scm::run()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Windows services are only supported on Windows")
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn run_production(stop: &Arc<AtomicBool>) -> Result<()> {
    let paths = Paths::production().context("Cannot find the filter folder")?;
    serve(paths, &listen_addresses(), true, stop)
}

#[cfg(windows)]
mod scm {
    use super::*;
    use std::ffi::OsString;
    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
        service_dispatcher,
    };

    define_windows_service!(ffi_main, service_main);

    pub fn run() -> Result<()> {
        service_dispatcher::start(super::super::SERVICE_NAME, ffi_main)?;
        Ok(())
    }

    fn set_status(
        handle: &ServiceStatusHandle,
        state: ServiceState,
        checkpoint: u32,
        failed: bool,
    ) {
        let pending = matches!(
            state,
            ServiceState::StartPending | ServiceState::StopPending
        );
        let _ = handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: if state == ServiceState::Running {
                ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
            } else {
                ServiceControlAccept::empty()
            },
            exit_code: if failed {
                ServiceExitCode::ServiceSpecific(1)
            } else {
                ServiceExitCode::Win32(0)
            },
            checkpoint,
            wait_hint: if pending {
                Duration::from_secs(10)
            } else {
                Duration::ZERO
            },
            process_id: None,
        });
    }

    fn service_main(_: Vec<OsString>) {
        let stop = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&stop);
        let handle =
            match service_control_handler::register(super::super::SERVICE_NAME, move |control| {
                match control {
                    ServiceControl::Stop | ServiceControl::Shutdown => {
                        signal.store(true, Ordering::Release);
                        ServiceControlHandlerResult::NoError
                    }
                    ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                    _ => ServiceControlHandlerResult::NotImplemented,
                }
            }) {
                Ok(h) => h,
                Err(_) => return,
            };
        set_status(&handle, ServiceState::StartPending, 1, false);
        // Running at once: the loop retries binding, so a taken port is
        // reported in the status file instead of failing the start.
        set_status(&handle, ServiceState::Running, 0, false);
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_production(&stop)));
        let failed = !matches!(outcome, Ok(Ok(())));
        set_status(&handle, ServiceState::StopPending, 1, false);
        set_status(&handle, ServiceState::Stopped, 0, failed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::config::load_status;
    use std::net::UdpSocket;

    fn paths(dir: &std::path::Path) -> Paths {
        Paths {
            config: dir.join("config.json"),
            status: dir.join("status.json"),
            lists: dir.join("lists"),
        }
    }

    fn wait_for<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
        for _ in 0..100 {
            if let Some(v) = probe() {
                return v;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("timed out waiting for {what}");
    }

    fn free_port() -> u16 {
        let s = UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap().port()
    }

    #[test]
    fn rebuild_from_disk_swaps_in_the_filter() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let shared = Shared::new(Filter::empty(), config::Config::default(), vec![]);
        let meta: SharedMeta = Arc::new(Mutex::new(Meta::default()));

        rebuild_from_disk(&p, &shared, &meta);
        assert_eq!(lock(&meta).state, State::NoLists);

        fetch::store(&p.lists, "adguard-dns", "||ads.example^\n").unwrap();
        fetch::store(
            &p.lists,
            "hagezi-tif",
            "||evil.example^\n||evil2.example^\n",
        )
        .unwrap();
        rebuild_from_disk(&p, &shared, &meta);
        let m = lock(&meta);
        assert_eq!(m.state, State::Ready);
        assert_eq!(m.domains, [1, 1, 2]);
        assert!(m.lists_updated.is_some());
        assert!(shared
            .filter
            .read()
            .unwrap()
            .dangerous
            .blocks("evil.example"));
    }

    #[test]
    fn broken_stored_list_keeps_the_working_set() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let shared = Shared::new(Filter::empty(), config::Config::default(), vec![]);
        let meta: SharedMeta = Arc::new(Mutex::new(Meta::default()));
        fetch::store(&p.lists, "adguard-dns", "||ads.example^\n").unwrap();
        rebuild_from_disk(&p, &shared, &meta);
        fetch::store(&p.lists, "adguard-dns", "<html>oops</html>").unwrap();
        rebuild_from_disk(&p, &shared, &meta);
        assert_eq!(lock(&meta).refresh_error, Some(ErrorCode::ListInvalid));
        assert!(shared.filter.read().unwrap().ads.blocks("ads.example"));
        fetch::store(&p.lists, "adguard-dns", "||ads.example^\n||b.example^\n").unwrap();
        rebuild_from_disk(&p, &shared, &meta);
        assert_eq!(lock(&meta).refresh_error, None);
    }

    #[test]
    fn serve_answers_reports_and_stops_quickly() {
        let dir = tempfile::tempdir().unwrap();
        let port = free_port();
        let addrs = vec![SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port)];
        let stop = Arc::new(AtomicBool::new(false));
        let p = paths(dir.path());
        let status_path = p.status.clone();
        let handle = {
            let stop = Arc::clone(&stop);
            thread::spawn(move || serve(p, &addrs, false, &stop).unwrap())
        };

        let status = wait_for("a listening status", || {
            load_status(&status_path).filter(|s| s.listening && s.state == State::NoLists)
        });
        assert!(config::fresh(&status, server::unix_now()));

        // Firefox's canary name always gets NXDOMAIN, which proves the
        // listener answers.
        let mut query = vec![0x00, 0x2a, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in crate::filter::dns::CANARY.split('.') {
            query.push(label.len() as u8);
            query.extend_from_slice(label.as_bytes());
        }
        query.extend_from_slice(&[0, 0, 1, 0, 1]);
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        client.send_to(&query, ("127.0.0.1", port)).unwrap();
        let mut buf = [0u8; 512];
        let n = client.recv(&mut buf).unwrap();
        assert_eq!(&buf[..2], &[0x00, 0x2a]);
        assert!(n > 12 && buf[3] & 0x0F == 3);

        let started = Instant::now();
        stop.store(true, Ordering::Release);
        handle.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(!load_status(&status_path).unwrap().listening);
    }

    #[test]
    fn port_in_use_is_reported_and_retried() {
        let dir = tempfile::tempdir().unwrap();
        let taken = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = taken.local_addr().unwrap().port();
        let addrs = vec![SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port)];
        let stop = Arc::new(AtomicBool::new(false));
        let p = paths(dir.path());
        let status_path = p.status.clone();
        let handle = {
            let stop = Arc::clone(&stop);
            thread::spawn(move || serve(p, &addrs, false, &stop).unwrap())
        };
        let status = wait_for("the port-in-use status", || {
            load_status(&status_path).filter(|s| s.last_error == Some(ErrorCode::PortInUse))
        });
        assert!(!status.listening);
        stop.store(true, Ordering::Release);
        handle.join().unwrap();
    }

    #[test]
    fn production_addresses_are_loopback_port_53() {
        for addr in listen_addresses() {
            assert!(addr.ip().is_loopback());
            assert_eq!(addr.port(), 53);
        }
    }
}
