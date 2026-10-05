//! Canonical bounded snapshots. No paths, SDDL, or caller-selected objects.
use anyhow::{ensure, Context, Result};
use serde_json::Value;

pub(super) const MAX_SD: usize = 16 * 1024;
const FLAGS: u16 = 0x8000 | 0x0004 | 0x0008 | 0x0400 | 0x1000;
const REMOVE: u32 = 0x000d_0002;
// Service-specific rights, standard service rights and the four generic bits.
// Other access bits have no evaluated service-grant semantics in this module.
pub(super) const SUPPORTED_MASK: u32 = 0xf000_0000 | 0x000f_01ff;

fn u16_at(b: &[u8], n: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        b.get(n..n + 2).context("Truncated WORD")?.try_into()?,
    ))
}
fn u32_at(b: &[u8], n: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(n..n + 4).context("Truncated DWORD")?.try_into()?,
    ))
}

pub(super) fn sid_string(b: &[u8]) -> Result<String> {
    ensure!(b.len() >= 8 && b[0] == 1 && b[1] <= 15, "Invalid SID");
    ensure!(b.len() == 8 + usize::from(b[1]) * 4, "Invalid SID size");
    let auth = b[2..8].iter().fold(0u64, |n, x| n * 256 + u64::from(*x));
    let mut s = format!("S-1-{auth}");
    for i in 0..usize::from(b[1]) {
        s.push_str(&format!("-{}", u32_at(b, 8 + i * 4)?));
    }
    Ok(s)
}

