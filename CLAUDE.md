# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

The **MVP is implemented in Rust** (`init` / `create` / `list` / `view` / `lint`),
plus follow-up commands: `edit` / `close` / `reopen`, GitHub `export` / `import`, and
shell `completions` (bash/zsh/fish; with dynamic id/label/status completion via hidden
`__complete-ids`/`__complete-labels` helpers — see `crates/cli/src/completions.rs`).
A lazygit-style **TUI `lazyissue`** is also implemented (ADR 0005).
Rust was chosen over Go by a head-to-head benchmark — see
`docs/adr/0002-implement-core-in-rust.md` and `bench/`.

The repo is a **Cargo workspace** (ADR 0005):
- `crates/core` — pkg `issue-core` (lib), **zero external deps** (std-only): modules
  `core` (pure logic), `storage` (I/O), `ops` (create/edit/close service layer shared by
  both binaries), `json` (hand-rolled JSON for export/import).
- `crates/cli` — pkg `issue`, bin `issue`, depends only on `issue-core` → stays std-only/offline.
- `crates/tui` — pkg `lazyissue`, bin `lazyissue`, depends on `issue-core` + ratatui/crossterm/notify.

`edit`/`close`/`reopen` edit frontmatter **surgically** (line-level, via
`core::update_frontmatter`): they preserve the body, key order, and any keys not in
the schema (e.g. a `priority:` field on dogfooding issues). They never rename the
file — the integer `id` is the stable identity, the filename slug is cosmetic.
`export` / `import` use GitHub REST-API issue JSON (`crates/core/src/json.rs` is a
hand-rolled, std-only JSON parser/serializer — no serde, per ADR 0002). `export` writes a pretty
JSON array to stdout; `import [FILE|stdin]` is lenient (snake_case or camelCase keys;
labels as strings or `{name}` objects) and remaps colliding ids non-destructively
(`imported #N (was #M)`), never overwriting files. See ADR 0004.

This is an **OSS project**. Per global instructions, write documentation, commit messages, and code comments in **English** (the requirements doc itself is in Japanese as a working design note).

### Layout & commands

- `crates/core/src/{core,storage,ops,json}.rs` — the shared `issue-core` lib (see status above).
- `crates/cli/src/main.rs` — `issue` CLI shell: arg parsing, command dispatch, help; calls `issue_core::ops`.
- `crates/tui/src/{main,app,event,ui,form,editor}.rs` — `lazyissue` TUI (ratatui).
- `bench/` — corpus generator + timing harness (reproducible language benchmark).

```sh
cargo build --release            # -> target/release/{issue, lazyissue}
cargo test --workspace           # 100 tests (issue-core incl. ops + TUI state logic)
cargo build -p issue --offline   # proves the CLI stays dependency-free
cargo run -p lazyissue           # launch the TUI (needs a real terminal)
```

**std-only for `issue-core` + the `issue` CLI** (no `serde`/`clap`) — ADR 0002, scoped by
ADR 0005. Don't add crates to those two without a new ADR. The **TUI is the only crate
allowed deps** (ratatui/crossterm/notify). `cargo clippy` needs `rustup component add clippy`.
`Cargo.lock` is committed at the workspace root.

Runtime: the issue directory is `$ISSUE_DIR` if set, else `./issue`. Keep the
core/storage/ops split intact — pure logic stays I/O-free; mutations go through `ops`.

## What this project is

A **local-first issue-management CLI** (the `issue` command). Issues are stored as frontmatter-bearing Markdown files under `$PROJECT_ROOT/issue/`, created via an interactive prompt flow. There is no server or remote backend — everything lives in the repo alongside the code it tracks (compare: `git-bug`, which is the main competitor to differentiate against).

Key design constraints (from `docs/requirements.md`):

- **CLI surface mirrors `gh issue`** as closely as possible. When designing commands/options/flags, default to matching `gh issue` semantics. GitHub-specific features are explicitly out of scope, *except* that GitHub Issues **export/import** compatibility is a goal.
- **Issue files are the contract.** Each issue is one Markdown file (`<id>-<slug>.md`) with this frontmatter schema (GitHub-aligned — see ADR 0003):
  - `id: integer` (≥1, no zero-padding — see ADR 0001), `title: string`, `status: open | closed`, `created: <YYYY-MM-DD>`, `updated: <YYYY-MM-DD>`, `labels: []`
  - **No `type`** (categorize with labels) and **no `related`** (cross-reference issues in the body, e.g. a `## Related` section with `- #N` links). Such keys in older files are ignored by the parser, not an error.
- Issues are created **interactively** by `issue create` (no flags), or non-interactively via `--title/--label/--status/--body`.

## Resolved design decisions

- **`id` generation.** Plain integer from 1, no zero-padding (ADR 0001). Allocation is **optimistic** (`max(existing id) + 1`, computed locally). IDs are **immutable** — never renumbered. Cross-branch merge collisions are tolerated and surfaced by `issue lint` (duplicate-id detection, non-zero exit); there is deliberately **no `rebase`/renumber** command, because a mutable id would break `#N` references, `related`, branch names, and commit messages. See ADR 0001 and `issue/1-issue-cli-mvp.md`.

## Open design questions (unresolved — do not assume an answer)

- **Agent-discoverability.** A goal is that a coding agent notices "this repo uses an issue-based workflow" *without* being explicitly told. Factor this into file layout / naming / conventions.
- **git-bug** differentiation/positioning; surfacing the issue workflow to coding agents.

## Future direction

The TUI `lazyissue` (à la Lazygit) is **implemented** in `crates/tui` (ADR 0005) — it was
once considered for a separate repo but lives in this workspace, consuming `issue-core`.
