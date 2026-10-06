//! App data in a saved copy: an archive of one account's app data folder, sealed
//! in 1 MB chunks with an AEAD (`Sealer`).
//! Frame: `last: u8 | nonce: [u8; 12] | len: u32 LE | sealed: [u8; len]`; the
//! associated data binds family, account and chunk position so frames can't be
//! swapped, reordered, dropped or extended.
//! Archive (plaintext): `'D' len:u16 path` | `'F' len:u16 path size:u64 bytes` | `'E'`.
use super::backup::{valid_relative, MAX_BYTES, MAX_FILES};
use anyhow::{bail, ensure, Result};
use rand::RngCore;
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

pub const CHUNK: usize = 1 << 20;
pub(crate) const MAGIC: &[u8; 8] = b"SBXDATA1";
const TAG: usize = 16;

pub trait Sealer {
    fn seal(&self, nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>>;
    fn open(&self, nonce: &[u8; 12], aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>>;
}

pub enum Item {
    Dir(String),
    File(String, u64),
}

pub trait Source {
    fn items(&mut self) -> Result<Vec<Item>>;
    fn read(&mut self, rel: &str, out: &mut dyn Write) -> Result<u64>;
}

pub trait Sink {
    fn dir(&mut self, rel: &str) -> Result<()>;
    fn file(&mut self, rel: &str, size: u64, data: &mut dyn Read) -> Result<()>;
}

fn aad(family: &str, sid: &str, index: u64, last: bool) -> Vec<u8> {
    let mut a = b"secblitz-appdata-v1\0".to_vec();
    a.extend_from_slice(family.as_bytes());
    a.push(0);
    a.extend_from_slice(sid.as_bytes());
    a.push(0);
    a.extend_from_slice(&index.to_le_bytes());
    a.push(last as u8);
    a
}

struct FrameWriter<'a> {
    sealer: &'a dyn Sealer,
    family: &'a str,
    sid: &'a str,
    out: &'a mut dyn Write,
    buf: Vec<u8>,
    index: u64,
    total: u64,
}

impl FrameWriter<'_> {
    fn frame(&mut self, last: bool) -> Result<()> {
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let sealed = self.sealer.seal(
            &nonce,
            &aad(self.family, self.sid, self.index, last),
            &self.buf,
        )?;
        ensure!(
            sealed.len() == self.buf.len() + TAG,
            "Unexpected sealed size"
        );
        self.out.write_all(&[last as u8])?;
        self.out.write_all(&nonce)?;
        self.out.write_all(&(sealed.len() as u32).to_le_bytes())?;
        self.out.write_all(&sealed)?;
        self.buf.clear();
        self.index += 1;
        Ok(())
    }
    fn finish(mut self) -> Result<u64> {
        self.frame(true)?;
        Ok(self.total)
    }
}

impl Write for FrameWriter<'_> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }
        if self.buf.len() == CHUNK {
            self.frame(false).map_err(std::io::Error::other)?;
        }
        let n = (CHUNK - self.buf.len()).min(data.len());
        self.buf.extend_from_slice(&data[..n]);
        self.total += n as u64;
        if self.total > MAX_BYTES {
            return Err(std::io::Error::other("Saved data too large"));
        }
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn write_frames(
    plain: &[u8],
    sealer: &dyn Sealer,
    family: &str,
    sid: &str,
    out: &mut dyn Write,
) -> Result<()> {
    out.write_all(MAGIC)?;
    let mut w = FrameWriter {
        sealer,
        family,
        sid,
        out,
        buf: Vec::new(),
        index: 0,
        total: 0,
    };
    w.write_all(plain)?;
    w.finish()?;
    Ok(())
}

pub fn encrypt(
    source: &mut dyn Source,
    sealer: &dyn Sealer,
    family: &str,
    sid: &str,
    out: &mut dyn Write,
) -> Result<u64> {
    out.write_all(MAGIC)?;
    let items = source.items()?;
    ensure!(items.len() <= MAX_FILES, "Too many files");
    let mut w = FrameWriter {
        sealer,
        family,
        sid,
        out,
        buf: Vec::new(),
        index: 0,
        total: 0,
    };
    for item in &items {
        match item {
            Item::Dir(rel) => {
                ensure!(valid_relative(rel), "Unexpected folder name");
                w.write_all(b"D")?;
                w.write_all(&(rel.len() as u16).to_le_bytes())?;
                w.write_all(rel.as_bytes())?;
            }
            Item::File(rel, size) => {
                ensure!(valid_relative(rel), "Unexpected file name");
                w.write_all(b"F")?;
                w.write_all(&(rel.len() as u16).to_le_bytes())?;
                w.write_all(rel.as_bytes())?;
                w.write_all(&size.to_le_bytes())?;
                let written = source.read(rel, &mut w)?;
                ensure!(written == *size, "A file changed while it was being saved");
            }
        }
    }
    w.write_all(b"E")?;
    w.finish()
}

