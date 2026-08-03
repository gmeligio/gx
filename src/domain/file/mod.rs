//! Managed files: how they are addressed, what they contain, and how they are found.
//!
//! A managed file is any file gx reads action references from — a workflow or a
//! composite action definition today, a GitLab CI file later. The four submodules
//! split that along the seams that matter:
//!
//! - [`site`] — addressing. Which file, and where within it. A leaf: it imports
//!   nothing else from the domain, so both manifest overrides and lint ignores can
//!   depend on it without depending on each other.
//! - [`actions`] — the references themselves, and the aggregate view of them.
//! - [`parsed`] — file bodies, per schema.
//! - [`scan`] — the port through which files are discovered and read.

pub mod actions;
pub mod parsed;
pub mod scan;
pub mod site;
