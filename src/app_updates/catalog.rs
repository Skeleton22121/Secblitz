//! Pure metadata validation, not a catalog loader or an execution authorization.
//! Production has NO catalog entries, including on Windows and in release builds.
use super::Release;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

pub const MAX_CATALOG_AGE_SECONDS: u64 = 30 * 24 * 60 * 60;
pub const MAX_STABLE_OBSERVATION_AGE_SECONDS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogReview {
    pub reviewed_at: u64,
    pub expires_at: u64,
    pub stable_observed_at: u64,
    pub stable_version_observed: String,
}

pub(super) fn validate(release: &Release, at: u64) -> Result<()> {
    let review = release
        .review
        .as_ref()
        .context("No dated catalog review; legacy releases are unsupported")?;
    let lifetime = review
        .expires_at
        .checked_sub(review.reviewed_at)
        .context("Invalid catalog review window")?;
    ensure!(
        (1..=MAX_CATALOG_AGE_SECONDS).contains(&lifetime),
        "Catalog review lifetime must be at most 30 days"
    );
    ensure!(
        at >= review.reviewed_at && at < review.expires_at,
        "Catalog review expired or clock predates the review"
    );
    ensure!(
        review.stable_observed_at >= review.reviewed_at && review.stable_observed_at <= at,
        "Publisher stable observation predates review or is in the future"
    );
    ensure!(
        at - review.stable_observed_at < MAX_STABLE_OBSERVATION_AGE_SECONDS,
        "Current stable publisher evidence is stale"
    );
    ensure!(
        !release.version.is_empty() && review.stable_version_observed == release.version,
        "Target is not the observed current stable release"
    );
    // This does not authenticate caller-supplied metadata, prove installed payload
    // identity, or turn freshness into a security assessment. All public mutation
    // APIs remain unconditionally unsupported, even when this check succeeds.
    Ok(())
}
