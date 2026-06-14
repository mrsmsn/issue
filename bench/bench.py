#!/usr/bin/env python3
import os, subprocess, time, statistics, sys

GO = "/Users/mosh/src/github.com/mrsmsn/issue/prototype/go/issue"
RS = "/Users/mosh/src/github.com/mrsmsn/issue/prototype/rust/target/release/issue"
env = dict(os.environ, ISSUE_DIR="/tmp/issue-bench/issue")

def run(cmd):
    subprocess.run(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

def bench(name, cmd, warmup=5, n=40):
    for _ in range(warmup):
        run(cmd)
    ts = []
    for _ in range(n):
        t0 = time.perf_counter()
        run(cmd)
        ts.append((time.perf_counter() - t0) * 1000.0)
    ts.sort()
    return name, min(ts), statistics.mean(ts), statistics.median(ts)

def show(rows):
    for name, mn, mean, med in rows:
        print(f"  {name:<12} min={mn:7.2f}ms  mean={mean:7.2f}ms  median={med:7.2f}ms")

print("=== list (scan+parse+sort+print 5000) ===")
g = bench("Go list",   [GO, "list"])
r = bench("Rust list", [RS, "list"])
show([g, r]); print(f"  -> Rust/Go mean ratio: {r[2]/g[2]:.2f}x")

print("=== list --status open --label perf (scan+parse+filter) ===")
g = bench("Go filt",   [GO, "list", "--status", "open", "--label", "perf"])
r = bench("Rust filt", [RS, "list", "--status", "open", "--label", "perf"])
show([g, r]); print(f"  -> Rust/Go mean ratio: {r[2]/g[2]:.2f}x")

print("=== lint (scan+parse+dup detect) ===")
g = bench("Go lint",   [GO, "lint"])
r = bench("Rust lint", [RS, "lint"])
show([g, r]); print(f"  -> Rust/Go mean ratio: {r[2]/g[2]:.2f}x")

print("=== view 2500 (single lookup) ===")
g = bench("Go view",   [GO, "view", "2500"])
r = bench("Rust view", [RS, "view", "2500"])
show([g, r]); print(f"  -> Rust/Go mean ratio: {r[2]/g[2]:.2f}x")
