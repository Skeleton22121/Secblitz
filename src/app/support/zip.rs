//! A minimal zip writer: stored (not compressed) files, built in memory.
use std::fmt;

pub const MAX_TOTAL: usize = 1_000_000;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    TooBig,
    BadName,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Error::TooBig => "The support file would be too big",
            Error::BadName => "A file name in the support file is not allowed",
        })
    }
}

impl std::error::Error for Error {}

/// A local date and time as the two 16-bit fields a zip entry stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DosTime {
    pub date: u16,
    pub time: u16,
}

impl DosTime {
    pub fn new(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Self {
        let year = year.clamp(1980, 2107) as u32 - 1980;
        DosTime {
            date: ((year << 9) | (month.clamp(1, 12) << 5) | day.clamp(1, 31)) as u16,
            time: ((hour.min(23) << 11) | (minute.min(59) << 5) | (second.min(59) / 2)) as u16,
        }
    }
}

fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('/')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        && !name.contains("..")
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub fn write(files: &[(String, Vec<u8>)], at: DosTime) -> Result<Vec<u8>, Error> {
    let total: usize = files.iter().map(|(n, d)| n.len() * 2 + d.len() + 100).sum();
    if total > MAX_TOTAL || files.len() > usize::from(u16::MAX) {
        return Err(Error::TooBig);
    }
    let mut out = Vec::with_capacity(total + 22);
    let mut central = Vec::new();
    for (name, data) in files {
        if !name_ok(name) {
            return Err(Error::BadName);
        }
        let crc = crc32fast::hash(data);
        let size = data.len() as u32;
        let offset = out.len() as u32;
        put32(&mut out, 0x0403_4b50);
        put16(&mut out, 10);
        put16(&mut out, 0x0800);
        put16(&mut out, 0);
        put16(&mut out, at.time);
        put16(&mut out, at.date);
        put32(&mut out, crc);
        put32(&mut out, size);
        put32(&mut out, size);
        put16(&mut out, name.len() as u16);
        put16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        put32(&mut central, 0x0201_4b50);
        put16(&mut central, 20);
        put16(&mut central, 10);
        put16(&mut central, 0x0800);
        put16(&mut central, 0);
        put16(&mut central, at.time);
        put16(&mut central, at.date);
        put32(&mut central, crc);
        put32(&mut central, size);
        put32(&mut central, size);
        put16(&mut central, name.len() as u16);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put16(&mut central, 0);
        put32(&mut central, 0);
        put32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let start = out.len() as u32;
    let count = files.len() as u16;
    out.extend_from_slice(&central);
    put32(&mut out, 0x0605_4b50);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, count);
    put16(&mut out, count);
    put32(&mut out, central.len() as u32);
    put32(&mut out, start);
    put16(&mut out, 0);
    Ok(out)
}

#[cfg(test)]
pub fn read(bytes: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let u16_at = |i: usize| Some(u16::from_le_bytes(bytes.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |i: usize| Some(u32::from_le_bytes(bytes.get(i..i + 4)?.try_into().ok()?));
    let eocd = bytes.len().checked_sub(22)?;
    if u32_at(eocd)? != 0x0605_4b50 || u16_at(eocd + 20)? != 0 {
        return None;
    }
    let count = usize::from(u16_at(eocd + 10)?);
    let mut at = u32_at(eocd + 16)? as usize;
    if at + u32_at(eocd + 12)? as usize != eocd {
        return None;
    }
    let mut out = Vec::new();
    for _ in 0..count {
        if u32_at(at)? != 0x0201_4b50 || u16_at(at + 10)? != 0 {
            return None;
        }
        let crc = u32_at(at + 16)?;
        let size = u32_at(at + 24)? as usize;
        let name_len = usize::from(u16_at(at + 28)?);
        let local = u32_at(at + 42)? as usize;
        let name = std::str::from_utf8(bytes.get(at + 46..at + 46 + name_len)?).ok()?;
        if u32_at(local)? != 0x0403_4b50 || u32_at(local + 22)? as usize != size {
            return None;
        }
        let local_name = usize::from(u16_at(local + 26)?);
        let local_extra = usize::from(u16_at(local + 28)?);
        let from = local + 30 + local_name + local_extra;
        let data = bytes.get(from..from + size)?;
        if crc32fast::hash(data) != crc || u32_at(local + 14)? != crc {
            return None;
        }
        out.push((name.to_owned(), data.to_vec()));
        at += 46 + name_len;
    }
    (at == eocd).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn when() -> DosTime {
        DosTime::new(2026, 10, 9, 14, 30, 8)
    }

    fn files() -> Vec<(String, Vec<u8>)> {
        vec![
            ("about.txt".into(), b"Secblitz 1.0.0\n".to_vec()),
            ("empty.txt".into(), Vec::new()),
            ("last-check.json".into(), vec![b'x'; 5000]),
        ]
    }

    #[test]
    fn what_goes_in_comes_back_out_with_matching_checksums() {
        let zip = write(&files(), when()).unwrap();
        assert_eq!(read(&zip).unwrap(), files());
    }

    #[test]
    fn the_bytes_follow_the_zip_layout() {
        let zip = write(&files()[..1], when()).unwrap();
        let data = b"Secblitz 1.0.0\n";
        assert_eq!(&zip[..4], b"PK\x03\x04");
        assert_eq!(&zip[4..6], &10u16.to_le_bytes());
        assert_eq!(&zip[6..8], &0x0800u16.to_le_bytes());
        assert_eq!(&zip[8..10], &0u16.to_le_bytes(), "stored, not compressed");
        assert_eq!(&zip[10..12], &when().time.to_le_bytes());
        assert_eq!(&zip[12..14], &when().date.to_le_bytes());
        assert_eq!(&zip[14..18], &crc32fast::hash(data).to_le_bytes());
        assert_eq!(&zip[18..22], &(data.len() as u32).to_le_bytes());
        assert_eq!(&zip[22..26], &(data.len() as u32).to_le_bytes());
        assert_eq!(&zip[26..28], &9u16.to_le_bytes());
        assert_eq!(&zip[28..30], &0u16.to_le_bytes());
        assert_eq!(&zip[30..39], b"about.txt");
        assert_eq!(&zip[39..39 + data.len()], data);
        let central = 39 + data.len();
        assert_eq!(&zip[central..central + 4], b"PK\x01\x02");
        let end = zip.len() - 22;
        assert_eq!(central + 46 + 9, end);
        assert_eq!(&zip[end..end + 4], b"PK\x05\x06");
        assert_eq!(&zip[end + 8..end + 10], &1u16.to_le_bytes());
        assert_eq!(&zip[end + 12..end + 16], &55u32.to_le_bytes());
        assert_eq!(&zip[end + 16..end + 20], &(central as u32).to_le_bytes());
    }

    #[test]
    fn an_empty_list_is_a_valid_empty_zip() {
        let zip = write(&[], when()).unwrap();
        assert_eq!(zip.len(), 22);
        assert_eq!(read(&zip).unwrap(), Vec::new());
    }

    #[test]
    fn dos_time_packs_local_date_and_time() {
        let t = DosTime::new(2026, 10, 9, 14, 30, 8);
        assert_eq!(t.date, ((46 << 9) | (10 << 5) | 9) as u16);
        assert_eq!(t.time, ((14 << 11) | (30 << 5) | 4) as u16);
        assert_eq!(DosTime::new(1970, 1, 1, 0, 0, 0).date >> 9, 0);
    }

    #[test]
    fn names_that_could_escape_the_folder_are_refused() {
        for name in [
            "", "../x.txt", "a/b.txt", "/x.txt", "a\\b.txt", "x..txt", "é.txt",
        ] {
            assert_eq!(
                write(&[(name.to_owned(), vec![1])], when()),
                Err(Error::BadName),
                "{name:?}"
            );
        }
    }

    #[test]
    fn a_support_file_over_the_size_cap_is_refused() {
        let big = vec![(String::from("big.txt"), vec![0u8; MAX_TOTAL])];
        assert_eq!(write(&big, when()), Err(Error::TooBig));
        let fits = vec![(String::from("big.txt"), vec![0u8; MAX_TOTAL - 500])];
        assert!(write(&fits, when()).is_ok());
    }

    #[test]
    fn a_damaged_zip_does_not_read_back() {
        let mut zip = write(&files(), when()).unwrap();
        zip[40] ^= 1;
        assert!(read(&zip).is_none());
    }
}
