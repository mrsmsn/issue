# 3. GitHub-aligned frontmatter schema (status open/closed; drop type and related)

- Date: 2026-06-14
- Status: Accepted
- Deciders: maintainer
- Related: `docs/requirements.md`, `issue/1-issue-cli-mvp.md`,
  `docs/adr/0001-issue-id-is-a-plain-integer.md`

## Context

The original frontmatter schema carried four status values
(`open | closed | in-progress | wontfix`), a dedicated `type` field, and a
`related` list of issue ids. The project's north star is to mirror GitHub Issues
(including future export/import compatibility), and GitHub models these concerns
differently:

- An issue has exactly two states: **open** or **closed** (with an optional
  close *reason*, not a distinct status).
- There is no first-class free-text "type" on the issue body; categorization is
  done with **labels**.
- Relationships between issues are expressed in the **body** as cross-references
  (`#N`), not as a structured field.

Carrying schema that GitHub does not have makes export/import lossy and forces
arbitrary mapping decisions (e.g. where does `in-progress` go?).

## Decision

Align the schema with GitHub:

1. **`status` is `open | closed` only.** `in-progress` and `wontfix` are removed.
   Finer-grained state is expressed with labels (e.g. `in-progress`, `wontfix`).
2. **Drop the `type` field.** Type/category is a label like any other (`bug`,
   `feature`, `epic`, …).
3. **Drop the `related` field.** Related issues are referenced in the body, by
   convention in a trailing `## Related` section with `- #N` links.

The resulting frontmatter is: `id`, `title`, `status`, `created`, `updated`,
`labels`.

## Consequences

- The parser still reads files containing legacy `type:` / `related:` lines (and
  any other unknown key, such as a `priority:`): unknown keys are **ignored**, not
  errors. So old files are not broken; they simply lose the dropped fields when a
  command rewrites them.
- `issue create` no longer has `--type` and no longer prompts for type;
  `issue edit` no longer has `--type`. `--status` accepts only `open`/`closed`.
- The existing epic `issue/1-issue-cli-mvp.md` was migrated: `type: epic` became a
  label `epic`, and the empty `related: []` was removed. (`priority: P1` is left in
  place — it is a tolerated extra field, not part of the canonical schema.)
- Export/import becomes mostly mechanical: `status` maps directly, `labels` map to
  GitHub labels, the body maps to the issue body. The remaining open work is the
  concrete JSON shape and id reconciliation on import — tracked on the MVP epic.
- `close`/`reopen` are unchanged (they already targeted open/closed). A future
  `--reason` on `close` could map to GitHub's `state_reason` if wanted.
