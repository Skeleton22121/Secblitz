//! Minimal DNS wire handling. Only the header and the single question are ever
//! read, and only a handful of fixed answers are ever built, so a hostile packet
//! has very little to work with.

/// Firefox asks for this name to learn whether it may switch on its own
/// encrypted DNS. An NXDOMAIN answer tells it not to.
pub const CANARY: &str = "use-application-dns.net";

const MAX_QUERY: usize = 4096;
pub const TYPE_A: u16 = 1;
pub const TYPE_AAAA: u16 = 28;
pub const TYPE_SVCB: u16 = 64;
pub const TYPE_HTTPS: u16 = 65;
const TYPE_CNAME: u16 = 5;
const CLASS_IN: u16 = 1;
/// Short, so an allowed site or an ended pause reaches browsers within seconds.
const TTL: u32 = 10;
const SAFE_SEARCH_TTL: u32 = 300;
const MAX_ADDRESSES: usize = 8;
const MAX_ANSWERS: u16 = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    /// Lowercase ASCII, no trailing dot, root is `""`.
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub id: u16,
    pub flags: u16,
    pub question: Question,
    /// Offset just past the question; the question bytes are `[12..question_end]`.
    pub question_end: usize,
}

fn be16(p: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([p[at], p[at + 1]])
}

/// Reads the one question that follows the 12-byte header. Compression
/// pointers are refused: a question never needs one.
fn parse_question(packet: &[u8]) -> Option<(Question, usize)> {
    let mut at = 12;
    let mut name = String::new();
    let mut wire_len = 1; // the root byte
    loop {
        let len = *packet.get(at)? as usize;
        at += 1;
        if len == 0 {
            break;
        }
        if len > 63 {
            // Also catches compression pointers (top bits set).
            return None;
        }
        wire_len += len + 1;
        if wire_len > 255 {
            return None;
        }
        let label = packet.get(at..at + len)?;
        at += len;
        // Any printable ASCII except the separator. Odd but well-formed names
        // (`*`, spaces in local names) are forwarded, never dropped: a dropped
        // query stalls the lookup until Windows gives up on the filter.
        if !label
            .iter()
            .all(|b| (b' '..=b'~').contains(b) && *b != b'.')
        {
            return None;
        }
        if !name.is_empty() {
            name.push('.');
        }
        name.extend(label.iter().map(|b| b.to_ascii_lowercase() as char));
    }
    let tail = packet.get(at..at + 4)?;
    let qtype = u16::from_be_bytes([tail[0], tail[1]]);
    let qclass = u16::from_be_bytes([tail[2], tail[3]]);
    Some((
        Question {
            name,
            qtype,
            qclass,
        },
        at + 4,
    ))
}

pub fn parse_query(packet: &[u8]) -> Option<Query> {
    if packet.len() < 12 || packet.len() > MAX_QUERY {
        return None;
    }
    let flags = be16(packet, 2);
    if flags & 0x8000 != 0 || (flags >> 11) & 0xF != 0 {
        return None;
    }
    if be16(packet, 4) != 1 || be16(packet, 6) != 0 || be16(packet, 8) != 0 {
        return None;
    }
    let (question, question_end) = parse_question(packet)?;
    Some(Query {
        id: be16(packet, 0),
        flags,
        question,
        question_end,
    })
}

fn reply(query: &[u8], q: &Query, rcode: u16, answers: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(q.question_end + 32);
    out.extend_from_slice(&q.id.to_be_bytes());
    let flags = 0x8000 | (q.flags & 0x0100) | 0x0080 | rcode;
    out.extend_from_slice(&flags.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&answers.to_be_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&query[12..q.question_end]);
    out
}

/// The answer for a blocked name: `0.0.0.0` / `::` for address lookups, an
/// empty successful answer for everything else (HTTPS, TXT, ...).
pub fn blocked_reply(query: &[u8], q: &Query) -> Vec<u8> {
    let rdata: &[u8] = match q.question.qtype {
        TYPE_A => &[0; 4],
        TYPE_AAAA => &[0; 16],
        _ => return reply(query, q, 0, 0),
    };
    let mut out = reply(query, q, 0, 1);
    out.extend_from_slice(&[0xC0, 0x0C]);
    out.extend_from_slice(&q.question.qtype.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&TTL.to_be_bytes());
    out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
    out.extend_from_slice(rdata);
    out
}

