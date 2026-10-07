//! The DNS listeners and forwarder of the filter service. Portable (std sockets and threads).
//! Only loopback peers are answered; unblocked names go unchanged to the PC's own DNS servers.

use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, PoisonError, RwLock, RwLockReadGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rand::rngs::SysRng;
use rand::TryRng;

use super::activity::Activity;
use super::config::{Config, Lookups};
use super::dns::{self, Query};
use super::doh::{self, Doh, Fallback};
use super::matcher::{Filter, HashSet64, Kind};
use super::safe_search;

const MAX_PACKET: usize = 4096;
const WORKERS: usize = 16;
const QUEUE: usize = 256;
const MAX_TCP_CONNECTIONS: usize = 32;
const TCP_IDLE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(500);
pub const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(2);
const SECONDS_PER_DAY: u64 = 86_400;

pub const FALLBACK_UPSTREAM: [IpAddr; 2] = [
    IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9)),
    IpAddr::V4(Ipv4Addr::new(149, 112, 112, 112)),
];

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
pub struct Stats {
    pub blocked: [AtomicU64; 5],
    pub day: AtomicU64,
    /// Unix seconds; 0 for none.
    pub dangerous_at: AtomicU64,
}

impl Stats {
    pub fn record(&self, kind: Kind, now: u64) {
        if kind == Kind::Dangerous {
            self.dangerous_at.store(now, Ordering::Relaxed);
        }
        let today = now / SECONDS_PER_DAY;
        let seen = self.day.load(Ordering::Relaxed);
        if seen != today
            && self
                .day
                .compare_exchange(seen, today, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        {
            for counter in &self.blocked {
                counter.store(0, Ordering::Relaxed);
            }
        }
        self.blocked[kind.index()].fetch_add(1, Ordering::Relaxed);
    }

    /// Picks up today's counts from before a restart (the last status file),
    /// so "blocked today" does not drop to zero after a crash or a reboot.
    pub fn resume(&self, day: u64, counts: [u64; 5], now: u64) {
        if day != now / SECONDS_PER_DAY {
            return;
        }
        self.day.store(day, Ordering::Relaxed);
        for (counter, count) in self.blocked.iter().zip(counts) {
            counter.store(count, Ordering::Relaxed);
        }
    }

    pub fn resume_dangerous_at(&self, at: Option<u64>) {
        self.dangerous_at.store(at.unwrap_or(0), Ordering::Relaxed);
    }

    pub fn dangerous_at(&self) -> Option<u64> {
        Some(self.dangerous_at.load(Ordering::Relaxed)).filter(|t| *t > 0)
    }

    pub fn snapshot(&self, now: u64) -> (u64, [u64; 5]) {
        let today = now / SECONDS_PER_DAY;
        if self.day.load(Ordering::Relaxed) != today {
            return (today, [0; 5]);
        }
        (
            today,
            self.blocked.each_ref().map(|c| c.load(Ordering::Relaxed)),
        )
    }
}

pub struct Shared {
    pub filter: RwLock<Arc<Filter>>,
    pub config: RwLock<Config>,
    pub allow: RwLock<HashSet64>,
    pub upstream: RwLock<Vec<SocketAddr>>,
    pub stats: Stats,
    pub activity: Activity,
    pub doh: Doh,
    pub fallback: Fallback,
    pub local_suffixes: RwLock<Vec<String>>,
    /// Set when every upstream server failed, so the loop refreshes them early.
    pub upstream_failed: AtomicBool,
}

impl Shared {
    pub fn new(filter: Filter, config: Config, upstream: Vec<SocketAddr>) -> Self {
        Shared {
            filter: RwLock::new(Arc::new(filter)),
            allow: RwLock::new(allowed(&config)),
            config: RwLock::new(config),
            upstream: RwLock::new(upstream),
            stats: Stats::default(),
            activity: Activity::default(),
            doh: Doh::quad9(),
            fallback: Fallback::default(),
            local_suffixes: RwLock::new(Vec::new()),
            upstream_failed: AtomicBool::new(false),
        }
    }

    #[cfg(test)]
    pub fn with_doh(mut self, doh: Doh) -> Self {
        self.doh = doh;
        self
    }

    pub fn set_config(&self, config: Config) {
        *self.allow.write().unwrap_or_else(PoisonError::into_inner) = allowed(&config);
        let mut current = self.config.write().unwrap_or_else(PoisonError::into_inner);
        // A fresh start, perhaps on another network.
        if config.private_lookups && !current.private_lookups {
            self.fallback.worked();
        }
        *current = config;
    }

    pub fn set_local_suffixes(&self, suffixes: Vec<String>) {
        *self
            .local_suffixes
            .write()
            .unwrap_or_else(PoisonError::into_inner) = suffixes;
    }

