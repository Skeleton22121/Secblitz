//! Private lookups: names go to Quad9 over HTTPS. The service is the PC's own DNS server, so the client
//! uses Quad9's built-in addresses and never asks the system for `dns.quad9.net` (it would loop back here).

use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE};

use super::dns::{self, Query};

pub const HOST: &str = "dns.quad9.net";
pub const URL: &str = "https://dns.quad9.net/dns-query";
pub const PINNED: [SocketAddr; 3] = [
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9)), 443),
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(149, 112, 112, 112)), 443),
    SocketAddr::new(
        IpAddr::V6(Ipv6Addr::new(0x2620, 0x00fe, 0, 0, 0, 0, 0, 0x00fe)),
        443,
    ),
];

const MEDIA_TYPE: &str = "application/dns-message";
const CONNECT_TIMEOUT: Duration = Duration::from_millis(1500);
const TOTAL_TIMEOUT: Duration = Duration::from_millis(2500);
const IDLE_CONNECTION: Duration = Duration::from_secs(30);
/// A reused connection is quick or dead (Wi-Fi switch, sleep, NAT timeout), so the first try is short.
const WARM_FIRST_TRY: Duration = Duration::from_millis(1000);
/// Keeps a NAT or firewall from dropping an idle connection.
const KEEPALIVE_AFTER: Duration = Duration::from_secs(10);
const KEEPALIVE_EVERY: Duration = Duration::from_secs(5);
const MAX_REPLY: usize = 64 * 1024;
pub const FALLBACK_FOR: Duration = Duration::from_secs(120);
const PROBE_LEASE: Duration = Duration::from_secs(5);

pub struct Doh {
    url: String,
    pinned: bool,
    client: Mutex<Option<Client>>,
    last_answer: Mutex<Option<Instant>>,
}

impl Doh {
    pub fn quad9() -> Doh {
        Doh::new(URL, true)
    }

    #[cfg(test)]
    pub fn plain_http(url: &str) -> Doh {
        Doh::new(url, false)
    }

    fn new(url: &str, pinned: bool) -> Doh {
        Doh {
            url: url.to_string(),
            pinned,
            client: Mutex::new(None),
            last_answer: Mutex::new(None),
        }
    }

    fn client(&self) -> Option<Client> {
        let mut slot = self.client.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
            *slot = build(self.pinned);
        }
        slot.clone()
    }

    fn forget_connections(&self) {
        *self.client.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    fn recently_answered(&self, now: Instant) -> bool {
        self.last_answer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some_and(|t| now.saturating_duration_since(t) < IDLE_CONNECTION)
    }

    /// A stale pooled connection is replaced and the lookup retried once.
    pub fn ask(&self, packet: &[u8], q: &Query) -> Option<Vec<u8>> {
        // Id 0 on the wire lets the provider reuse its answers (RFC 8484).
        let wire = dns::with_id(packet, 0);
        let started = Instant::now();
        let deadline = started + TOTAL_TIMEOUT;
        let warm = self.recently_answered(started);
        for attempt in 0..2 {
            let left = deadline.checked_duration_since(Instant::now())?;
            if left < Duration::from_millis(100) {
                return None;
            }
            let limit = if attempt == 0 && warm {
                left.min(WARM_FIRST_TRY)
            } else {
                left
            };
            let client = self.client()?;
            match self.exchange(&client, &wire, q, limit) {
                Ok(reply) => {
                    *self
                        .last_answer
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
                    return Some(dns::with_id(&reply, q.id));
                }
                Err(true) if attempt == 0 => self.forget_connections(),
                Err(_) => return None,
            }
        }
        None
    }

    /// `Err(true)` when asking again on a new connection could help.
    fn exchange(
        &self,
        client: &Client,
        wire: &[u8],
        q: &Query,
        left: Duration,
    ) -> Result<Vec<u8>, bool> {
        let response = client
            .post(&self.url)
            .header(CONTENT_TYPE, MEDIA_TYPE)
            .header(ACCEPT, MEDIA_TYPE)
            .timeout(left)
            .body(wire.to_vec())
            .send()
            .map_err(|_| true)?;
        if response.status().as_u16() != 200 {
            return Err(false);
        }
        let is_dns = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .is_some_and(|v| v.trim().eq_ignore_ascii_case(MEDIA_TYPE));
        if !is_dns
            || response
                .content_length()
                .is_some_and(|n| n > MAX_REPLY as u64)
        {
            return Err(false);
        }
        let mut body = Vec::new();
        response
            .take(MAX_REPLY as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|_| true)?;
        if body.len() > MAX_REPLY {
            return Err(false);
        }
        if dns::reply_matches(&body, 0, &q.question) {
            Ok(body)
        } else {
            Err(false)
        }
    }
}

