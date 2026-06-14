# 5. TUI (`lazyissue`) with ratatui; split into a Cargo workspace

- Date: 2026-06-14
- Status: Accepted
- Deciders: maintainer
- Related: `docs/adr/0002-implement-core-in-rust.md` (scoped by this ADR),
  `docs/requirements.md`, `issue/1-issue-cli-mvp.md`

## Context

`docs/requirements.md` / `CLAUDE.md` envisioned a lazygit-style TUI ("LazyIssue"),
with the core "kept cleanly separable so a TUI can consume it." Building it surfaced
two facts: (1) the repo was a single **binary** crate with `core`/`storage`/`json`
as private modules — nothing could reuse them; and (2) a TUI needs terminal raw-mode,
which the std library cannot do, conflicting with **ADR 0002 (std-only, no crates)**.

## Decision

1. **Cargo workspace.** Extract the logic into a library and split into three crates:
   - `crates/core` — package `issue-core` (lib), **zero external deps** (the existing
     `core`/`json`/`storage` modules, plus a new `ops` service layer).
   - `crates/cli` — package `issue`, bin `issue`, depends only on `issue-core`.
   - `crates/tui` — package `lazyissue`, bin `lazyissue`, depends on `issue-core` +
     `ratatui`/`crossterm`/`notify`.
   The root `Cargo.toml` is a virtual workspace manifest (members + shared
   `[workspace.package]`/`[workspace.dependencies]` + the release profile).
2. **Scope ADR 0002 to `issue-core` + the `issue` CLI.** They stay std-only and build
   offline. The TUI is the **explicit, only** exception allowed external crates:
   `ratatui` (widgets/layout), `crossterm` (raw mode/events, used via
   `ratatui::crossterm`), `notify` (file-watch). Do not add crates to core/cli without
   a new ADR.
3. **Single-source mutations in `issue-core::ops`** (`create_issue`/`set_status`/
   `edit_issue`) so the CLI and TUI share one implementation rather than duplicating
   create/edit/close orchestration. The CLI commands keep their exact printed strings.

## TUI shape (`lazyissue`)

- **3-pane lazygit layout:** Filters (Open/Closed/All + labels with counts) │ Issues
  list │ Detail; `Tab`/`h`/`l` switch panes. **vim + arrow** keys.
- v1: browse + detail, `c` close / `o` reopen, `n` create / `e` edit (modal form),
  `b` body via `$EDITOR`, `/` title search, status/label filter, `R` reload, `?` help.
- **Live reload** via `notify`, debounced ~200ms; in-app mutations reload immediately
  (idempotent, so the watcher event is harmless).
- Body editing shells out to `$VISUAL`/`$EDITOR`/`vi` (suspend → temp file → resume).

## Consequences

- Build: `cargo build` (workspace), `cargo run -p lazyissue`. The `issue` CLI still
  builds offline (`cargo build -p issue --offline`) proving it stayed dependency-free.
- `cargo test --workspace` covers `issue-core` (incl. `ops`) and headless TUI
  state-transition tests; TUI rendering is verified manually (needs a real TTY).
- The TUI shares all logic via `issue-core`; a change to schema/ops is reflected in
  both binaries automatically.
- Build/fetching the TUI crate requires crates.io network access; the CLI does not.