pub(super) fn trusted_owner(owner: &[u8]) -> Result<()> {
    let sid = sid_string(owner)?;
    ensure!(
        matches!(
            sid.as_str(),
            "S-1-5-18"
                | "S-1-5-32-544"
                | "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464"
        ),
        "Service owner is not SYSTEM, Administrators or TrustedInstaller"
    );
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct State {
    pub flags: u16,
    pub owner: Vec<u8>,
    pub group: Vec<u8>,
    pub acl: Option<Vec<u8>>,
}

impl State {
    // This is a structural transition constraint, not rollback authorization.
    // Repair is many-to-one; the engine must supply its durable before-image.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn check_transition(&self, desired: &Self) -> Result<()> {
        trusted_owner(&self.owner)?;
        ensure!(
            self.owner == desired.owner
                && self.group == desired.group
                && self.flags == desired.flags,
            "Service owner, group or descriptor flags changed"
        );
        ensure!(
            self.repair()? == *desired || desired.repair()? == *self,
            "Service DACL drift or non-repair transition"
        );
        Ok(())
    }
    /// Native self-relative descriptors may have different section ordering;
    /// normalize only container offsets, never ACE bytes/order or ACL padding.
    pub fn from_sd(sd: &[u8]) -> Result<Self> {
        ensure!(
            (20..=MAX_SD).contains(&sd.len()) && sd[0] == 1 && sd[1] == 0,
            "Invalid descriptor header"
        );
        let flags = u16_at(sd, 2)?;
        ensure!(
            flags & 0x8000 != 0 && flags & !FLAGS == 0,
            "Unsupported descriptor flags"
        );
        ensure!(u32_at(sd, 12)? == 0, "SACL snapshots are forbidden");
        let mut ranges = Vec::new();
        let mut read_sid = |slot| -> Result<Vec<u8>> {
            let start = u32_at(sd, slot)? as usize;
            ensure!(
                start >= 20 && start <= sd.len() && start.is_multiple_of(4),
                "Invalid SID offset"
            );
            let header = sd.get(start..start + 8).context("Truncated SID")?;
            let end = start + 8 + usize::from(header[1]) * 4;
            let bytes = sd.get(start..end).context("Truncated SID")?;
            sid_string(bytes)?;
            ranges.push((start, end));
            Ok(bytes.to_vec())
        };
        let owner = read_sid(4)?;
        let group = read_sid(8)?;
        let start = u32_at(sd, 16)? as usize;
        let acl = if start == 0 {
            None
        } else {
            ensure!(
                flags & 4 != 0 && start >= 20 && start <= sd.len() && start.is_multiple_of(4),
                "Invalid DACL offset"
            );
            let size = usize::from(u16_at(sd, start + 2)?);
            let end = start + size;
            let bytes = sd.get(start..end).context("Truncated DACL")?;
            acl_entries(bytes)?;
            ranges.push((start, end));
            Some(bytes.to_vec())
        };
        ranges.sort_unstable();
        ensure!(
            ranges.windows(2).all(|r| r[0].1 <= r[1].0),
            "Overlapping descriptor sections"
        );
        Ok(Self {
            flags,
            owner,
            group,
            acl,
        })
    }

    pub fn sd(&self) -> Vec<u8> {
        let mut b = vec![0; 20];
        b[0] = 1;
        b[2..4].copy_from_slice(&self.flags.to_le_bytes());
        for (slot, part) in [
            (4, Some(&self.owner)),
            (8, Some(&self.group)),
            (16, self.acl.as_ref()),
        ] {
            if let Some(part) = part {
                let offset = b.len() as u32;
                b[slot..slot + 4].copy_from_slice(&offset.to_le_bytes());
                b.extend(part);
            }
        }
        b
    }

    pub fn value(&self) -> Value {
        use std::fmt::Write;
        let mut s = String::from("dacl-v1:");
        for byte in self.sd() {
            write!(&mut s, "{byte:02x}").unwrap();
        }
        Value::String(s)
    }

    pub fn parse(value: &Value) -> Result<Self> {
        let s = value
            .as_str()
            .context("Service DACL state must be a string")?;
        ensure!(
            s.len() <= 8 + MAX_SD * 2,
            "Service DACL state exceeds limit"
        );
        let hex = s
            .strip_prefix("dacl-v1:")
            .context("Invalid service DACL state version")?;
        ensure!(
            hex.len().is_multiple_of(2)
                && hex
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Noncanonical DACL hex"
        );
        let b: Vec<u8> = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|c| {
                let digit = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
                digit(c[0]) * 16 + digit(c[1])
            })
            .collect();
        let state = Self::from_sd(&b)?;
        ensure!(
            state.sd() == b,
            "Noncanonical DACL descriptor offsets or trailing bytes"
        );
        Ok(state)
    }

    pub fn repair(&self) -> Result<Self> {
        trusted_owner(&self.owner)?;
        let acl = self
            .acl
            .as_ref()
            .context("NULL or absent DACL requires manual review")?;
        let entries = acl_entries(acl)?;
        let mut next = self.clone();
        let target = next.acl.as_mut().unwrap();
        for (at, size) in entries {
            let ace = &acl[at..at + size];
            ensure!(
                ace[0] == 0 && ace[1] == 0,
                "Deny, inherited, flagged or unsupported ACE requires manual review"
            );
            let principal = sid_string(&ace[8..])?;
            let mask = u32_at(ace, 4)?;
            ensure!(
                mask & !SUPPORTED_MASK == 0,
                "Unsupported service access mask requires manual review"
            );
            if !matches!(principal.as_str(), "S-1-1-0" | "S-1-5-11" | "S-1-5-32-545") {
                continue;
            }
            if mask & (REMOVE | 0x10000000 | 0x40000000) == 0 {
                continue;
            }
            // Map every generic bit on a changed ACE, preserving its safe rights.
            let mut mapped = mask & !0xf0000000;
            for (generic, rights) in [
                (0x80000000, 0x2008d),
                (0x40000000, 0x20002),
                (0x20000000, 0x20170),
                (0x10000000, 0xf01ff),
            ] {
                if mask & generic != 0 {
                    mapped |= rights;
                }
            }
            mapped &= !REMOVE;
            target[at + 4..at + 8].copy_from_slice(&mapped.to_le_bytes());
        }
        Ok(next)
    }
}

