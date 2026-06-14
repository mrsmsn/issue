# 1. Issue id is a plain integer (no zero-padding)

- Date: 2026-06-14
- Status: Accepted
- Deciders: maintainer
- Related: `issue/1-issue-cli-mvp.md` (epic), `docs/requirements.md`

> Note: ADR filenames follow the adr-tools convention (4-digit zero-padded,
> e.g. `0001-...`). This is a separate numbering scheme from the product's
> issue ids and does **not** contradict this decision.

## Context

`issue` is a local-first issue manager that stores one issue per Markdown file
under `$PROJECT_ROOT/issue/` with YAML frontmatter. Every issue needs a stable
`id`. We had to choose how that id is formatted on disk (frontmatter `id` and the
`<id>-<slug>.md` filename).

The original draft used a fixed-width, zero-padded sequence (`0001`, `0002`, …),
inherited from a generic scaffolding convention rather than deliberated. Two
problems surfaced:

1. **Fixed width has a ceiling that breaks silently.** With 4 digits the sequence
   tops out at `9999`. Worse, the moment it overflows, *lexical* (filename) sort
   breaks: `"10000" < "9999"` because the first character `'1' < '9'`. So the one
   benefit zero-padding buys — filenames sorting in creation order — fails exactly
   when the project grows past the chosen width.
2. **The padding is not actually needed here.** The CLI reads frontmatter, so it
   can sort the listing by `id` *numerically*. Nothing in the tool depends on the
   lexical order of filenames.

## Decision

The issue `id` is a **plain base-10 integer, starting at `1`, with no
zero-padding** (`1`, `2`, …, `12345`).

- The frontmatter `id` is the integer; the filename is `<id>-<slug>.md`
  (e.g. `1-issue-cli-mvp.md`).
- Ordering in any listing is guaranteed by sorting `id` **numerically**, never by
  relying on filename lexical order.
- There is no maximum id and no width to choose.

## Alternatives considered

### A. Plain integer, numeric sort — **chosen**

- No ceiling, no overflow, no width to guess.
- Matches `gh issue`, our UX north star, which displays unpadded ids (`#1`, `#42`,
  `#1234`).
- Cost: raw filenames are not visually column-aligned in a plain `ls`. Mitigated —
  see "Anticipated objection" below.

### B. Fixed-width zero-padded (`NNNN`)

- Pros: visually aligned filenames; lexical sort matches creation order *until
  overflow*.
- Cons: must guess a max width (4? 6? 8?) — wrong eventually or wasteful early;
  silent sort breakage on overflow; diverges from `gh issue`. The headline benefit
  (sort) is redundant because the tool sorts numerically anyway.

### C. Distributed id (ULID / short hash)

- Pros: collision-free across branches without coordination.
- Cons: not human-readable, hard to quote verbally or for agents; loses the simple
  "#N" mental model. Rejected earlier in favor of a readable sequence; the
  team-merge collision concern it addresses is tracked separately (see Consequences).

## Anticipated objection: "a bare `N` is hard to read — make it `NNNN`"

This is expected to come up. The request usually means one of two things, and both
are satisfied **without** baking padding into the id:

1. **"Listings should be column-aligned."** Alignment is a *presentation* concern,
   not an *identity* concern. The `issue list` command can right-align / pad the id
   column at render time (e.g. show `  1`, ` 42`, `123`). This yields aligned output
   *and* keeps the canonical id integer, so there is still no overflow problem.
2. **"Padded ids look more official / ticket-like."** `gh issue` — the tool we
   deliberately mirror — uses unpadded ids. Matching it is a feature, not a gap.

If a contributor still wants padded *filenames* for cosmetics, the only acceptable
form is **display/derivation-time padding with a minimum width that grows
automatically** (pad to at least 4, but never truncate or cap). The canonical `id`
in frontmatter stays an integer regardless. We do **not** adopt a fixed maximum
width, because that reintroduces the overflow bug this decision exists to remove.

## Consequences

- `issue create` assigns `max(existing id) + 1`, starting at `1`.
- Listings sort by numeric `id`; never trust filename lexical order.
- Bare `ls` shows un-aligned filenames; alignment, if wanted, is the `issue list`
  view layer's job.
- Out of scope of this ADR: id **collisions** when two branches both create the next
  integer and later merge. That is a distributed-coordination problem orthogonal to
  id *formatting*, and remains an open question on the MVP epic
  (`issue/1-issue-cli-mvp.md`).