pub fn empty_reply(query: &[u8], q: &Query) -> Vec<u8> {
    reply(query, q, 0, 0)
}

pub fn nxdomain_reply(query: &[u8], q: &Query) -> Vec<u8> {
    reply(query, q, 3, 0)
}

pub fn servfail_reply(query: &[u8], q: &Query) -> Vec<u8> {
    reply(query, q, 2, 0)
}

pub fn build_query(id: u16, name: &str, qtype: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(18 + name.len());
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&0x0100u16.to_be_bytes());
    out.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&encode_name(name));
    out.extend_from_slice(&qtype.to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    out
}

fn encode_name(name: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len() + 2);
    for label in name.split('.').filter(|l| !l.is_empty()) {
        let label = &label.as_bytes()[..label.len().min(63)];
        out.push(label.len() as u8);
        out.extend_from_slice(label);
    }
    out.push(0);
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub ttl: u32,
    pub bytes: Vec<u8>,
}

pub fn parse_addresses(reply: &[u8], qtype: u16) -> Option<Vec<Address>> {
    let want = match qtype {
        TYPE_A => 4,
        TYPE_AAAA => 16,
        _ => return None,
    };
    if reply.len() < 12 || be16(reply, 2) & 0x8000 == 0 || be16(reply, 2) & 0xF != 0 {
        return None;
    }
    if be16(reply, 4) != 1 {
        return None;
    }
    let answers = be16(reply, 6).min(MAX_ANSWERS);
    let (_, mut at) = parse_question(reply)?;
    let mut out = Vec::new();
    for _ in 0..answers {
        at = skip_name(reply, at)?;
        let fixed = reply.get(at..at + 10)?;
        let kind = be16(fixed, 0);
        let class = be16(fixed, 2);
        let ttl = u32::from_be_bytes([fixed[4], fixed[5], fixed[6], fixed[7]]);
        let len = be16(fixed, 8) as usize;
        at += 10;
        let data = reply.get(at..at + len)?;
        at += len;
        if kind == qtype && class == CLASS_IN && len == want && out.len() < MAX_ADDRESSES {
            out.push(Address {
                ttl,
                bytes: data.to_vec(),
            });
        }
    }
    Some(out)
}

fn skip_name(packet: &[u8], mut at: usize) -> Option<usize> {
    for _ in 0..128 {
        let len = *packet.get(at)? as usize;
        match len {
            0 => return Some(at + 1),
            l if l & 0xC0 == 0xC0 => {
                packet.get(at + 1)?;
                return Some(at + 2);
            }
            l if l > 63 => return None,
            l => at += 1 + l,
        }
    }
    None
}