fn acl_entries(acl: &[u8]) -> Result<Vec<(usize, usize)>> {
    ensure!(
        acl.len() >= 8 && matches!(acl[0], 2 | 4) && acl[1] == 0 && u16_at(acl, 6)? == 0,
        "Invalid ACL header"
    );
    ensure!(
        usize::from(u16_at(acl, 2)?) == acl.len() && acl.len().is_multiple_of(4),
        "Invalid ACL size"
    );
    let mut at = 8;
    let mut entries = Vec::new();
    for _ in 0..u16_at(acl, 4)? {
        let header = acl.get(at..at + 4).context("Truncated ACE")?;
        let size = usize::from(u16_at(header, 2)?);
        ensure!(size >= 4 && size.is_multiple_of(4), "Invalid ACE size");
        let ace = acl.get(at..at + size).context("Truncated ACE")?;
        if matches!(ace[0], 0 | 1) {
            sid_string(ace.get(8..).context("Truncated ALLOW/DENY ACE")?)?;
        }
        entries.push((at, size));
        at += size;
    }
    // Padding is retained verbatim. It is not another ACE.
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{controls, repair_target, validate_value};
    const ID: &str = "permissions.service.bits";

    fn sid(authority: u8, subs: &[u32]) -> Vec<u8> {
        let mut b = vec![1, subs.len() as u8, 0, 0, 0, 0, 0, authority];
        for sub in subs {
            b.extend(sub.to_le_bytes());
        }
        b
    }
    fn state(aces: &[(u8, u8, u32, Vec<u8>)]) -> State {
        let mut acl = vec![2, 0, 0, 0, 0, 0, 0, 0];
        acl[4..6].copy_from_slice(&(aces.len() as u16).to_le_bytes());
        for (kind, flags, mask, sid) in aces {
            acl.extend([*kind, *flags]);
            acl.extend(((8 + sid.len()) as u16).to_le_bytes());
            acl.extend(mask.to_le_bytes());
            acl.extend(sid);
        }
        let size = acl.len() as u16;
        acl[2..4].copy_from_slice(&size.to_le_bytes());
        State {
            flags: 0x9004,
            owner: sid(5, &[18]),
            group: sid(5, &[32, 544]),
            acl: Some(acl),
        }
    }
    fn sample(mask: u32) -> State {
        state(&[
            (0, 0, mask, sid(5, &[11])),
            (0, 0, 0xf01ff, sid(5, &[32, 544])),
        ])
    }

    #[test]
    fn deterministic_exact_roundtrip_changes_only_selected_masks() {
        for principal in [sid(1, &[0]), sid(5, &[11]), sid(5, &[32, 545])] {
            let original = state(&[
                (0, 0, 0xf01ff, principal),
                (0, 0, 0xf01ff, sid(5, &[18])),
                (0, 0, 0xf01ff, sid(5, &[32, 544])),
                (0, 0, 0xf01ff, sid(5, &[21, 1, 2, 3, 1001])),
            ]);
            let target = original.repair().unwrap();
            let mut expected = original.clone();
            expected.acl.as_mut().unwrap()[12..16].copy_from_slice(&0x201fdu32.to_le_bytes());
            assert_eq!(target, expected);
            assert_eq!(State::parse(&original.value()).unwrap(), original);
            assert_eq!(State::parse(&target.value()).unwrap(), target);
            assert_eq!(target.repair().unwrap(), target);
            original.check_transition(&target).unwrap();
            target.check_transition(&original).unwrap();
            assert_eq!(
                repair_target(ID, &original.value()).unwrap(),
                target.value()
            );
        }
    }

    #[test]
    fn generic_mapping_preserves_safe_service_rights() {
        for (mask, expected) in [
            (0x40000000, 0x20000),
            (0x10000000, 0x201fd),
            (0xf0000000, 0x201fd),
            (0xc0000000, 0x2008d),
            (0x60000000, 0x20170),
            (0x000d0015, 0x15),
            (0x80000000, 0x80000000),
            (0x20000000, 0x20000000),
        ] {
            let target = sample(mask).repair().unwrap();
            assert_eq!(u32_at(target.acl.as_ref().unwrap(), 12).unwrap(), expected);
        }
        let zero = sample(2).repair().unwrap();
        assert_eq!(u32_at(zero.acl.as_ref().unwrap(), 12).unwrap(), 0);
        assert_eq!(u16_at(zero.acl.as_ref().unwrap(), 4).unwrap(), 2);
    }

    #[test]
    fn unsupported_masks_are_observable_but_never_safe_or_restorable() {
        // MAXIMUM_ALLOWED, ACCESS_SYSTEM_SECURITY, SYNCHRONIZE and reserved bits
        // are not ordinary evaluated service grants. Check every unsupported bit,
        // including on an unrelated principal, rather than silently retaining it.
        for bit in (0..32).map(|shift| 1u32 << shift) {
            if bit & SUPPORTED_MASK != 0 {
                continue;
            }
            for principal in [sid(5, &[11]), sid(5, &[32, 544])] {
                for mask in [bit, bit | 2] {
                    let before = state(&[(0, 0, mask, principal.clone())]);
                    validate_value(ID, &before.value()).unwrap();
                    assert!(repair_target(ID, &before.value()).is_err());
                    assert!(before.check_transition(&before).is_err());
                    let safe = state(&[(0, 0, 0, principal.clone())]);
                    assert!(safe.check_transition(&before).is_err());
                }
            }
        }
    }

    #[test]
    fn transitions_do_not_collapse_distinct_safe_descriptors() {
        let before = sample(2 | 0x10);
        let after = before.repair().unwrap();
        let different_safe = sample(0x20);
        assert_ne!(after, different_safe);
        assert!(after.check_transition(&different_safe).is_err());
        // An inverse transform is deliberately not proof of journal provenance:
        // different before-images can share the same target. Only the engine's
        // durable before-image authorizes which of these can actually be restored.
        let another_before = sample(0x40000 | 0x10);
        assert_ne!(another_before, before);
        assert_eq!(another_before.repair().unwrap(), after);
        after.check_transition(&another_before).unwrap();

        let mut reordered = after.clone();
        // Move the first 20-byte ACE after the administrator ACE.
        reordered.acl.as_mut().unwrap()[8..].rotate_left(20);
        assert!(after.check_transition(&reordered).is_err());
        for mask in [0, 0x10, 0x20, 0x20000] {
            let arbitrary = state(&[(0, 0, mask, sid(5, &[21, 1, 2, 3, 1001]))]);
            assert!(after.check_transition(&arbitrary).is_err());
        }
    }

    #[test]
    fn duplicate_principal_aces_and_padding_are_preserved_at_the_size_bound() {
        let mut before = state(&[(0, 0, 2, sid(5, &[11])), (0, 0, 0x40000010, sid(5, &[11]))]);
        let acl = before.acl.as_mut().unwrap();
        let length = MAX_SD - 20 - before.owner.len() - before.group.len();
        acl.resize(length, 0xa5);
        acl[2..4].copy_from_slice(&(length as u16).to_le_bytes());
        let encoded = before.value();
        assert_eq!(encoded.as_str().unwrap().len(), 8 + MAX_SD * 2);
        validate_value(ID, &encoded).unwrap();
        let after = before.repair().unwrap();
        let acl = after.acl.as_ref().unwrap();
        assert_eq!(u32_at(acl, 12).unwrap(), 0);
        assert_eq!(u32_at(acl, 32).unwrap(), 0x20010);
        assert!(acl[48..].iter().all(|b| *b == 0xa5));
        after.check_transition(&before).unwrap();
        assert_eq!(State::parse(&after.value()).unwrap(), after);

        let acl = before.acl.as_mut().unwrap();
        acl.extend([0; 4]);
        let length = acl.len() as u16;
        acl[2..4].copy_from_slice(&length.to_le_bytes());
        assert!(validate_value(ID, &before.value()).is_err());
    }

    #[test]
    fn drift_including_safe_to_safe_owner_group_and_flags_is_rejected() {
        let before = sample(0xf01ff);
        let after = before.repair().unwrap();
        let mut drift = after.clone();
        drift.acl.as_mut().unwrap()[12] ^= 0x10; // Safe SERVICE_START edit.
        assert!(drift.check_transition(&before).is_err());
        for changed in [
            State {
                owner: sid(5, &[32, 544]),
                ..after.clone()
            },
            State {
                group: sid(5, &[18]),
                ..after.clone()
            },
            State {
                flags: 0x8004,
                ..after.clone()
            },
        ] {
            assert!(changed.check_transition(&before).is_err());
        }
        let mut changed_admin = after.clone();
        let acl = changed_admin.acl.as_mut().unwrap();
        acl[32] ^= 0x10;
        assert!(changed_admin.check_transition(&before).is_err());
    }

    #[test]
    fn rejects_ambiguous_acls_untrusted_owner_and_null_but_empty_is_compliant() {
        for (kind, flags) in [(1, 0), (0, 16), (0, 8), (5, 0), (9, 0), (0, 1)] {
            let s = state(&[(kind, flags, 2, sid(5, &[11]))]);
            validate_value(ID, &s.value()).unwrap();
            assert!(repair_target(ID, &s.value()).is_err());
        }
        let empty = state(&[]);
        assert_eq!(empty.repair().unwrap(), empty);
        for flags in [0x8000, 0x8004] {
            let null = State {
                flags,
                acl: None,
                ..empty.clone()
            };
            validate_value(ID, &null.value()).unwrap();
            assert!(repair_target(ID, &null.value()).is_err());
        }
        let untrusted = State {
            owner: sid(5, &[11]),
            ..empty
        };
        assert!(untrusted.repair().is_err());
    }

    #[test]
    fn trusted_owner_requires_exact_sid_not_a_prefix_or_rid() {
        let installer = sid(
            5,
            &[
                80, 956008885, 3418522649, 1831038044, 1853292631, 2271478464,
            ],
        );
        for owner in [sid(5, &[18]), sid(5, &[32, 544]), installer.clone()] {
            let before = State { owner, ..sample(2) };
            let after = before.repair().unwrap();
            assert_eq!(after.owner, before.owner);
            after.check_transition(&before).unwrap();
        }
        let mut lookalike = installer;
        *lookalike.last_mut().unwrap() ^= 1;
        for owner in [
            sid(5, &[18, 0]),
            sid(5, &[21, 1, 2, 3, 544]),
            sid(1, &[18]),
            sid(5, &[80]),
            lookalike,
        ] {
            let before = State { owner, ..sample(2) };
            validate_value(ID, &before.value()).unwrap();
            assert!(before.repair().is_err());
        }
    }

    #[test]
    fn malicious_encodings_and_descriptors_never_become_targets() {
        let good = sample(2).value();
        let string = good.as_str().unwrap();
        for bad in [
            Value::Null,
            serde_json::json!({"dacl": string}),
            serde_json::json!("service-dacl-repair-v1"),
            Value::String(string.to_ascii_uppercase()),
            Value::String(format!("{string}00")),
            Value::String(format!("{string}:extra")),
            Value::String(format!("dacl-v1:{}", "0".repeat(MAX_SD * 2 + 2))),
        ] {
            assert!(validate_value(ID, &bad).is_err());
        }
        for id in [
            "BITS",
            "permissions.service.winDefend",
            "permissions.service.bits;cmd",
            "permissions.service.BITS",
        ] {
            assert!(validate_value(id, &good).is_err());
            assert!(repair_target(id, &good).is_err());
        }
        let bytes = sample(2).sd();
        for length in 0..bytes.len() {
            assert!(State::from_sd(&bytes[..length]).is_err());
        }
        for (slot, value) in [(4, u32::MAX), (8, 20), (16, 20), (12, 20), (16, 21)] {
            let mut corrupt = bytes.clone();
            corrupt[slot..slot + 4].copy_from_slice(&value.to_le_bytes());
            assert!(State::from_sd(&corrupt).is_err());
        }
        let mut bad = sample(2);
        bad.acl.as_mut().unwrap()[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(State::parse(&bad.value()).is_err());
        let mut bad = sample(2);
        bad.acl.as_mut().unwrap()[10..12].copy_from_slice(&4u16.to_le_bytes());
        assert!(State::parse(&bad.value()).is_err());
        for control in controls() {
            assert!(validate_value(&control.id, &control.target).is_err());
            validate_value(&control.id, &good).unwrap();
        }
    }
}
