//! UI-independent application layer shared by the GUI.
//!
//! Nothing here draws pixels or reads input. Pages in `crate::gui` call into
//! these modules; everything is unit-testable on the host.
pub mod flow;
pub mod history;
pub mod last_check;
pub mod score;
pub mod settings;
pub mod tools;
pub mod worker;
