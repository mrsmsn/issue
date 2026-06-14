# Benchmark: Go vs Rust core (language selection)

This directory holds the reproducible head-to-head used to pick the implementation
language. See `docs/adr/0002-implement-core-in-rust.md` for the decision and results.

The Go prototype is no longer in the tree (Rust was adopted), so re-running the full
comparison requires re-creating the Go binary from that spec. The corpus generator
and timing harness are kept here because they are language-agnostic and remain
useful for profiling the shipping Rust CLI.

## Files

- `gen_issues.go` — deterministic corpus generator (stdlib Go, fixed seed). Writes
  `N` frontmatter-bearing Markdown issues to an output directory.
- `bench.py` — timing harness: 5 warm-up + 40 timed runs per command, reports
  min/mean/median in milliseconds. Edit the `GO` / `RS` paths at the top to point at
  the binaries under test.

## Reproduce

```sh
# 1. generate a 5000-issue corpus
go run bench/gen_issues.go 5000 /tmp/issue-bench/issue

# 2. build the release binary
cargo build --release

# 3. point bench.py at ./target/release/issue and run
python3 bench/bench.py
```

## Result (Apple M4, warm cache, 5000 issues)

| Operation                    | Go (mean) | Rust (mean) | Rust / Go     |
|------------------------------|-----------|-------------|---------------|
| `list`                       | 61.0 ms   | 36.1 ms     | 0.59× (~1.7×) |
| `list --status --label`      | 59.3 ms   | 35.3 ms     | 0.59×         |
| `lint`                       | 78.5 ms   | 35.9 ms     | 0.46× (~2.2×) |
| `view <id>`                  | 78.4 ms   | 45.5 ms     | 0.58×         |

Rust won every operation; output was byte-identical between the two. → **Rust adopted.**
