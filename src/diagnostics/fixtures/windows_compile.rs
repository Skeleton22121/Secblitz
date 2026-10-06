//! Compile-only Windows launcher harness; generates no native evidence.
pub mod permissions {
    pub struct Finding { pub title: String, pub status: String }
    pub fn audit() -> Result<Vec<Finding>, ()> {
        panic!("Compile-only harness must never execute native collection")
    }
}
#[path = "../../diagnostics.rs"]
pub mod diagnostics;