    pub fn lookups(&self, now: u64) -> Lookups {
        if !read(&self.config).private_active(now) {
            Lookups::Plain
        } else if self.fallback.active() {
            Lookups::PrivateFallback
        } else {
            Lookups::Private
        }
    }
}

fn allowed(config: &Config) -> HashSet64 {
    HashSet64::from_names(config.allow.iter().map(String::as_str))
}

pub fn upstream_addrs(servers: &[IpAddr]) -> Vec<SocketAddr> {
    let servers = if servers.is_empty() {
        &FALLBACK_UPSTREAM[..]
    } else {
        servers
    };
    servers.iter().map(|ip| SocketAddr::new(*ip, 53)).collect()
}

pub enum Action {
    Reply(Vec<u8>),
    Forward,
    SafeSearch(&'static str),
}

pub fn decide(packet: &[u8], shared: &Shared, now: u64) -> Option<(Query, Action)> {
    let q = dns::parse_query(packet)?;
    if q.question.name == dns::CANARY {
        let reply = dns::nxdomain_reply(packet, &q);
        return Some((q, Action::Reply(reply)));
    }
    let on = read(&shared.config).active(now);
    let filter = Arc::clone(&read(&shared.filter));
    let kind = filter.decide_allowing(&q.question.name, on, &read(&shared.allow));
    if let Some(kind) = kind {
        shared.stats.record(kind, now);
        shared.activity.record(&q.question.name, kind, now);
        let reply = dns::blocked_reply(packet, &q);
        return Some((q, Action::Reply(reply)));
    }
    if on.safe_search && q.question.qclass == 1 {
        if let Some(target) = safe_search::target_for(&q.question.name) {
            match q.question.qtype {
                dns::TYPE_A | dns::TYPE_AAAA => return Some((q, Action::SafeSearch(target))),
                // Address hints would lead around safe search.
                dns::TYPE_HTTPS | dns::TYPE_SVCB => {
                    let reply = dns::empty_reply(packet, &q);
                    return Some((q, Action::Reply(reply)));
                }
                _ => {}
            }
        }
    }
    Some((q, Action::Forward))
}

fn unspecified_for(server: &SocketAddr) -> SocketAddr {
    match server {
        SocketAddr::V4(_) => SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0),
        SocketAddr::V6(_) => SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), 0),
    }
}

fn random_id() -> u16 {
    SysRng
        .try_next_u32()
        .expect("the system random generator failed") as u16
}

/// One UDP exchange with one server: a fresh socket (so a fresh random source
/// port), connected to the server so nothing else can answer. Replies with the
/// wrong id or question are skipped until the time is up.
fn ask_udp(
    packet: &[u8],
    q: &Query,
    id: u16,
    server: &SocketAddr,
    timeout: Duration,
) -> Option<Vec<u8>> {
    let socket = UdpSocket::bind(unspecified_for(server)).ok()?;
    socket.connect(server).ok()?;
    socket.send(&dns::with_id(packet, id)).ok()?;
    let deadline = std::time::Instant::now() + timeout;
    let mut buf = vec![0u8; 65_535];
    loop {
        let left = deadline.checked_duration_since(std::time::Instant::now())?;
        socket
            .set_read_timeout(Some(left.max(Duration::from_millis(1))))
            .ok()?;
        let n = socket.recv(&mut buf).ok()?;
        if dns::reply_matches(&buf[..n], id, &q.question) {
            return Some(buf[..n].to_vec());
        }
    }
}

