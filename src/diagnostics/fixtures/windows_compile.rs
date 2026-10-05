//! Compile-only Windows launcher harness. No native evidence is generated.
//! This type-shaped, panicking audit boundary permits independent typechecking
//! before the coordinator exports diagnostics, without editing shared files.
//! The production call uses the real crate::permissions::audit() API.
pub mod permissions {
    pub struct Finding { pub title: String, pub status: String }
    pub fn audit() -> Result<Vec<Finding>, ()> {
        panic!("Compile-only harness must never execute native collection")
    }
}
#[path = "../../diagnostics.rs"]
pub mod diagnostics;
