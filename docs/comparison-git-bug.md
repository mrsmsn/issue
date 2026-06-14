# Positioning: `issue` vs. git-bug

[git-bug](https://github.com/git-bug/git-bug) is the closest prior art to this project and
our main point of comparison. Both are **local-first / offline** issue trackers that live in
a git repo with no server. This doc records how `issue` deliberately differs, so design
decisions stay consistent and users can pick the right tool.

## The one-line difference

> **git-bug stores issues as objects inside git's database — you need the git-bug tool to
> read them. `issue` stores issues as plain Markdown files in the working tree — any human or
> coding agent can read and edit them with no tool, no API, and no auth.**

git-bug "embeds issues, comments, and more as objects in a git repository (_not files!_)".
That is a real strength (see below) but it means the data is opaque without git-bug. `issue`
makes the opposite bet: issues are `issue/<id>-<slug>.md` files you can `cat`, `grep`, edit in
any editor, review in a normal PR diff, and read on the GitHub web UI — and that a coding
agent notices and uses without being told (the agent-discoverability goal).

## Side-by-side

| Dimension | `issue` (this project) | git-bug |
|---|---|---|
| **Storage** | Plain Markdown + YAML frontmatter files in the working tree (`issue/*.md`) | Operations serialized as JSON in **git blobs / refs** (not working-tree files) |
| **Readable without the tool** | **Yes** — `cat`/`grep`/editor/PR diff/GitHub web/coding agents | No — requires `git bug` to materialize state |
| **ID** | Sequential integer, gh-style (`#1`, `#42`); immutable, easy to quote | Content hash of the first operation, shown as a 7-char prefix |
| **Merge / conflict model** | Plain git text merge of files; logical id collisions are **detected** by `issue lint` (no auto-renumber) | **Operation-based CRDT** — entities merge **conflict-free** (Lamport-clock ordering) |
| **Comments / threads** | None as a first-class model — one Markdown body you edit (discussion lives in the body or the PR) | First-class comment threads with full history |
| **Per-field history** | git history of the file (`git log -p issue/<id>-*.md`) | Operation log: granular, per-field, independent of file diffs |
| **Identities / signing** | None (git author of the commit) | First-class identities, optional cryptographic signing |
| **GitHub/GitLab interop** | One-shot `export`/`import` (GitHub-shaped JSON) | Mature **bidirectional bridges** that sync with GitHub/GitLab |
| **Interfaces** | CLI (`issue`, mirrors `gh issue`) + TUI (`lazyissue`) | CLI + TUI + **web UI** |
| **Working tree** | Adds `issue/*.md` (visible alongside code — intentional) | Leaves the working tree untouched (no clutter) |
| **Scope** | Small, focused, std-only core; quick to install/build | Broad, mature decentralized bug tracker |

## Where `issue` is the better fit

- **Coding agents & automation.** An agent (or any script) reads/writes `issue/*.md` directly
  — no binary, API token, or schema knowledge required. This is the headline reason to exist.
- **Human/PR-native review.** Issues change in the same diff as the code, reviewable in a
  normal pull request and on the GitHub web file view.
- **Quotable IDs & familiar UX.** `#42` beats a hash for talking, branch names (`42-foo`),
  and commit references; the CLI mirrors `gh issue`, so there's little to learn.
- **Minimal footprint.** A std-only core (no deps) and trivial install; the TUI is optional.

## Where git-bug is the better fit

- **Conflict-free distributed collaboration.** Its CRDT model merges concurrent edits without
  conflicts — stronger than our "tolerate + `lint`-detect id collisions" approach.
- **Rich tracker model.** Comment threads, granular per-field history, and identities/signing
  out of the box.
- **Live platform sync.** Bidirectional GitHub/GitLab bridges (we only do one-shot JSON
  export/import).
- **A web UI**, and an uncluttered working tree (data hidden in git objects).

## Consequences for our roadmap

- Don't chase parity on CRDT merges, comment threads, or live bridges — those are git-bug's
  turf and conflict with our "plain files, readable by anything" thesis.
- Do double down on the differentiators: file readability/diffability, agent-discoverability,
  gh-like ergonomics, and lightweight GitHub export/import.
- The id-collision trade-off is deliberate (ADR 0001): we keep human, sequential, immutable
  ids and surface collisions via `issue lint` rather than adopting hashes/CRDTs.

## Sources

- [git-bug — README](https://github.com/git-bug/git-bug)
- [git-bug — `doc/model.md` (operations, hash ids, conflict-free merge)](https://github.com/git-bug/git-bug/blob/master/doc/model.md)