struct FrameReader<'a> {
    sealer: &'a dyn Sealer,
    family: &'a str,
    sid: &'a str,
    input: &'a mut dyn Read,
    buf: Vec<u8>,
    pos: usize,
    index: u64,
    done: bool,
}

impl FrameReader<'_> {
    fn next_frame(&mut self) -> Result<()> {
        ensure!(!self.done, "Read past the end");
        let mut head = [0u8; 17];
        self.input.read_exact(&mut head)?;
        let last = match head[0] {
            0 => false,
            1 => true,
            _ => bail!("Damaged saved data"),
        };
        let nonce: [u8; 12] = head[1..13].try_into().expect("12 bytes");
        let len = u32::from_le_bytes(head[13..17].try_into().expect("4 bytes")) as usize;
        ensure!((TAG..=CHUNK + TAG).contains(&len), "Damaged saved data");
        let mut sealed = vec![0u8; len];
        self.input.read_exact(&mut sealed)?;
        self.buf = self.sealer.open(
            &nonce,
            &aad(self.family, self.sid, self.index, last),
            &sealed,
        )?;
        self.pos = 0;
        self.index += 1;
        if last {
            self.done = true;
            let mut extra = [0u8; 1];
            ensure!(self.input.read(&mut extra)? == 0, "Damaged saved data");
        } else {
            ensure!(self.buf.len() == CHUNK, "Damaged saved data");
        }
        Ok(())
    }
}

impl Read for FrameReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while self.pos == self.buf.len() {
            if self.done {
                return Ok(0);
            }
            self.next_frame().map_err(std::io::Error::other)?;
        }
        let n = (self.buf.len() - self.pos).min(out.len());
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

fn read_path(r: &mut dyn Read) -> Result<String> {
    let mut len = [0u8; 2];
    r.read_exact(&mut len)?;
    let len = u16::from_le_bytes(len) as usize;
    ensure!(len > 0 && len <= 1024, "Damaged saved data");
    let mut raw = vec![0u8; len];
    r.read_exact(&mut raw)?;
    let path = String::from_utf8(raw)?;
    ensure!(valid_relative(&path), "Unexpected name in saved data");
    Ok(path)
}

pub fn decrypt(
    input: &mut dyn Read,
    sealer: &dyn Sealer,
    family: &str,
    sid: &str,
    sink: &mut dyn Sink,
) -> Result<()> {
    let mut magic = [0u8; 8];
    input.read_exact(&mut magic)?;
    ensure!(&magic == MAGIC, "Damaged saved data");
    let mut r = FrameReader {
        sealer,
        family,
        sid,
        input,
        buf: Vec::new(),
        pos: 0,
        index: 0,
        done: false,
    };
    let mut count = 0usize;
    loop {
        let mut tag = [0u8; 1];
        r.read_exact(&mut tag)?;
        match tag[0] {
            b'D' => {
                let path = read_path(&mut r)?;
                sink.dir(&path)?;
            }
            b'F' => {
                let path = read_path(&mut r)?;
                let mut size = [0u8; 8];
                r.read_exact(&mut size)?;
                let size = u64::from_le_bytes(size);
                ensure!(size <= MAX_BYTES, "Damaged saved data");
                let mut limited = (&mut r).take(size);
                sink.file(&path, size, &mut limited)?;
                ensure!(limited.limit() == 0, "Damaged saved data");
            }
            b'E' => break,
            _ => bail!("Damaged saved data"),
        }
        count += 1;
        ensure!(count <= MAX_FILES, "Too many files");
    }
    // The archive must end exactly at the final frame.
    let mut extra = [0u8; 1];
    ensure!(r.read(&mut extra)? == 0 && r.done, "Damaged saved data");
    Ok(())
}

#[cfg(test)]
pub struct DirSource {
    root: std::path::PathBuf,
}

#[cfg(test)]
impl DirSource {
    pub fn new(root: &std::path::Path) -> Self {
        DirSource {
            root: root.to_path_buf(),
        }
    }
}

