# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

The **MVP is implemented in Rust** (`init` / `create` / `list` / `view` / `lint`),
plus follow-up status/edit commands (`edit` / `close` / `reopen`).
Rust was chosen over Go by a head-to-head benchmark — see
`docs/adr/0002-implement-core-in-rust.md` and `bench/`.

`edit`/`close`/`reopen` edit frontmatter **surgically** (line-level, via
`core::update_frontmatter`): they preserve the body, key order, and any keys not in
the schema (e.g. a `priority:` field on dogfooding issues). They never rename the
file — the integer `id` is the stable identity, the filename slug is cosmetic.
Still **not** implemented: `export` / `import` (blocked on the GitHub field-mapping
design question — `status` has 4 values vs GitHub's open/closed; `type`/`related`
have no native GitHub equivalent).

This is an **OSS project**. Per global instructions, write documentation, commit messages, and code comments in **English** (the requirements doc itself is in Japanese as a working design note).

### Layout & commands

- `src/main.rs` — CLI shell: hand-rolled arg parsing, command dispatch, help.
- `src/core.rs` — **pure logic, no I/O** (slug, id allocation, frontmatter parse, sort/filter, lint, date) — unit-testable; this is the layer a future TUI consumes.
- `src/storage.rs` — **all filesystem I/O**: issue-dir resolution, concurrent loader, write/render, lookup.
- `bench/` — corpus generator + timing harness (reproducible language benchmark).

```sh
cargo build --release     # -> target/release/issue
cargo test                # 30 tests (core + storage)
```

**std-only, no external crates** (no `serde`/`clap`) — a deliberate constraint from ADR 0002 for offline, dependency-free builds. Don't add crates without revisiting it. `cargo clippy` needs `rustup component add clippy`. `Cargo.lock` is committed (binary crate).

Runtime: the issue directory is `$ISSUE_DIR` if set, else `./issue`. Keep the core/storage split intact — logic stays I/O-free.

## What this project is

A **local-first issue-management CLI** (the `issue` command). Issues are stored as frontmatter-bearing Markdown files under `$PROJECT_ROOT/issue/`, created via an interactive prompt flow. There is no server or remote backend — everything lives in the repo alongside the code it tracks (compare: `git-bug`, which is the main competitor to differentiate against).

Key design constraints (from `docs/requirements.md`):

- **CLI surface mirrors `gh issue`** as closely as possible. When designing commands/options/flags, default to matching `gh issue` semantics. GitHub-specific features are explicitly out of scope, *except* that GitHub Issues **export/import** compatibility is a goal.
- **Issue files are the contract.** Each issue is one Markdown file (`<id>-<slug>.md`) with this frontmatter schema:
  - `id: integer` (≥1, no zero-padding — see ADR 0001), `title: string`, `status: open | closed | in-progress | wontfix`, `type: string`, `created: <YYYY-MM-DD>`, `updated: <YYYY-MM-DD>`, `labels: []`, `related: []`
- Issues are created **interactively** by `issue create` (no flags), or non-interactively via `--title/--type/--label/--status/--body`.

## Resolved design decisions

- **`id` generation.** Plain integer from 1, no zero-padding (ADR 0001). Allocation is **optimistic** (`max(existing id) + 1`, computed locally). IDs are **immutable** — never renumbered. Cross-branch merge collisions are tolerated and surfaced by `issue lint` (duplicate-id detection, non-zero exit); there is deliberately **no `rebase`/renumber** command, because a mutable id would break `#N` references, `related`, branch names, and commit messages. See ADR 0001 and `issue/1-issue-cli-mvp.md`.

## Open design questions (unresolved — do not assume an answer)

- **Agent-discoverability.** A goal is that a coding agent notices "this repo uses an issue-based workflow" *without* being explicitly told. Factor this into file layout / naming / conventions.
- **GitHub export/import** field mapping; **git-bug** differentiation; whether to fix the `type` vocabulary or leave it free-form.

## Future direction (not in this repo yet)

A TUI app "Lazyissue" (à la Lazygit) is envisioned, possibly in a separate repository. Keep the core CLI/storage layer cleanly separable so a TUI can consume it.
