//! Parse self-relative DACL bytes without pointer casts or unbounded ACE/SID reads.
use super::state::{MAX_SD, SUPPORTED_MASK};
use anyhow::{ensure, Context, Result};

const DANGEROUS: u32 = 0x000d_0002 | 0x1000_0000 | 0x4000_0000;

#[derive(Debug, Default)]
pub(super) struct Assessment {
    pub candidates: Vec<String>,
    pub complex: bool,
    pub unrestricted: bool,
    /// Set only when something that could GRANT access was not understood
    /// (unknown descriptor flags, callback/object/unknown allow-type ACEs).
    /// Deny, audit, inherited and extra-mask ACEs never widen access, so they
    /// make `complex` true but leave this false.
    pub unevaluated_grant: bool,
}

fn word(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .context("Truncated WORD")?
            .try_into()?,
    ))
}
fn dword(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .context("Truncated DWORD")?
            .try_into()?,
    ))
}

fn sid(bytes: &[u8]) -> Result<String> {
    ensure!(bytes.len() >= 8 && bytes[0] == 1, "Invalid SID header");
    let count = bytes[1] as usize;
    ensure!(
        count <= 15 && bytes.len() == 8 + count * 4,
        "Invalid SID length"
    );
    let authority = bytes[2..8]
        .iter()
        .fold(0u64, |n, b| (n << 8) | u64::from(*b));
    let mut result = format!("S-1-{authority}");
    for i in 0..count {
        result.push_str(&format!("-{}", dword(bytes, 8 + i * 4)?));
    }
    Ok(result)
}

