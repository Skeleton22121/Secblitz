//! The servers the filter forwards to and whether the connection is metered.
//! Other hosts get an empty answer so portable code still builds.

use std::net::{IpAddr, Ipv6Addr};

const MAX_SERVERS: usize = 4;
const MAX_SUFFIXES: usize = 16;

/// The old Windows default DNS addresses `fec0:0:0:ffff::1` to `::3`, which
/// appear on adapters that have no real IPv6 DNS server.
fn is_site_local_default(ip: &Ipv6Addr) -> bool {
    let s = ip.segments();
    s[..7] == [0xfec0, 0, 0, 0xffff, 0, 0, 0] && (1..=3).contains(&s[7])
}

/// Keeps only addresses worth forwarding to, in order and without repeats:
/// not loopback (that is this filter), not unspecified, not IPv6 link-local
/// (it would need an interface number) and not the Windows placeholders.
pub fn usable_servers(found: impl IntoIterator<Item = IpAddr>) -> Vec<IpAddr> {
    let mut out: Vec<IpAddr> = Vec::new();
    for ip in found {
        let ip = ip.to_canonical();
        let skip = ip.is_loopback()
            || ip.is_unspecified()
            || match ip {
                IpAddr::V6(v6) => {
                    (v6.segments()[0] & 0xffc0) == 0xfe80 || is_site_local_default(&v6)
                }
                IpAddr::V4(_) => false,
            };
        if !skip && !out.contains(&ip) {
            out.push(ip);
            if out.len() == MAX_SERVERS {
                break;
            }
        }
    }
    out
}