/// Quad9 answers HTTP/1.1 with an error, so the client needs HTTP/2.
fn build(pinned: bool) -> Option<Client> {
    let mut builder = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .gzip(false)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .pool_idle_timeout(IDLE_CONNECTION)
        .tcp_keepalive(KEEPALIVE_AFTER)
        .tcp_keepalive_interval(KEEPALIVE_EVERY);
    if pinned {
        builder = builder.https_only(true).resolve_to_addrs(HOST, &PINNED);
    }
    builder.build().ok()
}

#[derive(Default)]
pub struct Fallback {
    until: Mutex<Option<Instant>>,
}

impl Fallback {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Instant>> {
        self.until.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn may_try(&self, now: Instant) -> bool {
        let mut until = self.lock();
        match *until {
            None => true,
            Some(t) if now >= t => {
                *until = Some(now + PROBE_LEASE);
                true
            }
            Some(_) => false,
        }
    }

    pub fn worked(&self) {
        *self.lock() = None;
    }

    pub fn failed(&self, now: Instant) {
        *self.lock() = Some(now + FALLBACK_FOR);
    }

    pub fn active(&self) -> bool {
        self.lock().is_some()
    }
}

const LOCAL_ENDINGS: [&str; 10] = [
    "local",
    "localhost",
    "lan",
    "home",
    "home.arpa",
    "internal",
    "localdomain",
    "corp",
    "intranet",
    "private",
];

/// Public-looking endings that only routers answer to.
const ROUTER_NAMES: [&str; 12] = [
    "fritz.box",
    "speedport.ip",
    "routerlogin.net",
    "routerlogin.com",
    "tplinkwifi.net",
    "tplinkap.net",
    "tplinkrepeater.net",
    "router.asus.com",
    "orbilogin.com",
    "orbilogin.net",
    "mynetworksettings.com",
    "myfiosgateway.com",
];

fn under(name: &str, suffix: &str) -> bool {
    name.len() > suffix.len()
        && name.ends_with(suffix)
        && name.as_bytes()[name.len() - suffix.len() - 1] == b'.'
}

/// `name` is lowercase without a closing dot; asking a public server would leak it.
pub fn is_local_name(name: &str, suffixes: &[String]) -> bool {
    if name.is_empty() || !name.contains('.') {
        return true;
    }
    LOCAL_ENDINGS.iter().any(|s| under(name, s))
        || ROUTER_NAMES.iter().any(|s| name == *s || under(name, s))
        || suffixes.iter().any(|s| name == s || under(name, s))
        || private_reverse_name(name)
}

fn private_reverse_name(name: &str) -> bool {
    if let Some(rest) = name.strip_suffix(".in-addr.arpa") {
        let mut octets = Vec::new();
        for label in rest.split('.') {
            match label.parse::<u8>() {
                Ok(o) if octets.len() < 4 => octets.push(o),
                _ => return false,
            }
        }
        octets.reverse();
        return private_v4_start(&octets);
    }
    if let Some(rest) = name.strip_suffix(".ip6.arpa") {
        let mut nibbles = Vec::new();
        for label in rest.split('.') {
            match (label.len(), u8::from_str_radix(label, 16)) {
                (1, Ok(n)) if nibbles.len() < 32 => nibbles.push(n),
                _ => return false,
            }
        }
        nibbles.reverse();
        // fc00::/7 (local addresses) and fe80::/10 (this link only).
        return matches!(
            nibbles.as_slice(),
            [0xf, 0xc | 0xd, ..] | [0xf, 0xe, 0x8..=0xb, ..]
        );
    }
    false
}

fn private_v4_start(octets: &[u8]) -> bool {
    match octets {
        [10 | 127, ..] | [169, 254, ..] | [192, 168, ..] => true,
        [172, b, ..] => (16..=31).contains(b),
        [100, b, ..] => (64..=127).contains(b),
        _ => false,
    }
}

#[cfg(test)]
pub(crate) mod mock {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    pub(crate) fn query(name: &str, id: u16) -> (Vec<u8>, Query) {
        let packet = dns::build_query(id, name, dns::TYPE_A);
        let q = dns::parse_query(&packet).unwrap();
        (packet, q)
    }