pub(super) fn assess(sd: &[u8]) -> Result<Assessment> {
    ensure!(
        (20..=MAX_SD).contains(&sd.len()) && sd[0] == 1 && sd[1] == 0,
        "Invalid security descriptor header"
    );
    let control = word(sd, 2)?;
    ensure!(control & 0x8000 != 0, "Descriptor is not self-relative");
    let mut result = Assessment::default();
    let offset = dword(sd, 16)? as usize;
    ensure!(
        control & 4 != 0 || offset == 0,
        "DACL offset without DACL_PRESENT"
    );
    // An advisory DACL-only query need not include owner/group. Unknown control
    // semantics must still never produce the ordinary no-candidate result.
    result.complex = control & !(0x8000 | 0x0004 | 0x0008 | 0x0400 | 0x1000) != 0;
    // No control flag can grant access: a missing DACL is handled below.
    if control & 4 == 0 || offset == 0 {
        result.unrestricted = true;
        return Ok(result);
    }
    ensure!(
        offset >= 20 && offset.is_multiple_of(4),
        "Invalid DACL offset"
    );
    let acl = sd.get(offset..).context("DACL outside descriptor")?;
    ensure!(
        acl.len() >= 8 && matches!(acl[0], 2 | 4) && acl[1] == 0 && word(acl, 6)? == 0,
        "Invalid ACL header"
    );
    let size = word(acl, 2)? as usize;
    ensure!(size >= 8 && size.is_multiple_of(4), "Invalid ACL size");
    let acl = acl.get(..size).context("Truncated ACL")?;
    let mut at = 8;
    for _ in 0..word(acl, 4)? {
        let header = acl.get(at..at + 4).context("Truncated ACE header")?;
        let size = word(header, 2)? as usize;
        ensure!(size >= 4 && size.is_multiple_of(4), "Invalid ACE size");
        let ace = acl.get(at..at + size).context("Truncated ACE")?;
        at += size;
        // Denies, inheritance, object/callback/conditional ACEs are not evaluated.
        if ace[0] != 0 || ace[1] != 0 {
            result.complex = true;
        }
        // Types that can grant access but are not evaluated here: compound (4),
        // object (5), callback (9), callback object (11) and anything unknown. Deny (1, 6,
        // 10, 12) and audit/alarm/label types only ever narrow or observe.
        if matches!(ace[0], 4 | 5 | 9 | 11) || ace[0] > 0x13 {
            result.unevaluated_grant = true;
        }
        if matches!(ace[0], 0 | 1) {
            let mask = dword(ace, 4)?;
            result.complex |= mask & !SUPPORTED_MASK != 0;
            let principal = sid(ace.get(8..).context("Missing ACE SID")?)?;
            if ace[0] == 0
                && ace[1] & 8 == 0
                && mask & DANGEROUS != 0
                && matches!(principal.as_str(), "S-1-1-0" | "S-1-5-11" | "S-1-5-32-545")
            {
                result.candidates.push(format!(
                    "{principal}: ALLOW mask 0x{mask:08x}, risky bits 0x{:08x}",
                    mask & DANGEROUS
                ));
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sd(aces: &[(u8, u8, u32, &[u32])]) -> Vec<u8> {
        let mut b = vec![0; 28];
        b[0] = 1;
        b[2] = 4;
        b[3] = 0x80;
        b[16] = 20;
        b[20] = 2;
        b[24..26].copy_from_slice(&(aces.len() as u16).to_le_bytes());
        for (kind, flags, mask, subs) in aces {
            b.extend([*kind, *flags]);
            b.extend(((16 + subs.len() * 4) as u16).to_le_bytes());
            b.extend(mask.to_le_bytes());
            b.extend([1, subs.len() as u8, 0, 0, 0, 0, 0, 5]);
            for sub in *subs {
                b.extend(sub.to_le_bytes());
            }
        }
        let size = (b.len() - 20) as u16;
        b[22..24].copy_from_slice(&size.to_le_bytes());
        b
    }
    #[test]
    fn detects_each_dangerous_right_but_preserves_start_query_and_admins() {
        for bit in [2, 0x10000, 0x40000, 0x80000, 0x10000000, 0x40000000] {
            for principal in [&[11][..], &[32, 545][..]] {
                assert_eq!(
                    assess(&sd(&[(0, 0, bit, principal)]))
                        .unwrap()
                        .candidates
                        .len(),
                    1
                );
            }
        }
        assert!(
            assess(&sd(&[(0, 0, 0x20019, &[11]), (0, 0, 0xf01ff, &[32, 544])]))
                .unwrap()
                .candidates
                .is_empty()
        );
        let mut everyone = sd(&[(0, 0, 2, &[0])]);
        everyone[43] = 1;
        assert_eq!(assess(&everyone).unwrap().candidates.len(), 1);
    }
    #[test]
    fn complex_acls_are_not_effective_access_claims() {
        let a = assess(&sd(&[(1, 0, 2, &[11]), (0, 16, 2, &[11])])).unwrap();
        assert!(a.complex);
        assert_eq!(a.candidates.len(), 1);
        let a = assess(&sd(&[(0, 8, 2, &[11]), (9, 0, 2, &[11])])).unwrap();
        assert!(a.complex);
        assert!(a.candidates.is_empty());
    }
    #[test]
    fn deny_inherited_and_extra_mask_aces_are_not_unevaluated_grants() {
        // A DACL with a deny, an inherited ACE and an extra mask bit is
        // understood well enough to say no risky grant exists.
        let a = assess(&sd(&[
            (1, 0, 2, &[11]),
            (0, 16, 0x20019, &[11]),
            (0, 0, 0x0100_0001, &[32, 544]),
        ]))
        .unwrap();
        assert!(a.complex && !a.unevaluated_grant && a.candidates.is_empty());
        // A callback allow ACE could grant anything, so it stays unknown.
        assert!(assess(&sd(&[(9, 0, 1, &[11])])).unwrap().unevaluated_grant);
        let mut flags = sd(&[]);
        flags[3] |= 1;
        // An unknown control flag cannot grant access by itself.
        let a = assess(&flags).unwrap();
        assert!(a.complex && !a.unevaluated_grant);
        // A compound allow ACE could grant access too.
        assert!(assess(&sd(&[(4, 0, 1, &[11])])).unwrap().unevaluated_grant);
    }
    #[test]
    fn null_and_empty_dacl_differ_and_malformed_data_fails_closed() {
        let empty = sd(&[]);
        assert!(!assess(&empty).unwrap().unrestricted);
        let mut null = empty.clone();
        null[16] = 0;
        assert!(assess(&null).unwrap().unrestricted);
        let b = sd(&[(0, 0, 2, &[11])]);
        for length in 0..b.len() {
            assert!(assess(&b[..length]).is_err());
        }
        for (offset, value) in [(16, 255), (22, 4), (30, 0), (37, 255)] {
            let mut bad = b.clone();
            bad[offset] = value;
            assert!(assess(&bad).is_err());
        }
    }

    #[test]
    fn unsupported_masks_and_descriptor_flags_cannot_report_plain_no_candidates() {
        for bit in (0..32).map(|shift| 1u32 << shift) {
            if bit & SUPPORTED_MASK == 0 {
                for principal in [&[11][..], &[32, 544][..]] {
                    let assessment = assess(&sd(&[(0, 0, bit, principal)])).unwrap();
                    assert!(assessment.complex);
                    assert!(assessment.candidates.is_empty());
                }
            }
        }
        let mut unsupported = sd(&[]);
        unsupported[3] |= 1; // SE_DACL_AUTO_INHERIT_REQ is not evaluated.
        assert!(assess(&unsupported).unwrap().complex);
        let a = assess(&sd(&[(1, 0, 2, &[11]), (0, 0, 2, &[11])])).unwrap();
        assert!(a.complex);
        assert_eq!(a.candidates.len(), 1); // Candidate, never effective access.
    }

    #[test]
    fn malformed_headers_cannot_masquerade_as_empty_or_unrestricted() {
        let empty = sd(&[]);
        for (offset, value) in [(1, 1), (2, 0), (21, 1), (26, 1), (27, 1)] {
            let mut bad = empty.clone();
            bad[offset] = value;
            assert!(assess(&bad).is_err(), "offset {offset}");
        }
        let mut oversized = empty;
        oversized.resize(MAX_SD + 1, 0);
        assert!(assess(&oversized).is_err());
    }
}