/// Skips a one-word suffix of up to three letters: it would be a public ending like `com` and make the internet look local.
pub fn usable_suffixes(found: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for suffix in found {
        let suffix = suffix.trim().trim_matches('.').to_ascii_lowercase();
        let plain = !suffix.is_empty()
            && suffix.len() <= 253
            && suffix
                .split('.')
                .all(|l| (1..=63).contains(&l.len()) && l.bytes().all(is_name_byte));
        if plain && (suffix.contains('.') || suffix.len() > 3) && !out.contains(&suffix) {
            out.push(suffix);
            if out.len() == MAX_SUFFIXES {
                break;
            }
        }
    }
    out
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

pub fn is_metered(cost: i32, over_data_limit: bool, roaming: bool) -> bool {
    // 2 = fixed, 3 = variable (NL_NETWORK_CONNECTIVITY_COST_HINT).
    matches!(cost, 2 | 3) || over_data_limit || roaming
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use std::net::Ipv4Addr;
    use std::ptr::null;
    use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS, WIN32_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST,
        IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_TUNNEL, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows_sys::Win32::Networking::WinSock::{
        AF_INET, AF_INET6, AF_UNSPEC, NL_NETWORK_CONNECTIVITY_HINT, SOCKADDR_IN, SOCKADDR_IN6,
        SOCKET_ADDRESS,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    /// SAFETY contract: `address` comes from a successful GetAdaptersAddresses
    /// call whose buffer is still alive.
    unsafe fn socket_ip(address: &SOCKET_ADDRESS) -> Option<IpAddr> {
        if address.lpSockaddr.is_null() {
            return None;
        }
        let family = (*address.lpSockaddr).sa_family;
        if family == AF_INET
            && address.iSockaddrLength as usize >= std::mem::size_of::<SOCKADDR_IN>()
        {
            let sa = &*(address.lpSockaddr as *const SOCKADDR_IN);
            // S_addr holds the address in network byte order.
            let octets = sa.sin_addr.S_un.S_addr.to_ne_bytes();
            Some(IpAddr::V4(Ipv4Addr::from(octets)))
        } else if family == AF_INET6
            && address.iSockaddrLength as usize >= std::mem::size_of::<SOCKADDR_IN6>()
        {
            let sa = &*(address.lpSockaddr as *const SOCKADDR_IN6);
            Some(IpAddr::V6(Ipv6Addr::from(sa.sin6_addr.u.Byte)))
        } else {
            None
        }
    }

    fn adapter_buffer() -> Option<Vec<u64>> {
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
        let mut size: u32 = 16 * 1024;
        for _ in 0..4 {
            // u64 elements keep the buffer aligned for the structures in it.
            let mut buf = vec![0u64; (size as usize).div_ceil(8)];
            // SAFETY: the buffer is at least `size` bytes and 8-byte aligned.
            let rc: WIN32_ERROR = unsafe {
                GetAdaptersAddresses(
                    AF_UNSPEC as u32,
                    flags,
                    null::<c_void>(),
                    buf.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>(),
                    &mut size,
                )
            };
            match rc {
                ERROR_SUCCESS => return Some(buf),
                ERROR_BUFFER_OVERFLOW => continue,
                _ => return None,
            }
        }
        None
    }

    pub fn upstream_servers() -> Vec<IpAddr> {
        let Some(buf) = adapter_buffer() else {
            return Vec::new();
        };
        let mut found = Vec::new();
        // SAFETY: `buf` holds a valid linked list written by Windows; every
        // pointer followed below points inside it and it outlives the loop.
        unsafe {
            let mut adapter = buf.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            while !adapter.is_null() {
                let a = &*adapter;
                if a.OperStatus == IfOperStatusUp
                    && a.IfType != IF_TYPE_SOFTWARE_LOOPBACK
                    && a.IfType != IF_TYPE_TUNNEL
                {
                    let mut dns = a.FirstDnsServerAddress;
                    while !dns.is_null() {
                        let d = &*dns;
                        found.extend(socket_ip(&d.Address));
                        dns = d.Next;
                    }
                }
                adapter = a.Next;
            }
        }
        usable_servers(found)
    }

    /// SAFETY contract: `p` is null or points to a NUL-terminated UTF-16 string.
    unsafe fn wide_string(p: *const u16) -> Option<String> {
        if p.is_null() {
            return None;
        }
        let mut units = Vec::new();
        for i in 0..256 {
            let c = *p.add(i);
            if c == 0 {
                break;
            }
            units.push(c);
        }
        String::from_utf16(&units).ok()
    }

    pub fn dns_suffixes() -> Vec<String> {
        let Some(buf) = adapter_buffer() else {
            return Vec::new();
        };
        let mut found = Vec::new();
        // SAFETY: as in `upstream_servers`; the suffix strings live inside
        // the same buffer and end with a NUL within their bounds.
        unsafe {
            let mut adapter = buf.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
            while !adapter.is_null() {
                let a = &*adapter;
                if a.OperStatus == IfOperStatusUp
                    && a.IfType != IF_TYPE_SOFTWARE_LOOPBACK
                    && a.IfType != IF_TYPE_TUNNEL
                {
                    found.extend(wide_string(a.DnsSuffix));
                    let mut suffix = a.FirstDnsSuffix;
                    while !suffix.is_null() {
                        let s = &*suffix;
                        let end = s
                            .String
                            .iter()
                            .position(|c| *c == 0)
                            .unwrap_or(s.String.len());
                        found.extend(String::from_utf16(&s.String[..end]).ok());
                        suffix = s.Next;
                    }
                }
                adapter = a.Next;
            }
        }
        usable_suffixes(found)
    }

    type GetHint = unsafe extern "system" fn(*mut NL_NETWORK_CONNECTIVITY_HINT) -> WIN32_ERROR;

    pub fn metered() -> bool {
        let dll: Vec<u16> = "iphlpapi.dll\0".encode_utf16().collect();
        // SAFETY: plain library and symbol lookup with NUL-terminated names;
        // GetNetworkConnectivityHint is missing before Windows 10 version 2004,
        // so it is looked up at run time instead of linked.
        unsafe {
            let module = LoadLibraryW(dll.as_ptr());
            if module.is_null() {
                return false;
            }
            let Some(proc) = GetProcAddress(module, c"GetNetworkConnectivityHint".as_ptr().cast())
            else {
                return false;
            };
            let get: GetHint = std::mem::transmute(proc);
            // SAFETY: plain C struct for which all-zero bytes are a valid initial value.
            let mut hint: NL_NETWORK_CONNECTIVITY_HINT = std::mem::zeroed();
            if get(&mut hint) != ERROR_SUCCESS {
                return false;
            }
            is_metered(hint.ConnectivityCost, hint.OverDataLimit, hint.Roaming)
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn upstream_servers() -> Vec<IpAddr> {
        Vec::new()
    }

    pub fn dns_suffixes() -> Vec<String> {
        Vec::new()
    }

    pub fn metered() -> bool {
        false
    }
}

pub fn upstream_servers() -> Vec<IpAddr> {
    imp::upstream_servers()
}

pub fn dns_suffixes() -> Vec<String> {
    imp::dns_suffixes()
}

pub fn metered() -> bool {
    imp::metered()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ips(list: &[&str]) -> Vec<IpAddr> {
        list.iter().map(|s| s.parse().unwrap()).collect()
    }

    #[test]
    fn drops_loopback_unspecified_and_link_local() {
        let got = usable_servers(ips(&[
            "127.0.0.1",
            "0.0.0.0",
            "::1",
            "::",
            "fe80::1",
            "fec0:0:0:ffff::1",
            "fec0:0:0:ffff::3",
            "192.168.1.1",
        ]));
        assert_eq!(got, ips(&["192.168.1.1"]));
    }

    #[test]
    fn keeps_order_dedupes_and_caps_at_four() {
        let got = usable_servers(ips(&[
            "10.0.0.1",
            "10.0.0.2",
            "10.0.0.1",
            "2001:4860:4860::8888",
            "10.0.0.3",
            "10.0.0.4",
        ]));
        assert_eq!(
            got,
            ips(&["10.0.0.1", "10.0.0.2", "2001:4860:4860::8888", "10.0.0.3"])
        );
    }

    #[test]
    fn other_site_local_addresses_are_kept() {
        let got = usable_servers(ips(&["fec0:0:0:ffff::4", "fd00::1"]));
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn suffixes_are_cleaned_and_public_endings_dropped() {
        let got = usable_suffixes(
            [
                "Corp.Example.COM.",
                "fritz.box",
                "com",
                "lan",
                "home",
                "",
                "bad name.example",
                "fritz.box",
                "a..b",
            ]
            .map(String::from),
        );
        assert_eq!(got, ["corp.example.com", "fritz.box", "home"]);
    }

    #[test]
    fn suffixes_are_capped() {
        let many = (0..40).map(|i| format!("net{i}.example"));
        assert_eq!(usable_suffixes(many).len(), MAX_SUFFIXES);
    }

    #[test]
    fn metered_when_costly_limited_or_roaming() {
        assert!(!is_metered(0, false, false));
        assert!(!is_metered(1, false, false));
        assert!(is_metered(2, false, false));
        assert!(is_metered(3, false, false));
        assert!(is_metered(1, true, false));
        assert!(is_metered(1, false, true));
    }
}