#[cfg(test)]
impl Source for DirSource {
    fn items(&mut self) -> Result<Vec<Item>> {
        fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<Item>) -> Result<()> {
            let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let rel = e
                    .path()
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                let meta = std::fs::symlink_metadata(e.path())?;
                ensure!(!meta.file_type().is_symlink(), "Link");
                if meta.is_dir() {
                    out.push(Item::Dir(rel));
                    walk(root, &e.path(), out)?;
                } else {
                    out.push(Item::File(rel, meta.len()));
                }
            }
            Ok(())
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out)?;
        Ok(out)
    }
    fn read(&mut self, rel: &str, out: &mut dyn Write) -> Result<u64> {
        let mut f = std::fs::File::open(self.root.join(rel))?;
        Ok(std::io::copy(&mut f, out)?)
    }
}

#[cfg(test)]
pub struct DirSink {
    root: std::path::PathBuf,
}

#[cfg(test)]
impl DirSink {
    pub fn new(root: &std::path::Path) -> Self {
        DirSink {
            root: root.to_path_buf(),
        }
    }
}

#[cfg(test)]
impl Sink for DirSink {
    fn dir(&mut self, rel: &str) -> Result<()> {
        std::fs::create_dir_all(self.root.join(rel))?;
        Ok(())
    }
    fn file(&mut self, rel: &str, _size: u64, data: &mut dyn Read) -> Result<()> {
        let mut f = std::fs::File::create(self.root.join(rel))?;
        std::io::copy(data, &mut f)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fake([u8; 32]);
    impl Fake {
        fn stream(&self, nonce: &[u8; 12], len: usize) -> Vec<u8> {
            let mut out = Vec::with_capacity(len);
            let mut counter = 0u64;
            while out.len() < len {
                let mut h = Sha256::new();
                h.update(self.0);
                h.update(nonce);
                h.update(counter.to_le_bytes());
                out.extend_from_slice(&h.finalize());
                counter += 1;
            }
            out.truncate(len);
            out
        }
        fn tag(&self, nonce: &[u8; 12], aad: &[u8], ct: &[u8]) -> [u8; 16] {
            let mut h = Sha256::new();
            h.update(self.0);
            h.update(nonce);
            h.update((aad.len() as u64).to_le_bytes());
            h.update(aad);
            h.update(ct);
            h.finalize()[..16].try_into().unwrap()
        }
    }
    impl Sealer for Fake {
        fn seal(&self, nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
            let mut ct: Vec<u8> = plain
                .iter()
                .zip(self.stream(nonce, plain.len()))
                .map(|(a, b)| a ^ b)
                .collect();
            let tag = self.tag(nonce, aad, &ct);
            ct.extend_from_slice(&tag);
            Ok(ct)
        }
        fn open(&self, nonce: &[u8; 12], aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
            ensure!(sealed.len() >= 16, "short");
            let (ct, tag) = sealed.split_at(sealed.len() - 16);
            ensure!(self.tag(nonce, aad, ct) == tag, "tag");
            Ok(ct
                .iter()
                .zip(self.stream(nonce, ct.len()))
                .map(|(a, b)| a ^ b)
                .collect())
        }
    }

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("LocalState/sub")).unwrap();
        fs::create_dir_all(dir.path().join("Settings")).unwrap();
        fs::create_dir_all(dir.path().join("Empty")).unwrap();
        fs::write(dir.path().join("LocalState/sub/notes.txt"), b"my notes").unwrap();
        fs::write(
            dir.path().join("Settings/settings.dat"),
            vec![7u8; 3 * CHUNK + 5],
        )
        .unwrap();
        dir
    }

    fn roundtrip(sealer: &dyn Sealer) -> (Vec<u8>, tempfile::TempDir) {
        let src = tree();
        let mut out = Vec::new();
        let plain = encrypt(
            &mut DirSource::new(src.path()),
            sealer,
            "Fam_8wekyb3d8bbwe",
            "S-1-5-21-1-2-3-1001",
            &mut out,
        )
        .unwrap();
        assert!(plain > 3 * CHUNK as u64);
        let dst = tempfile::tempdir().unwrap();
        decrypt(
            &mut &out[..],
            sealer,
            "Fam_8wekyb3d8bbwe",
            "S-1-5-21-1-2-3-1001",
            &mut DirSink::new(dst.path()),
        )
        .unwrap();
        (out, dst)
    }

    #[test]
    fn roundtrip_restores_every_file_and_folder() {
        let (_, dst) = roundtrip(&Fake([1; 32]));
        assert_eq!(
            fs::read(dst.path().join("LocalState/sub/notes.txt")).unwrap(),
            b"my notes"
        );
        assert_eq!(
            fs::read(dst.path().join("Settings/settings.dat"))
                .unwrap()
                .len(),
            3 * CHUNK + 5
        );
        assert!(dst.path().join("Empty").is_dir());
    }

    #[test]
    fn ciphertext_hides_content() {
        let (out, _) = roundtrip(&Fake([1; 32]));
        assert!(!out.windows(8).any(|w| w == b"my notes"));
    }

    #[test]
    fn wrong_key_family_or_account_fails() {
        let (out, _) = roundtrip(&Fake([1; 32]));
        let dst = tempfile::tempdir().unwrap();
        for (key, fam, sid) in [
            ([2u8; 32], "Fam_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1001"),
            ([1u8; 32], "Other_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1001"),
            ([1u8; 32], "Fam_8wekyb3d8bbwe", "S-1-5-21-1-2-3-1002"),
        ] {
            assert!(decrypt(
                &mut &out[..],
                &Fake(key),
                fam,
                sid,
                &mut DirSink::new(dst.path())
            )
            .is_err());
        }
    }

    #[test]
    fn truncation_reordering_and_flips_are_detected() {
        let (out, _) = roundtrip(&Fake([1; 32]));
        let dst = tempfile::tempdir().unwrap();
        let open = |bytes: &[u8]| {
            decrypt(
                &mut &bytes[..],
                &Fake([1; 32]),
                "Fam_8wekyb3d8bbwe",
                "S-1-5-21-1-2-3-1001",
                &mut DirSink::new(dst.path()),
            )
        };
        let frame = 1 + 12 + 4 + CHUNK + 16;
        let without_last = &out[..MAGIC.len() + 3 * frame];
        assert!(open(without_last).is_err());
        let mut swapped = out.clone();
        let (a, b) = (MAGIC.len(), MAGIC.len() + frame);
        let first = out[a..a + frame].to_vec();
        swapped[a..a + frame].copy_from_slice(&out[b..b + frame]);
        swapped[b..b + frame].copy_from_slice(&first);
        assert!(open(&swapped).is_err());
        let mut flipped = out.clone();
        flipped[MAGIC.len() + 40] ^= 1;
        assert!(open(&flipped).is_err());
        let mut junk = out.clone();
        junk.push(0);
        assert!(open(&junk).is_err());
    }

    /// A hand-made archive with an escaping path must be refused before
    /// anything is written.
    #[test]
    fn unpack_refuses_bad_paths() {
        for bad in ["../evil", "a/../../evil", "C:/Windows/x", "a\\b", "a:ads"] {
            let mut plain = Vec::new();
            plain.push(b'F');
            plain.extend_from_slice(&(bad.len() as u16).to_le_bytes());
            plain.extend_from_slice(bad.as_bytes());
            plain.extend_from_slice(&1u64.to_le_bytes());
            plain.push(b'x');
            plain.push(b'E');
            let sealer = Fake([1; 32]);
            let mut out = Vec::new();
            write_frames(
                &plain,
                &sealer,
                "Fam_8wekyb3d8bbwe",
                "S-1-5-21-1-2-3-1001",
                &mut out,
            )
            .unwrap();
            let dst = tempfile::tempdir().unwrap();
            assert!(
                decrypt(
                    &mut &out[..],
                    &sealer,
                    "Fam_8wekyb3d8bbwe",
                    "S-1-5-21-1-2-3-1001",
                    &mut DirSink::new(dst.path())
                )
                .is_err(),
                "{bad}"
            );
            assert_eq!(fs::read_dir(dst.path()).unwrap().count(), 0, "{bad}");
        }
    }

    #[test]
    fn empty_folder_roundtrips() {
        let src = tempfile::tempdir().unwrap();
        let sealer = Fake([3; 32]);
        let mut out = Vec::new();
        encrypt(
            &mut DirSource::new(src.path()),
            &sealer,
            "F_8wekyb3d8bbwe",
            "S-1-5-21-1-2-3-1001",
            &mut out,
        )
        .unwrap();
        let dst = tempfile::tempdir().unwrap();
        decrypt(
            &mut &out[..],
            &sealer,
            "F_8wekyb3d8bbwe",
            "S-1-5-21-1-2-3-1001",
            &mut DirSink::new(dst.path()),
        )
        .unwrap();
    }
}
