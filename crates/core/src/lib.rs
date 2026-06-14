//! `issue-core` — the shared, std-only core of the issue tool.
//!
//! [`core`] is pure logic (frontmatter parse, slug, id allocation, sort/filter,
//! lint, date, in-place frontmatter edits, GitHub-JSON mapping). [`storage`]
//! handles filesystem I/O. [`ops`] is the small service layer that composes the
//! two into the create/edit/close mutations shared by the CLI and the TUI.
//! [`json`] is a minimal std-only JSON parser/serializer.
//!
//! No external crates here (ADR 0002, scoped by ADR 0005): both the `issue` CLI
//! and this library stay dependency-free. The TUI is the only target allowed
//! external crates.

pub mod core;
pub mod json;
pub mod ops;
pub mod storage;