    pub(crate) fn answer_to(wire: &[u8], ip: [u8; 4]) -> Vec<u8> {
        let q = dns::parse_query(wire).unwrap();
        let mut out = wire[..2].to_vec();
        out.extend_from_slice(&[0x81, 0x80, 0, 1, 0, 1, 0, 0, 0, 0]);
        out.extend_from_slice(&wire[12..q.question_end]);
        out.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4]);
        out.extend_from_slice(&ip);
        out
    }

    pub(crate) struct Request {
        pub(crate) method: String,
        pub(crate) content_type: String,
        pub(crate) accept: String,
        pub(crate) body: Vec<u8>,
    }

    pub(crate) struct Reply {
        pub(crate) status: u16,
        pub(crate) content_type: &'static str,
        pub(crate) body: Vec<u8>,
    }

    pub(crate) struct Mock {
        pub(crate) url: String,
        pub(crate) hits: Arc<AtomicUsize>,
        pub(crate) wire: Arc<Mutex<Vec<Request>>>,
    }

    fn read_request(stream: &mut std::net::TcpStream) -> Option<Request> {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).ok()?;
            head.push(byte[0]);
        }
        let text = String::from_utf8_lossy(&head).to_string();
        let mut lines = text.lines();
        let method = lines.next()?.split(' ').next()?.to_string();
        let header = |name: &str| {
            text.lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.trim()
                        .eq_ignore_ascii_case(name)
                        .then(|| v.trim().to_string())
                })
                .unwrap_or_default()
        };
        let length: usize = header("content-length").parse().unwrap_or(0);
        let mut body = vec![0u8; length];
        stream.read_exact(&mut body).ok()?;
        Some(Request {
            method,
            content_type: header("content-type"),
            accept: header("accept"),
            body,
        })
    }

    pub(crate) fn serve(
        handler: impl Fn(usize, &Request) -> Option<Reply> + Send + 'static,
    ) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/dns-query", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let wire = Arc::new(Mutex::new(Vec::new()));
        let (h, w) = (Arc::clone(&hits), Arc::clone(&wire));
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Some(request) = read_request(&mut stream) else {
                    continue;
                };
                let n = h.fetch_add(1, Ordering::SeqCst);
                let reply = handler(n, &request);
                w.lock().unwrap().push(request);
                let Some(reply) = reply else { continue };
                let head = format!(
                    "HTTP/1.1 {} X\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    reply.status,
                    reply.content_type,
                    reply.body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&reply.body);
            }
        });
        Mock { url, hits, wire }
    }

    pub(crate) fn good(ip: [u8; 4]) -> impl Fn(usize, &Request) -> Option<Reply> + Send + 'static {
        move |_, r| {
            Some(Reply {
                status: 200,
                content_type: MEDIA_TYPE,
                body: answer_to(&r.body, ip),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::*;
    use super::*;
    use std::net::TcpListener;
    use std::sync::atomic::Ordering;
    use std::thread;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn local(name: &str) -> bool {
        is_local_name(name, &names(&["corp.example.com", "fritz.box"]))
    }

    #[test]
    fn single_words_and_home_endings_are_local() {
        for name in [
            "",
            "printer",
            "wpad",
            "nas.local",
            "router.lan",
            "tv.home",
            "x.home.arpa",
            "files.internal",
            "pc.localdomain",
            "a.b.nas.local",
            "fritz.box",
            "nas.fritz.box",
            "speedport.ip",
            "routerlogin.net",
            "www.routerlogin.net",
            "tplinkwifi.net",
            "dc01.corp",
            "wiki.intranet",
            "scanner.private",
            "app.localhost",
        ] {
            assert!(local(name), "{name}");
        }
    }

    #[test]
    fn ordinary_names_are_not_local() {
        for name in [
            "example.com",
            "www.example.com",
            "localhost.example.com",
            "mylocal.com",
            "homeinternal.example",
            "internal.example",
            "home.arpa.example.com",
            "e164.arpa",
            "corp.com",
            "fritz.box.example.com",
            "notrouterlogin.net",
            "evil.speedport.ip.example",
        ] {
            assert!(!local(name), "{name}");
        }
    }

    #[test]
    fn the_endings_must_match_whole_labels() {
        assert!(!local("x.evillocal"));
        assert!(!local("x.notlan"));
        assert!(local("x.lan"));
        assert!(!local("lan.example"));
    }

    #[test]
    fn network_suffixes_are_local() {
        assert!(local("laptop.corp.example.com"));
        assert!(local("corp.example.com"));
        assert!(local("box.fritz.box"));
        assert!(!local("example.com"));
        assert!(!local("evilcorp.example.com"));
        assert!(!local("corp.example.com.evil.net"));
        assert!(!is_local_name("laptop.corp.example.com", &[]));
    }

    #[test]
    fn private_reverse_lookups_stay_local() {
        for name in [
            "1.0.0.10.in-addr.arpa",
            "5.1.168.192.in-addr.arpa",
            "1.1.254.169.in-addr.arpa",
            "9.9.16.172.in-addr.arpa",
            "9.9.31.172.in-addr.arpa",
            "9.9.64.100.in-addr.arpa",
            "9.9.127.100.in-addr.arpa",
            "1.0.0.127.in-addr.arpa",
            "10.in-addr.arpa",
            "168.192.in-addr.arpa",
        ] {
            assert!(local(name), "{name}");
        }
    }

    #[test]
    fn public_reverse_lookups_are_not_local() {
        for name in [
            "9.9.9.9.in-addr.arpa",
            "8.8.8.8.in-addr.arpa",
            "9.9.15.172.in-addr.arpa",
            "9.9.32.172.in-addr.arpa",
            "9.9.63.100.in-addr.arpa",
            "9.9.128.100.in-addr.arpa",
            "1.1.253.169.in-addr.arpa",
            "172.in-addr.arpa",
            "256.1.1.10.in-addr.arpa",
            "x.1.1.10.in-addr.arpa",
            "5.4.3.2.1.in-addr.arpa",
            "in-addr.arpa",
        ] {
            assert!(!local(name), "{name}");
        }
    }

    fn nibbles(address: &str) -> String {
        let a: Ipv6Addr = address.parse().unwrap();
        let hex: String = a.octets().iter().map(|b| format!("{b:02x}")).collect();
        let mut labels: Vec<String> = hex.chars().map(String::from).collect();
        labels.reverse();
        format!("{}.ip6.arpa", labels.join("."))
    }

    #[test]
    fn local_ipv6_reverse_lookups_stay_local() {
        for address in ["fd12:3456::1", "fc00::1", "fe80::1", "febf::5"] {
            assert!(local(&nibbles(address)), "{address}");
        }
        assert!(local("d.f.ip6.arpa"));
        assert!(local("8.e.f.ip6.arpa"));
    }

    #[test]
    fn public_ipv6_reverse_lookups_are_not_local() {
        for address in ["2620:fe::fe", "2001:db8::1", "fec0::1", "ff02::1"] {
            assert!(!local(&nibbles(address)), "{address}");
        }
        assert!(!local("e.f.ip6.arpa"));
        assert!(!local("xx.f.ip6.arpa"));
        assert!(!local("ip6.arpa"));
    }

    #[test]
    fn fallback_waits_two_minutes_then_probes_one_at_a_time() {
        let fb = Fallback::default();
        let t0 = Instant::now();
        assert!(!fb.active());
        assert!(fb.may_try(t0));
        fb.failed(t0);
        assert!(fb.active());
        assert!(!fb.may_try(t0));
        assert!(!fb.may_try(t0 + FALLBACK_FOR - Duration::from_secs(1)));
        let t1 = t0 + FALLBACK_FOR;
        assert!(fb.may_try(t1));
        assert!(!fb.may_try(t1));
        assert!(!fb.may_try(t1 + Duration::from_secs(4)));
        assert!(fb.active());
        let t2 = t1 + Duration::from_secs(3);
        fb.failed(t2);
        assert!(!fb.may_try(t2 + FALLBACK_FOR - Duration::from_secs(1)));
        assert!(fb.may_try(t2 + FALLBACK_FOR));
        fb.worked();
        assert!(!fb.active());
        assert!(fb.may_try(t2));
        assert!(fb.may_try(t2));
    }

    #[test]
    fn a_lost_probe_does_not_block_the_next_one_for_ever() {
        let fb = Fallback::default();
        let t0 = Instant::now();
        fb.failed(t0);
        let t1 = t0 + FALLBACK_FOR;
        assert!(fb.may_try(t1));
        assert!(!fb.may_try(t1 + PROBE_LEASE - Duration::from_millis(1)));
        assert!(fb.may_try(t1 + PROBE_LEASE));
    }

    #[test]
    fn a_lookup_goes_out_with_id_zero_and_comes_back_with_the_callers_id() {
        let mock = serve(good([5, 6, 7, 8]));
        let doh = Doh::plain_http(&mock.url);
        let (packet, q) = query("example.com", 0xBEEF);
        let reply = doh.ask(&packet, &q).expect("an answer");
        assert_eq!(&reply[..2], &[0xBE, 0xEF]);
        assert_eq!(reply[reply.len() - 4..], [5, 6, 7, 8]);
        assert!(dns::reply_matches(&reply, 0xBEEF, &q.question));
        let seen = mock.wire.lock().unwrap();
        let sent = &seen[0];
        assert_eq!(sent.method, "POST");
        assert_eq!(sent.content_type, MEDIA_TYPE);
        assert_eq!(sent.accept, MEDIA_TYPE);
        assert_eq!(&sent.body[..2], &[0, 0]);
        assert_eq!(sent.body[2..], packet[2..]);
    }

    #[test]
    fn the_content_type_may_carry_parameters_in_any_case() {
        let mock = serve(|_, r| {
            Some(Reply {
                status: 200,
                content_type: "Application/DNS-Message; charset=binary",
                body: answer_to(&r.body, [1, 1, 1, 1]),
            })
        });
        let doh = Doh::plain_http(&mock.url);
        let (packet, q) = query("example.com", 5);
        assert!(doh.ask(&packet, &q).is_some());
    }

    fn refused_by(reply: impl Fn(&Request) -> Reply + Send + 'static) -> usize {
        let mock = serve(move |_, r| Some(reply(r)));
        let doh = Doh::plain_http(&mock.url);
        let (packet, q) = query("example.com", 9);
        assert!(doh.ask(&packet, &q).is_none());
        mock.hits.load(Ordering::SeqCst)
    }

    #[test]
    fn an_error_status_is_a_failure_and_is_not_repeated() {
        let hits = refused_by(|r| Reply {
            status: 503,
            content_type: MEDIA_TYPE,
            body: answer_to(&r.body, [1, 1, 1, 1]),
        });
        assert_eq!(hits, 1);
    }

    #[test]
    fn a_redirect_is_a_failure() {
        refused_by(|r| Reply {
            status: 302,
            content_type: MEDIA_TYPE,
            body: answer_to(&r.body, [1, 1, 1, 1]),
        });
    }

    #[test]
    fn a_web_page_instead_of_an_answer_is_a_failure() {
        // What a captive portal sends for every request.
        refused_by(|r| Reply {
            status: 200,
            content_type: "text/html",
            body: answer_to(&r.body, [1, 1, 1, 1]),
        });
    }

    #[test]
    fn an_answer_to_another_question_is_a_failure() {
        refused_by(|_| {
            let other = dns::build_query(0, "other.example", dns::TYPE_A);
            Reply {
                status: 200,
                content_type: MEDIA_TYPE,
                body: answer_to(&other, [6, 6, 6, 6]),
            }
        });
    }

    #[test]
    fn an_answer_with_the_wrong_id_is_a_failure() {
        refused_by(|r| {
            let mut body = answer_to(&r.body, [6, 6, 6, 6]);
            body[1] = 1;
            Reply {
                status: 200,
                content_type: MEDIA_TYPE,
                body,
            }
        });
    }

    #[test]
    fn an_oversized_answer_is_a_failure() {
        refused_by(|r| {
            let mut body = answer_to(&r.body, [6, 6, 6, 6]);
            body.resize(MAX_REPLY + 1, 0);
            Reply {
                status: 200,
                content_type: MEDIA_TYPE,
                body,
            }
        });
    }

    #[test]
    fn a_connection_that_was_closed_is_retried_once() {
        let ok = good([9, 9, 9, 1]);
        let mock = serve(move |n, r| if n == 0 { None } else { ok(n, r) });
        let doh = Doh::plain_http(&mock.url);
        let (packet, q) = query("example.com", 3);
        let reply = doh.ask(&packet, &q).expect("the second try works");
        assert_eq!(reply[reply.len() - 4..], [9, 9, 9, 1]);
        assert_eq!(mock.hits.load(Ordering::SeqCst), 2);

        let dead = serve(|_, _| None);
        let doh = Doh::plain_http(&dead.url);
        assert!(doh.ask(&packet, &q).is_none());
        assert_eq!(dead.hits.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_connection_that_went_quiet_is_replaced_instead_of_failing() {
        let ok = good([7, 7, 7, 7]);
        let mock = serve(move |n, r| {
            if n == 1 {
                thread::sleep(Duration::from_millis(1300));
                None
            } else {
                ok(n, r)
            }
        });
        let doh = Doh::plain_http(&mock.url);
        let (packet, q) = query("example.com", 4);
        assert!(doh.ask(&packet, &q).is_some());
        let start = Instant::now();
        let reply = doh.ask(&packet, &q).expect("the new connection answers");
        assert_eq!(reply[reply.len() - 4..], [7, 7, 7, 7]);
        assert!(start.elapsed() < TOTAL_TIMEOUT);
        assert_eq!(mock.hits.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn nothing_listening_fails_within_the_time_limit() {
        let url = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}/dns-query", l.local_addr().unwrap())
        };
        let doh = Doh::plain_http(&url);
        let (packet, q) = query("example.com", 3);
        let start = Instant::now();
        assert!(doh.ask(&packet, &q).is_none());
        // Windows retries a refused connection for about two seconds.
        assert!(start.elapsed() < TOTAL_TIMEOUT + Duration::from_millis(500));
    }

    #[test]
    fn a_server_that_never_answers_stops_at_the_time_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/dns-query", listener.local_addr().unwrap());
        let held = thread::spawn(move || {
            let mut open = Vec::new();
            while let Ok((s, _)) = listener.accept() {
                open.push(s);
                if open.len() == 2 {
                    break;
                }
            }
            thread::sleep(Duration::from_secs(4));
        });
        let doh = Doh::plain_http(&url);
        let (packet, q) = query("example.com", 3);
        let start = Instant::now();
        assert!(doh.ask(&packet, &q).is_none());
        assert!(start.elapsed() < TOTAL_TIMEOUT + Duration::from_secs(1));
        drop(held);
    }

    #[test]
    fn quad9_is_pinned_to_its_addresses() {
        assert_eq!(PINNED[0], "9.9.9.9:443".parse().unwrap());
        assert_eq!(PINNED[1], "149.112.112.112:443".parse().unwrap());
        assert_eq!(PINNED[2], "[2620:fe::fe]:443".parse().unwrap());
        assert!(URL.contains(HOST));
        assert!(Doh::quad9().client().is_some());
    }

    #[test]
    #[ignore = "needs the internet"]
    fn live_quad9_answers() {
        let doh = Doh::quad9();
        let (packet, q) = query("example.com", 0x1234);
        let reply = doh.ask(&packet, &q).expect("Quad9 should answer");
        assert!(dns::reply_matches(&reply, 0x1234, &q.question));
        let found = dns::parse_addresses(&reply, dns::TYPE_A).expect("addresses");
        assert!(!found.is_empty());
    }
}
