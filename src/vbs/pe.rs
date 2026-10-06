//! Static checks on a driver image's PE headers for memory-integrity compatibility.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeProblem {
    NotAnImage,
    SectionAlignment,
    /// A section is both writable and executable (the old INIT section is
    /// tolerated: Windows removes its write permission).
    WritableAndExecutable(String),
    ImportTableExecutable,
}

const PAGE: u32 = 0x1000;
const SCN_EXECUTE: u32 = 0x2000_0000;
const SCN_WRITE: u32 = 0x8000_0000;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        b.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// `bytes` must hold the headers (the first 64 KiB is plenty).
pub fn scan_pe(bytes: &[u8]) -> Vec<PeProblem> {
    match scan_headers(bytes) {
        Some(problems) => problems,
        None => vec![PeProblem::NotAnImage],
    }
}

fn scan_headers(b: &[u8]) -> Option<Vec<PeProblem>> {
    if b.get(..2)? != b"MZ" {
        return None;
    }
    let pe = u32_at(b, 0x3c)? as usize;
    if b.get(pe..pe.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let coff = pe + 4;
    let sections = usize::from(u16_at(b, coff + 2)?);
    let optional_size = usize::from(u16_at(b, coff + 16)?);
    let opt = coff + 20;
    // PE32+ (64-bit) and PE32 differ only in where the directories start.
    let (count_at, dirs_at) = match u16_at(b, opt)? {
        0x20b => (opt + 108, opt + 112),
        0x10b => (opt + 92, opt + 96),
        _ => return None,
    };
    if sections == 0 || sections > 96 {
        return None;
    }
    let alignment = u32_at(b, opt + 32)?;
    let directories = u32_at(b, count_at)?;
    // Data directory 12 is the import address table.
    let iat = if directories > 12 {
        u32_at(b, dirs_at + 12 * 8)?
    } else {
        0
    };
    let table = opt.checked_add(optional_size)?;
    let mut problems = Vec::new();
    if alignment == 0 || alignment % PAGE != 0 {
        problems.push(PeProblem::SectionAlignment);
    }
    for index in 0..sections {
        let at = table.checked_add(index.checked_mul(40)?)?;
        let name_bytes = b.get(at..at.checked_add(8)?)?;
        let virtual_size = u32_at(b, at + 8)?;
        let address = u32_at(b, at + 12)?;
        let raw_size = u32_at(b, at + 16)?;
        let flags = u32_at(b, at + 36)?;
        let name: String = name_bytes
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| {
                if c.is_ascii_graphic() {
                    char::from(*c)
                } else {
                    '?'
                }
            })
            .collect();
        if alignment != 0
            && alignment % PAGE == 0
            && address % PAGE != 0
            && !problems.contains(&PeProblem::SectionAlignment)
        {
            problems.push(PeProblem::SectionAlignment);
        }
        let executable = flags & SCN_EXECUTE != 0;
        if executable && flags & SCN_WRITE != 0 && !name.eq_ignore_ascii_case("INIT") {
            problems.push(PeProblem::WritableAndExecutable(name));
        } else if executable && iat != 0 {
            let size = virtual_size.max(raw_size);
            if iat >= address && u64::from(iat) < u64::from(address) + u64::from(size) {
                problems.push(PeProblem::ImportTableExecutable);
            }
        }
    }
    Some(problems)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(in crate::vbs) struct Pe {
        pub(in crate::vbs) alignment: u32,
        pub(in crate::vbs) iat: u32,
        pub(in crate::vbs) sections: Vec<(&'static str, u32, u32, u32)>,
        pub(in crate::vbs) pe32: bool,
    }

    impl Default for Pe {
        fn default() -> Self {
            Pe {
                alignment: 0x1000,
                iat: 0,
                sections: vec![
                    (".text", 0x1000, 0x800, 0x6000_0020),
                    (".data", 0x2000, 0x400, 0xC000_0040),
                    ("PAGE", 0x3000, 0x400, 0x6000_0020),
                ],
                pe32: false,
            }
        }
    }

    fn put16(b: &mut [u8], at: usize, v: u16) {
        b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn put32(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub(in crate::vbs) fn build(pe: &Pe) -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        b[..2].copy_from_slice(b"MZ");
        put32(&mut b, 0x3c, 0x80);
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        put16(&mut b, coff, 0x8664);
        put16(&mut b, coff + 2, pe.sections.len() as u16);
        let optional = if pe.pe32 { 224 } else { 240 };
        put16(&mut b, coff + 16, optional);
        let opt = coff + 20;
        put16(&mut b, opt, if pe.pe32 { 0x10b } else { 0x20b });
        put32(&mut b, opt + 32, pe.alignment);
        put32(&mut b, opt + 36, 0x200);
        let (count_at, dirs_at) = if pe.pe32 {
            (opt + 92, opt + 96)
        } else {
            (opt + 108, opt + 112)
        };
        put32(&mut b, count_at, 16);
        put32(&mut b, dirs_at + 12 * 8, pe.iat);
        let table = opt + usize::from(optional);
        for (i, (name, address, size, flags)) in pe.sections.iter().enumerate() {
            let at = table + i * 40;
            b[at..at + name.len()].copy_from_slice(name.as_bytes());
            put32(&mut b, at + 8, *size);
            put32(&mut b, at + 12, *address);
            put32(&mut b, at + 16, *size);
            put32(&mut b, at + 36, *flags);
        }
        b
    }

    #[test]
    fn a_clean_driver_passes_in_both_image_formats() {
        for pe32 in [false, true] {
            let pe = Pe {
                pe32,
                iat: 0x2010,
                ..Pe::default()
            };
            assert_eq!(scan_pe(&build(&pe)), vec![], "pe32={pe32}");
        }
    }

    #[test]
    fn section_alignment_must_be_a_whole_page() {
        for alignment in [0, 0x20, 0x200, 0x800, 0x1800] {
            let pe = Pe {
                alignment,
                ..Pe::default()
            };
            assert!(
                scan_pe(&build(&pe)).contains(&PeProblem::SectionAlignment),
                "{alignment:#x}"
            );
        }
        for alignment in [0x1000, 0x2000] {
            let pe = Pe {
                alignment,
                ..Pe::default()
            };
            assert_eq!(scan_pe(&build(&pe)), vec![], "{alignment:#x}");
        }
        let pe = Pe {
            sections: vec![(".text", 0x1200, 0x800, 0x6000_0020)],
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![PeProblem::SectionAlignment]);
    }

    #[test]
    fn a_section_that_is_writable_and_executable_is_flagged() {
        let pe = Pe {
            sections: vec![
                (".text", 0x1000, 0x800, 0x6000_0020),
                (".hack", 0x2000, 0x400, 0xE000_0020),
            ],
            ..Pe::default()
        };
        assert_eq!(
            scan_pe(&build(&pe)),
            vec![PeProblem::WritableAndExecutable(".hack".into())]
        );
        let pe = Pe {
            sections: vec![
                (".a", 0x1000, 0x800, 0xC000_0040),
                (".b", 0x2000, 0x400, 0x2000_0020),
            ],
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![]);
    }

    #[test]
    fn the_old_init_section_is_tolerated_because_windows_removes_its_write_permission() {
        for name in ["INIT", "init"] {
            let pe = Pe {
                sections: vec![
                    (".text", 0x1000, 0x800, 0x6000_0020),
                    (name, 0x2000, 0x400, 0xE000_0020),
                ],
                ..Pe::default()
            };
            assert_eq!(scan_pe(&build(&pe)), vec![], "{name}");
        }
    }

    #[test]
    fn the_import_table_must_not_be_in_an_executable_section() {
        let pe = Pe {
            iat: 0x1100,
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![PeProblem::ImportTableExecutable]);
        let pe = Pe {
            iat: 0x1000 + 0x800 - 1,
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![PeProblem::ImportTableExecutable]);
        let pe = Pe {
            iat: 0x1000 + 0x800,
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![]);
        let pe = Pe {
            iat: 0x2008,
            ..Pe::default()
        };
        assert_eq!(scan_pe(&build(&pe)), vec![]);
    }

    #[test]
    fn broken_or_truncated_files_are_reported_never_waved_through() {
        let good = build(&Pe::default());
        assert_eq!(scan_pe(&[]), vec![PeProblem::NotAnImage]);
        assert_eq!(scan_pe(b"MZ"), vec![PeProblem::NotAnImage]);
        assert_eq!(scan_pe(&good[..0x90]), vec![PeProblem::NotAnImage]);
        assert_eq!(scan_pe(&good[..0x170]), vec![PeProblem::NotAnImage]);
        let mut bad = good.clone();
        bad[0x80..0x84].copy_from_slice(b"PX\0\0");
        assert_eq!(scan_pe(&bad), vec![PeProblem::NotAnImage]);
        let mut bad = good.clone();
        put16(&mut bad, 0x84 + 20, 0x1234);
        assert_eq!(scan_pe(&bad), vec![PeProblem::NotAnImage]);
        let mut bad = good.clone();
        put32(&mut bad, 0x3c, 0xFFFF_FFF0);
        assert_eq!(scan_pe(&bad), vec![PeProblem::NotAnImage]);
        let mut bad = good.clone();
        put16(&mut bad, 0x84 + 2, 0);
        assert_eq!(scan_pe(&bad), vec![PeProblem::NotAnImage]);
        let mut bad = good;
        put16(&mut bad, 0x84 + 2, 500);
        assert_eq!(scan_pe(&bad), vec![PeProblem::NotAnImage]);
    }
}
