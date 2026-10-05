use super::*;
use serde_json::Value;

/// Bounds and type checks apply even to fixture/imported bytes. This function is
/// private: serialized reports are never accepted as instructions or native facts.
pub(super) fn decode(id: ProbeId, bytes: &[u8]) -> Result<Evidence, UnknownReason> {
    if bytes.len() > MAX_OUTPUT_BYTES {
        return Err(UnknownReason::OutputLimit);
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let UniqueValue(value) =
        serde_json::from_slice(bytes).map_err(|_| UnknownReason::InvalidData)?;
    validate(&value, 0)?;
    macro_rules! parse {
        ($variant:ident) => {
            serde_json::from_value(value)
                .map(Evidence::$variant)
                .map_err(|_| UnknownReason::InvalidData)
        };
    }
    match id {
        ProbeId::UpdateCache => parse!(UpdateCache),
        ProbeId::UpdateHistory => parse!(UpdateHistory),
        ProbeId::DefenderHealth => parse!(DefenderHealth),
        ProbeId::DefenderPolicy => parse!(DefenderPolicy),
        ProbeId::SecurityProviders => parse!(SecurityProviders),
        ProbeId::Management => parse!(Management),
        ProbeId::SecureBoot => parse!(SecureBoot),
        ProbeId::Tpm => parse!(Tpm),
        ProbeId::BitLocker => parse!(BitLocker),
        ProbeId::Vbs => parse!(Vbs),
        ProbeId::WinRe => parse!(WinRe),
        ProbeId::Accounts => parse!(Accounts),
        ProbeId::RemoteAccess => parse!(RemoteAccess),
        ProbeId::Software => parse!(Software),
        ProbeId::BrowserExtensions => parse!(BrowserExtensions),
        ProbeId::Storage => parse!(Storage),
        ProbeId::Ntfs => parse!(Ntfs),
        ProbeId::Backup => parse!(Backup),
        ProbeId::Adapters => parse!(Adapters),
        ProbeId::Dns => parse!(Dns),
        ProbeId::Proxy => parse!(Proxy),
        ProbeId::Vpn => parse!(Vpn),
        ProbeId::Permissions => parse!(Permissions),
        ProbeId::OsSupport => parse!(OsSupport),
        ProbeId::SecureBootCerts => parse!(SecureBootCerts),
        ProbeId::DefenderProtection => parse!(DefenderProtection),
        ProbeId::SmartScreen => parse!(SmartScreen),
        ProbeId::UpdatePolicy => parse!(UpdatePolicy),
        ProbeId::LegacyFeatures => parse!(LegacyFeatures),
        ProbeId::HostsFile => parse!(HostsFile),
        ProbeId::Persistence => parse!(Persistence),
        ProbeId::AccountHygiene => parse!(AccountHygiene),
        ProbeId::Sharing => parse!(Sharing),
        ProbeId::FirewallRules => parse!(FirewallRules),
    }
}

fn validate(v: &Value, depth: usize) -> Result<(), UnknownReason> {
    if depth > 16 {
        return Err(UnknownReason::InvalidData);
    }
    match v {
        Value::String(s) if s.len() > 256 || s.chars().any(char::is_control) => {
            return Err(UnknownReason::InvalidData)
        }
        Value::Array(a) => {
            if a.len() > MAX_ITEMS {
                return Err(UnknownReason::OutputLimit);
            }
            for child in a {
                validate(child, depth + 1)?;
            }
        }
        Value::Object(o) => {
            if o.len() > 64 {
                return Err(UnknownReason::OutputLimit);
            }
            for (key, child) in o {
                if key.len() > 64 {
                    return Err(UnknownReason::InvalidData);
                }
                validate(child, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn diagnostic(id: ProbeId, bytes: &[u8]) -> Diagnostic {
    match decode(id, bytes) {
        Ok(evidence) => Diagnostic {
            id,
            scope: id.scope(),
            observed_at_unix_seconds: now(),
            source: id.source().into(),
            status: Status::Unknown,
            evidence: Some(evidence),
            failure: None,
            assessments: vec![],
        },
        Err(reason) => unavailable(id, reason),
    }
}

// serde_json::Value normally accepts duplicate object keys (last value wins).
// Duplicate native facts are corrupt evidence, not an opportunity to pick the
// most favorable status. The byte cap and serde_json recursion limit apply first.
struct UniqueValue(Value);
impl<'de> serde::Deserialize<'de> for UniqueValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueValue;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| UniqueValue(Value::Number(v)))
                    .ok_or_else(|| E::custom("Nonfinite number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut result = Vec::new();
                while let Some(UniqueValue(value)) = seq.next_element()? {
                    result.push(value);
                }
                Ok(UniqueValue(Value::Array(result)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut result = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if result.contains_key(&key) {
                        return Err(serde::de::Error::custom("Duplicate key"));
                    }
                    let UniqueValue(value) = map.next_value()?;
                    result.insert(key, value);
                }
                Ok(UniqueValue(Value::Object(result)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// REAgentC output is localized. Recognize only one unambiguous English status
/// line; other languages, duplicates, stderr and failed exit codes stay Unknown.
/// No paths/BCD identifiers from the command are retained in the report.
pub(super) fn winre(bytes: &[u8]) -> WinRe {
    let text = if bytes.starts_with(&[0xff, 0xfe])
        || bytes.iter().take(64).filter(|b| **b == 0).count() > 8
    {
        let bytes = bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes);
        if !bytes.len().is_multiple_of(2) {
            return WinRe {
                enabled: Reading::Unknown(UnknownReason::InvalidData),
            };
        }
        String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
        .ok()
    } else {
        // English status text is ASCII in OEM/ANSI output too. Never replace
        // malformed bytes and accidentally promote a corrupted status line.
        std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
            .ok()
            .map(str::to_owned)
    };
    let Some(text) = text else {
        return WinRe {
            enabled: Reading::Unknown(UnknownReason::InvalidData),
        };
    };
    let mut states = text.lines().filter_map(|line| {
        let (key, value) = line.trim().split_once(':')?;
        (key.trim() == "Windows RE status").then_some(value.trim())
    });
    let first = states.next();
    let enabled = if states.next().is_some() {
        Reading::Unknown(UnknownReason::InvalidData)
    } else {
        match first {
            Some("Enabled") => Reading::Known(true),
            Some("Disabled") => Reading::Known(false),
            _ => Reading::Unknown(UnknownReason::Unavailable),
        }
    };
    WinRe { enabled }
}
