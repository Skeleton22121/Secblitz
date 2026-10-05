//! Standalone non-test compilation probe. No lib.rs/CLI integration is required.
//! Build with scripts/test-app-updates-production.py; cfg(test) is NOT enabled.
#[allow(dead_code)]
#[path = "../../src/app_updates.rs"]
mod app_updates;

fn main() {
    assert_eq!(app_updates::MAX_CATALOG_AGE_SECONDS, 30 * 24 * 60 * 60);
    assert_eq!(app_updates::MAX_STABLE_OBSERVATION_AGE_SECONDS, 24 * 60 * 60);
    let capabilities = app_updates::capabilities();
    assert!(!capabilities.supported);
    assert!(!capabilities.native_backend);
    assert!(capabilities.reviewed_packages.is_empty());
    assert!(app_updates::discover().is_err());
    assert!(app_updates::plan("Microsoft.VisualStudioCode", "1.140.0", 600).is_err());
    assert!(app_updates::approve(uuid::Uuid::nil(), "digest", Default::default()).is_err());
    assert!(app_updates::start(uuid::Uuid::nil(), "digest").is_err());
    assert!(app_updates::verify(uuid::Uuid::nil()).is_err());
    assert!(app_updates::get(uuid::Uuid::nil()).is_err());
    assert!(app_updates::list().is_err());
}
