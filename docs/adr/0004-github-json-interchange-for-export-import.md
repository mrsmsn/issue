# 4. GitHub-JSON interchange for export/import (lenient import, non-destructive id remap)

- Date: 2026-06-14
- Status: Accepted
- Deciders: maintainer
- Related: `docs/adr/0003-github-aligned-frontmatter-schema.md`,
  `docs/adr/0002-implement-core-in-rust.md`, `issue/1-issue-cli-mvp.md`

## Context

A stated goal is GitHub Issues **export/import compatibility**. After ADR 0003
the local schema (`id/title/status/created/updated/labels` + body) maps cleanly to
a GitHub issue, so we need to fix (a) the wire format, (b) how lenient import is,
and (c) what happens to ids on import when they collide with local issues.

## Decision

**Format: the GitHub REST API issue JSON shape, as a top-level array.** Each
element on export:

```json
{
  "number": 1,
  "title": "…",
  "state": "open",                 // "open" | "closed"
  "state_reason": null,            // "completed" when closed, else null
  "labels": [{ "name": "cli" }],   // array of {name} objects
  "created_at": "2026-06-14T00:00:00Z",
  "updated_at": "2026-06-14T00:00:00Z",
  "body": "…markdown…"
}
```

- `export` writes the pretty-printed array to stdout (sorted by id); redirect to a
  file. Local dates (`YYYY-MM-DD`) are widened to `…T00:00:00Z`.
- **Import is lenient** so it accepts dumps from both the REST API and the `gh` CLI:
  - keys in snake_case (`created_at`) **or** camelCase (`createdAt`);
  - `labels` as an array of strings **or** of `{ "name": … }` objects;
  - `body` / `number` optional; `state` anything-but-"closed" → open;
  - datetimes reduced to their date part; missing created → today, missing updated → created.
- **id reconciliation on import is non-destructive.** For each incoming issue: keep
  its `number` if present and not already used (by an existing local issue or an
  earlier issue in the same batch); otherwise assign `max(used) + 1`. Remapped
  issues are reported as `imported #<new> (was #<old>)`. Existing files are never
  overwritten (a name clash is skipped with a warning).

JSON is parsed and serialized by a hand-rolled, std-only module (`src/json.rs`),
per ADR 0002 (no external crates).

## Alternatives considered

- **`gh` camelCase as the canonical export shape.** Rejected for export (the REST
  API shape is the more universal "GitHub Issues JSON"), but import accepts it.
- **Preserve numbers and error/skip on collision.** Rejected: skipping loses data;
  erroring makes importing into a non-empty repo painful. Remapping is complete and
  non-destructive — consistent with the project's "ids are immutable, collisions
  are surfaced not auto-renamed" stance, since here we are *minting new local
  issues*, not renumbering existing ones.
- **Round-trip every field GitHub has** (assignees, milestone, comments, …).
  Out of scope: those are GitHub-specific features the project explicitly excludes.
  Unknown keys on import are ignored; local-only concerns live in the body.

## Consequences

- `issue export` / `issue import [FILE|-stdin]` are implemented; `export | import`
  round-trips identically (verified).
- Fields GitHub has but we don't model are dropped on export (we emit only the
  mapped subset) and ignored on import.
- A local `priority:` (or any extra frontmatter key) is **not** exported — export
  reflects the canonical schema only. If round-tripping extra keys ever matters,
  revisit here.
- Possible future work: `export --output FILE`, a `--dry-run` for import, and
  mapping `close --reason` to `state_reason`.
