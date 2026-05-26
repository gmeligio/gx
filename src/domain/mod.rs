pub mod action;
pub mod diff;
pub mod event;
pub mod lock;
pub mod manifest;
pub mod resolution;
pub mod workflow;
pub mod workflow_actions;
pub mod workflow_parsed;

/// Wraps a parsed value with a flag indicating whether format migration occurred.
/// Used by manifest parsing only — lock loading uses `Store::load()` directly.
#[derive(Debug)]
pub struct Parsed<T> {
    pub value: T,
    pub migrated: bool,
}