fn ask_tcp(
    packet: &[u8],
    q: &Query,
    id: u16,
    server: &SocketAddr,
    timeout: Duration,
) -> Option<Vec<u8>> {
    let mut stream = TcpStream::connect_timeout(server, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    let out = dns::with_id(packet, id);
    let mut frame = Vec::with_capacity(out.len() + 2);
    frame.extend_from_slice(&(out.len() as u16).to_be_bytes());
    frame.extend_from_slice(&out);
    stream.write_all(&frame).ok()?;
    let mut len = [0u8; 2];
    stream.read_exact(&mut len).ok()?;
    let mut body = vec![0u8; u16::from_be_bytes(len) as usize];
    stream.read_exact(&mut body).ok()?;
    dns::reply_matches(&body, id, &q.question).then_some(body)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    Udp,
    Tcp,
}

/// The upstream answer for `packet`, or `None` when no server gave a valid one.
/// Each server is tried once, in order. A truncated answer goes back as it is
/// to a UDP client (which then asks again over TCP, within the buffer size it
/// announced); for a TCP client it is repeated over TCP to the same server.
/// The caller's transaction id is put back.
pub fn forward_checked(
    packet: &[u8],
    q: &Query,
    upstream: &[SocketAddr],
    timeout: Duration,
    via: Via,
) -> Option<Vec<u8>> {
    for server in upstream {
        let id = random_id();
        let Some(mut reply) = ask_udp(packet, q, id, server, timeout) else {
            continue;
        };
        if dns::truncated(&reply) && via == Via::Tcp {
            match ask_tcp(packet, q, id, server, timeout) {
                Some(full) => reply = full,
                None => continue,
            }
        }
        return Some(dns::with_id(&reply, q.id));
    }
    None
}

pub fn forward(
    packet: &[u8],
    q: &Query,
    upstream: &[SocketAddr],
    timeout: Duration,
    via: Via,
) -> Vec<u8> {
    forward_checked(packet, q, upstream, timeout, via)
        .unwrap_or_else(|| dns::servfail_reply(packet, q))
}

/// With private lookups on, non-local names go to Quad9 encrypted. If that fails, all lookups go plain for a while
/// (captive portals may refuse it). Local names and everything with private lookups off go to the network's servers.
fn lookup(packet: &[u8], q: &Query, shared: &Shared, via: Via) -> Option<Vec<u8>> {
    lookup_at(packet, q, shared, via, Instant::now())
}

fn lookup_at(packet: &[u8], q: &Query, shared: &Shared, via: Via, now: Instant) -> Option<Vec<u8>> {
    if read(&shared.config).private_active(unix_now())
        && !doh::is_local_name(&q.question.name, &read(&shared.local_suffixes))
        && shared.fallback.may_try(now)
    {
        match shared.doh.ask(packet, q) {
            Some(reply) => {
                shared.fallback.worked();
                return Some(fit_for(reply, packet, q, via));
            }
            None => shared.fallback.failed(now),
        }
    }
    let upstream = read(&shared.upstream).clone();
    forward_checked(packet, q, &upstream, UPSTREAM_TIMEOUT, via)
}

/// Cut to what a UDP client said it can take, so it asks again over TCP.
fn fit_for(reply: Vec<u8>, packet: &[u8], q: &Query, via: Via) -> Vec<u8> {
    if via == Via::Udp && reply.len() > dns::udp_limit(packet, q) {
        dns::truncated_copy(&reply)
    } else {
        reply
    }
}

fn safe_search_answer(packet: &[u8], q: &Query, target: &str, shared: &Shared) -> Vec<u8> {
    let ask = dns::build_query(random_id(), target, q.question.qtype);
    let found = dns::parse_query(&ask)
        .and_then(|asked| lookup(&ask, &asked, shared, Via::Tcp))
        .and_then(|reply| dns::parse_addresses(&reply, q.question.qtype));
    match found {
        Some(addresses) => dns::safe_search_reply(packet, q, target, &addresses),
        None => {
            shared.upstream_failed.store(true, Ordering::Release);
            dns::servfail_reply(packet, q)
        }
    }
}

fn answer(packet: &[u8], shared: &Shared, via: Via) -> Option<Vec<u8>> {
    let (q, action) = decide(packet, shared, unix_now())?;
    Some(match action {
        Action::Reply(reply) => reply,
        Action::SafeSearch(target) => safe_search_answer(packet, &q, target, shared),
        Action::Forward => lookup(packet, &q, shared, via).unwrap_or_else(|| {
            shared.upstream_failed.store(true, Ordering::Release);
            dns::servfail_reply(packet, &q)
        }),
    })
}

pub fn allowed_peer(addr: &SocketAddr) -> bool {
    addr.ip().to_canonical().is_loopback()
}

/// Answers UDP queries until `stop` is set. Worker threads that are still
/// waiting for an upstream server finish on their own.
pub fn serve_udp(socket: UdpSocket, shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    if socket.set_read_timeout(Some(POLL)).is_err() {
        return;
    }
    let (tx, rx) = mpsc::sync_channel::<(Vec<u8>, SocketAddr)>(QUEUE);
    let rx = Arc::new(Mutex::new(rx));
    for _ in 0..WORKERS {
        let Ok(out) = socket.try_clone() else {
            continue;
        };
        let rx = Arc::clone(&rx);
        let shared = Arc::clone(&shared);
        thread::spawn(move || loop {
            let job = rx.lock().unwrap_or_else(PoisonError::into_inner).recv();
            let Ok((packet, peer)) = job else {
                return;
            };
            if let Some(reply) = answer(&packet, &shared, Via::Udp) {
                let _ = out.send_to(&reply, peer);
            }
        });
    }
    let mut buf = vec![0u8; MAX_PACKET + 1];
    while !stop.load(Ordering::Acquire) {
        match socket.recv_from(&mut buf) {
            Ok((n, peer)) => {
                if n <= MAX_PACKET && allowed_peer(&peer) {
                    // A full queue means overload: drop the query.
                    let _ = tx.try_send((buf[..n].to_vec(), peer));
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            // Windows reports an earlier ICMP error on UDP as a reset: ignore.
            Err(e) if e.kind() == io::ErrorKind::ConnectionReset => {}
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
}

struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn serve_connection(mut stream: TcpStream, shared: &Shared) {
    if stream.set_nonblocking(false).is_err()
        || stream.set_read_timeout(Some(TCP_IDLE)).is_err()
        || stream.set_write_timeout(Some(TCP_IDLE)).is_err()
    {
        return;
    }
    loop {
        let mut len = [0u8; 2];
        if stream.read_exact(&mut len).is_err() {
            return;
        }
        let len = u16::from_be_bytes(len) as usize;
        // Nothing this filter answers is larger; close without reading it.
        if !(12..=MAX_PACKET).contains(&len) {
            return;
        }
        let mut packet = vec![0u8; len];
        if stream.read_exact(&mut packet).is_err() {
            return;
        }
        let Some(reply) = answer(&packet, shared, Via::Tcp) else {
            return;
        };
        let mut frame = Vec::with_capacity(reply.len() + 2);
        frame.extend_from_slice(&(reply.len() as u16).to_be_bytes());
        frame.extend_from_slice(&reply);
        if stream.write_all(&frame).is_err() {
            return;
        }
    }
}

pub fn serve_tcp(listener: TcpListener, shared: Arc<Shared>, stop: Arc<AtomicBool>) {
    if listener.set_nonblocking(true).is_err() {
        return;
    }
    let open = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, peer)) => {
                if !allowed_peer(&peer) || open.load(Ordering::Acquire) >= MAX_TCP_CONNECTIONS {
                    continue;
                }
                open.fetch_add(1, Ordering::AcqRel);
                let slot = Slot(Arc::clone(&open));
                let shared = Arc::clone(&shared);
                let spawned = thread::Builder::new()
                    .name("secblitz-filter-tcp".into())
                    .spawn(move || {
                        let _slot = slot;
                        serve_connection(stream, &shared);
                    });
                // On failure the closure (and its slot) is dropped, freeing it.
                drop(spawned);
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BindError {
    PortInUse,
    Other(String),
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindError::PortInUse => write!(f, "The DNS port is in use"),
            BindError::Other(e) => write!(f, "Cannot listen: {e}"),
        }
    }
}

impl std::error::Error for BindError {}

fn bind_one(addr: &SocketAddr) -> io::Result<(UdpSocket, TcpListener)> {
    let udp = UdpSocket::bind(addr)?;
    let tcp = TcpListener::bind(addr)?;
    Ok((udp, tcp))
}

/// UDP and TCP listeners for every address. An IPv6 address that cannot be
/// bound (IPv6 switched off) is skipped; IPv4 must work.
pub fn bind(addrs: &[SocketAddr]) -> Result<(Vec<UdpSocket>, Vec<TcpListener>), BindError> {
    let mut udp = Vec::new();
    let mut tcp = Vec::new();
    for addr in addrs {
        match bind_one(addr) {
            Ok((u, t)) => {
                udp.push(u);
                tcp.push(t);
            }
            Err(_) if addr.is_ipv6() => {}
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::AddrInUse | io::ErrorKind::PermissionDenied
                ) =>
            {
                return Err(BindError::PortInUse)
            }
            Err(e) => return Err(BindError::Other(e.to_string())),
        }
    }
    if udp.is_empty() {
        return Err(BindError::Other("no address could be bound".into()));
    }
    Ok((udp, tcp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::lists::NEVER_BLOCK;
    use crate::filter::matcher::{Category, HashSet64};

    const ANSWER_IP: [u8; 4] = [1, 2, 3, 4];

    fn query_bytes(name: &str, qtype: u16, id: u16) -> Vec<u8> {
        let mut p = id.to_be_bytes().to_vec();
        p.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
        for label in name.split('.') {
            p.push(label.len() as u8);
            p.extend_from_slice(label.as_bytes());
        }
        p.push(0);
        p.extend_from_slice(&qtype.to_be_bytes());
        p.extend_from_slice(&1u16.to_be_bytes());
        p
    }

    fn answer_for(query: &[u8], id: u16, ip: [u8; 4], flags: u16) -> Vec<u8> {
        let q = dns::parse_query(query).expect("query");
        let mut out = id.to_be_bytes().to_vec();
        out.extend_from_slice(&flags.to_be_bytes());
        let answers: u16 = if flags & 0x0200 != 0 { 0 } else { 1 };
        out.extend_from_slice(&[0, 1]);
        out.extend_from_slice(&answers.to_be_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&query[12..q.question_end]);
        if answers == 1 {
            out.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4]);
            out.extend_from_slice(&ip);
        }
        out
    }

    fn last_four(reply: &[u8]) -> [u8; 4] {
        reply[reply.len() - 4..].try_into().unwrap()
    }

    fn blocking(names: &[&str]) -> Filter {
        Filter {
            ads: Category {
                block: HashSet64::from_names(names.iter().copied()),
                allow: HashSet64::default(),
            },
            ..Filter::default()
        }
    }

    fn ads_on() -> Config {
        Config {
            ads: true,
            ..Config::default()
        }
    }

    fn fake_upstream() -> SocketAddr {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = socket.local_addr().unwrap();
        thread::spawn(move || {
            let mut buf = [0u8; 512];
            while let Ok((n, from)) = socket.recv_from(&mut buf) {
                let id = u16::from_be_bytes([buf[0], buf[1]]);
                let _ = socket.send_to(&answer_for(&buf[..n], id, ANSWER_IP, 0x8180), from);
            }
        });
        addr
    }

    fn start_udp(shared: Arc<Shared>) -> (SocketAddr, Arc<AtomicBool>, thread::JoinHandle<()>) {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = socket.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let handle = {
            let stop = Arc::clone(&stop);
            thread::spawn(move || serve_udp(socket, shared, stop))
        };
        (addr, stop, handle)
    }

    fn ask(server: SocketAddr, packet: &[u8]) -> Vec<u8> {
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        client.send_to(packet, server).unwrap();
        let mut buf = [0u8; 1024];
        let n = client.recv(&mut buf).expect("reply");
        buf[..n].to_vec()
    }

    #[test]
    fn udp_blocks_and_forwards() {
        let upstream = fake_upstream();
        let shared = Arc::new(Shared::new(
            blocking(&["ads.example"]),
            ads_on(),
            vec![upstream],
        ));
        let (addr, stop, handle) = start_udp(Arc::clone(&shared));

        let blocked = ask(addr, &query_bytes("ads.example", 1, 0x1111));
        assert_eq!(&blocked[..2], &[0x11, 0x11]);
        assert_eq!(last_four(&blocked), [0, 0, 0, 0]);

        let allowed = ask(addr, &query_bytes("ok.example", 1, 0x2222));
        assert_eq!(&allowed[..2], &[0x22, 0x22]);
        assert_eq!(last_four(&allowed), ANSWER_IP);

        assert_eq!(shared.stats.snapshot(unix_now()).1, [1, 0, 0, 0, 0]);
        stop.store(true, Ordering::Release);
        handle.join().unwrap();
    }

    #[test]
    fn forward_ignores_spoofed_reply() {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = socket.local_addr().unwrap();
        thread::spawn(move || {
            let mut buf = [0u8; 512];
            let (n, from) = socket.recv_from(&mut buf).unwrap();
            let id = u16::from_be_bytes([buf[0], buf[1]]);
            let _ = socket.send_to(
                &answer_for(&buf[..n], id.wrapping_add(1), [6, 6, 6, 6], 0x8180),
                from,
            );
            let _ = socket.send_to(&answer_for(&buf[..n], id, ANSWER_IP, 0x8180), from);
        });
        let packet = query_bytes("ok.example", 1, 0x4242);
        let q = dns::parse_query(&packet).unwrap();
        let reply = forward(&packet, &q, &[addr], Duration::from_secs(3), Via::Udp);
        assert_eq!(last_four(&reply), ANSWER_IP);
        assert_eq!(&reply[..2], &[0x42, 0x42]);
    }

    #[test]
    fn forward_retries_over_tcp_on_truncation() {
        // The same port for UDP and TCP; retry in the rare case it is taken.
        let (udp, tcp) = loop {
            let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = tcp.local_addr().unwrap().port();
            if let Ok(udp) = UdpSocket::bind(("127.0.0.1", port)) {
                break (udp, tcp);
            }
        };
        let addr = udp.local_addr().unwrap();
        thread::spawn(move || {
            let mut buf = [0u8; 512];
            let (n, from) = udp.recv_from(&mut buf).unwrap();
            let id = u16::from_be_bytes([buf[0], buf[1]]);
            let _ = udp.send_to(&answer_for(&buf[..n], id, [0; 4], 0x8380), from);
        });
        thread::spawn(move || {
            let (mut conn, _) = tcp.accept().unwrap();
            let mut len = [0u8; 2];
            conn.read_exact(&mut len).unwrap();
            let mut body = vec![0u8; u16::from_be_bytes(len) as usize];
            conn.read_exact(&mut body).unwrap();
            let id = u16::from_be_bytes([body[0], body[1]]);
            let reply = answer_for(&body, id, ANSWER_IP, 0x8180);
            let mut frame = (reply.len() as u16).to_be_bytes().to_vec();
            frame.extend_from_slice(&reply);
            conn.write_all(&frame).unwrap();
        });
        let packet = query_bytes("big.example", 1, 0x0A0B);
        let q = dns::parse_query(&packet).unwrap();
        let reply = forward(&packet, &q, &[addr], Duration::from_secs(3), Via::Tcp);
        assert!(!dns::truncated(&reply));
        assert_eq!(last_four(&reply), ANSWER_IP);
        assert_eq!(&reply[..2], &[0x0A, 0x0B]);
    }

    #[test]
    fn forward_passes_truncation_back_to_udp_clients() {
        let upstream = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = upstream.local_addr().unwrap();
        thread::spawn(move || {
            let mut buf = [0u8; 512];
            let (n, from) = upstream.recv_from(&mut buf).unwrap();
            let id = u16::from_be_bytes([buf[0], buf[1]]);
            let _ = upstream.send_to(&answer_for(&buf[..n], id, [0; 4], 0x8380), from);
        });
        let packet = query_bytes("big.example", 1, 0x0C0D);
        let q = dns::parse_query(&packet).unwrap();
        let reply = forward(&packet, &q, &[addr], Duration::from_secs(3), Via::Udp);
        // The client asks again over TCP; it never gets more than it asked for.
        assert!(dns::truncated(&reply));
        assert_eq!(&reply[..2], &[0x0C, 0x0D]);
    }

    #[test]
    fn forward_all_dead_returns_servfail() {
        let closed = {
            let s = UdpSocket::bind("127.0.0.1:0").unwrap();
            s.local_addr().unwrap()
        };
        let packet = query_bytes("ok.example", 1, 0x0001);
        let q = dns::parse_query(&packet).unwrap();
        let reply = forward(&packet, &q, &[closed], Duration::from_millis(200), Via::Udp);
        assert_eq!(reply[3] & 0x0F, 2);
        assert_eq!(&reply[..2], &[0, 1]);
    }

    #[test]
    fn canary_gets_nxdomain() {
        let shared = Shared::new(Filter::empty(), Config::default(), vec![]);
        let packet = query_bytes(dns::CANARY, 1, 7);
        match decide(&packet, &shared, 1000) {
            Some((_, Action::Reply(r))) => assert_eq!(r[3] & 0x0F, 3),
            _ => panic!("expected an NXDOMAIN reply"),
        }
    }

    #[test]
    fn paused_forwards_everything() {
        let config = Config {
            ads: true,
            paused_until: Some(2000),
            ..Config::default()
        };
        let shared = Shared::new(blocking(&["ads.example"]), config, vec![]);
        let packet = query_bytes("ads.example", 1, 7);
        assert!(matches!(
            decide(&packet, &shared, 1000),
            Some((_, Action::Forward))
        ));
        assert!(matches!(
            decide(&packet, &shared, 2000),
            Some((_, Action::Reply(_)))
        ));
    }

    #[test]
    fn never_block_forwards() {
        let mut filter = blocking(&["update.microsoft.com", "ads.example"]);
        filter.never = HashSet64::from_names(NEVER_BLOCK.iter().copied());
        let shared = Shared::new(filter, ads_on(), vec![]);
        let packet = query_bytes("update.microsoft.com", 1, 7);
        assert!(matches!(
            decide(&packet, &shared, 1000),
            Some((_, Action::Forward))
        ));
    }

    #[test]
    fn garbage_is_dropped() {
        let shared = Shared::new(Filter::empty(), Config::default(), vec![]);
        assert!(decide(&[1, 2, 3], &shared, 1000).is_none());
    }

    #[test]
    fn counts_resume_only_for_the_same_day() {
        let now = 20_000 * SECONDS_PER_DAY + 3_600;
        let stats = Stats::default();
        stats.resume(20_000, [5, 6, 7, 8, 9], now);
        stats.record(Kind::Ads, now);
        assert_eq!(stats.snapshot(now), (20_000, [6, 6, 7, 8, 9]));
        let fresh = Stats::default();
        fresh.resume(19_999, [5, 6, 7, 8, 9], now);
        assert_eq!(fresh.snapshot(now), (20_000, [0, 0, 0, 0, 0]));
    }

    #[test]
    fn udp_ignores_non_loopback() {
        assert!(allowed_peer(&"127.0.0.1:5353".parse().unwrap()));
        assert!(allowed_peer(&"127.9.9.9:1".parse().unwrap()));
        assert!(allowed_peer(&"[::1]:5353".parse().unwrap()));
        assert!(allowed_peer(&"[::ffff:127.0.0.1]:5353".parse().unwrap()));
        assert!(!allowed_peer(&"192.168.1.5:5353".parse().unwrap()));
        assert!(!allowed_peer(&"8.8.8.8:53".parse().unwrap()));
        assert!(!allowed_peer(&"[2001:db8::1]:53".parse().unwrap()));
        assert!(!allowed_peer(&"[::ffff:10.0.0.1]:53".parse().unwrap()));
    }

    #[test]
    fn tcp_answers_and_rejects_oversize_frame() {
        let shared = Arc::new(Shared::new(blocking(&["ads.example"]), ads_on(), vec![]));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let handle = {
            let (shared, stop) = (Arc::clone(&shared), Arc::clone(&stop));
            thread::spawn(move || serve_tcp(listener, shared, stop))
        };

        let mut conn = TcpStream::connect(addr).unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let packet = query_bytes("ads.example", 1, 0x0909);
        let mut frame = (packet.len() as u16).to_be_bytes().to_vec();
        frame.extend_from_slice(&packet);
        conn.write_all(&frame).unwrap();
        let mut len = [0u8; 2];
        conn.read_exact(&mut len).unwrap();
        let mut body = vec![0u8; u16::from_be_bytes(len) as usize];
        conn.read_exact(&mut body).unwrap();
        assert_eq!(&body[..2], &[0x09, 0x09]);
        assert_eq!(last_four(&body), [0, 0, 0, 0]);

        let mut bad = TcpStream::connect(addr).unwrap();
        bad.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        bad.write_all(&[0xFF, 0xFF]).unwrap();
        let mut rest = Vec::new();
        // Closed at once: end of stream or a reset, never an answer.
        let _ = bad.read_to_end(&mut rest);
        assert!(rest.is_empty());

        stop.store(true, Ordering::Release);
        handle.join().unwrap();
    }

    #[test]
    fn bind_failure_reports_port_in_use() {
        let taken = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = taken.local_addr().unwrap();
        assert_eq!(bind(&[addr]).err(), Some(BindError::PortInUse));
    }

    #[test]
    fn bind_gives_both_kinds_of_listener() {
        let (udp, tcp) = bind(&["127.0.0.1:0".parse().unwrap()]).unwrap();
        assert_eq!((udp.len(), tcp.len()), (1, 1));
    }

    #[test]
    fn blocked_counts_reset_on_new_day() {
        let stats = Stats::default();
        let day0 = 10 * SECONDS_PER_DAY + 5;
        stats.record(Kind::Ads, day0);
        stats.record(Kind::Ads, day0 + 1);
        stats.record(Kind::Dangerous, day0 + 2);
        assert_eq!(stats.snapshot(day0 + 3), (10, [2, 0, 1, 0, 0]));
        // Tomorrow, before anything is blocked: nothing counted yet.
        assert_eq!(stats.snapshot(day0 + SECONDS_PER_DAY), (11, [0; 5]));
        stats.record(Kind::Tracking, day0 + SECONDS_PER_DAY);
        assert_eq!(
            stats.snapshot(day0 + SECONDS_PER_DAY),
            (11, [0, 1, 0, 0, 0])
        );
    }

    fn family_filter() -> Filter {
        let cat = |names: &[&str]| Category {
            block: HashSet64::from_names(names.iter().copied()),
            allow: HashSet64::default(),
        };
        Filter {
            ads: cat(&["ads.example"]),
            dangerous: cat(&["evil.example"]),
            adult: cat(&["adult.example"]),
            gambling: cat(&["bet.example"]),
            never: HashSet64::from_names(NEVER_BLOCK.iter().copied()),
            ..Filter::default()
        }
    }

    fn family_on() -> Config {
        Config {
            ads: true,
            dangerous: true,
            adult: true,
            gambling: true,
            ..Config::default()
        }
    }

    fn blocked_kind(shared: &Shared, name: &str, now: u64) -> bool {
        matches!(
            decide(&query_bytes(name, 1, 7), shared, now),
            Some((_, Action::Reply(_)))
        )
    }

    #[test]
    fn family_switches_block_their_own_lists() {
        let shared = Shared::new(family_filter(), family_on(), vec![]);
        for name in ["adult.example", "www.adult.example", "bet.example"] {
            assert!(blocked_kind(&shared, name, 1000), "{name}");
        }
        assert_eq!(shared.stats.snapshot(1000).1, [0, 0, 0, 2, 1]);
        shared.set_config(Config {
            adult: false,
            ..family_on()
        });
        assert!(!blocked_kind(&shared, "adult.example", 1000));
        assert!(blocked_kind(&shared, "bet.example", 1000));
    }

    #[test]
    fn allowed_sites_get_through_every_list() {
        let shared = Shared::new(family_filter(), family_on(), vec![]);
        for name in ["evil.example", "adult.example", "ads.example"] {
            assert!(blocked_kind(&shared, name, 1000), "{name}");
        }
        shared.set_config(Config {
            allow: vec!["evil.example".into(), "adult.example".into()],
            ..family_on()
        });
        assert!(!blocked_kind(&shared, "evil.example", 1000));
        assert!(!blocked_kind(&shared, "cdn.evil.example", 1000));
        assert!(!blocked_kind(&shared, "adult.example", 1000));
        assert!(blocked_kind(&shared, "ads.example", 1000));
        shared.set_config(family_on());
        assert!(blocked_kind(&shared, "evil.example", 1000));
    }

    #[test]
    fn blocks_are_remembered_for_the_app() {
        let shared = Shared::new(family_filter(), family_on(), vec![]);
        let now = 40 * SECONDS_PER_DAY + 10;
        blocked_kind(&shared, "ads.example", now);
        blocked_kind(&shared, "evil.example", now + 1);
        assert_eq!(shared.stats.dangerous_at(), Some(now + 1));
        let recent = shared.activity.recent_now(now + 2);
        let seen: Vec<_> = recent
            .items
            .iter()
            .map(|i| (i.name.as_str(), i.kind))
            .collect();
        assert_eq!(
            seen,
            [
                ("evil.example", Kind::Dangerous),
                ("ads.example", Kind::Ads)
            ]
        );
        assert_eq!(
            shared.activity.history_now(now + 2).days[0].blocked,
            [1, 0, 1, 0, 0]
        );
    }

    #[test]
    fn nothing_is_remembered_while_paused() {
        let config = Config {
            paused_until: Some(5000),
            ..family_on()
        };
        let shared = Shared::new(family_filter(), config, vec![]);
        assert!(!blocked_kind(&shared, "evil.example", 1000));
        assert!(shared.activity.recent_now(1000).items.is_empty());
        assert!(shared.activity.history_now(1000).days.is_empty());
        assert_eq!(shared.stats.dangerous_at(), None);
        assert!(blocked_kind(&shared, "evil.example", 5000));
        assert_eq!(shared.activity.recent_now(5000).items.len(), 1);
    }

    fn safe_search_on() -> Config {
        Config {
            safe_search: true,
            ..Config::default()
        }
    }

    fn answers_in(reply: &[u8]) -> u16 {
        u16::from_be_bytes([reply[6], reply[7]])
    }

    #[test]
    fn safe_search_names_become_an_alias_with_the_targets_addresses() {
        let upstream = fake_upstream();
        let shared = Shared::new(Filter::empty(), safe_search_on(), vec![upstream]);
        for (name, target) in [
            ("www.google.com", "forcesafesearch.google.com"),
            ("google.co.uk", "forcesafesearch.google.com"),
            ("www.bing.com", "strict.bing.com"),
            ("www.youtube.com", "restrictmoderate.youtube.com"),
            ("duckduckgo.com", "safe.duckduckgo.com"),
        ] {
            let packet = query_bytes(name, 1, 0x3131);
            let reply = answer(&packet, &shared, Via::Udp).unwrap();
            assert_eq!(&reply[..2], &[0x31, 0x31], "{name}");
            assert_eq!(reply[3] & 0x0F, 0, "{name}");
            assert_eq!(answers_in(&reply), 2, "{name}");
            let q = dns::parse_query(&packet).unwrap();
            let first = &reply[q.question_end..];
            assert_eq!(&first[2..4], &[0, 5], "{name}: first record is the alias");
            let alias_len = u16::from_be_bytes([first[10], first[11]]) as usize;
            let wire: Vec<u8> = dns::build_query(0, target, 1)[12..].to_vec();
            assert_eq!(
                &first[12..12 + alias_len],
                &wire[..wire.len() - 4],
                "{name}"
            );
            assert_eq!(last_four(&reply), ANSWER_IP, "{name}");
        }
    }

    #[test]
    fn safe_search_leaves_other_names_and_other_switch_states_alone() {
        let upstream = fake_upstream();
        let on = Shared::new(Filter::empty(), safe_search_on(), vec![upstream]);
        for name in ["mail.google.com", "music.youtube.com", "example.com"] {
            let reply = answer(&query_bytes(name, 1, 1), &on, Via::Udp).unwrap();
            assert_eq!(answers_in(&reply), 1, "{name}");
        }
        let off = Shared::new(Filter::empty(), Config::default(), vec![upstream]);
        let reply = answer(&query_bytes("www.google.com", 1, 1), &off, Via::Udp).unwrap();
        assert_eq!(answers_in(&reply), 1);
        let paused = Shared::new(
            Filter::empty(),
            Config {
                paused_until: Some(5000),
                ..safe_search_on()
            },
            vec![upstream],
        );
        assert!(matches!(
            decide(&query_bytes("www.google.com", 1, 1), &paused, 1000),
            Some((_, Action::Forward))
        ));
    }

    #[test]
    fn safe_search_hides_the_address_hints_in_https_records() {
        let shared = Shared::new(Filter::empty(), safe_search_on(), vec![]);
        for qtype in [dns::TYPE_HTTPS, dns::TYPE_SVCB] {
            match decide(&query_bytes("www.google.com", qtype, 1), &shared, 1000) {
                Some((_, Action::Reply(r))) => {
                    assert_eq!(r[3] & 0x0F, 0);
                    assert_eq!(answers_in(&r), 0);
                }
                _ => panic!("expected an empty answer"),
            }
        }
        assert!(matches!(
            decide(&query_bytes("www.google.com", 16, 1), &shared, 1000),
            Some((_, Action::Forward))
        ));
    }

    #[test]
    fn safe_search_fails_with_servfail_when_nothing_answers() {
        let closed = {
            let s = UdpSocket::bind("127.0.0.1:0").unwrap();
            s.local_addr().unwrap()
        };
        let shared = Shared::new(Filter::empty(), safe_search_on(), vec![closed]);
        let reply = answer(&query_bytes("www.google.com", 1, 9), &shared, Via::Udp).unwrap();
        assert_eq!(reply[3] & 0x0F, 2);
        assert!(shared.upstream_failed.load(Ordering::Acquire));
    }

    #[test]
    fn a_blocked_search_site_is_blocked_not_aliased() {
        let mut filter = blocking(&["www.bing.com"]);
        filter.never = HashSet64::default();
        let config = Config {
            ads: true,
            safe_search: true,
            ..Config::default()
        };
        let shared = Shared::new(filter, config, vec![]);
        assert!(blocked_kind(&shared, "www.bing.com", 1000));
    }

    use crate::filter::doh::mock;

    const PRIVATE_IP: [u8; 4] = [5, 6, 7, 8];

    fn private_on() -> Config {
        Config {
            private_lookups: true,
            ..Config::default()
        }
    }

    fn private_shared(config: Config, mock_url: &str) -> (Shared, SocketAddr) {
        let upstream = fake_upstream();
        let shared = Shared::new(Filter::empty(), config, vec![upstream])
            .with_doh(Doh::plain_http(mock_url));
        (shared, upstream)
    }

    fn answered_by(shared: &Shared, name: &str, via: Via) -> [u8; 4] {
        last_four(&answer(&query_bytes(name, 1, 7), shared, via).unwrap())
    }

    fn hits(m: &mock::Mock) -> usize {
        m.hits.load(Ordering::SeqCst)
    }

    fn broken() -> mock::Mock {
        mock::serve(|_, _| {
            Some(mock::Reply {
                status: 503,
                content_type: "text/plain",
                body: Vec::new(),
            })
        })
    }

    #[test]
    fn private_off_never_touches_the_encrypted_way() {
        let m = mock::serve(mock::good(PRIVATE_IP));
        let (shared, _) = private_shared(Config::default(), &m.url);
        assert_eq!(answered_by(&shared, "example.com", Via::Udp), ANSWER_IP);
        assert_eq!(hits(&m), 0);
        assert_eq!(shared.lookups(1000), Lookups::Plain);
    }

    #[test]
    fn private_on_sends_public_names_to_the_encrypted_way() {
        let m = mock::serve(mock::good(PRIVATE_IP));
        let (shared, _) = private_shared(private_on(), &m.url);
        assert_eq!(answered_by(&shared, "example.com", Via::Udp), PRIVATE_IP);
        assert_eq!(hits(&m), 1);
        assert_eq!(shared.lookups(1000), Lookups::Private);
        let reply = answer(&query_bytes("example.com", 1, 0x4242), &shared, Via::Udp).unwrap();
        assert_eq!(&reply[..2], &[0x42, 0x42]);
    }

    #[test]
    fn local_names_stay_on_the_network() {
        let m = mock::serve(mock::good(PRIVATE_IP));
        let (shared, _) = private_shared(private_on(), &m.url);
        shared.set_local_suffixes(vec!["corp.example.com".to_string()]);
        for name in ["printer", "nas.local", "router.lan", "pc.corp.example.com"] {
            assert_eq!(answered_by(&shared, name, Via::Udp), ANSWER_IP, "{name}");
        }
        assert_eq!(hits(&m), 0);
        assert_eq!(shared.lookups(1000), Lookups::Private);
    }

    #[test]
    fn a_pause_sends_everything_the_plain_way() {
        let m = mock::serve(mock::good(PRIVATE_IP));
        let config = Config {
            paused_until: Some(unix_now() + 600),
            ..private_on()
        };
        let (shared, _) = private_shared(config, &m.url);
        assert_eq!(answered_by(&shared, "example.com", Via::Udp), ANSWER_IP);
        assert_eq!(hits(&m), 0);
        assert_eq!(shared.lookups(unix_now()), Lookups::Plain);
    }

    #[test]
    fn a_failure_answers_plain_and_the_next_lookups_skip_the_encrypted_way() {
        let m = broken();
        let (shared, _) = private_shared(private_on(), &m.url);
        assert_eq!(answered_by(&shared, "example.com", Via::Udp), ANSWER_IP);
        assert_eq!(hits(&m), 1);
        assert_eq!(shared.lookups(1000), Lookups::PrivateFallback);
        assert_eq!(answered_by(&shared, "example.org", Via::Udp), ANSWER_IP);
        assert_eq!(hits(&m), 1);
    }

    #[test]
    fn the_encrypted_way_is_tried_again_after_the_wait() {
        let m = mock::serve(|n, r| {
            if n == 0 {
                Some(mock::Reply {
                    status: 503,
                    content_type: "text/plain",
                    body: Vec::new(),
                })
            } else {
                mock::good(PRIVATE_IP)(n, r)
            }
        });
        let (shared, _) = private_shared(private_on(), &m.url);
        let start = Instant::now();
        let ask = |at: Instant| {
            let packet = query_bytes("example.com", 1, 3);
            let q = dns::parse_query(&packet).unwrap();
            last_four(&lookup_at(&packet, &q, &shared, Via::Udp, at).unwrap())
        };
        assert_eq!(ask(start), ANSWER_IP);
        assert_eq!(
            ask(start + doh::FALLBACK_FOR - Duration::from_secs(1)),
            ANSWER_IP
        );
        assert_eq!(hits(&m), 1);
        assert_eq!(ask(start + doh::FALLBACK_FOR), PRIVATE_IP);
        assert_eq!(hits(&m), 2);
        assert_eq!(shared.lookups(1000), Lookups::Private);
    }

    #[test]
    fn switching_private_lookups_off_and_on_starts_afresh() {
        let m = broken();
        let (shared, _) = private_shared(private_on(), &m.url);
        answered_by(&shared, "example.com", Via::Udp);
        assert_eq!(shared.lookups(1000), Lookups::PrivateFallback);
        shared.set_config(Config::default());
        assert_eq!(shared.lookups(1000), Lookups::Plain);
        shared.set_config(private_on());
        assert_eq!(shared.lookups(1000), Lookups::Private);
        answered_by(&shared, "example.com", Via::Udp);
        assert_eq!(hits(&m), 2);
    }

    #[test]
    fn a_long_answer_is_cut_for_udp_but_not_for_tcp() {
        let m = mock::serve(|_, r| {
            let mut body = mock::answer_to(&r.body, PRIVATE_IP);
            body.resize(body.len() + 700, 0);
            Some(mock::Reply {
                status: 200,
                content_type: "application/dns-message",
                body,
            })
        });
        let (shared, _) = private_shared(private_on(), &m.url);
        let packet = query_bytes("example.com", 1, 11);
        let udp = answer(&packet, &shared, Via::Udp).unwrap();
        assert!(dns::truncated(&udp));
        assert_eq!(answers_in(&udp), 0);
        assert_eq!(&udp[..2], &11u16.to_be_bytes());
        let tcp = answer(&packet, &shared, Via::Tcp).unwrap();
        assert!(!dns::truncated(&tcp));
        assert_eq!(answers_in(&tcp), 1);
        assert!(tcp.len() > 512);
    }

    #[test]
    fn safe_search_addresses_come_through_the_encrypted_way() {
        let m = mock::serve(mock::good(PRIVATE_IP));
        let config = Config {
            safe_search: true,
            ..private_on()
        };
        let (shared, _) = private_shared(config, &m.url);
        let reply = answer(&query_bytes("www.google.com", 1, 5), &shared, Via::Udp).unwrap();
        assert_eq!(answers_in(&reply), 2);
        assert_eq!(last_four(&reply), PRIVATE_IP);
        assert_eq!(hits(&m), 1);
        let seen = m.wire.lock().unwrap();
        let asked = dns::parse_query(&seen[0].body).unwrap();
        assert_eq!(asked.question.name, "forcesafesearch.google.com");
    }

    #[test]
    fn upstream_falls_back_to_quad9() {
        let addrs = upstream_addrs(&[]);
        assert_eq!(addrs[0], "9.9.9.9:53".parse().unwrap());
        let own = upstream_addrs(&["192.168.1.1".parse().unwrap()]);
        assert_eq!(own, vec!["192.168.1.1:53".parse().unwrap()]);
    }
}
