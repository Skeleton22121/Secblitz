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
        ProbeId::AccountSetup => parse!(AccountSetup),
        ProbeId::WindowsHello => parse!(WindowsHello),
        ProbeId::DnsEncryption => parse!(DnsEncryption),
        ProbeId::WifiSecurity => parse!(WifiSecurity),
        ProbeId::Autostart => parse!(Autostart),
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

/// `dsregcmd /status` prints many identity lines (device, tenant, account). Only
/// the single `NgcSet` line is read; everything else is dropped and nothing from
/// the output is retained. A missing, duplicated or unexpected value stays Unknown,
/// so a changed output format can never be read as "no PIN" or "PIN set".
pub(super) fn dsreg(bytes: &[u8]) -> WindowsHello {
    let unknown = |reason| WindowsHello {
        pin_set: Reading::Unknown(reason),
    };
    if bytes.len() > MAX_OUTPUT_BYTES {
        return unknown(UnknownReason::OutputLimit);
    }
    let text = String::from_utf8_lossy(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes));
    let mut values = text.lines().filter_map(|line| {
        let (key, value) = line.trim().trim_matches('|').trim().split_once(':')?;
        (key.trim() == "NgcSet").then(|| value.trim().to_owned())
    });
    let first = values.next();
    if values.next().is_some() {
        return unknown(UnknownReason::InvalidData);
    }
    WindowsHello {
        pin_set: match first.as_deref() {
            Some("YES") => Reading::Known(true),
            Some("NO") => Reading::Known(false),
            Some(_) => Reading::Unknown(UnknownReason::InvalidData),
            None => Reading::Unknown(UnknownReason::Unavailable),
        },
    }
}

/// Wi-Fi security type from the two WLAN algorithm numbers (DOT11_AUTH_ALGORITHM
/// and DOT11_CIPHER_ALGORITHM). Anything not recognised is "Other", which the
/// rules treat as unknown, never as protected.
pub(super) fn wifi_class(security_enabled: bool, auth: u32, cipher: u32) -> &'static str {
    const WEP: [u32; 3] = [1, 5, 0x101];
    match auth {
        1 if !security_enabled || cipher == 0 => "Open",
        1 if WEP.contains(&cipher) => "Wep",
        2 => "Wep",
        3..=5 => "Old",
        6 | 7 => match cipher {
            2 => "Old",
            4 | 8 | 9 | 0xa => "Strong",
            _ => "Other",
        },
        8 | 9 | 11 => "Strong",
        _ => "Other",
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(super) fn wifi_rank(class: &str) -> u8 {
    match class {
        "Open" => 0,
        "Wep" => 1,
        "Old" => 2,
        "Other" => 3,
        _ => 4,
    }
}
