# issue

**A local-first issue tracker that lives in your repo.** Issues are plain
frontmatter-bearing Markdown files under `issue/` — one file per issue, versioned
alongside the code they track. No server, no remote backend, no account. Just files.

The CLI surface mirrors `gh issue` as closely as possible, so muscle memory carries over.
Where it differs from tools like [`git-bug`](https://github.com/git-bug/git-bug): issues
are **readable, diff-able Markdown** you can edit by hand, and they round-trip with GitHub
Issues via JSON `export`/`import`.

```sh
brew install mrsmsn/tap/issue
issue init
issue create --title "Fix the flaky test" --label bug
issue list
```

## Highlights

- **Local-first & offline.** Everything is files in your working tree; works with no network.
- **Files are the contract.** Each issue is one `<id>-<slug>.md` with a small, GitHub-aligned
  frontmatter schema — readable and editable without the tool.
- **`gh issue`-aligned CLI.** Commands, flags, and semantics follow `gh issue` by default.
- **GitHub interchange.** `export`/`import` speak GitHub REST/GraphQL issue JSON, with
  non-destructive id reconciliation.
- **`lazyissue` TUI.** A Lazygit-style terminal UI for browsing, filtering, and editing.
- **Shell completions** for bash/zsh/fish, with dynamic id/label/status completion.
- **Tiny & dependency-free core.** The `issue` CLI is std-only Rust (no `serde`/`clap`),
  so it builds offline and stays small.
- **Agent-friendly.** A repo that contains an `issue/` directory advertises its own
  workflow — a coding agent can read an issue file and pick up the task unaided.

## Install

### Homebrew (recommended)

```sh
brew install mrsmsn/tap/issue
```

Downloads a prebuilt, per-architecture binary (no compile) and installs both `issue` and
`lazyissue`, plus shell completions — so `issue <Tab>` works with no extra setup.

### From source

Requires a Rust toolchain.

```sh
cargo build --release
# -> target/release/issue
# -> target/release/lazyissue
```

## Quick start

```sh
issue init                      # create the ./issue directory (idempotent)

issue create                    # interactive: prompts for title/labels on stdin
issue create --title "Bug X" --label bug --label p1 --body "Steps to reproduce…"

issue list                      # all issues
issue list --status open --label bug
issue view 1                    # print a single issue file

issue edit 1 --add-label needs-repro --status closed
issue close 1
issue reopen 1
```

## Commands

| Command | Description |
| --- | --- |
| `issue init` | Create the issue directory (idempotent; never overwrites). |
| `issue create` | Create an issue — interactive, or via flags. |
| `issue list` | List issues, tab-separated, with optional filters. |
| `issue view <id>` | Print a single issue file. |
| `issue edit <id>` | Edit fields in place (title/status/labels/body). |
| `issue close <id>` | Set status to `closed` and bump `updated`. |
| `issue reopen <id>` | Set status to `open` and bump `updated`. |
| `issue lint` | Detect duplicate ids; exits non-zero if any are found. |
| `issue export` | Print all issues as a GitHub-shaped JSON array (stdout). |
| `issue import [FILE]` | Import issues from GitHub-shaped JSON (file or stdin). |
| `issue completions <shell>` | Print a completion script (`bash`/`zsh`/`fish`). |
| `issue version` | Print the version (also `--version`/`-V`). |

`issue --help` / `-h` prints the top-level help; run `issue <command> --help` for
command-specific options. Key flags:

- **`create`** — `--title T`, `--label L` (repeatable), `--status open|closed`
  (default `open`), `--body TEXT`. With no flags it prompts interactively.
- **`edit`** — `--title T`, `--status S`, `--add-label L` / `--remove-label L`
  (repeatable), `--body TEXT`. Updates fields in place — the filename is never renamed.
- **`list`** — `--status S`, `--label L` filters. Output is
  `<id>\t<status>\t<title>\t<labels>`.

## Issue file format

Each issue is `issue/<id>-<slug>.md`. The integer `id` is the stable identity; the slug
is cosmetic. The issue directory is `$ISSUE_DIR` if set, otherwise `./issue`.

```markdown
---
id: 1
title: Issue CLI MVP
status: open
created: 2026-06-14
updated: 2026-06-14
labels: [feature, epic]
---

## Background
Why this issue exists…

## Related
- #2
```

Schema (GitHub-aligned — see [ADR 0003](docs/adr/0003-github-aligned-frontmatter-schema.md)):

| key | meaning |
| --- | --- |
| `id` | Plain integer ≥ 1, no zero-padding. Immutable — never renumbered. |
| `title` | One-line summary. |
| `status` | `open` or `closed`. |
| `created` / `updated` | `YYYY-MM-DD`. |
| `labels` | Inline list, e.g. `[bug, p1]`. Use labels for categorization. |

There is intentionally **no `type` field** (categorize with labels) and **no `related`
field** (cross-reference issues in the body, e.g. a `## Related` section with `- #N`
links). Unknown keys in a file (such as a `priority:` field) are preserved on edit, not
dropped. Ids are allocated optimistically as `max(existing) + 1`; cross-branch collisions
are tolerated and surfaced by `issue lint` rather than auto-renumbered (see
[ADR 0001](docs/adr/0001-issue-id-is-a-plain-integer.md)).

## GitHub interchange

Round-trip with GitHub Issues using their JSON shape (see
[ADR 0004](docs/adr/0004-github-json-interchange-for-export-import.md)):

```sh
issue export > issues.json          # pretty JSON array, sorted by id
issue import issues.json            # or: gh issue list --json … | issue import
```

`import` is lenient — it accepts REST (`created_at`) or GraphQL (`createdAt`) key shapes
and labels as either strings or `{ "name": … }` objects. It never overwrites existing
files: a source issue number is kept when free, otherwise a fresh id is assigned and the
remap is reported (`imported #N (was #M)`).

## lazyissue (TUI)

A Lazygit-style terminal UI built on [ratatui](https://ratatui.rs). Three panes —
**Filters**, **Issues**, **Detail** — over the same `issue/` directory.

```sh
lazyissue
```

| Key | Action |
| --- | --- |
| `j` / `k`, `↓` / `↑` | Move down / up |
| `g` / `G` | Jump to first / last |
| `Ctrl-d` / `Ctrl-u` | Half-page down / up |
| `Tab` / `BackTab`, `h` / `l` | Cycle panes |
| `Enter` / `Space` | Apply highlighted filter (Filters pane) |
| `n` | New issue |
| `e` | Edit issue |
| `b` | Edit issue body in `$EDITOR` |
| `c` / `o` | Close / reopen the selected issue |
| `/` | Search by title (case-insensitive) |
| `R` | Reload from disk |
| `?` | Toggle help |
| `q` / `Esc` | Quit |

## Shell completions

Homebrew installs completions automatically. To set them up manually, source the output of:

```sh
issue completions bash   # or: zsh, fish
```

Completion is dynamic: it suggests existing issue ids, labels, and statuses as you type.

## Development

This is a Cargo workspace ([ADR 0005](docs/adr/0005-tui-lazyissue-and-workspace-split.md)):

- **`crates/core`** (`issue-core`) — pure logic, storage, the create/edit/close service
  layer, and a hand-rolled JSON parser. **Zero external dependencies** (std-only).
- **`crates/cli`** (`issue`) — the CLI shell; depends only on `issue-core`, so it stays
  std-only and offline.
- **`crates/tui`** (`lazyissue`) — the TUI; the only crate allowed dependencies
  (ratatui/crossterm/notify).

```sh
cargo test --workspace          # run the full test suite
cargo build -p issue --offline  # proves the CLI stays dependency-free
cargo run -p lazyissue          # launch the TUI (needs a real terminal)
```

Keeping `issue-core` and the `issue` CLI std-only is a hard rule — don't add crates to
them without a new ADR (rationale in
[ADR 0002](docs/adr/0002-implement-core-in-rust.md)). Design decisions live in
[`docs/adr/`](docs/adr/) and contributor guidance in [`CLAUDE.md`](CLAUDE.md).

### Architecture decisions

- [ADR 0001](docs/adr/0001-issue-id-is-a-plain-integer.md) — issue id is a plain integer
- [ADR 0002](docs/adr/0002-implement-core-in-rust.md) — implement the core in Rust
- [ADR 0003](docs/adr/0003-github-aligned-frontmatter-schema.md) — GitHub-aligned frontmatter schema
- [ADR 0004](docs/adr/0004-github-json-interchange-for-export-import.md) — GitHub JSON interchange for export/import
- [ADR 0005](docs/adr/0005-tui-lazyissue-and-workspace-split.md) — the `lazyissue` TUI and workspace split
- [ADR 0006](docs/adr/0006-prebuilt-binary-homebrew-distribution.md) — prebuilt-binary Homebrew distribution

## License

[MIT](LICENSE).
