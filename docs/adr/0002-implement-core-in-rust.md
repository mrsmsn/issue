# 2. Implement the core CLI in Rust (chosen over Go by benchmark)

- Date: 2026-06-14
- Status: Accepted
- Deciders: maintainer
- Related: `issue/1-issue-cli-mvp.md` (epic), `docs/requirements.md`,
  `docs/adr/0001-issue-id-is-a-plain-integer.md`

## Context

The MVP (`init` / `create` / `list` / `view`, plus `lint`) had to be built in a
single systems language. The hot path is `issue list`: scan every Markdown file in
the issue directory, parse its frontmatter, sort numerically by `id`, optionally
filter, and print. In a team/OSS repo this directory can hold thousands of files,
so the per-invocation cost of "scan + parse N files" — including process startup,
since a CLI pays it on every call — is what matters.

Rather than pick by intuition, we implemented the **same spec twice** — once in Go,
once in Rust — and benchmarked them head-to-head.

To keep the comparison fair, both implementations:

- use **only their standard library** (no YAML/CLI crates or modules; a hand-rolled
  minimal frontmatter parser), so neither benefits from a third-party fast path and
  both build fully offline;
- produce **byte-identical output** (verified: `diff` over the full 5000-line `list`
  output and the filtered output matched exactly);
- read files concurrently with a thread pool sized to the available parallelism;
- read each file only up to the closing `---` frontmatter fence (never the body).

## Benchmark

- Corpus: **5000** generated issue files (`bench/gen_issues.go`, deterministic).
- Harness: `bench/bench.py` — 5 warm-up runs, then 40 timed runs per command;
  warm filesystem cache. Machine: Apple M4 (10 cores), macOS.
- Release builds (`cargo build --release` with LTO; `go build -ldflags='-s -w'`).

| Operation                     | Go (mean) | Rust (mean) | Rust / Go     |
|-------------------------------|-----------|-------------|---------------|
| `list` (5000)                 | 61.0 ms   | 36.1 ms     | 0.59× (~1.7×) |
| `list --status open --label`  | 59.3 ms   | 35.3 ms     | 0.59×         |
| `lint` (scan + dup detect)    | 78.5 ms   | 35.9 ms     | 0.46× (~2.2×) |
| `view <id>` (single lookup)   | 78.4 ms   | 45.5 ms     | 0.58×         |

Rust was faster on every operation, by ~1.7–2.2×. Both were correct and feature-
equivalent (30 Rust tests, full Go test suite — all green). A meaningful share of
the gap is process-startup overhead (visible in single-`view`), which a CLI pays on
every invocation, so it counts.

## Decision

The core `issue` CLI is implemented in **Rust**, std-only, at the repository root
(`Cargo.toml`, `src/`). The Go prototype is dropped. The benchmark harness and
corpus generator are kept under `bench/` so the comparison is reproducible.

## Alternatives considered

- **Go — rejected.** Slower on the hot path here, and the startup cost is paid on
  every CLI call. Idiomatic and quick to write, but it lost the measured race.
- **Rust — chosen.** Fastest measured; std-only build is offline and dependency-
  free; the clean core/storage split (pure logic vs I/O) keeps a future TUI
  ("LazyIssue") able to consume the same layers.

## Consequences

- Build: `cargo build --release` → `target/release/issue`. Test: `cargo test`.
- std-only: no `serde`/`clap`. Arg parsing, the frontmatter parser, the date
  computation (Hinnant civil-from-days), and the worker pool are hand-rolled. New
  contributors should not reach for crates without revisiting this decision.
- `Cargo.lock` is committed (binary crate).
- Reproduce the benchmark: `go run bench/gen_issues.go 5000 /tmp/issue-bench/issue`
  then `python3 bench/bench.py` (see `bench/README.md`).
- The byte-identical-output requirement was a test artifact; the shipping CLI may
  later add presentation niceties (e.g. column alignment) to `list` without
  affecting this decision.
