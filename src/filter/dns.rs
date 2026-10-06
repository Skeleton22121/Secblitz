//! Minimal DNS wire handling. Only the header and the single question are ever
//! read, and only a handful of fixed answers are ever built, so a hostile packet
//! has very little to work with.

/// Firefox asks for this name to learn whether it may switch on its own
/// encrypted DNS. An NXDOMAIN answer tells it not to.
pub const CANARY: &str = "use-application-dns.net";

const MAX_QUERY: usize = 4096;
const TYPE_A: u16 = 1;
const TYPE_AAAA: u16 = 28;
const TTL: u32 = 60;

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

pub fn nxdomain_reply(query: &[u8], q: &Query) -> Vec<u8> {
    reply(query, q, 3, 0)
}

pub fn servfail_reply(query: &[u8], q: &Query) -> Vec<u8> {
    reply(query, q, 2, 0)
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
        assert_eq!(u32::from_be_bytes([a[6], a[7], a[8], a[9]]), 60);
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
            }
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