pub fn safe_search_reply(query: &[u8], q: &Query, target: &str, addresses: &[Address]) -> Vec<u8> {
    let addresses = &addresses[..addresses.len().min(MAX_ADDRESSES)];
    let mut out = reply(query, q, 0, 1 + addresses.len() as u16);
    out.extend_from_slice(&[0xC0, 0x0C]);
    out.extend_from_slice(&TYPE_CNAME.to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    out.extend_from_slice(&SAFE_SEARCH_TTL.to_be_bytes());
    let name = encode_name(target);
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    let target_at = out.len();
    out.extend_from_slice(&name);
    for address in addresses {
        out.extend_from_slice(&(0xC000 | target_at as u16).to_be_bytes());
        let kind = if address.bytes.len() == 4 {
            TYPE_A
        } else {
            TYPE_AAAA
        };
        out.extend_from_slice(&kind.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
        out.extend_from_slice(&address.ttl.clamp(30, SAFE_SEARCH_TTL).to_be_bytes());
        out.extend_from_slice(&(address.bytes.len() as u16).to_be_bytes());
        out.extend_from_slice(&address.bytes);
    }
    out
}

pub fn with_id(packet: &[u8], id: u16) -> Vec<u8> {
    let mut out = packet.to_vec();
    if out.len() >= 2 {
        out[..2].copy_from_slice(&id.to_be_bytes());
    }
    out
}

pub fn reply_matches(reply: &[u8], id: u16, q: &Question) -> bool {
    if reply.len() < 12 || be16(reply, 0) != id || be16(reply, 2) & 0x8000 == 0 {
        return false;
    }
    if be16(reply, 4) != 1 {
        return false;
    }
    match parse_question(reply) {
        Some((got, _)) => got == *q,
        None => false,
    }
}

pub fn truncated(reply: &[u8]) -> bool {
    reply.len() >= 4 && be16(reply, 2) & 0x0200 != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::StdRng, RngExt, SeedableRng};

    fn query_bytes(name: &str, qtype: u16) -> Vec<u8> {
        let mut p = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in name.split('.').filter(|l| !l.is_empty()) {
            p.push(label.len() as u8);
            p.extend_from_slice(label.as_bytes());
        }
        p.push(0);
        p.extend_from_slice(&qtype.to_be_bytes());
        p.extend_from_slice(&1u16.to_be_bytes());
        p
    }

    fn parsed(name: &str, qtype: u16) -> (Vec<u8>, Query) {
        let p = query_bytes(name, qtype);
        let q = parse_query(&p).unwrap();
        (p, q)
    }

    #[test]
    fn parse_accepts_a_query() {
        let p = query_bytes("www.Example.com", 1);
        let q = parse_query(&p).unwrap();
        assert_eq!(q.id, 0x1234);
        assert_eq!(q.question.name, "www.example.com");
        assert_eq!(q.question.qtype, 1);
        assert_eq!(q.question.qclass, 1);
        assert_eq!(q.question_end, p.len());
    }

    #[test]
    fn parse_accepts_root_and_extra_records() {
        let mut p = query_bytes("", 2);
        assert_eq!(parse_query(&p).unwrap().question.name, "");
        // An EDNS record in the additional section is fine.
        p[11] = 1;
        p.extend_from_slice(&[0, 0, 41, 16, 0, 0, 0, 0, 0, 0, 0]);
        assert!(parse_query(&p).is_some());
    }

    #[test]
    fn parse_rejects_response() {
        let mut p = query_bytes("example.com", 1);
        p[2] |= 0x80;
        assert!(parse_query(&p).is_none());
    }

    #[test]
    fn parse_rejects_non_query_opcode_and_answers() {
        let mut p = query_bytes("example.com", 1);
        p[2] |= 0x10;
        assert!(parse_query(&p).is_none());
        let mut p = query_bytes("example.com", 1);
        p[7] = 1;
        assert!(parse_query(&p).is_none());
        let mut p = query_bytes("example.com", 1);
        p[9] = 1;
        assert!(parse_query(&p).is_none());
    }

    #[test]
    fn parse_rejects_two_questions() {
        let mut p = query_bytes("example.com", 1);
        p[5] = 2;
        assert!(parse_query(&p).is_none());
        p[5] = 0;
        assert!(parse_query(&p).is_none());
    }

    #[test]
    fn parse_rejects_compression_pointer() {
        let mut p = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        p.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1]);
        assert!(parse_query(&p).is_none());
    }

    #[test]
    fn parse_rejects_truncated_name() {
        let p = query_bytes("example.com", 1);
        for cut in 12..p.len() {
            assert!(parse_query(&p[..cut]).is_none(), "cut at {cut}");
        }
    }

    #[test]
    fn parse_rejects_long_label() {
        let long = "a".repeat(64);
        assert!(parse_query(&query_bytes(&format!("{long}.com"), 1)).is_none());
        let ok = "a".repeat(63);
        assert!(parse_query(&query_bytes(&format!("{ok}.com"), 1)).is_some());
    }

    #[test]
    fn parse_rejects_long_name_and_control_bytes() {
        let label = "a".repeat(60);
        let name = format!("{label}.{label}.{label}.{label}.{label}");
        assert!(parse_query(&query_bytes(&name, 1)).is_none());
        assert!(parse_query(&query_bytes("exa\u{1}mple.com", 1)).is_none());
        assert!(parse_query(&query_bytes("caf\u{e9}.com", 1)).is_none());
        assert!(parse_query(&query_bytes("_dmarc.example.com", 16)).is_some());
    }

    #[test]
    fn parse_accepts_odd_printable_names_so_they_are_forwarded() {
        let q = parse_query(&query_bytes("My PC.*.Local", 1)).unwrap();
        assert_eq!(q.question.name, "my pc.*.local");
    }

    #[test]
    fn parse_rejects_oversize_packet() {
        let mut p = query_bytes("example.com", 1);
        p.resize(4097, 0);
        assert!(parse_query(&p).is_none());
        p.truncate(4096);
        assert!(parse_query(&p).is_some());
    }

    #[test]
    fn parse_rejects_empty() {
        assert!(parse_query(&[]).is_none());
        assert!(parse_query(&[0; 11]).is_none());
    }

    #[test]
    fn blocked_a_answers_zero_ip() {
        let (p, q) = parsed("ads.example.com", 1);
        let r = blocked_reply(&p, &q);
        assert_eq!(be16(&r, 0), 0x1234);
        assert_eq!(be16(&r, 2) & 0x8000, 0x8000);
        assert_eq!(be16(&r, 2) & 0x0100, 0x0100);
        assert_eq!(be16(&r, 2) & 0x0080, 0x0080);
        assert_eq!(be16(&r, 2) & 0xF, 0);
        assert_eq!(be16(&r, 4), 1);
        assert_eq!(be16(&r, 6), 1);
        assert_eq!(&r[12..q.question_end], &p[12..]);
        let a = &r[q.question_end..];
        assert_eq!(&a[..2], &[0xC0, 0x0C]);
        assert_eq!(be16(a, 2), 1);
        assert_eq!(be16(a, 4), 1);
        assert_eq!(u32::from_be_bytes([a[6], a[7], a[8], a[9]]), TTL);
        assert_eq!(be16(a, 10), 4);
        assert_eq!(&a[12..], &[0, 0, 0, 0]);
    }

    #[test]
    fn blocked_aaaa_answers_unspecified() {
        let (p, q) = parsed("ads.example.com", 28);
        let r = blocked_reply(&p, &q);
        assert_eq!(be16(&r, 6), 1);
        let a = &r[q.question_end..];
        assert_eq!(be16(a, 2), 28);
        assert_eq!(be16(a, 10), 16);
        assert_eq!(&a[12..], &[0; 16]);
    }

    #[test]
    fn blocked_https_is_empty_noerror() {
        let (p, q) = parsed("ads.example.com", 65);
        let r = blocked_reply(&p, &q);
        assert_eq!(be16(&r, 2) & 0xF, 0);
        assert_eq!(be16(&r, 6), 0);
        assert_eq!(r.len(), q.question_end);
    }

    #[test]
    fn nxdomain_sets_rcode_3() {
        let (p, q) = parsed(CANARY, 1);
        let r = nxdomain_reply(&p, &q);
        assert_eq!(be16(&r, 2) & 0xF, 3);
        assert_eq!(be16(&r, 6), 0);
        let s = servfail_reply(&p, &q);
        assert_eq!(be16(&s, 2) & 0xF, 2);
    }

    #[test]
    fn reply_matches_checks_id_and_question() {
        let (p, q) = parsed("Example.com", 1);
        let r = blocked_reply(&p, &q);
        assert!(reply_matches(&r, 0x1234, &q.question));
        assert!(!reply_matches(&r, 0x1235, &q.question));
        let (_, other) = parsed("other.com", 1);
        assert!(!reply_matches(&r, 0x1234, &other.question));
        assert!(!reply_matches(&p, 0x1234, &q.question));
        let changed = with_id(&r, 7);
        assert!(reply_matches(&changed, 7, &q.question));
    }

    #[test]
    fn truncated_reads_tc_bit() {
        let (p, q) = parsed("example.com", 1);
        let mut r = blocked_reply(&p, &q);
        assert!(!truncated(&r));
        r[2] |= 0x02;
        assert!(truncated(&r));
        assert!(!truncated(&[1]));
    }

    fn v4(a: [u8; 4]) -> Address {
        Address {
            ttl: 120,
            bytes: a.to_vec(),
        }
    }

    #[test]
    fn built_query_parses_back() {
        let p = build_query(0xBEEF, "forcesafesearch.google.com", TYPE_AAAA);
        let q = parse_query(&p).unwrap();
        assert_eq!(q.id, 0xBEEF);
        assert_eq!(q.question.name, "forcesafesearch.google.com");
        assert_eq!(q.question.qtype, TYPE_AAAA);
        assert_eq!(q.question.qclass, 1);
        assert_eq!(q.flags & 0x0100, 0x0100);
        assert_eq!(q.question_end, p.len());
    }

    #[test]
    fn safe_search_reply_is_a_cname_and_addresses() {
        let (p, q) = parsed("www.google.com", TYPE_A);
        let found = [v4([1, 2, 3, 4]), v4([5, 6, 7, 8])];
        let r = safe_search_reply(&p, &q, "forcesafesearch.google.com", &found);
        assert_eq!(be16(&r, 0), 0x1234);
        assert_eq!(be16(&r, 2) & 0x8000, 0x8000);
        assert_eq!(be16(&r, 2) & 0xF, 0);
        assert_eq!(be16(&r, 4), 1);
        assert_eq!(be16(&r, 6), 3);
        assert_eq!(&r[12..q.question_end], &p[12..]);
        let a = &r[q.question_end..];
        assert_eq!(&a[..2], &[0xC0, 0x0C]);
        assert_eq!(be16(a, 2), TYPE_CNAME);
        assert_eq!(be16(a, 4), 1);
        let cname_len = be16(a, 10) as usize;
        let target = encode_name("forcesafesearch.google.com");
        assert_eq!(&a[12..12 + cname_len], &target[..]);
        let target_at = q.question_end + 12;
        let first = &a[12 + cname_len..];
        assert_eq!(be16(first, 0), 0xC000 | target_at as u16);
        assert_eq!(be16(first, 2), TYPE_A);
        let ttl = u32::from_be_bytes([first[6], first[7], first[8], first[9]]);
        assert_eq!(ttl, 120);
        assert_eq!(be16(first, 10), 4);
        assert_eq!(&first[12..16], &[1, 2, 3, 4]);
        assert_eq!(&first[16 + 12..16 + 16], &[5, 6, 7, 8]);
        assert_eq!(first.len(), 32);
    }

    #[test]
    fn safe_search_pointer_reaches_the_target_name() {
        let (p, q) = parsed("www.bing.com", TYPE_A);
        let r = safe_search_reply(&p, &q, "strict.bing.com", &[v4([9, 9, 9, 9])]);
        let pointer_at = r.len() - 16;
        let at = (be16(&r, pointer_at) & 0x3FFF) as usize;
        assert_eq!(&r[at..at + 7], b"\x06strict");
        assert!(reply_matches(&r, 0x1234, &q.question));
    }

    #[test]
    fn safe_search_reply_without_addresses_is_just_the_alias() {
        let (p, q) = parsed("www.google.com", TYPE_AAAA);
        let r = safe_search_reply(&p, &q, "forcesafesearch.google.com", &[]);
        assert_eq!(be16(&r, 6), 1);
        assert_eq!(be16(&r, 2) & 0xF, 0);
        let aaaa = Address {
            ttl: 5_000,
            bytes: vec![0x20; 16],
        };
        let r = safe_search_reply(&p, &q, "forcesafesearch.google.com", &[aaaa]);
        let tail = &r[r.len() - 28..];
        assert_eq!(be16(tail, 2), TYPE_AAAA);
        let ttl = u32::from_be_bytes([tail[6], tail[7], tail[8], tail[9]]);
        assert_eq!(ttl, 300);
        assert_eq!(be16(tail, 10), 16);
    }

    fn upstream_answer(name: &str, qtype: u16, answers: &[(u16, Vec<u8>)], rcode: u8) -> Vec<u8> {
        let mut out = vec![0xAB, 0xCD, 0x81, 0x80 | rcode, 0, 1];
        out.extend_from_slice(&(answers.len() as u16).to_be_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&encode_name(name));
        out.extend_from_slice(&qtype.to_be_bytes());
        out.extend_from_slice(&[0, 1]);
        for (kind, data) in answers {
            out.extend_from_slice(&[0xC0, 0x0C]);
            out.extend_from_slice(&kind.to_be_bytes());
            out.extend_from_slice(&[0, 1, 0, 0, 0, 60]);
            out.extend_from_slice(&(data.len() as u16).to_be_bytes());
            out.extend_from_slice(data);
        }
        out
    }

    #[test]
    fn addresses_are_read_past_aliases() {
        let cname = encode_name("alias.example");
        let reply = upstream_answer(
            "t.example",
            TYPE_A,
            &[
                (TYPE_CNAME, cname),
                (TYPE_A, vec![1, 1, 1, 1]),
                (TYPE_A, vec![2, 2, 2, 2]),
            ],
            0,
        );
        let got = parse_addresses(&reply, TYPE_A).unwrap();
        let bytes: Vec<&[u8]> = got.iter().map(|a| a.bytes.as_slice()).collect();
        assert_eq!(bytes, [&[1, 1, 1, 1][..], &[2, 2, 2, 2][..]]);
        assert_eq!(got[0].ttl, 60);
        assert!(parse_addresses(&reply, TYPE_AAAA).unwrap().is_empty());
        let v6 = upstream_answer("t.example", TYPE_AAAA, &[(TYPE_AAAA, vec![7; 16])], 0);
        assert_eq!(
            parse_addresses(&v6, TYPE_AAAA).unwrap()[0].bytes,
            vec![7; 16]
        );
        assert!(parse_addresses(&v6, TYPE_A).unwrap().is_empty());
    }

    #[test]
    fn addresses_refuse_bad_answers() {
        let ok = upstream_answer("t.example", TYPE_A, &[(TYPE_A, vec![1, 1, 1, 1])], 0);
        assert!(parse_addresses(&ok, TYPE_A).is_some());
        let servfail = upstream_answer("t.example", TYPE_A, &[], 2);
        assert_eq!(parse_addresses(&servfail, TYPE_A), None);
        let nodata = upstream_answer("t.example", TYPE_A, &[], 0);
        assert_eq!(parse_addresses(&nodata, TYPE_A), Some(vec![]));
        let ask = build_query(1, "t.example", TYPE_A);
        assert_eq!(parse_addresses(&ask, TYPE_A), None);
        for cut in 0..ok.len() {
            assert_eq!(parse_addresses(&ok[..cut], TYPE_A), None, "cut at {cut}");
        }
        let odd = upstream_answer("t.example", TYPE_A, &[(TYPE_A, vec![1, 1, 1])], 0);
        assert!(parse_addresses(&odd, TYPE_A).unwrap().is_empty());
        assert_eq!(parse_addresses(&ok, 15), None);
    }

    #[test]
    fn at_most_eight_addresses() {
        let many: Vec<(u16, Vec<u8>)> = (0..20).map(|i| (TYPE_A, vec![1, 1, 1, i])).collect();
        let reply = upstream_answer("t.example", TYPE_A, &many, 0);
        assert_eq!(
            parse_addresses(&reply, TYPE_A).unwrap().len(),
            MAX_ADDRESSES
        );
    }

    #[test]
    fn empty_reply_has_no_answers() {
        let (p, q) = parsed("www.google.com", TYPE_HTTPS);
        let r = empty_reply(&p, &q);
        assert_eq!(be16(&r, 2) & 0xF, 0);
        assert_eq!(be16(&r, 6), 0);
        assert_eq!(r.len(), q.question_end);
    }

    #[test]
    fn parse_never_panics() {
        let mut rng = StdRng::seed_from_u64(0x5ec0_b117);
        for _ in 0..10_000 {
            let len = rng.random_range(0..600);
            let mut p = vec![0u8; len];
            rng.fill(&mut p[..]);
            if len >= 12 && rng.random_bool(0.5) {
                p[2] &= 0x07;
                p[4] = 0;
                p[5] = 1;
                p[6..10].fill(0);
            }
            if let Some(q) = parse_query(&p) {
                let _ = blocked_reply(&p, &q);
                let _ = nxdomain_reply(&p, &q);
                let _ = safe_search_reply(&p, &q, "strict.bing.com", &[]);
            }
            let _ = parse_addresses(&p, TYPE_A);
            let q = Question {
                name: "a.com".into(),
                qtype: 1,
                qclass: 1,
            };
            let _ = reply_matches(&p, 0, &q);
            let _ = truncated(&p);
            let _ = with_id(&p, 1);
        }
    }
}
