//! Optional signed delivery lane (not TUF). `releases/delivery.json`, signed by the compiled root,
//! delegates one key to ONE exact `releases/candidate.json` for at most seven days; no recursive
//! keys, thresholds or runtime endpoints. Only a 404 before enrollment permits v1 fallback.
//! Holdback is forward-only: every observed candidate advances the release floor, even at 0%.
use super::*;

pub(super) const LIFETIME: u64 = 7 * 86400;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Authorization {
    pub schema: u32,
    pub role: String,
    pub sequence: u64,
    pub origin: String,
    pub target: String,
    pub published_at: u64,
    pub expires_at: u64,
    pub version: String,
    pub manifest_sha256: String,
    pub manifest_key: String,
    pub rollout: Rollout,
    pub health: HealthCommand,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Rollout {
    pub salt: String,
    pub basis_points: u16,
}

// Signed metadata selects only a compiled enum. It cannot supply argv or code.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum HealthCommand {
    UpdateHealthV1,
}
impl HealthCommand {
    pub fn args(self) -> [&'static str; 3] {
        match self {
            Self::UpdateHealthV1 => ["update", "health", "--json"],
        }
    }
}

// Also used for durable state: authenticate and validate it, but do not apply
// wall-clock expiry to the record of a previously accepted sequence.
pub(super) fn decode(
    bytes: &[u8],
    root: &[u8; 32],
    expected_origin: &reqwest::Url,
) -> Result<Authorization> {
    let a: Authorization = serde_json::from_slice(&verified_payload(bytes, root)?)?;
    ensure!(
        a.schema == 1
            && a.role == "secblitz-delivery"
            && a.sequence > 0
            && a.target == super::arch::TARGET,
        "Unsupported delivery authorization"
    );
    ensure!(
        a.origin == expected_origin.as_str(),
        "Delivery origin mismatch"
    );
    stable(&a.version)?;
    ensure!(
        a.expires_at > a.published_at && a.expires_at - a.published_at <= LIFETIME,
        "Invalid delegation lifetime"
    );
    ensure!(
        valid_hash(&a.manifest_sha256)
            && valid_hash(&a.manifest_key)
            && valid_hash(&a.rollout.salt)
            && a.rollout.basis_points <= 10000,
        "Invalid delivery key, digest or rollout"
    );
    let key = VerifyingKey::from_bytes(&delegated_key(&a)?)?;
    ensure!(!key.is_weak(), "Weak delegated key");
    Ok(a)
}

pub(super) fn fresh(a: &Authorization, time: u64) -> Result<()> {
    // No post-expiry grace for delegation. The v1 manifest keeps its existing
    // skew behavior, but must additionally satisfy this narrower authority.
    ensure!(
        a.published_at <= time.saturating_add(SKEW) && time < a.expires_at,
        "Delivery authorization expired or not yet valid"
    );
    Ok(())
}

pub(super) fn advance(a: &Authorization, previous: Option<&Authorization>) -> Result<()> {
    if let Some(p) = previous {
        ensure!(a.sequence >= p.sequence, "Delivery sequence rollback");
        if a.sequence == p.sequence {
            ensure!(a == p, "Delivery sequence equivocation");
        } else {
            ensure!(
                a.published_at >= p.published_at && stable(&a.version)? >= stable(&p.version)?,
                "Delivery publication/version rollback"
            );
            if a.version == p.version {
                ensure!(
                    a.rollout.salt == p.rollout.salt,
                    "Release cohort salt changed"
                );
            }
            // Higher sequences may reduce percentage/expiry, including to 0%.
            // The latest root authorization is authoritative, not a union of keys.
        }
    }
    Ok(())
}

fn delegated_key(a: &Authorization) -> Result<[u8; 32]> {
    hex::decode(&a.manifest_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid delegated key size"))
}

pub(super) fn candidate(raw: &[u8], a: &Authorization, time: u64) -> Result<Manifest> {
    fresh(a, time)?;
    ensure!(
        raw.len() <= MANIFEST_LIMIT && hex::encode(Sha256::digest(raw)) == a.manifest_sha256,
        "Candidate envelope differs from root authorization"
    );
    let m = verify(raw, &delegated_key(a)?, time)?;
    ensure!(m.version == a.version, "Delegated release version mismatch");
    Ok(m)
}

pub(super) fn cohort(device: &[u8; 16], rollout: &Rollout) -> Result<u16> {
    ensure!(
        valid_hash(&rollout.salt) && rollout.basis_points <= 10000,
        "Invalid rollout"
    );
    let mut hash = Sha256::new();
    hash.update(b"secblitz-rollout-v1\0");
    hash.update(hex::decode(&rollout.salt)?);
    hash.update(device);
    let digest = hash.finalize();
    Ok((u64::from_be_bytes(digest[..8].try_into().unwrap()) % 10000) as u16)
}

pub(super) fn eligible(device: &[u8; 16], a: &Authorization) -> Result<bool> {
    Ok(cohort(device, &a.rollout)? < a.rollout.basis_points)
}
