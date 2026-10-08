//! CPUID probes used to explain why a protection is or is not offered.

/// CPUID leaf 7, ECX bit 7: the same flag on Intel and AMD. Only x86 has this
/// flag, so kernel stack protection is never offered on an ARM processor.
pub fn cpu_has_shadow_stacks() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        cpuid_leaf7_ecx().is_some_and(|ecx| ecx & (1 << 7) != 0)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

#[cfg(target_arch = "x86_64")]
fn cpuid_leaf7_ecx() -> Option<u32> {
    use std::arch::x86_64::{__cpuid, __cpuid_count};
    // SAFETY: CPUID exists on every x86_64 processor and only reads.
    unsafe {
        if __cpuid(0).eax < 7 {
            return None;
        }
        Some(__cpuid_count(7, 0).ecx)
    }
}

/// CPUID leaf 1 bit 31, then leaf 0x40000000; None on bare hardware. Other
/// processors have no such leaf, so Windows' own answer is used instead.
pub fn cpu_hypervisor_vendor(windows_says_present: Option<bool>) -> Option<String> {
    #[cfg(target_arch = "x86_64")]
    {
        let _ = windows_says_present;
        hypervisor_vendor_x86()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        vendor_from_windows(windows_says_present)
    }
}

/// Windows does not name the vendor, so any hypervisor is "unknown": that
/// only counts when memory integrity is already running, never as a reason
/// to offer it. A PC where the answer is missing is treated as a virtual
/// machine too, because a wrong offer costs more than a missing one.
#[cfg(any(not(target_arch = "x86_64"), test))]
fn vendor_from_windows(present: Option<bool>) -> Option<String> {
    match present {
        Some(false) => None,
        _ => Some("unknown".into()),
    }
}

#[cfg(target_arch = "x86_64")]
fn hypervisor_vendor_x86() -> Option<String> {
    use std::arch::x86_64::{__cpuid, __cpuid_count};
    // SAFETY: CPUID exists on every x86_64 processor and only reads.
    let leaf = unsafe {
        if __cpuid(1).ecx & (1 << 31) == 0 {
            return None;
        }
        __cpuid_count(0x4000_0000, 0)
    };
    Some(vendor_text([leaf.ebx, leaf.ecx, leaf.edx]))
}

#[cfg(any(target_arch = "x86_64", test))]
fn vendor_text(registers: [u32; 3]) -> String {
    let mut raw = Vec::with_capacity(12);
    for part in registers {
        raw.extend_from_slice(&part.to_le_bytes());
    }
    let text: String = raw
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| {
            if b.is_ascii_graphic() || *b == b' ' {
                char::from(*b)
            } else {
                '?'
            }
        })
        .collect();
    // A flag without a readable vendor still means some hypervisor.
    if text.is_empty() {
        "unknown".into()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registers(name: &[u8; 12]) -> [u32; 3] {
        let word = |at: usize| u32::from_le_bytes(name[at..at + 4].try_into().unwrap());
        [word(0), word(4), word(8)]
    }

    #[test]
    fn without_a_cpuid_leaf_windows_decides_whether_a_hypervisor_runs() {
        assert_eq!(vendor_from_windows(Some(false)), None);
        assert_eq!(vendor_from_windows(Some(true)).as_deref(), Some("unknown"));
        assert_eq!(vendor_from_windows(None).as_deref(), Some("unknown"));
    }

    #[test]
    fn vendor_text_reads_the_twelve_byte_name() {
        assert_eq!(vendor_text(registers(b"Microsoft Hv")), "Microsoft Hv");
        assert_eq!(vendor_text(registers(b"KVMKVMKVM\0\0\0")), "KVMKVMKVM");
    }

    #[test]
    fn vendor_text_never_returns_empty_or_control_bytes() {
        assert_eq!(vendor_text([0, 0, 0]), "unknown");
        assert_eq!(
            vendor_text(registers(b"ab\x01\x7fcd\0\0\0\0\0\0")),
            "ab??cd"
        );
    }
}
