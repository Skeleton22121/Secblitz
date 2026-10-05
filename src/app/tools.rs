//! Background wrappers for the Tools page: Defender, Windows repair
//! (operations), Windows quality updates (patching), PC health tips
//! (diagnostics), password generation.
//! OWNER: tools agent. All long work runs off the UI thread and reports via
//! `iced::futures` streams/tasks created in `crate::gui::pages::tools`.
